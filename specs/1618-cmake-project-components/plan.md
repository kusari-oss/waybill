# Implementation Plan: A component for each CMake project the scanned tree builds

**Branch**: `1618-cmake-project-components` | **Date**: 2026-11-20 | **Spec**: [spec.md](spec.md)

## Summary

#1616: a CMake project in the scanned tree gets no component, so a monorepo's projects have no edges between them, and vendored code is invisible.

**The approach** (research R1–R11):
- **Components.** Read each literal `project(<name> [VERSION …])` in the CMake reader's existing single pass, and emit one `pkg:generic/<name>[@<version>]` main-module component per distinct project.
- **Edges.** Pair the searches that today land in C237 `in-tree` with those components, so the owning project gets an edge.
- **Ownership.** Rank CMake projects below other ecosystems' main modules and above submodules in the ownership index.
- **Root selection.** Admit CMake projects as root candidates only when no other ecosystem has a main module.
- **Scope.** Derive scope from the project's directory.
- **Annotations.** Mark projects the root build does not reach, and record vendored signals, choosing the verdict rule by a measurement whose criterion is fixed in R10.

Three planning amendments refine the spec (Clarifications): R2 root candidates, R4 scope, R6 C237.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- CMake reader and tokeniser: `cmake/fetches.rs` (`read_declarations`, `Vars`, `TEST_DIRS`), `cmake/reach.rs`, `cmake.rs` (`parse_vendored`).
- Pairing and ownership: `scan_fs/cmake_search_pair.rs` (`pair`, `attach`, `record_scopes`), `scan_fs/declared_attach.rs` (`OwnerIndex`).
- Root and parity: `generate/root_selector.rs` (`select_root`), the parity extractors.
- Crates: `serde_json`, `tracing`.

**Zero new Cargo dependencies** (FR-014).

**Storage**: N/A. In process per scan.

**Testing**:
- **Integration:** `waybill-cli/tests/cmake_project_components.rs`, cases P-1..P-19, P-ctl and P-rep (`contracts/wire.md`), in three formats through the parity extractors. P-ctl's golden comes from `main`'s binary before any source change.
- **Unit tests:** declaration parsing (versions), merge rules, `OwnerIndex` ranks and `owner_above`, the `select_root` candidate rule, the reach set, the signals.
- **Mutations** (`cp` restore, rebuild after the last one, `^error\[E…\]` pattern):
  - root candidacy;
  - rank order;
  - enclosing edge;
  - search-pairing;
  - C197 versus C238;
  - test-directory scope;
  - reach;
  - the `--include-vendored` merge.
- **CI:**
  - census (T001) on TheRock and the three quality-corpus targets;
  - TheRock measure-source against `main`;
  - public corpus (`regen_goldens`), predicted byte-identical (R12);
  - quality corpus against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows; paths are compared in their scan-relative form.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-007, ≤ 1.05× `main` on TheRock's source tree (median of ≥ 3).

**Constraints**:
- FR-012: byte-identical with no `project()`;
- SC-008: the root is unchanged when another ecosystem has a main module;
- FR-011: parity.

**Scale/Scope**: TheRock has 63 projects that its searches name (#1616, at `b1cae390`); the total number of `project()` declarations is re-counted in T001.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies. |
| III. Fail closed | ✅ A name built from a variable yields nothing. An unresolved version is reported as such (C238), never guessed. No vendored verdict is emitted unless the measured rule reaches its fixed precision bar (R10). |
| IV. Type-driven correctness | ✅ `DeclaredVersion` enum; a typed pairing kind; ranks as constants. |
| V. Specification compliance | ✅ Audited per row in `contracts/wire.md`: no native carrier. The existing uncatalogued `waybill:vendored` key gets its row (C240), and C55, C62, C197 and C237 extend existing rows. Every new row lands with its extractor. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs. TheRock and the quality corpus only in CI. |
| VIII. Completeness | ✅ Adds components for code that was invisible, including test, example and unreached projects (Q2). |
| IX. Accuracy | ✅ A source tree's own projects and their dependencies are stated; scope is not lowered by a consumer's conditions (R4). |
| X. Transparency | ✅ Every new judgement is recorded with its evidence: C239 for reach, C241 for vendored signals, C238 for unresolved versions, C62 for scope derivation. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1618-cmake-project-components/
├── spec.md
├── plan.md               # this file
├── research.md           # R1–R13
├── data-model.md
├── quickstart.md
├── contracts/wire.md     # rows C238–C241 and the extended rows; cases P-1..P-19, P-ctl, P-rep
├── checklists/requirements.md
└── measurements/         # later: census.md, vendored-sample.tsv, mutations.md, therock.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/cmake/fetches.rs     # ProjectDeclaration from the existing pass (name, DeclaredVersion via Vars)
waybill-cli/src/scan_fs/package_db/cmake/projects.rs    # NEW: merge, scope by directory, signals, entries
waybill-cli/src/scan_fs/package_db/cmake/reach.rs       # reached-from-root set
waybill-cli/src/scan_fs/package_db/cmake.rs             # emit project entries; parse_vendored merge (R5)
waybill-cli/src/scan_fs/cmake_search_pair.rs            # pair searches with project components; C237 paired.project; R4 scope
waybill-cli/src/scan_fs/declared_attach.rs              # RANK_CMAKE_PROJECT; owner_above
waybill-cli/src/scan_fs/mod.rs                          # enclosing edges (FR-006) at the existing attach site
waybill-cli/src/generate/root_selector.rs               # CMake candidates only without other main modules (R2)
waybill-cli/src/parity/extractors/*                     # C238–C241 extractors
waybill-cli/tests/cmake_project_components.rs           # P cases
waybill-cli/tests/fixtures/cmake_project_components/    # P-ctl tree and golden
docs/reference/sbom-format-mapping.md                   # C55, C62, C197, C237 extended; C238–C241 new
docs/ecosystems.md                                      # CMake: project components, ownership, root
CHANGELOG.md
```

**Structure decision.**
- **CMake-side logic** goes in a new `cmake/projects.rs`, beside the reader modules spec 1614 split out: `searches.rs`, `reach.rs`, `fetches.rs`.
- **Cross-reader rules** (pairing, ownership, root) are changed where they already live, so each rule keeps one definition.
