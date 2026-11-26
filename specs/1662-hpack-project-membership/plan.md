# Implementation Plan: Project membership for `package.yaml`

**Branch**: `1662-hpack-project-membership` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

## Summary

#1032's filter decides which `.cabal` files a governing `cabal.project` declares. This change gives it one more kind of candidate.

Each `package.yaml` in directory `D` takes part as the file hpack would generate, `D/<name>.cabal` (research R1). It is kept or dropped by the same matcher as the real `.cabal` files, so every entry form gets one verdict (R2). The "nothing matched, so leave the tree alone" fallback then counts both kinds (R4).

The measurement comes first: ogmios, the hpack corpus targets and haskell-language-server, on `main` and then on the branch.

## Technical Context

- **Language:** Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
- **Dependencies:** existing only (`globset`, `serde_yaml`, `tracing`). **Zero new Cargo dependencies.**
- **Code:**
  - `waybill-cli/src/scan_fs/package_db/haskell.rs`: the #1032 block and the hpack emit loop.
  - `haskell/hpack.rs`, for the manifest name.
- **Measurement only:** hpack 0.38.3 via `nix run nixpkgs#hpack` (R1). Python run as `python3 -I -B`, if needed.

## Constitution Check

| Rule | Status |
|---|---|
| I. Pure Rust, statically linked | Pass. No new code paths outside Rust; no dependencies. |
| Truthful output (the reason for #1032) | Pass. This removes a component the project's own build definition excludes. |
| Measure before designing (CLAUDE.md) | Pass. hpack naming measured (R1); targets measured before and after (FR-006). |
| Pre-PR gate 4.1.0 | Unchanged. |
| Zero new Cargo dependencies | Pass. |

## Design

1. **Read each manifest once, before #1032's block.** Keep the `(includes, read)` results keyed by path, and have the emit loop use them instead of calling `read_manifest` again. No behaviour change on its own.
2. **Build the stand-in path for each `package.yaml`:** `D/<name>.cabal`, where `<name>` is the manifest's `name:`, or the directory name if the manifest has no `name:` or can't be read.
3. **In #1032's block:**
   - pass the stand-ins to `cabal_files_declared_by` alongside `cabal_paths`;
   - treat "declared is empty" as true only if neither kind matched;
   - drop `package.yaml` paths whose stand-in is in the project's tree but not declared, with the same `starts_with(project_dir)` scope as for `.cabal`.
   The info log reports dropped `.cabal` and `package.yaml` counts separately.
   A dropped manifest is treated like a dropped `.cabal`: it isn't counted in C200 (`waybill:hpack-read-outcome`), and its FR-001b warnings and `!include` outcomes aren't recorded. The read before the filter exists only to get the name; outcomes and warnings are recorded for kept manifests only.
4. **Leave spec 1652's `root_declared_member_dirs` alone.** It is computed before the filter, from every discovered directory, and is a different question (R2).

## Tests (FR-007)

- **Fixture-level:** a `waybill-fixture-*` project with:
  - a `cabal.project` listing `./pkgs/a`;
  - `pkgs/a/package.yaml` and `pkgs/b/package.yaml`, with no `.cabal` files.
  It asserts `a` is emitted and `b` isn't. It's mutation-tested by disabling the stand-in, after which `b` reappears.
- **One unit test per R2 row**, plus:
  - the unnamed manifest (directory-name fallback);
  - the unreadable manifest;
  - FR-005's fallback, both when only hpack packages match and when nothing matches;
  - a project with no `packages:` field;
  - a stack-only project.

  Each row is shown to fail under a mutation of its matching arm. The mutation is reverted with `cp`, checked with `cmp`, and the code rebuilt.

## Measurement (FR-006)

1. Build `main`'s binary, then the branch's.
2. Scan with `--offline` and default flags:
   - ogmios at `435189ae…`, as committed and with the generated `.cabal` files moved aside (the corpus `no-generated-cabal` variant);
   - the other #1677 targets (hs-opentelemetry, lootbox, hs-temporal-sdk), plus a no-generated-cabal variant where the corpus has one;
   - haskell-language-server at its corpus pin.
3. Diff the sorted `pkg:hackage` sets and the C200 values for each target, and give a reason for every difference.

Results go in `measurements/membership.md` and `.tsv`. Corpus goldens are regenerated through the `public-corpus.yml` dispatch, and their diff is read against the table.

## Project Structure

```text
waybill-cli/src/scan_fs/package_db/haskell.rs        # #1032 block + emit loop
waybill-cli/src/scan_fs/package_db/haskell/hpack.rs  # name accessor, if not already exposed
waybill-cli/tests/…                                  # fixture-level positive control
waybill-cli/tests/fixtures/public_corpus/hpack-ogmios*/  # regenerated goldens
specs/1662-hpack-project-membership/measurements/
```

## Complexity Tracking

None.
