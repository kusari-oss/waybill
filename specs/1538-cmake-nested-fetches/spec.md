# Feature Specification: CMake fetches in nested projects, with standalone-build fallbacks marked

**Feature Branch**: `1538-cmake-nested-fetches`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1516 + #1519 — the CMake reader parses only the root CMakeLists.txt, cmake/, Modules/ and third_party/, so FetchContent_Declare / ExternalProject_Add calls in nested projects (third-party/, anything reached by add_subdirectory(), nested projects in a monorepo) are never seen; on a large C++ super-build repo report showed 6,161 CMake files matched and 6 components emitted from 137 FetchContent_Declare + 35 ExternalProject_Add calls. Parse every CMake file the shared walker hands the reader (or follow add_subdirectory() — decide which on evidence). Fixing discovery alone would flood documents with standalone-build fallbacks a super-build never takes (#1519): recognise the fallback shapes — a dependency provider (cmake_language(SET_DEPENDENCY_PROVIDER)), FETCHCONTENT_TRY_FIND_PACKAGE_MODE, FIND_PACKAGE_ARGS, if(NOT TARGET …) guards — and mark such inner fetches as declared-not-necessarily-built with the reason, rather than as built; when the super-project itself supplies a package of the same name, attach the inner declaration to that component (waybill:also-detected-via) instead of emitting a second. Source scanning can only mark the possibility; which fallbacks fired is build-tree evidence (#1544). Re-measure TheRock's counts at main in CI only via the fixture repo (never clone TheRock locally), and measure the public and quality corpora in CI. Synthetic waybill-fixture-* fixtures; zero new Cargo dependencies."

The spec number (1538) is the next in sequence. It is unrelated to issue #1538, which spec 1537 closed.

## Clarifications

### Session 2026-11-17

- Q: Which declaration kinds does widened discovery apply to? → A: Fetches only (`FetchContent_Declare`, `ExternalProject_Add`). Nested `find_package` and `pkg_check_modules` calls are counted on TheRock in CI, and a follow-up is filed with the numbers (FR-002, SC-008).
- Q: How is `FetchContent_Declare`'s first-declaration-wins rule treated? → A: As a fifth fallback shape. An inner `FetchContent_Declare` of a name an enclosing project also declares is merged into that component (C56), with the reason `first-declaration-wins`, guarded or not. Same-name declarations in sibling subprojects with no enclosing declaration stay separate components (FR-003, FR-005).
- Q: Does any enclosing `if()` mark a fetch conditional, or only the recognised fallback shapes? → A: Any enclosing `if()`, in every file including the root. Recognised shapes get their own reason. Any other condition gets the reason `conditional`, and the condition's text is recorded (FR-004, FR-010).
- Q: Does a fetch under `if(BUILD_TESTING)` get test scope? → A: Yes, as well as the conditional mark, including when `BUILD_TESTING` is `AND`-combined. Project-specific test options (`<PROJ>_BUILD_TESTS` and similar) are not matched by name, and stay `conditional` only (FR-008).

## Background

### What the reader reads today

The CMake reader reads a fixed set of files:
- the scan root's `CMakeLists.txt`;
- everything under `cmake/` and `Modules/`, recursively;
- `third_party/` at depth 1. It is read recursively only with `--cmake-third-party-recursive`.

The shared walker sees every CMake file, but it only decides whether the reader runs at all. It does not decide which files the reader parses.

### What that misses

A `FetchContent_Declare` or `ExternalProject_Add` anywhere else is never read. Examples:
- a `third-party/` directory (hyphen, not underscore);
- a subproject reached through `add_subdirectory()`;
- each project in a monorepo.

#1516 measured this on a large C++ super-build:
- `repo report` matched **6,161 CMake files**, and the reader emitted **6 components**;
- the tree holds **137 `FetchContent_Declare` and 35 `ExternalProject_Add` calls**.

Those figures were taken at an earlier waybill, and this feature re-measures them (SC-007).

### Why widening discovery alone is not enough

In a super-build, most inner fetches are fallbacks. A subproject declares them for when it is built **alone**. The super-build supplies the dependency instead, through:
- a dependency provider (`cmake_language(SET_DEPENDENCY_PROVIDER …)`);
- `FETCHCONTENT_TRY_FIND_PACKAGE_MODE`;
- `FIND_PACKAGE_ARGS` on the declaration;
- an `if(NOT TARGET …)` guard.

On the same tree, #1519 counted 172 inner fetch calls. Most are never taken in a full build, and googletest alone is declared in several places at different versions.

