# Feature Specification: CMake fetches made through user-defined wrappers

**Feature Branch**: `1539-cmake-fetch-wrappers`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1518 — fetches made through a user-defined wrapper around ExternalProject_Add / FetchContent_Declare are invisible. Super-builds wrap the built-ins to add logging, mirrors or common options, and the CMake reader only matches the built-in names. On ROCm's TheRock (pin b1cae390) this hides every third-party source tarball: they are all fetched through `therock_subproject_fetch` (cmake/therock_subproject.cmake:147), about 40 calls over 39 distinct tarballs per #1539 … Decide: which forwarding shapes count as a wrapper … how a call site's literal arguments bind to the wrapper's parameters … what a wrapper-fetched component is named … that a mirror URL gives no upstream identity … that spec 1538's conditional shapes, first-declaration-wins, enclosing-supplier merge and ownership apply unchanged to wrapper calls; and an annotation recording the wrapper name as evidence, registered as a catalog row. Whether operator-declared wrapper names (a flag) are needed … Measure … in CI only, via the fixture repo … Synthetic waybill-fixture-* fixtures; zero new Cargo dependencies." (full text in the conversation that created this branch)

The spec number (1539) is the next in sequence. It is unrelated to issue #1539, the TheRock cataloger, which this feature is a prerequisite of.

## Background

### What the reader sees today

Since spec 1538, the CMake reader reads `FetchContent_Declare` and `ExternalProject_Add` from every CMake file the scan walks. It matches those two command names only.

A project that wraps them in its own `function()` or `macro()` (to add logging, a mirror, or common options) fetches every dependency through a command the reader does not know. Today:
- **the call sites** of the wrapper are not read at all;
- **the one built-in call inside the wrapper's body** is read once, as a declaration whose name is a parameter such as `${target_name}`. It is counted under `unresolved.identity` (C231), and no component is emitted.

### The case that motivates it

ROCm's TheRock (pin `b1cae390`) fetches its third-party source tarballs through `therock_subproject_fetch` (`cmake/therock_subproject.cmake:147`). #1539 counted about 40 calls over 39 distinct tarballs. One call site (`third-party/sysdeps/common/zstd/CMakeLists.txt`):

```cmake
therock_subproject_fetch(therock-zstd-sources
  SOURCE_DIR "${_source_dir}"
  # Originally mirrored from: "https://github.com/facebook/zstd/releases/download/v1.5.7/zstd-1.5.7.tar.gz"
  URL "https://rocm-third-party-deps.s3.us-east-2.amazonaws.com/zstd-1.5.7.tar.gz"
  URL_HASH "SHA256=eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3"
  TOUCH "${_download_stamp}"
)
```

The wrapper does **not** forward `${ARGN}`, the shape #1518 proposed detecting:

```cmake
function(therock_subproject_fetch target_name)
  cmake_parse_arguments(PARSE_ARGV 1 ARG "CMAKE_PROJECT" "SOURCE_DIR;PREFIX;EXCLUDE_FROM_ALL" "TOUCH")
  ...
  ExternalProject_Add("${target_name}" ... SOURCE_DIR "${ARG_SOURCE_DIR}" ... ${_extra} ${ARG_UNPARSED_ARGUMENTS})
endfunction()
```

`URL` and `URL_HASH` are not among the parsed keywords, so they reach `ExternalProject_Add` through `${ARG_UNPARSED_ARGUMENTS}`. The name is the first parameter.

These tarballs are code compiled into ROCm, so none of TheRock's third-party sources appear in its SBOM today.

### Other wrappers in the same tree

Spec 1538's census (`specs/1538-cmake-nested-fetches/measurements/census.md`) found 17 fetch calls inside function or macro bodies, 8 of them with a `${…}` name. Among them:
- LLVM's `llvm_ExternalProject_Add` and `add_custom_libcxx`;
- LLVM's `builtin_*` and `runtime_*` functions, which call `llvm_ExternalProject_Add`.

The census (`measurements/census.md`) found 7 wrappers, all TheRock's or LLVM's. At the branch the reader recognises 9 (`measurements/therock.md`). rocdecode's and rocjpeg's test-build functions (`rocdecode_add_test_ext`, `rocjpeg_add_test_ext`) pass their parameters in non-name positions (`test_${TEST_EXE}`, `${TEST_SOURCE_DIR}`), which the census script did not count as forwarding. Their 14 calls build in-tree tests.

