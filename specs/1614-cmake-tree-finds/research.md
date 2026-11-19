# Research: Read find_package from every CMake file, with the conditions that reach it

Sources:
- **Census:** the fixture repository's `census/includes.py`, runs 44441297039 (`cadd702`), 44442550305 (`e8435cd`) and 44445588438 (`5580ac2`). TheRock `b1cae390` with submodules, CI only. The summary is copied in `measurements/census-summary.md`.
- **Document:** TheRock measure-source run 44426794245, which scanned `main` + spec 1613.
- **Local probe:** a synthetic tree run against `main`'s build (R7).

Every number below is from one of these, or labelled as a prediction.

## R1. One pre-pass computes the conditions that reach each CMake file

**Decision:** a new `cmake/reach.rs`, run once per scan before the fetch and package-search readers. For every CMake file it computes `Reach`:
- **`Unconditional`:** some path reaches the file with no condition, or nothing reaches it at all (a tree root, or a file reached only through unresolved paths);
- **`Conditional(conditions)`:** every path passes at least one condition. The value is the sorted union of the conditions on all paths.

The edges are:
- **`include(<path>)`**, resolved with `links.rs`'s existing rules (`resolve_include`). These cover literal paths and `CMAKE_CURRENT_SOURCE_DIR`, `CMAKE_CURRENT_LIST_DIR`, `PROJECT_SOURCE_DIR` and `CMAKE_SOURCE_DIR`. An included file inherits the includer's source directory.
- **`add_subdirectory(<dir>)`**, resolved against the current source directory, to `<dir>/CMakeLists.txt`.

Each edge carries the blocks open at its call site: `BlockStack::conditions()`, including `function: <name>` for a call in a body.

The result is memoised per file, with a cycle guard. A file in a cycle counts as reached unconditionally (spec edge case).

**Rationale:**
- **Same machinery as spec 1543.** `links.rs` already follows `include()` with "the blocks open at the `include()` site are the included file's first conditions". It does so only for link declarations, and per walk from each file, not per file. A per-file `Reach` gives fetches, package searches and links one answer.
- **Linear cost.** A memoised fixed point visits each file once. Enumerating paths, as `links.rs` does under `MAX_INCLUDE_DEPTH = 8`, would multiply along TheRock's add_subdirectory chains. The census computed the same quantity this way: `cond_reached`, section E.
- **The union of conditions** matches C232's existing rule for one component declared several times: the sorted union.

**Alternatives:**
- **Extend `links.rs`'s walk to all declaration kinds.** Rejected: it is per-walk and depth-bounded, and does not follow `add_subdirectory()`.
- **Evaluate conditions.** Out of scope (spec Assumptions).

**`links.rs` is unchanged.** Its include walk already gives links the include-site conditions. Moving it onto `Reach` would also give links the `add_subdirectory()` conditions; that is a separate change, and is noted.

## R2. Files reached only by include() are read (FR-004)

**Finding:** 1 file on TheRock (`libhsakmt/CMakeLists_wsl.txt`, one `find_package`).

**Decision:** `reach.rs` returns the extra files that `include()` reaches under names the walker pattern does not match, inside the scan root. The fetch and package-search readers read them, with their `Reach`.

**Rationale:**
- **The skip set is unaffected.** The include path is resolved inside the scan root, and the target file is read only because a walked file names it. A file under a default-skipped directory is still reached only if a walked file includes it.

## R3. Package searches are read in the fetch reader's pass

**Finding:** `read_fetches` (`cmake/fetches.rs:132`) already tokenizes every walked CMake file with `commands_checked`, and tracks `BlockStack` per file. The package-search reader is a regex over the narrow set (`parse_find_package_calls`, `cmake.rs:308`) that sees no blocks.

**Decision:**
- **One pass.** Read `find_package`, `pkg_check_modules` and `pkg_search_module` in the same pass over the same parsed commands. Each file's stack is seeded with its `Reach`.
- **Hits feed the existing emitter.** Each declaration becomes a `FindPackageHit` / `PkgCheckHit` with conditions, emitted through the existing `emit_find_package_entries` / `emit_pkg_check_module_entries`. Identity is unchanged: `pkg:generic/<lowercased name>`, the #1520 ranges and C103 casing.
- **The narrow set stays for the rest.** `file(DOWNLOAD)`, vendored directories, and the binary tier's call without a walker (`read()` with `walked = None`) are unchanged.

**Rationale:**
- **No new file reads** beyond R2.
- **Parse parity.** The tokenizer already handles comments, bracket arguments and multi-line calls, and the regex did not. Version and `EXACT` parsing reuse `find_package_requirement`.

