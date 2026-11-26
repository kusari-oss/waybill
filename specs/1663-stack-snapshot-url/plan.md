# Implementation Plan: Stack snapshots pinned by URL

**Branch**: `1663-stack-snapshot-url` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

## Summary

Three changes, plus a fourth added to scope at plan time (research R1).

1. **Read the string form of `original`** (R1). Today every `stack.yaml.lock` that pins its snapshot by name is rejected as a whole, packages included.
2. **Name a snapshot** from the resolver, a stackage-snapshots URL (R3), or any other URL (R4). Never `unknown`.
3. **Record the lock file** as the snapshot's source, merging duplicates (R6).
4. **Classify by source type:** every unreachable `hackage-snapshot` component is `build-toolchain` (R2), so the URL-pinned snapshot is explained the way resolver-named ones are.

## Technical Context

- **Language:** Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
- **Dependencies:** existing only (`serde`, `serde_yaml`, `serde_json`, `tracing`). **Zero new Cargo dependencies.**
- **Code:**
  - `waybill-cli/src/scan_fs/package_db/haskell.rs`: the lock types, `parse_stack_lock`, `build_snapshot_placeholder`, and Phase F's emit loop;
  - `waybill-cli/src/generate/orphan_reason.rs`: mechanism E.

## Constitution Check

| Rule | Status |
|---|---|
| I. Pure Rust, statically linked | Pass. |
| V. Native fields first | Pass. purl-spec's `download_url` qualifier carries the URL (R4); no new `waybill:*` row. |
| Measure before designing (CLAUDE.md) | Pass. R1–R3 measured; the corpus is measured before and after (FR-006). |
| Pre-PR gate 4.1.0 | Unchanged. |

## Design

1. **Lock types.** Make `StackSnapshotYaml.original` an untagged enum of a string or a map `{resolver?, url?}`. Read `completed.url`, which is already deserialized.
2. **`snapshot_identity(original, completed) -> SnapshotId`.** Follows R5, then R3 or R4. `SnapshotId` is either a resolver name, or a URL with no stackage match.
3. **`StackSnapshot`** gains `id: SnapshotId` and `source: PathBuf`. `extract_resolver_from_stack_yaml` sets `source` to the `stack.yaml`.
4. **`build_snapshot_placeholder`:**
   - a resolver → today's PURL shapes;
   - a URL → R4's PURL;
   - `source_path` → the lock file.
5. **Phase F.** Merge snapshots with the same PURL, keeping the first `source_path` and sorting the rest into `extra_source_paths`.
6. **`orphan_reason`.** Mechanism E fires for `waybill:source-type = hackage-snapshot`. The existing three-condition test is kept for other components, so nothing else moves.
7. **Invariant declarations.** Remove the two I7 rows for `hpack-hs-opentelemetry` and its no-generated-cabal variant.

## Tests (FR-007)

- **Unit tests** for `snapshot_identity`:
  - an `lts` URL;
  - a `nightly` URL, with padding;
  - a non-stackage URL;
  - a URL with a non-numeric part;
  - a resolver given as a string, and as a map;
  - `completed.url` as the fallback.
  Plus `parse_stack_lock` on the string form, which is the R1 regression.
- **A fixture-level positive control** (`waybill-fixture-*`) with four lock files: `lts` URL, `nightly` URL, non-stackage URL, and resolver name. One of the URL locks and the resolver lock name the same snapshot (FR-002). It asserts:
  - each identity;
  - one component for the shared snapshot, listing both lock files;
  - every snapshot either reachable or `build-toolchain`;
  - no `unknown`.
- **Mutations**, one per naming arm, the string form, the padding, the merge, and the orphan rule. Each is reverted with `cp`, checked with `cmp`, and followed by a rebuild.

## Measurement (FR-006)

1. On `main`, scan every Haskell corpus target with the harness's flags, and record:
   - snapshot components (identity, source files, reachability, orphan reason);
   - `pkg:hackage` sets;
   - lock-file parse warnings;
   - I7 counts.
2. Do the same on the branch, then diff them.

Expected (a prediction): hs-opentelemetry and lootbox gain lock-pinned versions and snapshot hashes. hs-opentelemetry's `unknown` becomes `stackage-lts-18.8@93a10755…`, with reason `build-toolchain`. haskell-language-server's snapshots gain their `stack.yaml` as source file; HLS's goldens are not hpack-filtered, so they move. The other targets are unchanged.

Results go in `measurements/`. Goldens are regenerated through `public-corpus.yml`. `hpack-*` goldens show only `pkg:hackage` changes.

## Project Structure

```text
waybill-cli/src/scan_fs/package_db/haskell.rs
waybill-cli/src/generate/orphan_reason.rs
waybill-cli/tests/stack_snapshot_identity.rs            # positive control
waybill-cli/tests/corpus_harness_195/invariant-declarations.toml
specs/1663-stack-snapshot-url/measurements/
```

## Complexity Tracking

None.
