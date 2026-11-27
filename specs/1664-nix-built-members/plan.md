# Implementation Plan: Nix-built Haskell packages count as project members

**Branch**: `1664-nix-built-members` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

## Summary

#1032's membership rule is the union of `cabal.project` and a sibling `stack.yaml`. This plan adds a third source: a `callCabal2nix <name> <literal path>` in any `.nix` file in the scan root. A directory admitted this way:
- records the `.nix` file in `waybill:source-files`;
- carries the new per-component row C248, `waybill:membership-source = nix-callcabal2nix`.

The measurement chose this form and only this form (research R1–R2): across the 12 Haskell corpus targets, it is the one form that names a rejected directory (temporal `tools/tix-to-markdown`). The `--nix-closure` tier is not used.

## Technical Context

- **Language:** Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
- **Dependencies:** existing only (`globset`, `serde_json`, `tracing`). **Zero new Cargo dependencies.**
- **Code:**
  - `waybill-cli/src/scan_fs/package_db/haskell.rs`:
    - discovery (`HaskellDiscoveredPaths`, `on_haskell_file`, the registration patterns, `discover_*`);
    - the #1032 block;
    - `resolve_root_declared_members`;
    - the member emit, which sets source paths and the annotation.
  - New module `waybill-cli/src/scan_fs/package_db/haskell/nix_refs.rs`: comment stripping, `callCabal2nix` recognition and path resolution. It is pure functions over `&str`, unit-tested.
  - `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`: the C248 extractors.
  - `docs/reference/sbom-format-mapping.md`: the C248 row and the C18 note.
- **Measurement:** `measurements/rejected.sh` (committed), rerun on the branch binary.

## Constitution Check

| Rule | Status |
|---|---|
| I. Pure Rust, statically linked | Pass. No new crate; no subprocess. |
| III. Fail closed | Pass. An unreadable `.nix` file, an unrecognised form or a non-literal path admits nothing. |
| V. Native fields first | Pass. The audit is in R4: no native carrier for "in scope because a build file names it". C248 ships with its extractors. |
| VIII/IX. Completeness / accuracy | Pass. Restores a shipped package (VIII), and the exclusions in FR-006 prevent false members (IX). |
| X. Transparency | Pass. The admitting file and the route are both in the document. |
| Measure before designing (CLAUDE.md) | Pass. R1 measured before design; FR-009 measures again after. |
| Pre-PR gate 4.1.0 | Unchanged. |

No violations, so there is no Complexity Tracking table.

## Design

1. **Discovery.** `.nix` files are listed on demand by `discover_nix_files`, a `safe_walk` over `*.nix` that runs only when the rule rejects something. *Implementation note:* the first draft added `**/*.nix` to the shared walker's patterns instead. That made the m924 repo report say the Haskell reader claims `flake.nix`, `default.nix` and `shell.nix`, which the gate's `no_table_entry_names_a_marker_a_reader_already_claims_m924` caught. The walker-audit gate needs no change, because there is no new `fn walk`.
2. **`nix_refs::callcabal2nix_dirs(text: &str) -> Vec<(String /*literal*/, usize /*line*/)>`** (R3):
   - strip comments;
   - find each `callCabal2nix` token that is not followed by `WithOptions`;
   - skip whitespace, then a quoted name, then whitespace;
   - accept the next token only if it starts with `./` or `../`, contains no `${`, and is not quoted.
3. **`nix_refs::resolve(nix_file, literal, rootfs) -> Option<PathBuf>`:** lexical normalisation of `nix_file.parent().join(literal)`, with no filesystem `canonicalize`. A result outside `rootfs` gives `None`.
4. **#1032 block.** The `.nix` files are read lazily, at most once per scan, and only when the first governing `cabal.project` would drop something (R6). This builds `nix_admitted: BTreeMap<PathBuf /*pkg dir*/, BTreeSet<PathBuf /*nix files*/>>` for directories that hold a candidate manifest. For each project, a candidate whose directory is in `nix_admitted` and under `project_dir` is kept, and if `declared` did not already contain it, it is recorded as Nix-admitted.
5. **`resolve_root_declared_members`.** The same union, for `root-only` (R5).
6. **Emit.** For a Nix-admitted member, the sorted Nix files are appended to `extra_source_paths`, and `waybill:membership-source = "nix-callcabal2nix"` is set in `extra_annotations` on every component that manifest produces.
7. **C248.** The row is added to `sbom-format-mapping.md`, its extractors are added to the three formats, and it is registered in `mod.rs`.

## Testing

- **Unit (`nix_refs`):**
  - the forms R1 found: an attribute prefix, multiple lines, `./.`, and `../`;
  - the exclusions: a comment, a block comment, an interpolated string, a variable, `fetchTarball`, `callCabal2nixWithOptions`, and an absolute path;
  - resolution outside the root.
- **Integration (`tests/nix_built_membership.rs`):** FR-008's three-package `waybill-fixture-*` repository. Only (i) is a member, with C18 and C248. A fourth case is a Nix reference to a directory `cabal.project` already admits: it carries no C248.
- **Mutations:** each recognised form and each exclusion, recorded in `measurements/mutations.md`. They use a `cp` backup, a `cmp` revert and a rebuild.
- **Corpus:** `rejected.sh` and the C248 count rerun on the branch. temporal gains `tix-to-markdown` and `hpc`, and every other diff is explained. Goldens are regenerated only through `public-corpus.yml`.

## Project Structure

```text
specs/1664-nix-built-members/
├── spec.md  plan.md  research.md  data-model.md  quickstart.md
├── contracts/membership.md
└── measurements/rejected.sh  rejected.tsv  rejected/
```
