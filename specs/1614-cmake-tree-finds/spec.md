# Feature Specification: Read find_package from every CMake file, with the conditions that reach it

**Feature Branch**: `1614-cmake-tree-finds`
**Created**: 2026-11-19
**Status**: Draft
**Input**: #1591 and #1605. Read `find_package`, `pkg_check_modules` and `pkg_search_module` from every CMake file in the tree, with the conditions that reach them.

## Background

The CMake reader reads package searches (`find_package`, `pkg_check_modules`, `pkg_search_module`) only from a narrow file set (spec 1538 FR-002):
- the root `CMakeLists.txt`;
- `cmake/**` and `Modules/**`;
- `third_party/*` at depth 1.

It reads each file on its own, and `find_package` is matched by a pattern that ignores `if()` blocks entirely. Fetch declarations alone are read tree-wide, with their in-file conditions (spec 1538, C232).

Measured on ROCm TheRock `b1cae390` with submodules, in CI only, in the fixture repository (`census/includes.py`, census-includes run 44441297039 and the runs after it; spec 1538's census-fetches run 44005855500):

| | count |
|---|---|
| `find_package` calls in the tree | 2,061 |
| distinct package names they search for | 225 |
| `pkg_check_modules` / `pkg_search_module` calls outside the narrow set | 42 / 3 |
| components from `find_package` in waybill's TheRock document today | 4 |
| of the 225 names, appearing nowhere in that document | 168 |
| names declared only under conditions, once the `include()` / `add_subdirectory()` chain reaching the declaring file counts | 131 |
| ... of which in-file `if()` alone makes conditional | 109 |
| names also declared by a fetch in the tree (the "try the system first" half of a fallback pair) | 19 |

The 225 names, classified by first match:

| Class | Names | Examples |
|---|---|---|
| a `project(<name>)` in the tree | 63 | hip, rocprofiler-sdk, llvm, rocblas, amd_comgr, and vendored gtest |
| a `<name>Config.cmake[.in]` / `<name>-config.cmake[.in]` in the tree | 25 | hsa-runtime64, rocmcmakebuildtools, and openmp, mpi, numa, libdw, libelf, zstd, sqlite3 |
| the project's own `Find<name>.cmake` | 45 | ffmpeg, lapack, blas, libxml2, fftw, grpc, libdrm_amdgpu |
| one of CMake's own find modules (CMake 3.31 ships 162) | 26 | threads, python3, git, pkgconfig, cudatoolkit, doxygen, boost, curl |
| none of these: a config package found on the system | 66 | drm, fmt, catch2, tbb, nlohmann_json, opencv, openblas |

#1605's own part is the same machinery, applied to fetches and to file reachability:
- **One file is never read.** `libhsakmt/CMakeLists_wsl.txt` is reached by `include()` under a name the reader does not open; it holds one `find_package`, under `WIN_SDK`.
- **Conditions are lost at `include()`.** 19 `*.cmake` files are included only under `if()`, holding 15 fetches and 15 finds.
- **Conditions are lost at `add_subdirectory()`.** 175 conditional `add_subdirectory()` calls cover directories with declarations, mostly tests, samples and examples.
- **Effect on fetches.** Counting these chain conditions makes 10 more of the 55 fetch names conditional-only. That is about 6–9 TheRock components that are required today: hotswap_intercept, hotswap_rocr, json-c, libvfio-user, otf2-source, and one nanobind pin.

## Clarifications

### Session 2026-11-19

- Q: Which package searches are not dependencies (FR-006)? → A: A fixed, documented list of build tools and CMake helpers yields no component (Threads, PkgConfig, Git, Doxygen, Python's interpreter). Everything else yields a component, including CMake find modules for libraries (CUDAToolkit, Boost, CURL, OpenGL).
- Q: What does a search for a project built elsewhere in the same tree produce (FR-007)? → A: No component and no edge; the search is recorded as satisfied in-tree on the declaring side. Vendored third-party code is visible only when another reader finds it. A project component for in-tree CMake projects is #1616.
- Q: Does C57 `waybill:build-reference` widen at the same time (FR-008)? → A: Yes. C57 reads the same files as FR-001, so the document cannot contain a package search that C57 ignores.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every package a source tree searches for is in its SBOM (Priority: P1)

An operator scans a CMake monorepo. Its projects search for external packages with `find_package`, `pkg_check_modules` and `pkg_search_module`, in files anywhere in the tree. The SBOM lists each external package the tree searches for, whichever file asks.

**Why this priority**: On TheRock, 168 of 225 searched-for names are absent from the document. A consumer cannot see that the tree needs ffmpeg, libdrm, libelf, numa or opencv.

**Independent Test**: A synthetic tree where a nested project's `CMakeLists.txt` and a nested `*.cmake` file search for packages yields one component per external package, with the declaring files as evidence.

**Acceptance Scenarios**:

1. **Given** `projects/a/CMakeLists.txt` calls `find_package(waybill_fixture_ext 2.1)`, **When** the tree is scanned, **Then** one component for `waybill_fixture_ext` appears, with range `>=2.1` and that file as evidence. Today it is absent.
2. **Given** two projects search for the same package, **When** scanned, **Then** one component appears, with both files as evidence and both ranges.
3. **Given** `pkg_check_modules(X REQUIRED waybill-fixture-pc>=1.0)` in a nested file, **When** scanned, **Then** a component appears as it would from the root `CMakeLists.txt` today.

---

### User Story 2 - A conditional search is not a runtime requirement (Priority: P1)

A package searched for only under a condition (a test option, a platform switch, a feature flag) is optional in the SBOM, and the conditions are recorded. The conditions counted are the `if()` blocks in the declaring file and the conditions of the `include()` and `add_subdirectory()` calls that reach that file. This applies to fetches as well as package searches.

**Why this priority**: Reading every file without its conditions would turn 131 of the 225 names into runtime requirements, a worse error than leaving them out. Conditions must arrive with the widening, not after it.

**Independent Test**: Synthetic trees where the only search for a package is (a) inside `if()`, (b) in a `*.cmake` file included inside `if()`, and (c) under a directory added by `add_subdirectory()` inside `if()`. Each gives an optional component, recording the conditions.

**Acceptance Scenarios**:

1. **Given** `if(BUILD_TESTING) add_subdirectory(tests) endif()`, and `tests/CMakeLists.txt` calls `find_package(waybill_fixture_test)` unconditionally, **When** scanned, **Then** the component is optional and its recorded conditions include `BUILD_TESTING`.
2. **Given** `if(WIN_SDK) include(cmake/win.cmake) endif()`, and `win.cmake` declares a fetch, **When** scanned, **Then** the fetched component is optional and its C232 conditions include `WIN_SDK`. Today it is required.
3. **Given** a package is searched for once under a condition and once unconditionally, **When** scanned, **Then** it is a runtime requirement: one unconditional declaration wins.
4. **Given** a search inside a `function()` or `macro()` body, **When** scanned, **Then** it is treated as spec 1538 treats a fetch in a body.

---

### User Story 3 - A search for something the tree builds itself is not a second component (Priority: P1)

When a `find_package` names something the scanned tree builds or fetches itself, the SBOM does not invent a second component for it. That covers another project in the same monorepo, a fetched dependency, a git submodule, or an installed-library record.

**Why this priority**: Without this, widening multiplies identities. TheRock's own projects (hip, rocblas, amd_comgr, …) would appear as `pkg:generic` third-party packages, and 19 fetched dependencies would appear twice.

**Independent Test**: Synthetic trees where a searched-for name matches, in turn, a fetch, a git submodule, an installed-library record, and a `project()` in another directory of the tree. None yields a second component for that name.

**Acceptance Scenarios**:

1. **Given** `find_package(waybill_fixture_fmt QUIET)` followed by `if(NOT waybill_fixture_fmt_FOUND) FetchContent_Declare(waybill_fixture_fmt ...)`, **When** scanned, **Then** exactly one component exists for it: the fetched one. It keeps spec 1538's fallback shape, and the search's file is recorded as evidence.
2. **Given** a git submodule `extern/waybill_fixture_sub` and a `find_package(waybill_fixture_sub)` elsewhere, **When** scanned, **Then** no `pkg:generic/waybill_fixture_sub` component appears.
3. **Given** a `project(waybill_fixture_lib)` in `projects/lib/` and a `find_package(waybill_fixture_lib)` in `projects/app/`, **When** scanned, **Then** no `waybill_fixture_lib` component and no edge appear, and the search is recorded as satisfied in-tree (FR-007).

---

### User Story 4 - Tools and CMake pseudo-packages are not dependencies (Priority: P2)

Some `find_package` names are build tools or CMake helpers, not packages the software depends on: `Threads`, `PkgConfig`, `Git`, `Doxygen`, Python as an interpreter. They yield no component (FR-006).

**Why this priority**: They are frequent (`Threads` is searched for 192 times on TheRock, `Git` 74 times, `PkgConfig` 31 times). Emitting them as dependencies would be noise in every C/C++ document.

**Independent Test**: A synthetic tree searching for `Threads`, `PkgConfig`, `Git` and a real library gives a result matching FR-006.

**Acceptance Scenarios**:

1. **Given** `find_package(Threads REQUIRED)`, `find_package(Git)` and `find_package(waybill_fixture_lib2)`, **When** scanned, **Then** only `waybill_fixture_lib2` yields a component.
2. **Given** `find_package(Python3 COMPONENTS Interpreter)`, **When** scanned, **Then** no component. **Given** `find_package(Python3 COMPONENTS Interpreter Development)`, **Then** one component.
3. **Given** `find_package(CUDAToolkit)`, **When** scanned, **Then** one component: a CMake find module for a library is not a tool.

---

### User Story 5 - A file reached only by include() is read (Priority: P3)

A CMake file pulled in by `include()` under a name the reader does not open on its own (TheRock's `CMakeLists_wsl.txt`) is read, with the conditions of the `include()` that reaches it.

**Why this priority**: One file and one declaration on TheRock. Small, but it falls out of the same chain machinery.

**Independent Test**: A synthetic tree where `if(WIN_SDK) include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt) endif()`, and that file searches for a package.

**Acceptance Scenarios**:

1. **Given** that tree, **When** scanned, **Then** the package appears, optional, with `WIN_SDK` among its recorded conditions.

### Edge Cases

- **Include path not resolvable statically.** The path holds a variable other than `CMAKE_CURRENT_SOURCE_DIR` or `CMAKE_CURRENT_LIST_DIR` (149 calls on TheRock), or names a module that is not in the tree (911, mostly CMake's own). The include contributes no conditions, and nothing is guessed.
- **Cycles.** An include or `add_subdirectory()` loop terminates. A file reached through a cycle is treated as reached unconditionally.
- **A file reached both conditionally and unconditionally.** It is unconditional: one unconditional path wins, as for declarations.
- **An `else()` branch.** Recorded as spec 1538 records it (`else: <condition>`).
- **A variable package name** (`find_package(${PKG})`). Skipped as today, with the existing debug log.
- **The same name in different casing** (`OpenSSL`, `openssl`). One component, keeping C103 for the declared casing, as today.
- **Files the walker skips** (default-skipped directories, `--exclude-path`) stay unread. A dot-directory listed by spec 1613 is read only by the readers that opted in (pip, npm, CMake), so a CMake file under `.github/` is read.
- **Test directories.** A search in a test directory is labelled by spec 1533's test-scope pass as before. Conditions add to that and do not replace it.
- **A tree with no nested CMake files, or no conditions.** Byte-identical output to `main` for every existing fixture whose package searches all sit in the narrow file set unconditionally.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CMake reader MUST read `find_package`, `pkg_check_modules` and `pkg_search_module` from every CMake file the shared walker reaches, not only from the narrow file set of spec 1538 FR-002.
- **FR-002**: Every declaration (package search or fetch) MUST carry the conditions it is made under. These are:
  - the `if()` / `elseif()` / `else()` blocks enclosing it in its own file;
  - the conditions of the `include()` that reaches its file;
  - the conditions of the `add_subdirectory()` chain that reaches its file;
  - a `function()` / `macro()` body, as spec 1538 records it.
- **FR-003**: A component is optional when every declaration of it is conditional, and a runtime requirement when any declaration of it is unconditional. Its recorded conditions are the union across its declarations: C232 for fetches, and the equivalent for package searches (FR-012).
- **FR-004**: A file reached by `include()` under a name the reader does not open on its own MUST be read, with the `include()`'s conditions. Include paths are resolved statically only for literal paths and for `CMAKE_CURRENT_SOURCE_DIR` / `CMAKE_CURRENT_LIST_DIR`.
- **FR-005**: A package search naming something the tree already represents as a component MUST NOT create a second component. That covers a fetch of the same name, a git submodule, and an installed-library record (spec 1531). The search's file is added as evidence on the existing component, and the pairing keeps spec 1538's fallback shapes.
- **FR-006**: A fixed, documented list of build tools and CMake helpers MUST yield no component: `Threads`, `PkgConfig`, `Git`, `Doxygen`, and Python searched for as an interpreter (`find_package(Python[3] COMPONENTS Interpreter)` without `Development`). The list lives in one place and is documented in `docs/ecosystems.md`. Every other name, including CMake find modules for libraries (CUDAToolkit, Boost, CURL, OpenGL), yields a component under FR-001.
- **FR-007**: A search for a name the scanned tree builds itself MUST yield no component and no edge. The tree builds a name when it holds a `project(<name>)`, or a `<name>Config.cmake[.in]` / `<name>-config.cmake[.in]`, matched case-insensitively. Such a search is recorded as satisfied in-tree in the document-scope coverage record, by name, so a consumer can tell it apart from a search that was never read (plan R9: on TheRock the declaring side is usually a submodule or the scan root). FR-005 takes precedence: a name that is also fetched, a submodule or an installed-library record correlates to that component. Vendored third-party code built in the tree is therefore visible only when another reader finds it. A project component for in-tree CMake projects is #1616.
- **FR-008**: C57 `waybill:build-reference` MUST take its package-search names from the same files as FR-001. A git submodule that a nested search names becomes `declared-and-used`, and each such change is traced in the corpus.
- **FR-009**: Output MUST be byte-identical to `main` for a tree whose package searches all sit, unconditionally, in the narrow file set, and whose fetches carry no `include()` or `add_subdirectory()` conditions.
- **FR-010**: The three formats MUST agree on every new or extended annotation. Any new catalog row is registered in `docs/reference/sbom-format-mapping.md` with its extractor in the same change.
- **FR-011**: Repeated scans MUST produce identical documents.
- **FR-012**: Conditions on a package-search component MUST be recorded in a form that names the declaration kind: a sibling row of C232, as C235 is for links (plan R5).
- **FR-013**: Zero new Cargo dependencies.

### Key Entities

- **Package search**: a `find_package`, `pkg_check_modules` or `pkg_search_module` declaration. Attributes: package name, declared casing, version range, file, conditions.
- **Reaching chain**: the `include()` and `add_subdirectory()` calls by which a CMake file is reached, each with its enclosing conditions. A file is conditionally reached when every path to it passes a condition.
- **In-tree satisfaction**: a package name the scanned tree itself provides. Either it builds the name (`project()` or a package-config template; FR-007, no component), or it represents it as a component already (a fetch, submodule or installed-library record; FR-005, which correlates).
- **Tool list**: the fixed, documented set of names that are build tools or CMake helpers (FR-006).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock's source tree, every external package name the tree searches for, outside FR-006 and FR-007, is represented in the document. That is most of the 168 absent names; the exact expected count is fixed by the plan from the census classes.
- **SC-002**: On TheRock's source tree, every component whose declarations are all conditional is non-runtime. That includes the 6–9 fetch components named in the Background.
- **SC-003**: On TheRock's source tree, no searched-for name yields a second component beside a fetch, submodule or installed-library record of the same name. 0 duplicate `bom-ref`s.
- **SC-004**: On TheRock's source tree, 0 unreachable components lack an I7 marker.
- **SC-005**: The formats agree: `waybill sbom parity-check` on TheRock shows no gap that `main` does not have, and C42, C62, C232 and any new row pass.
- **SC-006**: Two runners scanning one commit produce identical documents apart from timestamps.
- **SC-007**: The public-corpus invariant gate stays green, and every golden change is traced.
- **SC-008**: The quality corpus is traced against a `main` control, including cmake-nlohmann-json and cpp-mongo.
- **SC-009**: TheRock's source scan time, interleaved against `main`, is measured and reported. The budget is set in the plan from the measured cost of reading about 6,000 more CMake files for package searches; they are already read for fetches.

## Assumptions

- **Conditions are not evaluated**, only recorded, as in spec 1538. A condition that is always true in practice still makes a declaration optional, and the recorded conditions let a consumer judge.
- **"Every CMake file the walker reaches"** means the files spec 1538 already reads for fetches (`CMakeLists.txt` and `*.cmake`), plus files reached by `include()` (FR-004).
- **Spec 1533's test-scope pass is unchanged.** It still labels searches in test directories, and conditions add to it.
- **The version range of a search** is computed as today (#1520).
- **TheRock is measured in CI only**, through the fixture repository's workflows, and never cloned locally. Fixtures are synthetic `waybill-fixture-*` trees.
- **Out of scope:**
  - evaluating CMake variables beyond the two source-directory variables;
  - `ExternalProject` build-tree state (#1535);
  - the TheRock-specific superbuild functions (#1539).
