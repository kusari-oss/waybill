# Feature Specification: Read a CMake build tree to settle what a build actually used (#1535)

**Feature Branch**: `1652-cmake-build-tree`
**Created**: 2026-11-24
**Status**: Draft
**Input**: #1535, the first spec under tracker #1544. Read a CMake build tree after a build, to settle what source scanning can only mark as possible: which fetch fallbacks fired, which optional dependencies were configured in, and which external projects were actually downloaded. Refs #1519 (closed by spec 1538), #1540 (TheRock's own build-tree files, out of scope), #1537 (Dockerfiles, out of scope), #1656, #1463.

## Clarifications

### Session 2026-11-24

- Q: How is a build tree supplied? → A: Explicitly and repeatably. The operator names one or more build-tree paths, and every CMake build directory beneath each is read. A `CMakeCache.txt` inside the scanned tree switches nothing on by itself, so source-only scans stay byte-identical (SC-004), and TheRock's per-subproject and per-CI-stage build directories can be merged in one scan (FR-001, FR-002).
- Q: Does compile evidence change a component's scope? → A: No. An include path that resolves into a known component adds an edge and compile evidence only. Scope still comes from the declaration and its settlement (FR-004). Being compiled against does not say whether code ships, and no new compile-only scope is introduced (FR-009).
- Q: What happens to a component the build tree evidences but the source does not declare? → A: It is emitted in scope and marked as known only from the build tree. The build fetched it, so it is an input to what was built, and over-reporting is the safer error (spec 1641 FR-004). The marker lets a consumer see that build evidence is its only basis (FR-007).
- Planning amendment (research R1–R3, measured on 24 probe build trees): FR-003's signals are replaced by the measured ones. FetchContent is settled from the cache and is visible after configure alone. ExternalProject is settled by its `<name>-download` stamp, found by file name because `PREFIX`/`STAMP_DIR` move it. The stamp directory alone is never *taken*, because configure creates it.
- Planning amendment (research R4): ExternalProject/FetchContent info files (`*-urlinfo.txt`, `*-gitinfo.txt`) are not read. Their header says the format is internal and unsupported, and identity already comes from the source declaration. A build-only component therefore carries no URL or hash.
- Planning amendment (research R6): SC-001(a) is done. SC-001(b), a partial TheRock build, is the first implementation task, and its outcome may amend R1–R3 and R7.

## Background

