# Implementation Plan: CMake fetches in nested projects, with standalone-build fallbacks marked

**Branch**: `1538-cmake-nested-fetches` | **Date**: 2026-11-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1538-cmake-nested-fetches/spec.md`

## Summary

Issues #1516 and #1519, which land together.

**The defect.** The CMake reader reads fetch declarations only from the root `CMakeLists.txt`, `cmake/`, `Modules/` and `third_party/`. On TheRock (census, CI) that is **1 of 191** `FetchContent_Declare` / `ExternalProject_Add` calls, and it emits **0** fetch components. Following `add_subdirectory()` from the root would reach **0** of the 191, because TheRock reaches its subprojects through variables and its own functions.

**The fix, in four parts:**
1. **Discovery** (R1). Fetch declarations are read from every CMake file the walk reaches. `find_package` stays on today's file set (clarification 1). A single per-file pass tokenises CMake commands and keeps a stack of enclosing `if()` / `function()` blocks.
2. **Conditional marking** (R2–R4, R7, R13). A fetch under any `if()`, in a function or macro body, or matching a fallback shape is `LifecycleScope::Optional`, with C122 naming the shape and the new C232 holding the condition text. The shapes are the ones CMake 4.4.5 was measured to honour (R3): `FIND_PACKAGE_ARGS` (unless `NEVER`), `NOT TARGET`, `NOT <X>_FOUND`, `TRY_FIND_PACKAGE_MODE ALWAYS`, and a FetchContent-capable provider. `BUILD_TESTING` and test directories give `LifecycleScope::Test`.
3. **One component per package** (R5, R9). A FetchContent name an ancestor directory also declares is merged into the ancestor's component (first declaration wins, measured P1). A conditional fetch of a repository another reader already emitted is merged into that component. The new C233 records what was superseded.
4. **Ownership and accounting** (R6, R8). Each fetch is a dependency of its nearest enclosing project, through spec 1537's `OwnerIndex`, which on TheRock means the submodules. The new document-scope C231 counts every declaration read, so SC-002's "accounted for" is checkable from the document.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only. `regex`, `serde_json`, `tracing`, `waybill_common::resolution::{LifecycleScope, Relationship, ResolvedComponent}`, `waybill_common::types::purl`. **Zero new Cargo dependencies** (FR-011).
**Storage**: N/A. Everything is in process for one scan.
**Testing**:
- **Unit:** the command tokeniser (comments, bracket arguments, quoted strings, nested parentheses); the block stack (`if`/`elseif`/`else`/`endif`, `function`/`macro`); each shape's classification, including the `NEVER` cancellation and the provider's method list; first-declaration-wins against ancestors versus siblings; C231's accounting invariant.
- **Integration:** a new `waybill-cli/tests/cmake_nested_fetches.rs` covering the contract cases C-1 to C-14 (`contracts/wire.md`). Synthetic `waybill-fixture-*` trees, checked in all three formats.
- **Parity:** extractors for C231 to C233. The catalog gate lives in the lib target, so each row lands with its extractor.
- **Conformance:** the CycloneDX schema and `spdx3-validate`.
- **Mutations:** each shape, the merge rules and the accounting are mutated and must fail a test (`measurements/mutate.py`, the spec 1537 pattern).
- **CI:**
  - `census-fetches` at the branch (SC-002 to SC-004, SC-006);
  - the public corpus in compare mode (expected byte-identical, R12);
  - the quality corpus (cpp-mongo).

**Target Platform**: as today. The reader is pure Rust, with no subprocess.
**Project Type**: CLI.
**Performance Goals**: SC-006, TheRock's sparse tree at no more than 1.5× `main`'s median of 3 (6.32 s) in the same job.
**Constraints**:
- Deterministic.
- No CMake evaluation: nothing runs, and no variable is resolved beyond the existing same-file literal `set()`.
- No network.
- Fixtures use synthetic names.
**Scale/Scope**: TheRock has 6,172 CMake files and 191 fetch calls, of which 148 are conditional, 28 in test directories and 17 in function bodies.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates, and no subprocess. |
| III. Fail Closed | Pass. A condition the reader cannot evaluate marks the fetch conditional, never unconditional (FR-004). An unresolvable name is counted, never guessed (R8, R13). |
| IV. Type-Driven Correctness | Pass. The fallback shape is a closed enum whose string values are C122's. The block stack is a typed enum, not string matching on the fly. |
| V. Specification Compliance | Pass. The conditional marker reuses `LifecycleScope::Optional` and C122, so it renders natively in CycloneDX and SPDX 2.3 (R2). There are three new rows: C231 (accounting), C232 (condition text) and C233 (superseded declarations). Each has a Principle V audit showing no native field (R4, R5, R8). |
| VII. Test Isolation | Pass. Synthetic fixtures. TheRock is measured in CI only, and was never cloned locally. |
| VIII. Completeness | Pass. 190 of 191 fetch calls were invisible, and every one is now a component, a merge, or a counted unresolved name. |
| IX. Accuracy | Pass. Fallbacks are not presented as built (FR-003), and the shapes are taken from measured CMake behaviour, not documentation (R3). Ownership places each fetch on the submodule that declares it, not on the root. |
| X. Transparency | Pass. C231 makes every declaration accountable, and C232 states the condition. The `find_package` gap is published as SC-008's follow-up. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1538-cmake-nested-fetches/
├── spec.md
├── plan.md                 # this file
├── research.md             # R1–R13
├── data-model.md
├── quickstart.md
├── contracts/wire.md       # C-1…C-14, invariants
├── measurements/
│   ├── census.md           # TheRock census + baseline (SC-007, SC-008)
│   ├── census-summary.json, census-fetches.json, census-refs.txt
│   └── cmake-semantics/    # probe.sh, results.txt, cmake-version.txt (R3)
├── checklists/requirements.md
└── tasks.md                # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── cmake.rs                  # discovery split (fetch: all files; find_package: today's set)
├── cmake/                    # new submodule directory
│   ├── commands.rs           # tokeniser: commands, comments, bracket args, quoted strings
│   ├── blocks.rs             # if/elseif/else/endif + function/macro stack
│   └── fallback.rs           # FallbackShape enum, classification, TRY_FIND_PACKAGE_MODE / provider scope
waybill-cli/src/scan_fs/
├── declared_attach.rs        # OwnerIndex made reusable (pub(crate) owner_of on any path)
├── cmake_fetch_attach.rs     # new: owner edges, first-declaration-wins + existing-component merge, C231 counts
└── mod.rs                    # wire the pass; carry C231 to emit
waybill-cli/src/generate/                     # C231 document-scope; C232/C233 per-component, all three formats
waybill-cli/src/parity/extractors/            # C231–C233 extractors
waybill-cli/tests/cmake_nested_fetches.rs     # contract C-1…C-14
docs/reference/sbom-format-mapping.md         # C231–C233 rows; C122 and C62 new values
docs/ecosystems.md                            # CMake section
```

**Structure Decision**: the CMake reader grows a `cmake/` submodule for the tokeniser and the block stack. The file is already 2,199 lines, and both pieces are independently testable. Owner edges and merging run as a scan-stage pass next to spec 1537's, reusing its `OwnerIndex`.

## Complexity Tracking

None.
