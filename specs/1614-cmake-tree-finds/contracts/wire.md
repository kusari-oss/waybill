# Wire contract: Read find_package from every CMake file, with the conditions that reach it

**Fixtures.** Every case is a synthetic `waybill-fixture-*` tree, scanned with `--offline --root-name waybill-fixture-t --root-version 1`.

**"Optional"** means:
- CycloneDX `scope: excluded`, with C42 `optional` and C122 `cmake-conditional`;
- the matching SPDX 2.3 / SPDX 3 scope.

**Every assertion holds in all three formats** through the parity extractors: C42, C62, C122, C232, C236, C237.

## Reading

| Case | Tree | Expected |
|---|---|---|
| W-1 | `projects/a/CMakeLists.txt`: `find_package(waybill_fixture_ext 2.1)` | `pkg:generic/waybill_fixture_ext`, required, range `>=2.1`, evidence `projects/a/CMakeLists.txt`. Absent on `main`. |
| W-2 | W-1, plus `projects/b/cmake/deps.cmake`: `find_package(waybill_fixture_ext 3.0 EXACT)` | one component; evidence both files; ranges `>=2.1` and `==3.0` |
| W-3 | nested `pkg_check_modules(X REQUIRED waybill-fixture-pc>=1.0)` and `pkg_search_module(Y waybill-fixture-ps)` | components as milestone 155 emits them from the root today |
| W-4 | `find_package(${WAYBILL_PKG})` | no component; C237 `unresolved` 1 |

## Conditions

| Case | Tree | Expected |
|---|---|---|
| W-5 | `if(WAYBILL_OPT) find_package(waybill_fixture_opt) endif()` in a nested file | Optional; C236 `["WAYBILL_OPT"]` |
| W-6 | `if(WIN_SDK) include(cmake/win.cmake) endif()`; `win.cmake` declares a `FetchContent_Declare(waybill_fixture_win …)` | the fetch is Optional; C232 contains `WIN_SDK`. **Required on `main`.** |
| W-7 | `if(BUILD_TESTING) add_subdirectory(tests) endif()`; `tests/CMakeLists.txt` unconditionally does `find_package(waybill_fixture_t)` | Test scope, C62 `cmake-build-testing`, and C236 containing `BUILD_TESTING` |
| W-8 | `if(A) add_subdirectory(x) endif()`; `x/CMakeLists.txt`: `include(y.cmake)`; `y.cmake`: `if(B) find_package(waybill_fixture_ab) endif()` | Optional; C236 `["A","B"]` |
| W-9 | the same name under `if(A)` in one file and unconditional in another | required, no C236 |
| W-10 | a file reached by `include()` both inside `if(A)` and unconditionally | its declarations are unconditional |
| W-11 | `if(WIN_SDK) include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt) endif()`; that file does `find_package(waybill_fixture_wsl)` | Optional, C236 `["WIN_SDK"]` (FR-004) |
| W-12 | `include(a.cmake)` in `b.cmake` and `include(b.cmake)` in `a.cmake`, both included from the root | the scan terminates; both are unconditional |
| W-17 | `function(waybill_deps) find_package(waybill_fixture_body) endfunction()` in a nested file | Optional, as spec 1538 treats a fetch in a body; C236 contains `function: waybill_deps` |

## Tools, pairing, in-tree

| Case | Tree | Expected |
|---|---|---|
| W-13 | `find_package(Threads)`, `find_package(Git)`, `find_package(PkgConfig)`, `find_package(Doxygen)`, `find_package(Python3 COMPONENTS Interpreter)`, `find_package(Python3 COMPONENTS Interpreter Development)`, `find_package(CUDAToolkit)` | components only for `python3` (the second call) and `cudatoolkit`; C237 `tools` 5 |
| W-14a | `find_package(waybill_fixture_fmt QUIET)` + `if(NOT waybill_fixture_fmt_FOUND) FetchContent_Declare(waybill_fixture_fmt GIT_REPOSITORY …/fmt.git GIT_TAG v1.0.0)` | one component, `pkg:github/waybill-fixture/fmt@v1.0.0`, with the search's file in its evidence; no `pkg:generic/waybill_fixture_fmt`. **Both exist on `main`.** |
| W-14b | `find_package(waybill_fixture_inst)` + `sdk/lib/cmake/waybill_fixture_inst/waybill_fixture_instConfigVersion.cmake` (`PACKAGE_VERSION 4.5.6`) | one component, `pkg:generic/waybill_fixture_inst@4.5.6`, with both files as evidence. **Both exist on `main`.** If the search is only under `if(A)`, and nothing else links or requires the record, the record is Optional with C236 `["A"]`. |
| W-14c | a git submodule `extern/waybill_fixture_sub` (a real gitlink in a temporary repository) + `find_package(waybill_fixture_sub)` | no `pkg:generic/waybill_fixture_sub`; the submodule's C57 is `declared-and-used` |
| W-15 | `projects/lib/CMakeLists.txt`: `project(waybill_fixture_lib)`; `projects/app/CMakeLists.txt`: `find_package(waybill_fixture_lib)`; and `cmake/waybill_fixture_cfgConfig.cmake.in` + `find_package(waybill_fixture_cfg)` | no component and no edge for either; C237 `in-tree` `["waybill_fixture_cfg","waybill_fixture_lib"]` |

## Edges, identity, determinism

| Case | Tree | Expected |
|---|---|---|
| W-16 | W-1 inside a git submodule's directory | an edge from the submodule to the search component; no root anchor (R8) |
| W-ctl | every search in the narrow set, unconditional; fetches without chain conditions | byte-identical to the committed golden of `main`'s output once C237 is removed (FR-009). C237 is present, and its counts match the tree. |
| W-rep | the W-5..W-15 tree, scanned 20 times | identical documents |

## C237 shape

```json
{"components":N,"conditional":N,"in-tree":["…"],"in-tree-declarations":N,"paired":{"fetch":N,"installed":N,"submodule":N},"read":N,"test":N,"tools":N,"unresolved":N}
```

A string with sorted keys, as C231.

- **Counted by declaration:** `read`, `unresolved`, `tools`, `paired.*`, `in-tree-declarations`.
- **Counted by component:** `components`, `conditional`, `test`, as in C231.
- **Listed by name:** `in-tree`.
- **Invariant:** `read` = `unresolved` + `tools` + `paired.fetch` + `paired.installed` + `paired.submodule` + `in-tree-declarations` + the declarations of emitted components.

## Catalog rows

| Row | Change |
|---|---|
| **C236** `waybill:cmake-find-conditions` | new, per component: a JSON array of strings, sorted, in C232's format. Present only on a non-runtime component. |
| **C237** `waybill:cmake-find-coverage` | new, document scope, as above |
| **C232** | description extended: the conditions include the `include()` / `add_subdirectory()` chain |
| **C57** | description extended: names from every CMake file read for package searches |