Source scanning can only say what a CMake tree *might* fetch. Spec 1538 (#1519) marks a fetch the super-build probably does not take with one of six fallback shapes in C122 `waybill:optional-derivation` (`cmake-find-package-args`, `cmake-not-target`, `cmake-find-package-fallback`, `cmake-try-find-package-always`, `cmake-dependency-provider`, `cmake-function-body`). Spec 1641 (#1639) evaluates enclosing conditions against *defaults* and records them in C232 `waybill:cmake-fetch-conditions` and C236 `waybill:cmake-find-conditions`. Both are statements about possibility. Neither can be confirmed or refuted from source.

A build tree records the outcome. CMake leaves behind, per configured project, a cache of the values actually configured, a directory per FetchContent population, stamp files per ExternalProject step, and optionally a compilation database and a file-API reply. Reading those needs no injection into the build. Binaries cannot supply this: header-only libraries and build-only dependencies leave no trace in them (#1543's stated limit).

**What waybill does today** *(read from the code at `main` `317fbae6`; to be re-verified in research)*:
- A directory scan never descends into `build/`, `out/` or any dot-directory outside a short CI/docs list (`should_skip_default_descent`, `waybill-cli/src/scan_fs/package_db/project_roots.rs:38`; spec 1613's `LISTED_DOT_DIRECTORIES`). A build tree under `build/`, and the file API under `.cmake/`, are therefore never reached.
- Milestone 109's observer (`waybill-cli/src/scan_fs/binary/source_binding/cmake_observer.rs`) recognises a build directory by `CMakeCache.txt` plus `_deps/`, but only joins FetchContent declarations, and only to attribute binary fingerprint matches. It states that ExternalProject was deferred.
- Nothing reads `CMakeCache.txt` values, ExternalProject stamps, `compile_commands.json` or the file API.

**What is not yet known.** #1535 and #1544 both say their file lists were read from build-system sources, not observed in a build. Which files exist, at what size, under which CMake versions and generators, and how they look in a super-build that builds each subproject in its own tree, are all unmeasured. Per CLAUDE.md ("Measure external behaviour before designing around it"), this spec fixes behaviour only in terms of observations a research task will make (SC-001), and every number it will carry must trace to that observation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A fallback's outcome is stated, not guessed (Priority: P1)

A team scans the source tree of a CMake project together with the build tree it produced. For every fetch declaration that source scanning marks as a fallback or as conditional, the SBOM says whether this build took it, and on what evidence. A fallback that fired is reported as a dependency of the build; one that did not stays optional, now with evidence that it was not taken.

**Why this priority**: This is the question #1519 could only half-answer from source. On a super-build like TheRock, most of the fetches inside submodules are fallbacks; which of them actually ran decides what ships.

**Independent Test**: A small CMake project with a FetchContent declaration that has `FIND_PACKAGE_ARGS` and an `ExternalProject_Add`, configured and built twice: once with the dependency installed on the system, once without. Scanning each source+build pair gives *taken* for the build that fetched and *not taken* for the one that found the system package, in all three formats.

**Acceptance Scenarios**:

1. **Given** a FetchContent fallback that the build populated, **When** the source and build tree are scanned, **Then** the component is in scope, carries build evidence naming the population, and no longer carries the fallback's C122 value as its reason for being optional.
2. **Given** the same declaration in a build that found the system package instead, **When** scanned, **Then** the component stays optional with its spec-1538 C122 value, and its build evidence says *not taken*.
3. **Given** an `ExternalProject_Add` whose download step completed, **When** scanned, **Then** the component is in scope with build evidence naming the completed step.
4. **Given** a scan of the same source tree with no build tree supplied, **When** scanned, **Then** the output is byte-identical to `main`.

---

### User Story 2 - Conditions are evaluated against the configuration that was built (Priority: P2)

Spec 1641 evaluates `if()` chains against defaults, and leaves a chain *unknown* when a value is a cache string (#1656) or is overridden on the command line. With a build tree, the configured values are known. A fetch or package search gated on `BLAS STREQUAL MKL` is in scope only if the build configured `BLAS=MKL`.

**Why this priority**: It reuses spec 1641's evaluator and turns most *unknown* chains into decided ones for the build being described, but it is only meaningful once User Story 1 establishes how a build tree is supplied.

**Independent Test**: The same project configured with `-DBLAS=MKL` and with `-DBLAS=OpenBLAS`, each fetching under `if(BLAS STREQUAL <name>)`. Each scan puts exactly the configured backend in scope.

**Acceptance Scenarios**:

1. **Given** a fetch under `if(X)` where the source default of `X` is OFF but the build configured it ON, **When** source and build are scanned, **Then** the fetch is in scope and its evidence names the configured value.
2. **Given** a fetch under `if(BLAS STREQUAL MKL)` and a build configured with another value, **When** scanned, **Then** the fetch is optional with C122 `cmake-conditional`.
3. **Given** a condition the configured values still cannot decide, **When** scanned, **Then** it is treated as spec 1641 FR-004 treats unknown: kept in scope.

---

### User Story 3 - Header-only and build-only dependencies appear (Priority: P3)

A compilation database lists every translation unit with its include paths. An include directory that lies inside a fetched dependency's source, an external project's source, a submodule or an installed package prefix shows that the project compiled against it, including header-only libraries that leave no trace in a binary.

**Why this priority**: It closes the gap #1543 states as a limit of binary analysis, but depends on whether real builds leave a compilation database (it is opt-in in CMake), which SC-001 must measure first.

**Independent Test**: A fixture build with `CMAKE_EXPORT_COMPILE_COMMANDS=ON` that compiles against a header-only fetched dependency. The scan gives an edge from the compiling project to that dependency, with compile evidence.

**Acceptance Scenarios**:

1. **Given** a compilation database whose include paths reach a fetched header-only dependency, **When** scanned, **Then** the dependency has an edge from the project that compiled against it.
2. **Given** a build without a compilation database, **When** scanned, **Then** nothing about compile edges is claimed, and the document records that the evidence was absent.

---

### User Story 4 - The file API is used when present (Priority: P4)

When a CMake file-API reply is present in the build tree, its target, link and cache data are used as the most precise evidence available for the configured build.

**Why this priority**: Most valuable evidence, least likely to exist: a reply is written only if a query was placed before configure. Whether waybill should document that step, or offer to write the query itself, is a decision this spec records only after SC-001 shows how often replies exist in practice.

**Independent Test**: A fixture build with a file-API query placed before configure. The scan's link edges agree with the reply's target link data.

**Acceptance Scenarios**:

1. **Given** a build tree with a codemodel and cache reply, **When** scanned, **Then** configured values come from the reply in preference to `CMakeCache.txt`, and link edges agree with the reply.

### Edge Cases

- **Build tree without a matching source tree:** the build tree records a FetchContent population or ExternalProject download whose declaration is not in the scanned source. It is emitted, marked as known only from the build tree (FR-007), never dropped.
- **Source tree with several build trees:** a super-build configures each subproject in its own build directory, and CI stages split one build across machines (#1540). Every CMake build directory beneath each supplied build-tree path is read, and evidence merges on component identity (FR-002).
- **Partial or failed build:** a population directory exists but its stamp says the step did not complete, or the build was interrupted. A step that did not complete is not *taken*; the evidence says *incomplete*, and the component keeps its source-side scope (FR-008).
- **Stale build tree:** the build tree was configured from a different revision of the source. Evidence that names declarations the source no longer has follows the first edge case; where the build tree's recorded source directory does not lie within the scanned source tree, the mismatch is recorded at document scope.
- **A pre-populated source** (`FETCHCONTENT_SOURCE_DIR_<name>`, or `FETCHCONTENT_FULLY_DISCONNECTED`): the build used a local copy, not a download. The evidence says so and names the override, without a host path (FR-006).
- **Unreadable or truncated files** in the build tree: they are counted, never fatal, and never cause a component to be dropped (FR-008).
- **Absolute host paths** are everywhere in a build tree: cache values, compile commands, stamp contents. None reaches the SBOM unredacted (FR-006).
- **Very large build trees:** a full TheRock build is many gigabytes. Reading is bounded to the metadata files this spec names; object files, archives and binaries in the build tree are not opened by this feature.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (supplying the build tree)**: An operator MUST be able to name one or more build-tree paths for a source scan. Without one, a scan MUST behave exactly as today, including skipping `build/` and dot-directories, and produce byte-identical output. A `CMakeCache.txt` found inside the scanned tree MUST NOT by itself switch build-tree reading on (Clarifications).
- **FR-002 (discovery within a build tree)**: Beneath each named build-tree path, every CMake build directory (one holding a `CMakeCache.txt`) MUST be found and read. Evidence from several build directories MUST merge on component identity; when they disagree about whether a fetch was taken, *taken* wins, and the disagreement is counted.
- **FR-003 (fallback settlement)**: For every CMake fetch declaration, the build evidence MUST be classified as *taken*, *not taken*, *incomplete*, or *no evidence*:
  - FetchContent: the cache records the population (`<name>_SOURCE_DIR` or `FETCHCONTENT_SOURCE_DIR_<NAME>`), or `_deps/<name>-src/` exists → *taken*; none of those, and the cache has a `<name>_DIR` that is not FetchContent's own redirect → *not taken*.
  - ExternalProject: a `<name>-download` stamp exists, wherever `PREFIX`/`STAMP_DIR` put it → *taken*; a stamp directory without it → *incomplete*.
  - The exact rules are [contracts/settlement.md](contracts/settlement.md), measured in research R1–R3.
- **FR-004 (scope from settlement)**: A declaration that is *taken* MUST be in scope, whatever its spec-1538 fallback shape or spec-1641 condition. A declaration that is *not taken* keeps its source-side scope. *Incomplete* and *no evidence* MUST leave the source-side scope unchanged. Test scope (spec 1538 R7) is unaffected.
- **FR-005 (configured values)**: Values in each build directory's cache MUST be supplied to spec 1641's condition evaluator as the configured values, taking precedence over source defaults, for fetches and package searches whose declaring directory belongs to that build. Boolean, string and path values are read; a comparison (`STREQUAL`, `MATCHES`, version comparisons) whose operands are now known MUST be evaluated. A chain that is still undecided remains *unknown* and in scope (spec 1641 FR-004).
- **FR-006 (no host paths)**: No absolute path from the build tree or the host MUST appear in the emitted SBOM. Paths in evidence MUST be relative to the named build-tree root or to the scan root; a path outside both is replaced by a fixed marker. Cache values that are paths are never emitted. (cf. #1463.)
- **FR-007 (build-only components)**: A component evidenced only by the build tree, with no matching source declaration, MUST be emitted, in scope, and marked as known only from the build tree (Clarifications).
- **FR-008 (failure handling)**: Missing, unreadable or malformed build-tree files MUST NOT fail the scan, drop a component, or change a component's source-side scope. They MUST be counted in a document-scope coverage record.
- **FR-009 (compile edges)**: When a compilation database is present, an include directory that lies within a fetched dependency's population, an external project's source, a git submodule or an installed package prefix known to the scan MUST give an edge from the compiling project to that component, with compile evidence. Compile evidence MUST NOT on its own change a component's scope, and no compile-only scope is introduced (Clarifications).
- **FR-010 (file API)**: When a file-API reply is present, its cache and codemodel data MUST be preferred over `CMakeCache.txt` and the compilation database for the build it describes. waybill MUST NOT write a query into the build tree in this spec; whether to document or automate the query is recorded in research after SC-001.
- **FR-011 (records)**: Build evidence MUST be carried per component (the settlement state and its basis) and summarised at document scope (build directories read, declarations per settlement state, files unreadable, components known only from the build tree). Native fields MUST be used where CycloneDX, SPDX 2.3 and SPDX 3 have one (Principle V audit); otherwise new catalog rows, starting at the next free number (C244 at the time of writing), with extractors in all three formats.
- **FR-012 (parity and validity)**: The three formats MUST agree (`waybill sbom parity-check`), CycloneDX MUST validate against the vendored 1.6 schema, and repeat scans MUST give byte-identical output.
- **FR-013 (bounded reading)**: Only the metadata files this spec names MUST be read from a build tree. Object files, archives, binaries and downloaded source archives in the build tree are out of scope for this feature.
- **FR-014 (documentation)**: The new flag, the settlement states and the new catalog rows MUST be documented in the CLI reference, `docs/reference/sbom-format-mapping.md` and the consumer guide.
- **FR-015**: No new Cargo dependencies. Rust stable pinned by `rust-toolchain.toml`; `waybill-ebpf` untouched.

### Key Entities

- **Build tree**: an operator-named directory holding one or more CMake build directories, produced by configuring (and usually building) the scanned source.
- **Build directory**: one configured CMake project inside a build tree, identified by its cache file; it records the source directory it was configured from.
- **Fetch declaration**: a `FetchContent_Declare`, `ExternalProject_Add`, wrapper or `download_project()` call the source scan already emits (specs 1538–1540).
- **Settlement**: a declaration's build outcome: *taken*, *not taken*, *incomplete*, *no evidence*, with the basis (which build directory, which evidence kind).
- **Configured value**: a cache entry's value in one build directory, which overrides the source default for conditions in that build.
- **Compile evidence**: an include path from a compilation database that resolves into a known component.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (measure first)**: Before any requirement's file list is fixed, a research task captures real build trees and commits the probe beside this spec (`specs/1652-cmake-build-tree/measurements/`). At minimum: (a) the small two-configuration fixture of User Story 1, with and without the dependency installed, under at least two CMake versions and two generators; (b) a feasibility attempt at a partial TheRock build from `kusari-sandbox/TheRock` at pin `b1cae390`, in CI in `kusari-sandbox/waybill-fixture-therock-sdk`, never locally. It records which files exist, their count and size, and how each settlement state shows. If (b) is infeasible on a hosted runner, that is recorded with the reason and the spec proceeds on (a) plus any other public CMake super-build chosen in research.
- **SC-002**: On every captured fixture, each fetch declaration's settlement agrees with a hand-checked list for the build, with zero disagreements.
- **SC-003**: Every acceptance scenario has a fixture that gives the stated result in all three formats; on `main`, each changed fixture gives the source-only result (the control).
- **SC-004**: Source-only scans (no build tree named) of the public corpus, the quality corpus and TheRock's source tree are byte-identical to `main`.
- **SC-005**: On the TheRock build fixture, if SC-001(b) succeeds, a census reports, before and after, how many fallback and conditional components the build settles, by state, and how many of them are versioned. No figure is predicted before the census runs.
- **SC-006**: No emitted document on any fixture contains an absolute path from the build host; a test asserts this over every string in every format.
- **SC-007**: Removing any one settlement rule (FetchContent taken, FetchContent not taken, ExternalProject taken, incomplete, configured-value precedence, build-only marking) makes at least one test fail.
- **SC-008**: Scanning a source tree together with its build tree takes at most 1.10× the source-only scan time on the SC-001 fixtures, median of at least 3 runs; build-tree reading time is reported separately.

## Assumptions

- **Read-only.** Nothing is injected into, or written into, the build or the build tree.
- **The SBOM describes this build.** With a build tree named, configured values describe the build that was made, not the tree's defaults. Without one, spec 1641's default-configuration semantics are unchanged.
- **Over-reporting is the safer error** (spec 1641 FR-004): missing or ambiguous build evidence never narrows scope.
- **Out of scope:** TheRock-specific build files (`.smrev`, `artifact-*.toml`, stage directories; #1540, which belongs to the cataloger work in #1545); Dockerfiles and build-environment components (#1537); scanning archives ExternalProject downloaded; linker-launcher capture of linked static archives; `waybill trace`.
- **Census pattern.** Measurement follows specs 1639–1641: probe scripts committed with the spec, TheRock scanned only in CI in the fixture repository.
