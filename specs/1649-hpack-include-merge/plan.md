# Implementation Plan: hpack `!include` and YAML merge keys are read as hpack reads them

**Branch**: `1649-hpack-include-merge` | **Date**: 2026-11-23 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1649-hpack-include-merge/spec.md`. Refs #1673, #1674, #1677.

## Summary

The hpack reader parses each `package.yaml` alone. So an anchor defined in an included file is unknown, and the manifest is `unread`. Merge keys are never applied, so merged settings vanish while C200 says `read`.

The plan resolves both before parsing, the way hpack's include support works on the YAML event stream (research R1):
1. **Splice** every block-context `!include` with its file's text, re-indented, recursively and relative to the file that names it. This is bounded by the scanned root, cycles and a depth of 8 (R5).
2. **Rename** each `<<` key to a numbered unique key, so repeated ones parse (R4).
3. **Parse**, then fold the renamed keys in under hpack's measured precedence: explicit keys beat merges, and among merges the first wins (R3).
4. **Record** resolved and unresolved includes in C200 (R6).

Every behaviour it encodes was measured with hpack 0.38.3 (spec Background, R1, R3), and every corpus include is in a position the splice covers (R2). The acceptance check is agreement with hpack's own `build-depends` on every corpus manifest (R7).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `serde_yaml` 0.9.34 (already parses `package.yaml`), `serde_json` (C200), `tracing`. **Zero new Cargo dependencies.** Measurement only: hpack 0.38.3 through `nix run nixpkgs#hpack`, and Python 3 run with `-I -B`.
**Storage**: N/A. Per manifest, in process.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`.
- `s1649_*` unit tests in `hpack.rs` (the rename, the merge fold, R8's serde_yaml checks).
- A new integration file `waybill-cli/tests/hpack_include_merge.rs` (contracts/wire.md M1–M5, I1–I4, U1–U5, N1).
- Mutations (spec SC-005).
- The corpus: the seven `hpack-*` targets, regenerated through CI.

**Target Platform**: every host. Paths are joined and canonicalised with `std::path`.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-007, within 1.05× `main` on the hpack corpus targets. Splicing and renaming are linear in the manifest's text.
**Constraints**:
- byte identity without `!include` or `<<` (FR-007);
- no network (FR-006);
- no new field beyond C200 (FR-008).

**Scale/Scope**: `haskell/hpack.rs` (splice, rename, fold, C200), `haskell.rs` (threading `rootfs` to the reader), one integration test file, measurement scripts, docs, CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate. hpack is used for measurement only, never by waybill. |
| III. Fail closed | Pass. An unresolvable include leaves the manifest `partial` with its reason, and nothing is invented (FR-004). Reads are bounded by the scanned root, cycles and depth. |
| IV. Type-driven correctness | Pass. The unresolved reason is an enum matched exhaustively, and no `.unwrap()` is used in production. |
| V. Specification compliance | Pass. The behaviour follows hpack, the tool that defines `package.yaml`, as measured. |
| VIII. Completeness | The point: 30 corpus manifests currently `unread` or `partial` become readable (SC-003). |
| IX. Accuracy | Agreement with hpack's own `build-depends` is the acceptance check (R7, SC-002). |
| X. Transparency | C200 names every include, resolved or not, with the reason. |
| Strict boundaries | Pass. Only local files under the scanned root are read. |

**Gate: pass.** Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1649-hpack-include-merge/
├── plan.md
├── research.md            # R1–R9
├── data-model.md
├── quickstart.md
├── contracts/wire.md      # M1–M5, I1–I4, U1–U5, N1
├── measurements/          # hpack_agreement.py, counts, timing
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/haskell/hpack.rs   # splice_includes, rename_merge_keys, fold_merges, IncludeResolution, C200 keys
waybill-cli/src/scan_fs/package_db/haskell.rs         # pass rootfs to read_manifest; record include resolutions
waybill-cli/tests/hpack_include_merge.rs              # new: fixture cases
docs/reference/ (the hpack section of the Haskell reader docs), CHANGELOG.md
```

**Structure Decision**: the three new steps sit in front of the existing parse in `read_manifest` (data-model). The typed deserialize, `strip_tagged` and `lower` are unchanged, and a manifest that uses neither construct passes through untouched.

## Complexity Tracking

No violations.
