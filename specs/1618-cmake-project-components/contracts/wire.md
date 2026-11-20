# Wire contract

Every case runs in all three formats through the parity extractors for C42, C55, C62, C197, C236, C237 and C238–C241. Fixture names are `waybill-fixture-*`.

## New and changed catalog rows

| Row | Key | Shape | Emitted iff |
|---|---|---|---|
| C55 | `waybill:source-mechanism` | new value `cmake-project` | the component is an in-tree CMake project |
| C62 | `waybill:lifecycle-scope-derivation` | new value `source-example-directory` | a project under an example or benchmark directory |
| C197 | `waybill:version-absence` | new value `not-declared` | the project declares no `VERSION` |
| C237 | `waybill:cmake-find-coverage` | `paired.project: N` added, **only when N > 0** | a search was read (as today); the key is absent at 0, so FR-012 holds |
| C238 | `waybill:declared-version-expression` | string, verbatim | the version is an unresolved `${…}` expression |
| C239 | `waybill:cmake-build-reach` | closed enum `not-reached` | a scan-root `CMakeLists.txt` exists and does not reach the project |
| C240 | `waybill:vendored` | boolean `true` (existing key, now catalogued) | the chosen vendored rule holds; also `--include-vendored`, unchanged |
| C241 | `waybill:vendored-signals` | sorted JSON array of `path-segment`, `submodule-boundary`, `licence-file`, `not-reached` | at least one signal holds |

**Principle V audit, for each new row.** No CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 field states:
- that a version is an unresolved build-system expression;
- that the root build does not reach a project;
- which evidence marks code as vendored.

CycloneDX `pedigree` describes ancestry and patches, not where code sits in a tree; it is not used.

## Cases

- **P-1**: two projects; A's `find_package(B)`. → components A@1.0.0 and B@2.0.0 (`cmake-project`, `main-module`, `source`); edge A → B; C237 `paired.project = 1`, B not in `in-tree`.
- **P-2**: P-1 with the search inside `if(WAYBILL_FIXTURE_OPT)`. → edge A → B; B carries C236 `["WAYBILL_FIXTURE_OPT"]`; B has no scope and no C122.
- **P-3**: B found only from `tests/CMakeLists.txt`. → B is Test, with the C62 derivation from the search.
- **P-4**: `project(waybill-fixture-nover)`. → versionless PURL, C197 `not-declared`.
- **P-5**: `set(V 3.1.4)` + `project(waybill-fixture-var VERSION ${V})`. → `@3.1.4`, no C197, no C238.
- **P-6**: `project(waybill-fixture-expr VERSION ${FROM_ELSEWHERE})`. → versionless, C238 `${FROM_ELSEWHERE}`, no C197.
- **P-7**: the same name and version in two directories. → one component, both files in evidence.
- **P-8**: the same name, different versions. → two components.
- **P-8b**: the same name, once with `VERSION 1.0.0` and once with no `VERSION`. → one component `@1.0.0`, both files, no C197.
- **P-9**: `third_party/waybill-fixture-vendored/CMakeLists.txt`. → C241 contains `path-segment`; C240 per the chosen rule (R10).
- **P-10**: a project only under `tests/`. → Test, C62 `source-test-directory`.
- **P-11**: a project only under `examples/`. → Development, C62 `source-example-directory`.
- **P-12**: a scan-root `CMakeLists.txt` that does not `add_subdirectory` `other/`, where `other/` declares a project. → `other`'s project carries C239 `not-reached`; it is still reachable in the graph (enclosing edge or root).
- **P-13**: a CMake-only tree with a top-level `project(waybill-fixture-top VERSION 1.2.3)`. → root `pkg:generic/waybill-fixture-top@1.2.3`, in all three formats.
- **P-14**: P-13 plus a `pyproject.toml` at the scan root. → root identical to `main`'s (SC-008). The Python module owns the root directory and has an edge to the top project.
- **P-15**: one Python main module in `py/` and CMake projects in `a/` and `b/`. → root and root-selection annotation byte-identical to `main` (R2).
- **P-16**: a project whose name equals a FetchContent declaration's name. → no project component; its `CMakeLists.txt` is in the fetch's evidence.
- **P-16b**: a project whose name equals an installed-library record's (a `<name>ConfigVersion.cmake` installed under the tree). → no project component; its file is in the record's evidence.
- **P-16c**: a git submodule at `waybill-fixture-sub/` whose `CMakeLists.txt` declares `project(waybill-fixture-sub)`. → no project component. A submodule at `vendor-dir/` declaring `project(waybill-fixture-other)` → a project component, with an edge from the submodule to it.
- **P-17**: a project under an `--exclude-path` directory. → nothing.
- **P-18**: P-13 with `--root-name waybill-fixture-override --root-version 9.9.9`. → the project is demoted and anchored (m860). With `--root-purl pkg:generic/waybill-fixture-top@1.2.3`, it is folded into the root (#1626).
- **P-19**: `--split` on P-1. → one document per project boundary.
- **P-ctl**: a tree with CMake fetches and searches and **no** `project()`. → byte-identical to `main` (FR-012), against a golden generated from `main`'s binary before any source change. This includes C237, which must carry no `paired.project` key.
- **P-9b**: no vendored rule qualified (T002). → C241 present, no C240 on any project component.
- **P-rep**: P-1 to P-17 in one tree, scanned 20 times. → identical each time.
