# Feature Specification: A component for each CMake project the scanned tree builds

**Feature Branch**: `1618-cmake-project-components`
**Created**: 2026-11-20
**Status**: Draft
**Input**: #1616. A CMake project built in the scanned tree gets no component. When one project in a monorepo `find_package()`s another there is nothing to draw the edge to, and vendored code that no other reader finds is invisible.

## Background

Spec 1614 (#1591, #1605) reads package searches (`find_package`, `pkg_check_modules`, `pkg_search_module`) from every CMake file and pairs each with the component it names, in this order: a fetch, an installed-library record (spec 1531), a git submodule. A name **the tree builds itself** comes last. Such a name is:
- a `project(<name>)`;
- a `<name>Config.cmake.in` or `<name>-config.cmake.in`;
- a `<name>Config.cmake` with no version file beside it.

By spec 1614's FR-007, a search for such a name yields **no component and no edge**, and the name is listed in C237 (`waybill:cmake-find-coverage`) under `in-tree`.

Ownership follows the nearest enclosing *project*: a main module of any ecosystem, or a git submodule. That rule decides where CMake fetches, links and searches attach, and where pip requirements files attach. CMake has no project component, so `docs/ecosystems.md` records that "a CMake-only directory owns nothing".

**Measured on ROCm TheRock `b1cae390`** (with submodules; fixture repo `census/includes.py`, CI only). Of the 225 names the tree's `find_package` calls search for:
- 63 have a `project(<name>)` in the tree;
- 25 more have a `<name>Config.cmake[.in]` the tree installs.

Examples: hip, rocblas, rocprofiler-sdk and amd_comgr, and vendored gtest, zstd and sqlite3. Every one of those searches is recorded as satisfied in-tree, and none produces an edge.

**Measured on a synthetic tree** (waybill `main` `786091a8`, local). The tree is a top-level `project(waybill-fixture-top VERSION 1.2.3)` that does `add_subdirectory(libb)`, a `libb/` with `project(waybill-fixture-libb VERSION 0.4.0)`, and a top-level `find_package(waybill-fixture-libb)` and `find_package(ZLIB)`. The output:
- the document root is the scan directory's name at `0.0.0`, not `waybill-fixture-top@1.2.3`;
- the only component is `pkg:generic/zlib`;
- `waybill-fixture-libb` appears only in C237 `in-tree`.

The installed-library reader (spec 1531) names records `pkg:generic/<lowercased name>@<version>`.

## Clarifications

### Session 2026-11-20

- Q: Is an in-tree CMake project a main module, joining root selection and override demotion, or a project that owns without competing for the root? → A: A main module. Where a directory also holds another ecosystem's main module, the CMake project has the lowest precedence, so it neither displaces that module as root nor takes its ownership.
- Q: What happens to projects only under test, example or benchmark directories, and to projects the root build does not reach? → A: All are emitted. Projects under a test directory are Test (C62 `source-test-directory`). Projects under an example or benchmark directory are Development. A project the root build does not reach is emitted with an annotation saying so.
- Amended during planning (research R2): Q1's lowest precedence applies to the whole candidate set. CMake projects are root candidates only when no main module of any other ecosystem exists. Same-directory precedence alone would change the recorded root-selection heuristic, and in some layouts the root itself, for any repository with one non-CMake main module and CMake subprojects.
- Amended during planning (research R4): searches do not set an in-tree project's scope. Like a shipped library (spec 1617), a project the tree builds is not lowered by a consumer's conditional search. Such a search adds C236 only, and a project found only by test-context searches, with no directory-derived scope, is Test.
- Amended during planning (research R6): searches paired with a project component are counted in a new C237 key, `paired.project`. `in-tree` keeps the names satisfied only by config templates or installed config files, which still have no component. The key is emitted only when non-zero, so a scan with no project components keeps C237's bytes (FR-012).
- Amended during planning (research R7): C151 `waybill:unresolved-reason` is emitted only on design-tier components, and project components are source tier. So an unresolved `VERSION ${X}` is recorded in a new row, C238 `waybill:declared-version-expression`, holding the expression verbatim. "No version declared" extends C197 with `not-declared`.
- Amended after analysis: the vendored classification is recorded as C240 `waybill:vendored = true` (an existing, uncatalogued key) when the measured rule holds. First-party is its absence. C241 records the signals either way. If no rule reaches the precision bar (research R10), no verdict is emitted.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A monorepo's projects and the edges between them (Priority: P1)

A consumer scans a CMake monorepo, such as ROCm TheRock, in which one project finds another with `find_package`. They want each project the tree builds to be a component, with a dependency edge from the searching project to the project it finds. Then the document shows which of the repository's own libraries each library needs.

**Why this priority**: This is the defect. Today 63 of TheRock's 225 searched names are projects in the same tree, and the document draws none of those edges.

**Independent Test**: Scan a two-project fixture in which project A `find_package`s project B. Check that each format has a component for A and for B, and an edge A → B carrying the search's scope.

**Acceptance Scenarios**:

1. **Given** a tree where `a/CMakeLists.txt` declares `project(waybill-fixture-a VERSION 1.0.0)` and calls `find_package(waybill-fixture-b REQUIRED)`, and `b/CMakeLists.txt` declares `project(waybill-fixture-b VERSION 2.0.0)`, **When** it is scanned, **Then** the document has a component for each project, with its version, and an edge from `waybill-fixture-a` to `waybill-fixture-b`.
2. **Given** the same search made conditionally (inside `if(…)`), **When** it is scanned, **Then** the edge exists, `waybill-fixture-b` records the conditions (C236), and its scope still comes from its own directory. A project found only from test directories is Test (C62).
3. **Given** a project that no other project searches for, **When** it is scanned, **Then** it is still reachable from the document root, through the project that encloses it or through the root.

---

### User Story 2 - Vendored code is visible and marked as vendored (Priority: P1)

A consumer scans a tree that carries third-party code as CMake projects, such as gtest, zstd or sqlite3 under a `third_party/` directory. No lockfile or manifest names that code, so no other reader finds it. They want it in the document, marked so that it can be told apart from the repository's own projects.

**Why this priority**: Vendored copies are what vulnerability management most needs to see, and today nothing reports them.

**Independent Test**: Scan a fixture with `third_party/waybill-fixture-vendored/CMakeLists.txt` declaring a project. Check that the component exists, and that it carries the evidence of why it was judged vendored.

**Acceptance Scenarios**:

1. **Given** a project under a directory the classification treats as vendored, **When** it is scanned, **Then** the component carries a recorded classification and the signal or signals that produced it.
2. **Given** a first-party project, **When** it is scanned, **Then** it carries no vendored classification.
3. **Given** a project whose name equals a fetch, an installed-library record or a git submodule already in the document, **When** it is scanned, **Then** no second component is created. Spec 1614's one-identity order applies, and the project's directory joins the existing component's evidence.

---

### User Story 3 - A CMake-only repository describes itself (Priority: P2)

A consumer scans a repository whose only build system is CMake. Its top-level `CMakeLists.txt` declares `project(<name> VERSION <v>)`. They want the document's subject to reflect that project, rather than the scan directory's name at `0.0.0`.

**Why this priority**: It follows from US1, but it changes the root of every CMake-only scan, which is a larger behavioural change than adding components.

**Independent Test**: Scan a fixture whose only manifest is a top-level `CMakeLists.txt` with `project(waybill-fixture-top VERSION 1.2.3)`, and check the document root in each format.

**Acceptance Scenarios**:

1. **Given** a CMake-only tree with a top-level `project(waybill-fixture-top VERSION 1.2.3)`, **When** it is scanned with no root override, **Then** the document root is `pkg:generic/waybill-fixture-top@1.2.3`, in all three formats.
2. **Given** the same tree scanned with `--root-name`, **When** the override is active, **Then** the project behaves as other manifest-derived projects do under an override: demoted and anchored to the override root (milestone 860), or folded into it when it names the same coordinate (#1626).
3. **Given** a tree whose scan-root directory holds both a CMake project and another ecosystem's main module (for example a `pyproject.toml`), **When** it is scanned with no root override, **Then** the root is the same as on `main`, and the other module keeps the directory's ownership.

---

### Edge Cases

- **No version.** `project(<name>)` with no `VERSION`, or with `VERSION ${VAR}`, yields a component without a version. The two cases must be distinguishable in the document: no version was declared, or a version was declared but not resolved.
- **The same name in two directories.** One component per distinct (name, version). Two directories declaring the same name and version form one component, with both directories in its evidence. Different versions form two components. A declaration with no version, or with an unresolved one, merges into a versioned declaration of the same name; with no versioned declaration, all such declarations form one versionless component.
- **Several `project()` calls in one file, or `project()` inside `function()` / `macro()` bodies.** Only the calls the CMake reader already reads as project declarations count, at the top level of a listfile. A call with a variable name (`project(${X})`) yields nothing, as today.
- **Nested projects.** A project inside another project's directory is enclosed by it for ownership.
- **A project directory that is also a git submodule.** If the submodule's directory name equals the project's name, the submodule is its identity and no project component is emitted (FR-005). Otherwise the project is a separate component, enclosed by the submodule (FR-006).
- **A project named like an installed-library record or a fetch.** No project component is emitted; its declaring file joins that component's evidence (FR-005).
- **Projects only under test, example or benchmark directories.** Emitted. Under a test directory (the names spec 1615 and C62 use), the scope is Test with C62 `source-test-directory`. Under an example or benchmark directory, the scope is Development, with the derivation recorded. The research fixes the directory names from the measured trees.
- **Projects not reached from the root build** through `add_subdirectory`/`include` (spec 1614's reach). Emitted, with an annotation saying the root build does not reach them. Their scope is unchanged by that.
- **Excluded directories.** A `project()` under a directory `--exclude-path` names, or under the walker's skip set, yields nothing.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan MUST emit one component for each distinct CMake project the tree declares with a literal `project(<name> …)` call in a listfile the CMake reader reads, subject to FR-009. Projects under test, example or benchmark directories, and projects the root build does not reach, are emitted too (FR-015, FR-016).
- **FR-002**: The component's identity MUST be `pkg:generic/<lowercased name>`, with `@<version>` when the declaration gives a literal version. This is the same form as the installed-library record (spec 1531) that the project's build produces, so a source scan and an installed-tree scan name the same thing.
- **FR-003**: A project without a literal version MUST carry a marker that distinguishes "no version declared" from "a version declared through a variable that was not resolved". "No version declared" MUST be C197 `waybill:version-absence` with a new value, `not-declared`. "Declared through an unresolved variable" MUST be a new row, C238 `waybill:declared-version-expression`, holding the expression verbatim. C151 cannot carry it: C151 is emitted only on design-tier components, and project components are source tier (research R7). A `VERSION ${X}` that the declaring file's own literal `set(X …)` resolves is a literal version.
- **FR-004**: A package search whose name is satisfied by an in-tree project MUST yield a dependency edge from the search's owning project (or the document root when no project owns it) to that project's component. The project's scope comes from its directory (FR-015). A conditional search adds its conditions as C236 and never makes the project Optional. A project with no directory-derived scope that is found only by test-context searches is Test, with that derivation in C62 (research R4).
- **FR-005**: Spec 1614's one-identity order MUST hold, with in-tree projects last: fetch, installed-library record, git submodule, in-tree project. A project whose name an earlier entry claims MUST NOT become a second component; its declaring file MUST join that component's evidence.
- **FR-006**: Every project component MUST be reachable from the document root: through an edge from a searching project (FR-004); otherwise through an edge from its nearest enclosing project; otherwise from the document root. No project component may be left unreachable without an orphan reason.
- **FR-007**: An in-tree project MUST be a main module. It therefore takes part in:
  - **the ownership index:** it owns the CMake fetches, searches and links, and the pip requirements files, beneath it (nearest enclosing wins);
  - **root selection** (m127);
  - **override demotion** (m860, #1626);
  - **`--split`** boundaries.

  **Root selection:** CMake projects MUST be root candidates only when the scan has no main module of any other ecosystem (research R2).

  **Ownership:** where one directory holds both a CMake project and another ecosystem's main module, the other module MUST own the directory. A CMake project MUST outrank a git submodule in the same directory (research R3).
- **FR-008**: The scan MUST record, on each project component, which candidate signals hold (C241), and MUST mark the component vendored (C240 `waybill:vendored = true`) when the rule chosen by measurement holds. First-party is the absence of C240. The rule is chosen on TheRock and the quality corpus (SC-006) from these candidate signals:
  - a path segment such as `third_party`, `external`, `vendor` or `deps`;
  - a git-submodule boundary;
  - a licence file in the project directory;
  - whether the root build reaches the project.

  No signal may change a component's identity. If no single signal or two-signal conjunction reaches the precision bar fixed in research R10, no C240 is emitted for project components. C241 is still emitted, and the verdict goes back to the maintainer.
- **FR-009**: Directories excluded by `--exclude-path` or by the walker's skip set MUST contribute no project.
- **FR-010**: C237 MUST count searches paired with a project component under a new `paired.project` key. `in-tree` MUST keep listing the names satisfied only by config templates or installed config files. The accounting equation MUST still hold (research R6).
- **FR-011**: The three formats MUST agree on project components, their edges, scopes and annotations (`waybill sbom parity-check`). No two components may share a `bom-ref`.
- **FR-012**: Output MUST be byte-identical to `main` for any scan in which no listfile declares a literal `project()`.
- **FR-013**: Repeat scans MUST produce identical output.
- **FR-014**: No new Cargo dependencies.
- **FR-015**: A project under a test directory MUST be Test (C62 `source-test-directory`). A project under an example or benchmark directory MUST be Development, with its derivation recorded. The directory names are fixed by the research and listed in the catalog.
- **FR-016**: A project that the root build does not reach through `add_subdirectory` or `include` (spec 1614's reach) MUST carry an annotation saying so. That annotation MUST NOT change its scope or its reachability in the dependency graph (FR-006).

### Key Entities

- **In-tree project**: a `project(<name> [VERSION <v>])` declaration in a listfile the CMake reader reads. Attributes:
  - name, and version (or its absence);
  - declaring files (one or more directories);
  - enclosing project;
  - scope;
  - vendored classification and its signals.
- **Project edge**: a dependency from a searching (or enclosing) project to a project component. It carries the search's scope and evidence.
- **Ownership index**: the existing nearest-enclosing-project lookup, extended to include in-tree projects.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock's source tree (measure-source against `main`, CI only), every search counted in C237 `in-tree` because of a `project()` in the tree yields an edge to that project's component, unless spec 1614's one-identity order assigns the name to an earlier component. Research re-counts the population on the current pin; the 63 from #1616 is the starting figure. Every new component, edge and ownership change is traced in the measurements.
- **SC-002**: Unexplained orphans do not increase on TheRock, on the three quality-corpus targets, or on the public corpus.
- **SC-003**: `cmake-nlohmann-json`, `python-pytorch` and `cpp-mongo` in the quality corpus are traced component by component against a `main` control. Every change is explained by FR-001 to FR-010.
- **SC-004**: The public corpus is regenerated (`regen_goldens`), and every golden that changes is traced. Targets with no `project()` declaration are byte-identical (FR-012).
- **SC-005**: Parity-check agreement in all three formats, no duplicate `bom-ref`s, and identical repeat scans on two runners.
- **SC-006**: The vendored classification's signal is chosen from a measurement, recorded in the research. For each candidate signal, the measurement reports how many TheRock and quality-corpus projects it classifies, and how many of those agree with a hand-labelled sample. The criterion for choosing is stated before the counts are read.
- **SC-007**: Scan time on TheRock's source tree is at most 1.05× `main`'s, measured as the median of at least 3 runs.
- **SC-008**: On every scan that has a main module of any other ecosystem, the document root and the recorded root-selection heuristic are unchanged from `main`. This covers the quality corpus's `python-pytorch` and every public-corpus target.

## Assumptions

- The CMake reader's existing listfile coverage decides which declarations are seen: its walker, its exclusions, and spec 1614's reach. This spec adds no new CMake parsing beyond reading `project()`'s name and `VERSION`.
- Installed SDK trees contain no `project()` calls, so the SDK output does not change. The SDK's unnamed binaries are a separate item (#1534, #1527).
- `pkg:generic` is used because no purl-spec type exists for CMake projects. That matches spec 1531's installed records and spec 1614's search components.
- TheRock is measured only in CI, through `kusari-sandbox/waybill-fixture-therock-sdk`.