Emitting each of them as an ordinary component would turn a missing-dependency defect into an over-reporting one. A source scan cannot know which fallbacks fired; that is build-tree evidence (#1544). What it can do is say which declarations are conditional, and why.

### What already exists

- `find_package` and `pkg_check_modules` are read from the same file set (milestone 155). Since #1520, a `find_package` version is a requirement, not a resolution.
- Git submodules are components (#1515), and many nested CMake projects in a super-build are submodules.
- Spec 1537 established how a declaration finds its owner: the nearest enclosing project, then the document root.
- Existing vocabulary that may fit the new marker:
  - C57 `waybill:build-reference` (`declared-and-used` / `declared-only`);
  - C56 `waybill:also-detected-via`;
  - `waybill:build-inclusion` (`unknown` / `not-needed`);
  - the formats' native scope fields.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fetches declared in nested CMake projects appear in the SBOM (Priority: P1)

An engineer scans the source of a C++ project whose subprojects declare their own fetched dependencies, in `third-party/`, in an `add_subdirectory()` target, or in a sibling project of a monorepo. Every dependency those projects fetch appears as a component with its URL, pinned revision and declared hash, as a fetch in the root `CMakeLists.txt` does today.

**Why this priority**: This is the defect (#1516). The fetches a C/C++ project takes are its third-party code, and on the measured super-build almost none of them reached the SBOM.

**Independent Test**: Scan a fixture whose only fetch is declared in `third-party/fx/CMakeLists.txt`, reached by `add_subdirectory()` from the root. Check that one component for it is emitted, identical to the component the same declaration produces in the root file.

**Acceptance Scenarios**:

1. **Given** a fetch declared in `third-party/fx/CMakeLists.txt`, reached through `add_subdirectory(third-party/fx)`, **When** the tree is scanned, **Then** one component is emitted with the declaration's identity, URL and hash.
2. **Given** a monorepo with two sibling CMake projects, each declaring a different fetch, **When** the tree is scanned, **Then** both fetches are components.
3. **Given** a fetch declared in a file that only a non-CMake build would read, or in a directory the walker skips (vendored or excluded paths), **When** the tree is scanned, **Then** what is read follows the existing skip and exclusion rules, unchanged.

---

### User Story 2 - A standalone-build fallback is marked, not presented as built (Priority: P1)

The same engineer's subprojects guard their fetches: "use the super-build's copy if there is one, otherwise fetch". The SBOM lists those declarations as conditional, says which mechanism makes them conditional, and does not present them as dependencies the build necessarily took.

**Why this priority**: Without it, User Story 1 over-reports on exactly the trees it is for (#1519). The two issues must land together.

**Independent Test**: One fixture per fallback shape. In each, the inner fetch carries the conditional marker and a reason naming the shape. An unguarded control fetch in the same fixture carries neither.

**Acceptance Scenarios**:

1. **Given** an inner `FetchContent_Declare(… FIND_PACKAGE_ARGS …)`, **When** scanned, **Then** the component is marked conditional, with the reason `FIND_PACKAGE_ARGS`.
2. **Given** an inner fetch inside `if(NOT TARGET <t>)`, **When** scanned, **Then** it is marked conditional, with the guard as its reason.
3. **Given** a super-project that sets `FETCHCONTENT_TRY_FIND_PACKAGE_MODE` or installs a dependency provider, **When** scanned, **Then** fetches declared below it are marked conditional, with that mechanism as the reason.
4. **Given** an unguarded fetch with no provider in force, **When** scanned, **Then** it is not marked conditional.

---

### User Story 3 - One component when the super-project supplies the same package (Priority: P2)

When the super-project itself supplies a package, through its own fetch, a submodule, or a component another reader found, a subproject's fallback declaration of that package does not become a second component. It is recorded on the super-project's component as another place the package was detected.

**Why this priority**: The measured tree declares googletest in several places at different versions. Several components for one package that the build provides once make the inventory wrong, not merely long.

**Independent Test**: In a fixture, the root fetches `waybill-fixture-gtest` and a subproject guards its own fetch of the same package. Check that one component is emitted, and that it lists the subproject's declaration as an additional detection.

**Acceptance Scenarios**:

1. **Given** the root fetches package P and a subproject's guarded fallback also declares P, **When** scanned, **Then** P is one component, and the fallback appears in its additional-detection record.
2. **Given** the root `FetchContent_Declare`s P and a subproject declares P again with **no** guard, **When** scanned, **Then** P is one component, and the inner declaration is an additional detection with the reason `first-declaration-wins`.
3. **Given** two sibling subprojects that each declare P, with no enclosing declaration of P, **When** scanned, **Then** both stay components. A static scan cannot know their order.
4. **Given** a fallback for P at a different version than the super-project's, **When** scanned, **Then** P is still one component at the super-project's version. The fallback's version is preserved in the detection record, not as a second component.
5. **Given** a fallback for a package nothing else in the scan supplies, **When** scanned, **Then** it is emitted as its own component, marked conditional (User Story 2).

---

### User Story 4 - The super-build case measured in CI (Priority: P2)

The feature is checked against the tree that motivated it, a large C++ super-build (ROCm's TheRock), in CI only.

**Why this priority**: The defect's size was measured there, and so must its fix be. The tree must never be cloned locally.

**Independent Test**: Scan TheRock's source at waybill `main` and at the feature branch through the fixture repository's measurement workflow. Compare the fetch calls in the tree against the components, attachments and conditional marks in the output.

**Acceptance Scenarios**:

1. **Given** TheRock's source at its pinned revision, **When** scanned at the feature branch, **Then** every fetch call in the tree is accounted for: as a component, as an additional detection, or as skipped with a recorded reason.

### Edge Cases

- **A file reached by more than one route** (an `include()` and the walk) is read once. Its declarations are not duplicated.
- **A fetch in a test-only subtree** (`test/`, `tests/`, `unittests/`), or under `if(BUILD_TESTING)`, is attributed as a test dependency, not a runtime one.
- **A condition that combines `BUILD_TESTING` with `OR`** (`if(BUILD_TESTING OR FOO)`) does not imply a test-only fetch. It is `conditional` only.
- **A condition that cannot be read statically** (a variable set elsewhere, a generator expression) still marks the fetch conditional, with the reason `conditional` and the condition's text. It is never treated as unconditional.
- **A configuration switch rather than a fallback** (`if(WIN32)`, `if(ENABLE_FOO)`) also marks the fetch conditional (FR-004). That holds in the root file too, which changes today's root output by adding the marker.
- **A fetch inside a submodule's tree** belongs to that submodule (spec 1537's ownership rule). A fetch with no enclosing project belongs to the document root.
- **A CMake file that does not parse** is skipped with a warning naming its path. It never aborts the scan.
- **Very large trees.** The measured super-build has 6,161 CMake files. Reading all of them must not make the scan impractical (SC-006).
- **A subproject that is itself fetched** (its source arrives by `FetchContent`, so is not in the scanned tree) is not followed. Source scanning reads only what is on disk.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A `FetchContent_Declare` or `ExternalProject_Add` in any CMake file of the scanned project MUST be read, not only in the root, `cmake/`, `Modules/` and `third_party/`. Which files belong to "the scanned project" is decided by evidence (research). The two candidate rules are "every CMake file the walker finds" and "files reachable through `add_subdirectory()`/`include()` from the root". The existing walker skip set and `--exclude-path` still apply.
- **FR-002**: Widened discovery MUST apply to `FetchContent_Declare` and `ExternalProject_Add` only.
  - `find_package` and `pkg_check_modules` stay on today's file set, so their output is unchanged.
  - The measurement task MUST also count the `find_package` and `pkg_check_modules` calls in TheRock's nested CMake files, which this feature does not read, and file a follow-up issue with those numbers. Whether to read them is decided there, on evidence.
  - The reason: since #1520, a `find_package` names a minimum version, not a resolution, and in a super-build many such calls are satisfied by the super-build's own targets. That is #1519's over-reporting problem, with no measured volume behind it yet.
- **FR-003**: A fetch whose declaration matches a recognised fallback shape MUST be marked as declared-but-not-necessarily-built, with a reason naming the shape. The shapes are:
  - `FIND_PACKAGE_ARGS` on the declaration;
  - an enclosing `if(NOT TARGET …)` guard;
  - `FETCHCONTENT_TRY_FIND_PACKAGE_MODE` set by an enclosing project;
  - a dependency provider installed by an enclosing project;
  - **first declaration wins:** a `FetchContent_Declare` of a name an enclosing project also declares. CMake ignores every declaration of a name after the first, guarded or not, and a project's own declarations normally precede its `add_subdirectory()` calls. "Earlier" is approximated statically as "declared by an enclosing project", and the reason value says so. This shape does not apply to `ExternalProject_Add`, where a duplicate target name is an error, and it does not apply between sibling subprojects, whose order a static scan cannot know.

  The marker MUST use an existing waybill row or a format-native field where one fits, and add a new row only if none does (Constitution Principle V). The same marker MUST appear in all three formats.
- **FR-004**: A fetch inside **any** enclosing `if()` MUST be marked conditional, in every CMake file, the root included.
  - A recognised shape (FR-003) gives its own reason.
  - Any other condition, such as `if(WIN32)`, `if(ENABLE_FOO)` or a condition the reader cannot evaluate, gives the reason `conditional`, with the condition's text recorded.
  - An unrecognised condition MUST NOT be read as "unconditional".
- **FR-005**: When the scan already holds a component for the package a fallback declares (including a `first-declaration-wins` declaration, whose package is the enclosing project's same-name fetch), the declaration MUST be recorded on that component as an additional detection (C56), not emitted as a second component. "The scan holds" includes a super-project fetch, a git submodule, or another reader's component. Matching MUST use the package's identity (URL or repository), not its name alone, so that two packages that share a name are not merged.
- **FR-006**: A fallback for a package nothing else supplies MUST be emitted as its own component, marked per FR-003.
- **FR-007**: Each nested fetch MUST be a dependency of the project that declares it, by spec 1537's ownership rule. That is the nearest enclosing project component (main module or git submodule), and otherwise the document root. All three formats MUST agree on the root's direct targets (invariant I-1).
- **FR-008**: A fetch MUST carry the test lifecycle scope in either of two cases:
  - it is declared under a test-only directory (`test`, `tests` or `unittests`, the names #1524 uses);
  - it is inside `if(BUILD_TESTING)`, alone or `AND`-combined with other conditions. `BUILD_TESTING` is CTest's standard switch.

  It also keeps the conditional mark (FR-004). Project-specific test options (`<PROJ>_BUILD_TESTS`, `ENABLE_TESTS` and similar) are NOT matched by name; such a fetch is `conditional` only. That follows the test-scope rules of #1564 and #1524, which do not guess from names.
- **FR-009**: Fetch declarations that are counted but not emitted MUST be counted in the document, by reason, so that "accounted for" in SC-002 is checkable from the document alone. Examples are unparseable files, fetches of a subproject that is not on disk, and declarations merged by FR-005.
- **FR-010**: Output for a tree whose fetches are all in the root, `cmake/` and `Modules/` MUST be unchanged, except for two additions: the conditional marker on fetches inside an `if()` (FR-004), and this feature's document-scope rows.
- **FR-011**: No new Cargo dependency.
- **FR-012**: `--cmake-third-party-recursive` MUST still be accepted. Its behaviour becomes a subset of the default. Removing the flag is a separate decision.

### Key Entities

- **Fetch declaration**: one `FetchContent_Declare` or `ExternalProject_Add` call. It has a declaring file, an identity (URL or repository), a pinned revision or version, and a declared hash.
- **Fallback shape**: the reason a declaration is conditional. One of the five recognised shapes (four guards, plus first declaration wins), or `conditional` for any other enclosing `if()`, recorded with the condition's text.
- **Declaring project**: the component a fetch is a dependency of (FR-007).
- **Additional detection**: a fallback declaration recorded on an existing component instead of emitted as a second one (FR-005).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For each nested shape in User Story 1, a synthetic fixture's nested fetch is emitted with the same identity, URL and hash as the same declaration in the root file. That is 100% of shapes.
- **SC-002**: On TheRock's source tree, measured in CI only, every `FetchContent_Declare` and `ExternalProject_Add` call in the tree is accounted for in the document at the feature branch. Each is a component, an additional detection, or counted with a reason. **Zero are silently dropped.** The baseline count at `main` is measured first.
- **SC-003**: Every declaration that matches a recognised fallback shape carries the conditional marker with its reason. On the fixtures, 0 do not. On TheRock, the number marked and the number unmarked are both recorded, by shape.
- **SC-004**: No package appears as more than one component because of a fallback declaration (FR-005). Checked on the fixtures and on TheRock: the number of distinct package identities equals the number of fetch-derived components.
- **SC-005**: On the public corpus, every changed golden is explained by FR-001 to FR-009. On the quality corpus, every band that moves is re-authored with its cause, as spec 1537 did.
- **SC-006**: Scanning TheRock's source takes no more than 1.5× the time `main` takes, as a median of 3 runs of each in the same CI job.
- **SC-007**: The #1516 figures (6,161 files, 6 components, 137 + 35 calls) are re-measured at `main` before design decisions rest on them, and the measurement is committed.
- **SC-008**: TheRock's nested `find_package` and `pkg_check_modules` calls (those outside today's file set) are counted in CI, and a follow-up issue is filed with the counts (FR-002).

## Assumptions

- Source scanning cannot know which fallbacks a build takes. This feature marks the possibility only, and resolving it is #1544's build-tree work.
- The five fallback shapes cover the super-build patterns that matter. Any others found while measuring TheRock are recorded, and added only if they are standard CMake. Project-specific wrapper functions are #1518 and #1545, not this feature.
- A fetch's identity for FR-005 is its URL or repository, normalised as the existing reader already does (lowercased `pkg:github` owner and repository, #1515).
- TheRock is measured in CI only, through the fixture repository (`kusari-sandbox/waybill-fixture-therock-sdk`), and is never cloned locally.
- Fixtures use synthetic `waybill-fixture-*` names.
