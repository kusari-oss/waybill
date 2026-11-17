# Feature Specification: CMake fetches made through DownloadProject's `download_project()`

**Feature Branch**: `1540-cmake-download-project`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1594: CMake download_project() calls (the DownloadProject module, Crascit, MIT, widely vendored) are invisible. The function fetches at configure time but has no fetch call in its body … That template contains ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS} …) … Decide: whether to generalise spec 1539 so that a function body that configure_file()s a template found in the scan counts as a wrapper whose body is the template … or to recognise DownloadProject by name as an ExternalProject_Add alias with PROJ as the name, or both; how the template variables bind …; what the component is named …; that spec 1538 and 1539 rules … apply unchanged; and how C234 records it … Measure … in CI only via the fixture repo … never clone TheRock locally. Probe any CMake semantics the design depends on … Synthetic waybill-fixture-* fixtures; zero new Cargo dependencies." (full text in the conversation that created this branch)

The spec number (1540) is the next in sequence. It is unrelated to issue #1540, the TheRock build-tree cataloger.

## Background

### What the reader sees today

Spec 1539 (#1518) reads calls to user-defined wrappers. A wrapper is a `function()` or `macro()` whose body calls `FetchContent_Declare`, `ExternalProject_Add` or another wrapper, and passes its caller's arguments on. A call is bound as CMake binds it, and becomes a declaration like a direct one.

DownloadProject's `download_project()` is not one. Its body has no fetch call:

```cmake
function(download_project)
  set(options QUIET)
  set(oneValueArgs PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT)
  cmake_parse_arguments(DL_ARGS "${options}" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})
  ... defaults for PREFIX, DOWNLOAD_DIR, SOURCE_DIR, BINARY_DIR ...
  configure_file("${_DownloadProjectDir}/DownloadProject.CMakeLists.cmake.in"
                 "${DL_ARGS_DOWNLOAD_DIR}/CMakeLists.txt")
  execute_process(COMMAND ${CMAKE_COMMAND} ... WORKING_DIRECTORY "${DL_ARGS_DOWNLOAD_DIR}")
  execute_process(COMMAND ${CMAKE_COMMAND} --build . ...)
endfunction()
```

The fetch is in the template it configures, `DownloadProject.CMakeLists.cmake.in`:

```cmake
if(${DL_ARGS_BUILD_PROJECT})
    ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS}
                        SOURCE_DIR "${DL_ARGS_SOURCE_DIR}" BUILD_IN_SOURCE TRUE TEST_COMMAND "")
else()
    ExternalProject_Add(${DL_ARGS_PROJ}-download ${DL_ARGS_UNPARSED_ARGUMENTS}
                        SOURCE_DIR "${DL_ARGS_SOURCE_DIR}" ... UPDATE_COMMAND "" CONFIGURE_COMMAND "" ...)
endif()
```

The template forwards the caller's arguments the way spec 1539 binds: the name comes from a parsed variable, and the identity arrives through `${DL_ARGS_UNPARSED_ARGUMENTS}`. Two things keep the reader from seeing it:
- **The file is never read.** The shared walker hands the CMake reader `CMakeLists.txt` and `*.cmake` files only; a `.cmake.in` file is not among them.
- **Nothing links the two.** The body links the function to the template only through `configure_file()`, and the template's path is built from `${_DownloadProjectDir}`.

`configure_file()` is called without `@ONLY`, so CMake replaces `${…}` references in the template from the function's scope. That is CMake's documented behaviour, to be probed (see Assumptions).

### The case that motivates it

On TheRock (pin `b1cae390`, submodules at their recorded commits), spec 1539's census (`census-fetches` run 44096336322) counts **8** `download_project` calls passing `URL` or `GIT_REPOSITORY`. They are in rocprim and its vendored copies. The function and template above were read through the GitHub API at rocm-libraries `98e132f8`. One call:

```cmake
download_project(
  PROJ                  rocrand
  GIT_REPOSITORY        https://github.com/ROCmSoftwarePlatform/rocRAND.git
  GIT_TAG               develop
  GIT_SHALLOW           TRUE
  INSTALL_DIR           ${ROCRAND_ROOT}
  ...
  BUILD_PROJECT         TRUE
  UPDATE_DISCONNECTED   TRUE
)
```

None of these 8 appears in TheRock's SBOM, and none is counted in C231.

### The tag is sometimes a branch

The example's `GIT_TAG develop` is a branch name, not a release. A direct `ExternalProject_Add` with the same tag is emitted today with `develop` as its version. Whether that needs marking is in scope only if the census shows it matters on TheRock (FR-009).

## Clarifications

### Session 2026-11-17

- Q: How is `download_project()` recognised? → A: By name, as an `ExternalProject_Add` alias, only when its definition in the scan has DownloadProject's shape. The configured template is not read.
- Q: How is a URL fetch named when its archive does not parse as `<name>-<version>`? → A: `<PROJ>`, the name the caller gave.
- Q: Where does DownloadProject's keyword list come from? → A: From each definition: its `set()` lists and its `cmake_parse_arguments(DL_ARGS …)`, resolved statically. If they cannot be resolved, the call is unresolved.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A dependency fetched with `download_project()` is a component (Priority: P1)

An engineer scans a project that vendors DownloadProject and fetches dependencies with `download_project()`. Each call that names a source (a `URL` or a `GIT_REPOSITORY`) appears as a component with its version and declared hash, exactly as the `ExternalProject_Add` that DownloadProject generates for it would.

**Why this priority**: This is the defect (#1594). On TheRock, it hides 8 fetches in rocprim and its copies.

**Independent Test**: Scan a synthetic tree that vendors DownloadProject (its `.cmake` file and its template) and calls `download_project()` with a GitHub repository and a tag, and with a URL and a hash. Both are emitted, with the same fields a direct `ExternalProject_Add` with the generated arguments would give.

**Acceptance Scenarios**:

1. **Given** a `download_project(PROJ x GIT_REPOSITORY https://github.com/<o>/<r>.git GIT_TAG v1)` call, **When** scanned, **Then** one component `pkg:github/<o>/<r>@v1` is emitted, owned by the call site's project.
2. **Given** a `download_project(PROJ x URL <archive> URL_HASH SHA256=<h>)` call, **When** scanned, **Then** one component is emitted with that URL's version and that hash.
3. **Given** the definition's own keywords (upstream: `PROJ`, `PREFIX`, `DOWNLOAD_DIR`, `SOURCE_DIR`, `BINARY_DIR`, `BUILD_PROJECT`, `QUIET`), **When** a call passes them, **Then** none of them reaches the fetch's arguments, as in CMake.
4. **Given** a call that passes neither `URL` nor `GIT_REPOSITORY`, **When** scanned, **Then** nothing is emitted, and the call is counted under `unresolved.identity`.

---

### User Story 2 - The SBOM says how it was found (Priority: P2)

A reviewer can see that a component came from `download_project()`, not from a direct fetch, through C234 `waybill:cmake-fetch-wrapper` (spec 1539). The call site is in C18.

**Why this priority**: A binding inferred through a template, or a function recognised by name, is weaker evidence than a direct call. Saying so lets a consumer weigh it.

**Independent Test**: In User Story 1's fixtures, each component carries C234 naming `download_project`, and its C18 names the call site's file.

**Acceptance Scenarios**:

1. **Given** a component from a `download_project` call, **When** scanned, **Then** C234 records `download_project`, and C18 is the call site.
2. **Given** a tree with DownloadProject and no calls, **When** scanned, **Then** no component is emitted, and C231 is absent unless some fetch declaration was read (spec 1538 I-4).

---

### User Story 3 - TheRock's `download_project` calls are measured in CI (Priority: P2)

The maintainer sees, from CI runs in the fixture repository, what this changes on TheRock:
- every `download_project` call, with its identity and version;
- which calls became components, and which merged into an existing one;
- scan time against `main`.

**Why this priority**: The census decides whether the branch-name tag question (FR-009) is in scope, and confirms the 8 calls before the design is fixed.

**Independent Test**: The fixture repository's census and `measure-source` workflows, at this branch and at `main`.

**Acceptance Scenarios**:

1. **Given** the census at TheRock's pin, **When** it runs, **Then** it lists every `download_project` call, with its file, `PROJ`, identity (`URL` or `GIT_REPOSITORY` + `GIT_TAG`), and whether the tag looks like a branch, before the design is fixed.
2. **Given** the branch scan, **When** compared with the census, **Then** every call is accounted for as emitted, merged or unresolved.

### Edge Cases

- **A template with no definition in the scan.** If DownloadProject's `.cmake` file is excluded or absent, the calls are not recognised, and are counted nowhere. This matches spec 1539's "defined outside the scan".
- **A modified DownloadProject.** A project's copy may differ from the upstream module. A copy that keeps the shape in FR-001 is read with its own keyword lists (FR-002), so a copy that adds a keyword consumes it as CMake does; one that does not keep the shape is not recognised. The census (SC-001) reports any copy that differs.
- **A different function named `download_project`.** Without DownloadProject's shape, it is not recognised: its calls are read only if it is a spec 1539 wrapper by its own body.
- **`BUILD_PROJECT`.** The template's `if(${DL_ARGS_BUILD_PROJECT})` and its `else()` hold the same fetch name and forwarded arguments; the keyword changes what is built after the fetch, not what is fetched, and does not make the fetch conditional.
- **A call under a condition, or in a test directory.** The call site's spec 1538 shapes and scope apply (FR-005), as for any wrapper call.
- **The same dependency declared twice.** rocprim's vendored copies may declare the same repository and tag through `download_project()` in several places. Spec 1538's one-component-per-PURL and enclosing-supplier rules decide, unchanged (FR-005).
- **A variable in the call.** `${ROCRAND_ROOT}` and the like are expanded from a same-file literal `set()`, as in spec 1539. Otherwise they stay as written, and decide nothing unless the identity depends on them.
- **`download_project` wrapped again.** A project function that calls `download_project()` and forwards its arguments is a spec 1539 wrapper of it, and is followed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001: What is recognised.** A call to `download_project` (case-insensitive) MUST be read as an `ExternalProject_Add` alias, when at least one definition of that name in the scan has DownloadProject's shape:
  - its body calls `cmake_parse_arguments` with prefix `DL_ARGS`, and `PROJ` among the keywords it parses;
  - and it calls `configure_file()` on a template whose file name is `DownloadProject.CMakeLists.cmake.in`.

  The template itself is not read. A definition of that name without this shape is not DownloadProject. If every definition lacks the shape, the call is not recognised; if definitions disagree, spec 1539's FR-002 rule applies, and the call is unresolved and counted as ambiguous. Other configure-time generators are out of scope.
- **FR-002: Binding.** The call MUST bind as CMake binds it, with the keywords **of the definition it binds to**:
  - the options, one-value and multi-value keywords are read from that definition's `cmake_parse_arguments(DL_ARGS …)`, resolving the body's literal `set()` lists it names (`set(oneValueArgs PROJ PREFIX …)` then `"${oneValueArgs}"`). If a list cannot be resolved statically (set conditionally, appended, or from outside the body), the call is unresolved, never bound with a guessed list;
  - those keywords are consumed with spec 1539's measured rules: the last value of a one-value keyword wins, and a multi-value keyword takes every word up to the next keyword;
  - every other argument is forwarded, in order, to the generated `ExternalProject_Add`.

  At rocm-libraries `98e132f8` the upstream lists are `QUIET`; `PROJ PREFIX DOWNLOAD_DIR SOURCE_DIR BINARY_DIR BUILD_PROJECT`; and no multi-value keywords.
- **FR-003: Same fields as the generated call.** The component MUST be the one a direct `ExternalProject_Add` with the generated arguments would give: version, hash, mechanism (`cmake-externalproject`) and identity rules, including the GitHub rule and spec 1539's FR-005 archive naming for a URL fetch.
- **FR-004: Naming.** The generated declaration's name is `<PROJ>`, the name the caller gave, not DownloadProject's internal `<PROJ>-download` target. A URL fetch whose archive parses as `<name>-<version>` is named from the archive (spec 1539 FR-005); otherwise it is named `<PROJ>`. A GitHub `GIT_REPOSITORY` names the package by itself, as today. A call with no `PROJ` is not bound (FR-007).
- **FR-005: Spec 1538 and 1539 apply unchanged at the call site:**
  - conditions from the call site's enclosing blocks;
  - test scope from the call site's directory;
  - first-declaration-wins (FetchContent only; `download_project` generates an `ExternalProject_Add`, so it never applies);
  - the enclosing-supplier merge (R9);
  - one component per PURL;
  - ownership by the call site's file;
  - C231 accounting, including `wrappers`;
  - the template's internal `if(${DL_ARGS_BUILD_PROJECT})` does not make the fetch conditional.
- **FR-006: Evidence.** Each component produced by a `download_project` call MUST carry C234 with a chain ending in `download_project` as defined, after any spec 1539 wrapper around it, outermost first. No new catalog row is added.
- **FR-007: No identity, no component.** A call with no `PROJ`, no `URL` and no `GIT_REPOSITORY`, or whose identity depends on an unexpanded variable, MUST NOT emit a component, and is counted under C231 `unresolved.identity`.
- **FR-008: Direct calls and existing wrappers unchanged.** Every direct `FetchContent_Declare` / `ExternalProject_Add`, and every spec 1539 wrapper call, MUST produce byte-identical output to `main`.
- **FR-009: Branch-name tags.** A `GIT_TAG` that is a branch name is emitted as a direct call's would be today. Marking it is added only if the census (SC-001) shows `download_project` calls on TheRock with a branch-name tag that would otherwise be presented as a release. If it does, the marking reuses an existing row where one fits (Principle V audit), or is filed as a follow-up.
- **FR-010: Formats agree.** CycloneDX, SPDX 2.3 and SPDX 3 MUST carry the same components, edges and rows, and agree on the root's direct targets (spec 1537 I-1).
- **FR-011: No new dependencies.** Zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names, and a synthetic DownloadProject-shaped module, not a copy of the upstream file.

### Key Entities

- **DownloadProject definition**: a `download_project` function in a walked `.cmake` file with DownloadProject's shape (FR-001): `cmake_parse_arguments(DL_ARGS …)` parsing `PROJ`, and a `configure_file()` of `DownloadProject.CMakeLists.cmake.in`.
- **`download_project` call**: a call site with its file, enclosing blocks and literal arguments, bound to the definition.
- **Generated declaration**: the `ExternalProject_Add` the call produces, with its arguments resolved. It is treated exactly as a direct declaration.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (census first):** Before FR-001 to FR-004 are fixed in the plan, a CI census of TheRock at its pin lists every `download_project` definition and call, with its file, `PROJ`, identity and tag, whether the template is present beside each definition, and whether any copy differs from the upstream module's shape.
- **SC-002 (TheRock):** Every `download_project` call on TheRock with a `URL` or `GIT_REPOSITORY` is emitted, or merged into the component it duplicates. The count equals the census's.
- **SC-003 (no fabrication):** 0 components on TheRock are named after a DownloadProject keyword, a template variable, or a `${…}` expression.
- **SC-004 (accounting):** On TheRock, C231 V1 and V2 hold, and every census call is accounted for as emitted, merged or unresolved.
- **SC-005 (cost):** The median full-tree TheRock scan with submodules is at most 1.5× `main`'s, by the interleaved `measure-source` workflow.
- **SC-006 (corpora):** The public corpus goldens are byte-identical, or every change is traced to a `download_project` call in that target. Every quality-corpus movement is traced PURL by PURL against a `main` control.
- **SC-007 (tests have teeth):** The recognition rule, the keyword split (including a copy with an added keyword, and lists that cannot be resolved), the naming rule and the accounting each have a contract test, and a mutation that removes or weakens each is caught.
- **SC-008 (evidence):** 100% of `download_project` components carry C234 naming `download_project`, in all three formats.

## Assumptions

- **Scope is the CMake fetch reader only.** DownloadProject is the one configure-time generator measured on TheRock, and it is recognised by name and shape (clarification). Generalising to any `configure_file()`d template, and other generators such as hand-written `file(WRITE)` projects, are out of scope.
- **Static only.** No CMake is run on the scanned tree. The CMake behaviours the design depends on are probed with a real CMake on synthetic projects in plan, and the probe is committed:
  - that a `download_project` call fetches what an `ExternalProject_Add` with its non-DownloadProject arguments would, in order, with DownloadProject's keywords consumed;
  - that `BUILD_PROJECT` does not change what is fetched.
- **The upstream module's shape** (template name, keyword lists) is as read at rocm-libraries `98e132f8`. Keywords are read from each copy (FR-002), and the census reports any copy whose lists differ.
- **All measurement of TheRock is in CI,** through `kusari-sandbox/waybill-fixture-therock-sdk`. TheRock is never cloned or scanned locally. Single public files may be read through the GitHub API.