Several of these build in-tree sources and pass no URL or repository. They must stay unresolved, not become components.

### Out of core

A mirror URL names the mirror, not the upstream. TheRock records the upstream in a `# Originally mirrored from:` comment. Recovering it is project-specific and belongs in the TheRock cataloger (#1539), not in the CMake reader. The archive's name, version and declared hash are still available from the mirror URL.

## Clarifications

### Session 2026-11-17

- Q: What is a component fetched through a wrapper named, when the bound name is a build target such as `therock-zstd-sources`? → A: The archive filename's name when the filename parses as `<name>-<version>`, for wrapper calls only, falling back to the bound name otherwise. Direct calls keep today's naming.
- Q: When a wrapper is defined more than once, which definition does a call bind to? → A: Every definition. If all of them give the same component, it is emitted; if they differ, the call is unresolved.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A dependency fetched through a forwarding wrapper is a component (Priority: P1)

An engineer scans a C++ super-build that fetches its third-party sources through its own wrapper function. Each call to the wrapper that names a source (a URL or a repository) appears as a component with its version and declared hash, exactly as a direct `ExternalProject_Add` or `FetchContent_Declare` with the same arguments would.

**Why this priority**: This is the defect (#1518). On TheRock it hides every third-party source tarball.

**Independent Test**: Scan #1518's repro. It has a wrapper forwarding `${ARGN}`, one call to it, and one direct `ExternalProject_Add`. Check that both are emitted, the wrapped one with the same fields a direct call with its arguments would give, apart from its name (FR-005).

**Acceptance Scenarios**:

1. **Given** a wrapper that forwards `${ARGN}` to `ExternalProject_Add`, called once with a literal `URL` and `URL_HASH`, **When** scanned, **Then** one component is emitted with that URL's version and that hash.
2. **Given** a wrapper that parses its arguments with `cmake_parse_arguments` and forwards `${<prefix>_UNPARSED_ARGUMENTS}` (TheRock's shape), **When** a call passes `URL` and `URL_HASH` as unparsed arguments, **Then** the same component is emitted.
3. **Given** a wrapper that re-passes a parsed keyword by name (`URL ${ARG_URL}`), **When** called with `URL <literal>`, **Then** the same component is emitted.
4. **Given** a wrapper called five times with five different URLs, **When** scanned, **Then** five components are emitted, one per call.
5. **Given** the same tree, **When** scanned, **Then** the wrapper's own body is not also counted as an unresolved declaration (FR-008).

---

### User Story 2 - A wrapper around an in-tree build adds nothing false (Priority: P1)

The same engineer's tree also wraps `ExternalProject_Add` to build in-tree subprojects (LLVM's runtimes, test programs) with no URL and no repository. Those calls do not become components, and the SBOM says how many such calls were seen and why nothing was emitted.

**Why this priority**: Without it, User Story 1 fabricates components for things that are not third-party sources. Both stories must land together.

**Independent Test**: A fixture with a wrapper whose calls pass only `SOURCE_DIR`. No component is emitted, and C231 counts the calls as unresolved.

**Acceptance Scenarios**:

1. **Given** a wrapper call that binds no `URL` and no `GIT_REPOSITORY`, **When** scanned, **Then** no component is emitted and the call is counted under `unresolved.identity`.
2. **Given** a wrapper called inside `foreach()` with the name or URL taken from the loop variable, **When** scanned, **Then** no component is emitted and the call is counted as unresolved, never guessed.

---

### User Story 3 - The SBOM says how a wrapped fetch was found (Priority: P2)

A reviewer looking at a component that came from a wrapper call can see which wrapper produced it. That makes the evidence auditable: the call site is in C18, and the wrapper chain is in a new row.

**Why this priority**: An inferred binding through someone else's function is weaker evidence than a direct call. Saying so lets a consumer weigh it.

**Independent Test**: In User Story 1's fixtures, each wrapped component carries the wrapper's name. Direct calls do not carry the row.

**Acceptance Scenarios**:

1. **Given** a component from a call to `wf_fetch`, **When** scanned, **Then** its wrapper row names `wf_fetch`, and C18 names the call site's file, not the wrapper's file.
2. **Given** a wrapper that calls another wrapper, **When** scanned, **Then** the row lists both, outermost first.

---

### User Story 4 - TheRock's third-party sources are measured in CI (Priority: P2)

The maintainer sees, from CI runs in the fixture repository, what this changes on TheRock:
- how many wrapper definitions and call sites there are, and of which shapes;
- which calls became components, with versions and hashes;
- which calls stayed unresolved, and why;
- the scan time against `main`.

**Why this priority**: The forwarding shapes to support are chosen from this measurement (FR-001), not assumed.

**Independent Test**: The fixture repository's census and `measure-source` workflows, run at this branch and `main`.

**Acceptance Scenarios**:

1. **Given** the census, **When** run at TheRock's pin, **Then** every function or macro whose body calls a fetch built-in is listed with its forwarding shape and its call-site count, before the shape list is fixed.
2. **Given** the branch scan, **When** compared with the census, **Then** every wrapper call site is accounted for as emitted, merged or unresolved.

### Edge Cases

- **Defined twice.** A wrapper defined in two files: CMake uses the definition in effect at the call, which depends on evaluation order. The reader binds the call against both (FR-002). If they agree, one component is emitted; if they differ, nothing is emitted and C231 counts the call as ambiguous. A definition whose body is not a recognised wrapper still counts as a definition here, so a call is never bound through one definition while ignoring another.
- **Defined but never called.** Nothing is emitted, and the body is not counted as a declaration (FR-008).
- **Called but defined outside the scan,** for example in an installed CMake package. The call is not recognised. That is the same as today, and it is counted nowhere.
- **Recursion.** A wrapper that calls itself, directly or through another wrapper, is followed to a bounded depth and never loops (FR-004).
- **Both keyword sources.** A wrapper body with its own literal `URL` that also forwards the caller's arguments: the effective value follows CMake's rule for a repeated keyword. That rule is probed in plan, as spec 1538 probed its semantics.
- **Variable arguments.** A call-site argument `${VAR}` is expanded from a same-file literal `set()`, as for direct calls. A variable that cannot be expanded decides nothing unless the identity depends on it. The call is unresolved only if its bound `URL`, `GIT_REPOSITORY` or `GIT_TAG` still contains `${…}` (FR-007). TheRock's calls pass `SOURCE_DIR "${_source_dir}"` and `TOUCH "${_download_stamp}"`, which do not affect identity.
- **Positional parameters.** A wrapper whose name parameter is not first, or that reads `${ARGV0}`/`${ARGV1}`: bound by position like any other parameter.
- **An archive filename that does not parse.** A wrapper call whose URL ends in a filename without a `<name>-<version>` form (for example `v1.5.7.tar.gz`, or a URL with no version) is named from the bound name, as a direct call is (FR-005).
- **Shapes and merges.** A wrapper call under `if(NOT X_FOUND)`, or in a `tests/` directory, gets spec 1538's shapes from the **call site**. Conditions inside the wrapper body add to them (FR-006).
- **First declaration wins.** Wrapping `FetchContent_Declare` with the same bound name in an ancestor directory triggers first-declaration-wins exactly as two direct calls would.
- **A body that does more than fetch.** A wrapper body may call the built-in once alongside other commands. Only the fetch call matters.
- **A body that is not a wrapper.** A wrapper body that calls the built-in with entirely literal arguments, and forwards nothing, is not a wrapper. It is read as a direct fetch inside a function body (spec 1538's `cmake-function-body`), unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001: What counts as a wrapper.** A `function()` or `macro()` defined in any CMake file the reader reads MUST be treated as a fetch wrapper when its body calls `FetchContent_Declare`, `ExternalProject_Add`, or another wrapper (FR-004), and forwards the caller's arguments by a recognised shape. The recognised shapes were fixed from the census (SC-001), and include those the census finds carrying an identity on TheRock. They are (research R1):
  - `${ARGN}`, `${ARGV}`;
  - `${<P>_UNPARSED_ARGUMENTS}` after `cmake_parse_arguments(<P> …)` or `cmake_parse_arguments(PARSE_ARGV <n> <P> …)`;
  - a parsed variable `${<P>_<KW>}`, such as `URL ${<P>_URL}` or `URL ${<P>_SOURCE_URL}`;
  - a named parameter used as the fetch's name, such as `${target_name}`.
- **FR-002: Binding a call.** For each call to a wrapper, the reader MUST bind the call-site arguments to the wrapper's parameters as CMake would:
  - positional parameters first;
  - then `cmake_parse_arguments` options, one-value and multi-value keywords;
  - then the remainder.

  It then evaluates the fetch call in the body with those bindings. A default the body sets with a literal value, for a parameter the call did not pass, is applied. A wrapper defined more than once is bound against **every** definition, because which one CMake uses depends on evaluation order a static reader cannot know:
  - if every definition gives the same PURL and the same declared hash, the call is emitted once;
  - otherwise the call is unresolved, counted under C231 `unresolved.identity`, and also counted under a new C231 key for ambiguous wrapper calls, outside V1 (FR-008).
- **FR-003: Same fields as a direct call.** Apart from naming (FR-005), a bound wrapper call MUST produce the component, with the same version, hash, mechanism and identity rules, that a direct call with the bound arguments would. This includes the existing `GIT_REPOSITORY` handling and the parameterised-name rule (spec 1538 C-13b).
- **FR-004: Wrappers calling wrappers.** A wrapper whose body calls another wrapper MUST be followed transitively. A recursive chain stops at a bound depth, never loops, and is counted unresolved.
- **FR-005: Naming.** A component produced by a wrapper call with a `URL` MUST be named from the archive filename when the filename, with its archive extension removed, parses as `<name>-<version>`, where `<version>` is the version the URL gives and `<name>` is non-empty. `zstd-1.5.7.tar.gz`, bound to `therock-zstd-sources`, gives `pkg:generic/zstd@1.5.7`.
  - Otherwise the component is named from the bound name, as a direct call is.
  - A `GIT_REPOSITORY` fetch is named as a direct call is (FR-003); this rule applies to URL fetches only.
  - Direct calls are not affected (FR-011). Extending the rule to direct calls is a separate decision, made with its golden changes measured.
- **FR-006: Spec 1538 applies unchanged at the call site.** Wrapper calls MUST get spec 1538's behaviour as direct calls do:
  - the conditional shapes (C122) and C232, taken from the call site's enclosing blocks, with any conditions inside the wrapper body appended;
  - test scope from the call site's directory;
  - first-declaration-wins by bound name;
  - the enclosing-supplier merge (R9);
  - one component per PURL;
  - ownership by the call site's file.
- **FR-007: No identity, no component.** A bound call with no `URL` and no `GIT_REPOSITORY`, or one whose identity depends on an unexpanded variable, MUST NOT emit a component. It is counted under C231 `unresolved.identity`.
- **FR-008: Accounting.** C231 MUST stay exact (V1: `read == emitted + merged.* + unresolved.identity`).
  - Each wrapper call is one declaration read.
  - The fetch call inside a recognised wrapper's body is a template, not a declaration. A body is recognised only when its fetch call has no identity of its own, read as spec 1538 reads it (plan R2). A body call with a literal identity, such as spec 1538's C-13b, stays a direct fetch in a function body. It is counted under a new C231 key for wrapper definitions, outside V1, so it does not inflate `read` or `unresolved.identity`.
  - Wrapper calls left unresolved because their definitions disagree (FR-002) are counted under a second new key, also outside V1, as well as under `unresolved.identity`.
  - Both keys are registered with C231's documentation.
- **FR-009: Evidence row.** Each component produced by a wrapper call MUST carry a new per-component row, `waybill:cmake-fetch-wrapper`: a sorted JSON array of the wrapper chains that produced it, each chain outermost first. It is registered in the catalog with parity extractors for CycloneDX, SPDX 2.3 and SPDX 3. A component that also has a direct declaration keeps the row for its wrapped declarations.
- **FR-010: No operator flag unless measured.** No option for naming extra wrapper commands is added unless the census (SC-001) finds call sites with an identity that the static shapes cannot bind. If it does, the decision and the count are recorded, and a flag becomes its own follow-up.
- **FR-011: Direct calls unchanged.** Every direct `FetchContent_Declare` / `ExternalProject_Add` MUST produce byte-identical output to `main`. The document-level C231 may change where the tree defines a wrapper, because a wrapper body is no longer counted as a declaration (research R9). Components may not.
- **FR-012: Formats agree.** CycloneDX, SPDX 2.3 and SPDX 3 MUST carry the same wrapped components, edges and rows, and the three formats MUST agree on the root's direct targets (spec 1537 I-1).
- **FR-013: No new dependencies.** Zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names.

### Key Entities

- **Wrapper definition**: a function or macro name, its parameters, its argument-parsing calls, and the fetch call (or wrapper call) in its body with its forwarding shape. Defined in one or more files.
- **Wrapper call**: a call site, with its file, its enclosing blocks, and its literal arguments, bound to one definition.
- **Bound declaration**: the fetch the call would make, with arguments resolved. It is treated exactly as a direct declaration.
- **Wrapper chain**: the sequence of wrapper names from the call site to the built-in. It is the evidence row's value.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (census first):** Before the shape list in FR-001 is fixed, a CI census of TheRock at its pin lists every function or macro whose body calls a fetch built-in or another such function. For each, it records the forwarding shape, the number of call sites, and how many of those pass a URL or repository.
- **SC-002 (TheRock's third-party sources):**
  - Every `therock_subproject_fetch` call site whose `URL` is literal after same-file `set()` expansion is emitted as a component with the URL's version and the declared SHA-256, named from the archive filename where it parses (FR-005).
  - The census (`measurements/census.md`) counts 40 such calls, 36 with literal URLs and 4 whose URLs expand from same-file `set()`s. They cover 39 distinct URLs, because `third-party/fftw3` calls twice with one URL. So 40 declarations are emitted, as 39 components.
- **SC-003 (no fabrication):** Every wrapper call the census marks as having no URL or repository emits nothing and is counted unresolved. That covers LLVM's 13 in-tree calls and rocdecode's and rocjpeg's 14 test-build calls (`measurements/therock.md`). 0 components on TheRock have a name containing `${`. A component named from its bound name under FR-005's fallback (`therock-OpenBLAS-sources`) is a literal name the source declares, and is allowed.
- **SC-004 (accounting):** On TheRock, C231 V1 and V2 hold, and every census call site is accounted for as emitted, merged or unresolved. Wrapper definitions are counted separately: C231 `wrappers.definitions` matches the wrappers in the census, explained definition by definition: 9 at the branch, where the census script counted 7 because it under-detected two (`measurements/therock.md`).
- **SC-005 (cost):** The median full-tree scan time of TheRock with submodules is at most 1.5× `main`'s. It is measured by the interleaved `measure-source` workflow: a warm-up, then three alternating rounds.
- **SC-006 (corpora):**
  - The public corpus goldens are byte-identical, or every change is traced to a wrapper call in that target.
  - Every quality-corpus movement is traced PURL by PURL against `main`.
- **SC-007 (tests have teeth):** Each recognised forwarding shape, the binding rules, recursion and the accounting have a contract test. A mutation that removes or weakens each one is caught by a test.
- **SC-008 (evidence):** 100% of wrapped components carry `waybill:cmake-fetch-wrapper`, and 0 direct-only components do, in all three formats.

## Assumptions

- **Scope is the CMake fetch reader only.** `find_package` (#1591), Meson wraps (#1523) and TheRock's subproject topology (#1539) are out of scope.
- **Static only.** Nothing is evaluated by running CMake. A binding the static rules cannot make is unresolved, never guessed. As in spec 1538, CMake semantics that a rule depends on are probed with a real CMake in plan, and the probe is committed.
- **Definitions come from the same file set** spec 1538 reads fetches from: every CMake file the walker hands the reader. A wrapper defined in an `include()`d file outside the walked tree is not seen.
- **Mirror URLs stay as they are.** The upstream origin of a mirrored tarball is out of core (#1539). The component's download URL is the mirror's, and its identity is whatever the archive gives.
- **All measurement of TheRock is in CI,** through `kusari-sandbox/waybill-fixture-therock-sdk`. TheRock is never cloned or scanned locally.
