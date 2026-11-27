# Implementation Plan: Haskell lock-file packages record their lock file

**Branch**: `1665-lock-package-source` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

## Summary

Stack-lock package components and freeze-pin components get their lock or freeze file as `source_path`, as snapshots have since spec 1663 (R2). Phase F merges entries with the same PURL so that every file is listed (R3). Orphan reasons are predicted not to move, because no orphan list names either file (R4), and the existing deduplicator already unions source files (R5).

## Technical Context

- **Language:** Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
- **Dependencies:** existing only. **Zero new Cargo dependencies.**
- **Code:** `waybill-cli/src/scan_fs/package_db/haskell.rs`:
  - `StackLockEntry` and `CabalFreezeEntry`;
  - `parse_stack_lock` and `parse_cabal_freeze`;
  - `build_stack_lock_component` and `build_freeze_component`;
  - Phase F.
- **Docs:** a one-sentence note on the C18 row in `docs/reference/sbom-format-mapping.md`. No new row.

## Constitution Check

| Rule | Status |
|---|---|
| I. Pure Rust | Pass. |
| V. Native fields first | Pass. Existing C18; no new row. |
| X. Transparency | Pass. Restores provenance. |
| Measure before designing | Pass. R1 was measured before the design, and the branch is measured again after. |
| Pre-PR gate 4.1.0 | Unchanged. |

## Design

1. `StackLockEntry { name, version, source: PathBuf }`, and `CabalFreezeEntry::ExactPin { name, version, source }`. `RangeConstraint` is unchanged, because it emits nothing.
2. The parsers set `source` to their `path` argument.
3. The builders set `source_path: source.to_string_lossy()`.
4. **Phase F.** One `HashMap<purl, index into out>` serves the freeze and stack-lock loops together. A repeat PURL appends its `source_path` to the first component's `extra_source_paths`, sorted and unique. It still records the PURL in `seen_purls`, so later Phase G suppression is unchanged.

## Testing

- **Integration (`tests/lock_package_source.rs`):** FR-009's fixture, with `waybill-fixture-*` names.
  - `a/stack.yaml.lock` pins `waybill-fixture-reached-1.0.0` (named by `a/package.yaml`) and `waybill-fixture-unreached-1.0.0`.
  - `b/stack.yaml.lock` pins `waybill-fixture-reached-1.0.0` again, and `waybill-fixture-split-2.0.0`.
  - `c/stack.yaml.lock` pins `waybill-fixture-split-3.0.0`.
  - `d/cabal.project.freeze` holds `constraints: any.waybill-fixture-frozen ==4.0.0, any.waybill-fixture-reached ==1.0.0`, so `reached@1.0.0` lists `a`, `b` and `d` across both loops.
  - The test asserts each component's exact source-files set, and that `waybill-fixture-unreached` is still `edgeless-lock-pin`.
- **Unit:** parser tests assert that `source` is set.
- **Mutations** (`measurements/mutations.md`): drop the stack-lock source; drop the freeze source; revert the merge to first-wins; merge by name instead of PURL; leave the extra paths unsorted.
- **Corpus:** rerun `measure.sh` on the branch (`branch.tsv`), diff against `main`, and explain every line. Goldens are regenerated through `public-corpus.yml` only.