**Files read (amended at T006).** Package searches are read from the walked files **and** from every narrow-set file the walker did not hand the reader. The narrow set walks `cmake/**` with its own walker, which does not skip `vendor/`, so `cmake/modules/vendor/Extra.cmake` (milestone 156's fixture) would otherwise vanish. Fetches stay on the walked files, as spec 1538 decided.

**Risk:** a call the regex caught and the tokenizer does not, or the reverse. The existing find_package unit tests (`cmake.rs` m155 tests) and the corpus goldens are the check (FR-009).

## R4. Scope follows the fetch reader's rule

**Decision:** a package search is marked exactly as `fetches.rs::mark` marks a fetch.
- **Test:** the declaring file is in a test directory, or any condition (its own or its `Reach`) is a `BUILD_TESTING` condition. Recorded as C62 `source-test-directory` / `cmake-build-testing`.
- **Optional:** otherwise, a conditional declaration is `Optional`, with C122 `cmake-conditional`.
- **One component, several declarations:** its scope is the strongest across them, as `one_per_purl` does for fetches. It is required if any declaration is unconditional, Optional only if every one is, otherwise Test. Its conditions are the sorted union.

**Rationale:**
- **One rule.** A consumer reads a CMake package search's scope the way it already reads a fetch's.
- **Spec 1533 unchanged.** Its test-scope pass still applies after the readers.

**The shape set is unchanged.** Spec 1538's fallback shapes describe fetches. For a search, conditional versus unconditional is the only shape.

## R5. Conditions on a package search: a sibling row, C236 (FR-012)

**Decision:** a new row, C236 `waybill:cmake-find-conditions`: a per-component JSON array of strings, sorted, in C232's format. It is emitted only on a non-runtime component, as C232 is.

C232 itself gains values, not shape: a fetch's conditions now include the conditions of its `include()` and `add_subdirectory()` chain.

**Rationale:**
- **C232 means fetch conditions.** Its name and description say so.
- **Precedent.** Spec 1543 added C235 `waybill:cmake-link-conditions` as a sibling rather than widening C232.
- **The extractor is generic.** It is a `cdx_anno!` / annotation-envelope row, so a sibling costs one catalog row and its extractor.

## R6. Tools and CMake helpers (FR-006)

**Decision:** one constant list, matched case-insensitively:
- `Threads`, `PkgConfig`, `Git`, `Doxygen`, `PythonInterp`;
- `Python` / `Python3` when its `COMPONENTS` list contains `Interpreter` and not `Development` (nor `Development.Module` / `Development.Embed`).

A name on the list yields no component. It is counted in C237's `tools`.

**Evidence** (census, TheRock):

| Name | Calls |
|---|---|
| `Threads` | 192 |
| `Git` | 74 |
| `PkgConfig` | 31 |
| `Doxygen` | 10 |

Python appears as `python3` / `python`; whether it is a library is decided per call.

**Not on the list:** CMake find modules for libraries (CUDAToolkit, Boost, CURL, OpenGL, …). The spec's clarification is A.

## R7. Pairing with what the tree already represents (FR-005)

**Finding** (local probe, `main`'s build): both pairings are already duplicated on `main`, inside the narrow file set.
- `find_package(waybill_fixture_fmt QUIET)` plus a `FetchContent_Declare(waybill_fixture_fmt …)` under `if(NOT waybill_fixture_fmt_FOUND)` gives both `pkg:generic/waybill_fixture_fmt` and `pkg:github/waybill-fixture/fmt@v1.0.0`.
- `find_package(waybill_fixture_inst)` plus a `waybill_fixture_instConfigVersion.cmake` (`PACKAGE_VERSION 4.5.6`) gives both `pkg:generic/waybill_fixture_inst` and `pkg:generic/waybill_fixture_inst@4.5.6`.

**Census** (TheRock): 19 names pair with a fetch and 9 with a git submodule. 1 installed-library record (`wkmi`) is in the document.

**Decision**, applied in this order:
1. **Fetch.** A search whose lowercased name equals a fetch's declared name, after `fetches.rs` variable expansion, yields no component. The search's file is added to the fetch component's evidence. This happens inside the CMake reader, where both lists exist. The fetch keeps its spec 1538 shape and scope.
2. **Installed-library record** (C14 `cmake-package-version` / `pkg-config`, spec 1531). The record is built later, in `installed_lib::resolve`. So a post-pass in `scan_fs`, after it, removes `pkg:generic/<name>` (mechanism `cmake-find-package`) when a record with the same lowercased name exists, and adds the search's files to the record's evidence. The record's scope extends spec 1543's rule (`cmake_link_attach::attach` lines 139–147, then `apply_scope`) from link declarations to package searches. The record is Optional only when all of these hold:
   - every link declaration of it (C235) is conditional;
   - every package search for it (C236) is conditional;
   - no other reader has scoped it;
   - no exported-targets file targets it.

   One unconditional link or search keeps it required. The search conditions go in C236, beside any C235.
3. **Git submodule.** Same post-pass. A search whose name case-insensitively equals a submodule directory's last segment (C57's rule, spec 1530) yields no component.

In each case an edge runs from the search's owner to the paired component (R8).

**Rationale:** never two identities for one package (SC-003). The evidence of the search is kept on the survivor.

## R8. Edges: package searches attach like fetches

**Finding:**
- **Today:** search components reach the root through the "attach graph tops to a root with no edges" fallback. In the probe, the root has an edge to each one.
- **Fetches:** `cmake_fetch_attach::attach` draws one edge from each declaring file's owner (`OwnerIndex`: the nearest main module or git submodule), or a declared root anchor when the file has no owner.

**Decision:** the same attachment for package-search components. The mechanism is `cmake-find-package` / `cmake-pkg-check-modules`, and the pass also covers searches paired by R7. On TheRock, the owners are the submodules.

**Rationale:** spec 1537 records that the graph-top fallback strands a component as soon as the root has another edge.

## R9. Built in the tree (FR-007), and where that is recorded

**Decision:**
- **What counts.** A name the tree builds is the first argument of a `project()` in any walked CMake file, or the stem of a `<name>Config.cmake.in` / `<name>-config.cmake.in`. It is also the stem of a `<name>Config.cmake` / `<name>-config.cmake` with no version file beside it: a version file makes it an installed-library record (R7 step 2), which takes precedence.
- **Walker patterns.** The CMake registration gains the patterns `**/*Config.cmake.in` and `**/*-config.cmake.in`. These files are collected by name only, never parsed.
- **The result.** Such a search yields no component and no edge. The name is listed in the document-scope coverage record C237 under `in-tree`.
- **Where it is decided.** In the post-pass, after R7.2 and R7.3, never in the reader. A git submodule is usually also a `project()` (e.g. `base/rocm-cmake`). Dropping the search in the reader would lose the submodule pairing and its edge: 9 names on TheRock.

**Census** (TheRock):

| | Names |
|---|---|
| a `project()` in the tree | 63 |
| a package config in the tree | 25 |
| ... of these 88, claimed first by R7's pairings | 7 |

**Spec wording.** FR-007's "recorded as satisfied in-tree on the declaring side" is made concrete as C237's `in-tree` list. On TheRock the declaring side is usually a submodule or the scan root, so a document-scope record is the one place a consumer finds it. The spec is updated to say so.

## R10. C57 widens with the reader (FR-008)

**Finding:** `git_submodule::read` calls `cmake::collect_find_package_targets(scan_root)` (`git_submodule/mod.rs:263`), which reads the narrow set through `discover_cmake_files`.

**Decision:** C57's names come from the same declarations R3 reads: the walked files plus R2's included files. The CMake reader's marker files are passed to the submodule reader in `read_all`. `collect_find_package_targets` keeps its narrow behaviour for callers without a walker.

**Effect:** traced in the corpus. Every submodule moving from `declared-only` to `declared-and-used` is listed.

## R11. Coverage record, C237

**Decision:** a new row, C237 `waybill:cmake-find-coverage`: a document-scope JSON object, a string with sorted keys, in C231's style:

```json
{"components":N,"conditional":N,"in-tree":["<name>",...],"in-tree-declarations":N,"paired":{"fetch":N,"installed":N,"submodule":N},"read":N,"test":N,"tools":N,"unresolved":N}
```

- `read`: package-search declarations read;
- `unresolved`: declarations whose name holds `${`;
- `in-tree`: R9's names, sorted.

**Rationale:** spec 1538's C231 is the template; every declaration is accounted for.

## R12. Performance (SC-009)

**Finding:** the package searches are extracted from commands `read_fetches` already parses. The reach pre-pass reads the same commands, plus `include()` / `add_subdirectory()` arguments. The only new file reads are R2's included files (1 on TheRock).

**Decision:** SC-009's budget is **≤ 1.05× `main`'s median**, interleaved on TheRock's source tree, the same budget as spec 1613. The parsed commands are shared between the reach pre-pass, fetches and searches, not tokenized twice.

**Prediction**, to be measured: within noise.

## R13. SC-001's expected count

**Prediction:** `measurements/predict.py` applies the spec's rules in order to the census names and the TheRock document:

| Outcome | Names |
|---|---|
| **new component** | **110** (47 config packages, 43 project find modules, 20 CMake find modules) |
| built in tree (FR-007) | 81 |
| paired with a fetch | 19 |
| paired with a submodule | 9 |
| tool | 4 |
| Python, decided per call | 2 |

85 of the 110 are declared only under conditions.

**SC-001's measured count** is checked against 110 ± the Python calls. Any other difference is traced name by name.
