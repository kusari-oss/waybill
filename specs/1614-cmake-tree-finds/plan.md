# Implementation Plan: Read find_package from every CMake file, with the conditions that reach it

**Branch**: `1614-cmake-tree-finds` | **Date**: 2026-11-19 | **Spec**: [spec.md](spec.md)

## Summary

The CMake reader reads package searches (`find_package`, `pkg_check_modules`, `pkg_search_module`) only from a narrow file set, with no conditions. On TheRock that hides 168 of the 225 package names the tree searches for. It also loses the conditions of `include()` and `add_subdirectory()` for every declaration kind.

**Fixes:**
- **R1–R2:** a reach pre-pass computes, once per file, the conditions under which the file is reached. It also returns files reached only by `include()`.
- **R3–R5:** package searches are read from every walked file in the fetch reader's existing parse, scoped as fetches are. Their conditions go in a new sibling row, C236.
- **R6, R7, R9:** tools yield nothing. A search paired with a fetch, an installed-library record or a git submodule correlates to that component. A name the tree builds itself yields no component and is listed in the coverage row C237.
- **R8:** search components attach to their owner, as fetches do.
- **R10:** C57 reads the same declarations.

**Predicted on TheRock** (R13, census-derived): 110 new components, 85 of them non-runtime.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- the CMake tokenizer `installed_lib::cmake_version::{commands_checked, strip_comments}`;
- `cmake/blocks.rs` (`BlockStack`), `cmake/fetches.rs` (`read_fetches`, `mark`, `one_per_purl`);
- `cmake/links.rs` (`resolve_include`, `substitute`);
- `scan_fs::declared_attach::OwnerIndex`, `cmake_fetch_attach`, `cmake_link_attach`;
- `installed_lib::resolve`;
- `serde_json`, `globset`, `tracing`.

**Zero new Cargo dependencies** (FR-013).

**Storage**: N/A. In process per scan.

**Testing**:
- **Unit tests:**
  - `reach.rs`: an unconditional edge wins; a conditional union; a cycle; an unresolved path; `add_subdirectory()` chains; include source-directory inheritance.
  - The search extractor: blocks, bodies, `COMPONENTS`, `EXACT`, variable names.
  - FR-006's list, including the Python rule.
  - R7's pairing order.
- **Integration:** `waybill-cli/tests/scan_cmake_tree_finds.rs`, cases W-1..W-16 (`contracts/wire.md`), each asserted in all three formats through the parity extractors for C42, C62, C122, C232, C236 and C237.
  - **Byte-identity control** (FR-009): a narrow-set, unconditional tree, scanned against a golden of today's output.
  - **Determinism:** 20 repeated scans.
- **Mutations** (`measurements/mutations.md`): each rule removed or inverted. That covers:
  - reach (unconditional wins, union, cycle);
  - the widened file set;
  - include-reached files;
  - the seed into fetches;
  - search scope;
  - the tool list and the Python rule;
  - each pairing and the pairing order;
  - the in-tree rule and the walker patterns;
  - search attachment;
  - C57 widening;
  - the installed record's combined scope rule.
- **CI:**
  - TheRock: measure-source, two interleaved runs against `main`, a document diff, `waybill sbom parity-check`, and a repeat comparison across runners;
  - the public corpus at the branch, with `repeat=2`, every golden change traced;
  - the quality corpus against a `main` control.

**Target Platform**: the waybill CLI on Linux, macOS and Windows. No platform-gated code.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: SC-009, TheRock's source scan at **≤ 1.05× `main`'s median**, interleaved (R12). No new file reads beyond R2's included files: one on TheRock.

**Constraints**:
- FR-009: byte-identical output where every package search sits unconditionally in the narrow set, and no fetch has chain conditions;
- one component per package (SC-003);
- every new and extended annotation agrees across the three formats.

**Scale/Scope**: TheRock has 6,172 CMake files, 2,061 package-search calls, 225 names, 55 fetch names, 563 conditional `include()` calls and 175 conditional `add_subdirectory()` calls with declarations beneath.

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, statically linked | ✅ No dependencies, no subprocess. |
| III. Fail closed | ✅ Paths not resolvable statically contribute no conditions and nothing is guessed. Only FR-006's explicit list is dropped; an unknown name becomes a component. |
| IV. Type-driven correctness | ✅ `Reach` is an enum (`Unconditional` / `Conditional(set)`). The pairing outcomes and tool list are typed constants. Scope uses the existing `LifecycleScope` rules (`one_per_purl`). |
| V. Specification compliance | ✅ Scope is native: CycloneDX `scope`, SPDX 2.3 relationship types, SPDX 3 `LifecycleScopeType`. C236 and C237 are registered with their extractors in the same change. C232 gains values, not shape. |
| VII. Test isolation | ✅ Synthetic `waybill-fixture-*` trees in temp dirs; TheRock is never scanned locally. |
| VIII. Completeness | ✅ About 110 package names become visible on TheRock (R13). |
| IX. Accuracy | ✅ Conditions arrive with the widening, so 85 of the new components are non-runtime. Pairing removes the duplicate identities that exist on `main` today (R7 probe). |
| X. Transparency | ✅ C236 per component, and C237 accounting for every declaration, the in-tree names included. |

Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1614-cmake-tree-finds/
├── spec.md
├── plan.md               # this file
├── research.md           # R1–R13
├── data-model.md
├── quickstart.md
├── contracts/wire.md     # W-1..W-16
├── checklists/requirements.md
└── measurements/         # census-summary.md, predict.py; later mutations.md, therock.md, corpus.md, gate-log.md
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── cmake/reach.rs                 # new: R1, R2
├── cmake/fetches.rs               # seed each file's BlockStack with its Reach; read package searches in the same pass (R3, R4); fetch pairing (R7.1)
├── cmake/searches.rs              # new: search extraction, tool list (R6), in-tree names (R9), C236/C237 (R5, R11)
├── cmake.rs                       # read_with_downloads wires reach + searches; registration gains *Config.cmake.in patterns (R9)
├── git_submodule/mod.rs           # C57 names from the walked declarations (R10)
waybill-cli/src/scan_fs/
├── cmake_fetch_attach.rs          # attach package-search components too (R8)
├── cmake_search_pair.rs           # new: R7.2 / R7.3 post-pass (installed records, submodules), combined scope rule
└── mod.rs                         # calls the post-pass after installed_lib::resolve
docs/reference/sbom-format-mapping.md   # C232 values, C236, C237
waybill-cli/src/parity/extractors/      # C236, C237
docs/ecosystems.md                      # CMake: package searches, conditions, pairing, tool list
waybill-cli/tests/scan_cmake_tree_finds.rs
```

**Structure decision.** The search reader is new code in `cmake/searches.rs`, called from `read_fetches`'s loop over the same parsed commands. `fetches.rs` is 716 lines and would mix two declaration kinds otherwise. The R7 post-pass is a sibling of the existing `cmake_*_attach` passes.
