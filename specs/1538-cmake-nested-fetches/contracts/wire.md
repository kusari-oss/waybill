# Contract: what the document says about CMake fetch declarations

Each case is a synthetic `waybill-fixture-*` tree, scanned with `--offline` and checked in CycloneDX, SPDX 2.3 and SPDX 3. "Optional(x)" means:
- `LifecycleScope::Optional`, so CycloneDX `scope: excluded` and SPDX 2.3 `OPTIONAL_DEPENDENCY_OF`;
- C122 = `x`;
- C232 holds the conditions.

| # | Tree | Expected |
|---|---|---|
| C-1 | `add_subdirectory(third-party/fx)`. `third-party/fx/CMakeLists.txt` declares `waybill-fixture-hyphen-dir` by URL + SHA-256, unguarded | one component, identical to the same declaration placed in the root file; required |
| C-2 | two sibling projects, `a/` and `b/`, no root `add_subdirectory()`, each declaring a different fetch | both components, required (R1: the walk reads them, reachable or not) |
| C-3 | inner `FetchContent_Declare(… FIND_PACKAGE_ARGS …)` | Optional(`cmake-find-package-args`) |
| C-3b | C-3 plus `set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE NEVER)` in the root | required. NEVER cancels it (R3, P4). |
| C-4 | inner fetch in `if(NOT TARGET waybill_fixture_gtest)` | Optional(`cmake-not-target`). C232 = `["NOT TARGET waybill_fixture_gtest"]`. |
| C-5 | inner fetch in `if(NOT WaybillFixtureX_FOUND)` | Optional(`cmake-find-package-fallback`) |
| C-6 | root sets `FETCHCONTENT_TRY_FIND_PACKAGE_MODE ALWAYS`; an inner plain FetchContent and an inner plain ExternalProject_Add | FetchContent: Optional(`cmake-try-find-package-always`). ExternalProject: required. |
| C-7 | a tree file defines a provider with `SUPPORTED_METHODS FETCHCONTENT_MAKEAVAILABLE_SERIAL`; then the same with `FIND_PACKAGE` only | first: inner FetchContent Optional(`cmake-dependency-provider`). second: required. |
| C-8 | root fetch inside `if(WIN32)` | Optional(`cmake-conditional`), C232 = `["WIN32"]` (clarification 2: root files too) |
| C-9 | fetch in `tests/CMakeLists.txt`; another under `if(BUILD_TESTING AND X)`; a third under `if(BUILD_TESTING OR X)` | first two: test scope (C62 `source-test-directory`, `cmake-build-testing`), C232 kept on the second; third: Optional(`cmake-conditional`) |
| C-10 | root declares P at 1.0; `sub/` redeclares P at 2.0, unguarded | one component, P at 1.0. C18 lists both files. C233 = `[{"file":"sub/CMakeLists.txt","version":"2.0"}]`. C231 `merged.first-declaration-wins` = 1. |
| C-11 | siblings `a/` and `b/` each declare P at different versions, no ancestor declaration | two components (clarification 1) |
| C-12 | a git submodule `third_party/p` of repository R; inside another project, a guarded fetch of R at a different tag | one component, the submodule's. The fetch is in C233, and C231 `merged.existing-component` = 1. |
| C-12b | C-12 with the fetch **unguarded** | a separate component at its own tag; nothing merged (R9 applies to conditional fetches only) |
| C-13 | inside `function(wf_fetch)`: a literal-URL declaration, and one named `${name}` | literal: Optional(`cmake-function-body`), C232 = `["function: wf_fetch"]`. Parameterised: not emitted, C231 `unresolved.identity` = 1. |
| C-14 | a fetch inside submodule `third_party/x`'s tree, and one in a plain directory | first: an edge from the submodule. Second: a root edge. I-1 holds. |
| C-15 | C-1's tree plus `vendor-x/CMakeLists.txt` with a fetch, scanned with `--exclude-path vendor-x`; and a fetch under a directory the walker always skips (`node_modules/`) | neither is read or counted. FR-001's skip and exclusion rules apply to the widened walk. |

## Invariants (every case, and every corpus document)

- **I-1:** the root's direct targets agree in all three formats. This is spec 1537's invariant, unchanged.
- **I-2:** C231's V1 holds: `read = emitted + merged.* + unresolved.identity`.
- **I-3:** every conditional fetch carries C122. One whose shape comes from an enclosing block (`cmake-not-target`, `cmake-find-package-fallback`, `cmake-conditional`, `cmake-function-body`) also carries C232. `FIND_PACKAGE_ARGS`, `ALWAYS` and a provider have no condition text. No unconditional fetch carries C232. *Corrected at implementation:* this first required C232 on every conditional fetch, which those three shapes cannot meet.
- **I-4:** a scan with no CMake fetch declarations emits no C231, C232 or C233 (FR-010).
