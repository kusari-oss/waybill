# Feature Specification: TheRock's last two unexplained components

**Feature Branch**: `1543-therock-last-orphans`
**Created**: 2026-11-17
**Status**: Draft
**Input**: User description: "#1601 and #1602: the last two unexplained components on ROCm TheRock's full source tree … (1) #1601: pkg:generic/tensile, the uv workspace root the uv reader synthesizes … Decision (maintainer): attach it to the document root. Trace which readers synthesize workspace roots … and apply one rule to all of them … (2) #1602: pkg:generic/wkmi@1.0.0 … Decision (maintainer): a dependency edge from the link path, not an orphan reason. It is conditional (WIN_SDK) … a root edge, optional, carrying its condition … Generalize as link-path correlation in the CMake reader … Acceptance: on TheRock's source tree, 0 unreachable components without an I7 marker …" (full text in the conversation that created this branch)

The spec number (1543) is the next in sequence. It is unrelated to issue #1543, the built-artifacts tracker.

## Background

### Where this leaves off

Specs 1541 and 1542 took ROCm TheRock's full source tree from 841 unreachable components with no explanation to 2. Both remaining ones are filed defects. Measured in CI in `kusari-sandbox/waybill-fixture-therock-sdk`: `measure-source` run 44212411228, at `455a6d1f`, scanned with `--root-name TheRock --root-version 10.3.0`. TheRock is never scanned locally.

### #1601: a subproject's workspace root is left unattached (measured)

`rocm-libraries/projects/hipblaslt/tensilelite/pyproject.toml` declares `[project] name = "tensile"` and `[tool.uv.workspace] members = ["rocisa"]`. For it, the uv reader synthesizes a workspace root, `pkg:generic/tensile` (`waybill:component-role` = `workspace-root`).
- **Outgoing edges:** since spec 1541, the root's members resolve by PURL inside their lock, so it has edges to `pkg:pypi/rocisa@0.1.0` and `pkg:pypi/tensile@5.0.0`.
- **Incoming edges:** none.

Under an operator root, a subproject's main modules are dropped, and their edges re-anchored to the operator root. A synthesized workspace root is not a main module, so it stays behind with nothing pointing at it.

**Decision (maintainer, before specify):** attach it to the document root.

### #1602: a vendored SDK linked by path has no edge (measured)

`pkg:generic/wkmi@1.0.0` (`waybill:source-mechanism` = `cmake-package-version`) comes from the vendored prebuilt SDK at `rocm-systems/shared/amdgpu-windows-interop/wkmi/lnx/lib/cmake/wkmi/wkmiConfigVersion.cmake`. Read at `rocm-systems` `c1ab6df5` through the GitHub API:
- **No `find_package`.** Nothing calls `find_package(wkmi)`.
- **The WSL build** (`libhsakmt/CMakeLists.txt`: `if (WIN_SDK) include(CMakeLists_wsl.txt)`) links it by link directory and plain name: `target_link_directories(rocdxg PRIVATE …/wkmi/lnx/lib)` and `target_link_libraries(rocdxg PRIVATE wkmi)`. The package files are never read.
- **The Windows build** links `win/lib/wkmi.lib` by path. That variant has no package files.
- **The Linux build TheRock ships** never links it.
- **The binaries are tracked with DVC.** `lnx/lib/libwkmi.a.dvc` and `win/lib/wkmi.lib.dvc` are committed; the libraries themselves are gitignored.

**Decision (maintainer, before specify):** a dependency edge from the link path, not an orphan reason. The edge is conditional (`WIN_SDK`), and comes from a subproject with no component of its own.

### What is not yet traced

- Which readers synthesize workspace roots. The uv, bun and Kotlin DSL readers share one helper; whether anything else does is unchecked.
- Whether the CMake reader reads a file named `CMakeLists_wsl.txt` that is pulled in by `include()`.
- How the CMake reader records conditions on declarations other than fetches.
- What, if anything, should be said about a DVC pointer standing in for an absent library.

## Clarifications

### Session 2026-11-17

- Q: A uv or bun workspace scanned at its own root, without `--root-name`, makes one of its members the subject (measured), leaving the workspace root stranded. What should this feature do? → A: At the scan root, a synthesized workspace root outranks its own members as the subject. Everywhere else it is anchored.
- Q: C232 is `waybill:cmake-fetch-conditions`, defined for fetches. Which carrier for a conditional link? → A: A new row for links, with C232's shape, plus a new C122 value. C232's contract is unchanged.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A subproject's workspace root hangs off the document root (Priority: P1)

A consumer scans a repository with `--root-name` that holds a subproject organised as a workspace. The subproject's workspace root is reachable from the document root, its members are reachable through it, and nothing is left unattached.

**Why this priority**: it is one of TheRock's two remaining unexplained components, and the same shape occurs in any monorepo with uv, bun or Gradle workspaces in subdirectories.

**Independent Test**: a synthetic repository holding a uv workspace in a subdirectory, scanned with `--root-name`.

**Acceptance Scenarios**:

1. **Given** a uv workspace in a subdirectory, its root project and one more member listed in its lock, **When** scanned with `--root-name`, **Then** the document root reaches the workspace root, the workspace root reaches both members, and no component is unexplained.
2. **Given** the same for a bun or a Kotlin DSL workspace, **When** scanned, **Then** the same holds: one rule, every reader that synthesizes a workspace root.
3. **Given** a workspace at the scan root, scanned without `--root-name`, **When** scanned, **Then** the workspace root is the subject, and its members are reached through it. (Measured before this feature: a uv or bun member was the subject, and the workspace root was stranded with `foreign-build-system`.)
4. **Given** a member that already has an edge from the document root, **When** its workspace root is attached, **Then** the member is not given a second, contradictory edge, and its scope does not change.

---

### User Story 2 - A vendored library linked by path is a dependency (Priority: P1)

A consumer scans a source tree in which a build links a vendored prebuilt library by link directory and plain name. The library's component is reachable, through an edge from the project that links it, that says it is optional and under what condition the build links it.

**Why this priority**: it is TheRock's other remaining unexplained component. Vendored prebuilt SDKs consumed by link directory are a common C and C++ pattern, and today they appear as components nothing uses.

**Independent Test**: a synthetic tree holding a vendored installed library and a CMake file that links it by directory under a condition.

**Acceptance Scenarios**:

1. **Given** a vendored package with a CMake version file under `<prefix>/lib/cmake/<pkg>/`, and a CMake file doing `target_link_directories(<target> PRIVATE <prefix>/lib)` and `target_link_libraries(<target> PRIVATE <pkg>)` inside `if (SOME_FLAG)`, **When** scanned, **Then** the package's component is reachable from the linking file's owner (spec 1537: the nearest enclosing main module or git submodule, else the document root), optional, and carries the condition `SOME_FLAG`.
2. **Given** the same link with no condition, **When** scanned, **Then** the edge is unconditional, and the scope follows the reader's existing rules for a required declaration.
3. **Given** the library present (`lib<pkg>.a`), absent, or replaced by a DVC pointer (`lib<pkg>.a.dvc`), **When** scanned, **Then** the edge exists in all three cases. The document does not distinguish them (FR-007).
4. **Given** a link directory that is inside no installed-library record's prefix, **When** scanned, **Then** no edge and no component are invented.
5. **Given** the link in a file pulled in by `include()` that is not named `CMakeLists.txt` (such as `CMakeLists_wsl.txt`), **When** scanned, **Then** its link declarations are read, with the conditions open at the `include()` site, without reading files nothing includes.

---

### User Story 3 - TheRock's source tree has no unexplained component (Priority: P2)

On ROCm TheRock's full source tree, every unreachable component carries an I7 marker.

**Why this priority**: it confirms Stories 1 and 2 on the target that motivated them, and closes the work begun by spec 1541.

**Independent Test**: `measure-source` at the branch against `main`, probed with the corpus harness's I7 functions.

**Acceptance Scenarios**:

1. **Given** TheRock's full source tree, **When** scanned at the branch, **Then** 0 unreachable components carry no I7 marker.

---

### Edge Cases

- **A workspace root whose members are all unresolved.** It is attached. Its unresolved members are reported as today.
- **Two workspaces in different subdirectories.** Each root is attached.
- **A workspace root that is the document subject.** No anchor: it is the root.
- **Two synthesized workspace roots at the scan root** (say uv and bun). Neither is promoted; both are anchored, and the existing ladder picks the subject.
- **A link directory inside two records' prefixes** (nested prefixes). The innermost record that matches the linked name wins; if none matches, no edge.
- **A linked name matching no library or pointer in the directory.** No edge.
- **The same library linked under two different conditions.** Both conditions are recorded, as spec 1538 records several conditions on one fetch.
- **A link through an imported target (`<pkg>::<pkg>`) after `find_package`.** Already handled by spec 1531's `find_package` correlation; not changed.
- **Windows-style absolute paths to `.lib` files outside any record's prefix.** No edge, as for any path outside a prefix.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001: Trace before designing.** Before the design is fixed, the plan MUST establish from the code and measured runs:
  - which readers synthesize workspace roots, and how each is attached today with and without `--root-name`;
  - whether the CMake reader reads `CMakeLists_*.txt` files pulled in by `include()`;
  - how it records conditions on declarations other than fetches;
  - how installed-library records (spec 1531) expose their prefix to the CMake reader.

  The findings are recorded with the counts they explain.
- **FR-002: Workspace roots attached.** One rule for every reader that synthesizes a workspace root (uv, bun, Kotlin DSL):
  - without `--root-name`, exactly one synthesized workspace root whose manifest is at the scan root is the subject, ahead of its members;
  - otherwise, a synthesized workspace root that no relationship targets is a root anchor: the document root depends on it, whatever root each format chooses.

  Decided from the trace (`measurements/trace.md`): the root was stranded in 7 of 12 measured shapes, not only under `--root-name`.
- **FR-003: Members unchanged.** Attaching a workspace root MUST NOT give its members a second contradictory edge, or change their scope or their existing edges.
- **FR-004: Link-path correlation.** A linked name MUST become a dependency edge to the installed-library record's component when both hold:
  - a CMake link directory (`target_link_directories`, `link_directories`) lies inside that record's prefix;
  - the name (in `target_link_libraries` or `link_libraries`) matches that record: `lib<name>.a`, `lib<name>.so`, `lib<name>.so.*`, or a DVC pointer to one of them in that directory.

  The edge comes from the linking file's owner, found the way spec 1538 attaches CMake fetches: the nearest enclosing main module or git submodule, else a root anchor. No component stands for the consuming target itself.
- **FR-005: Conditions and scope.** A component linked only inside `if` blocks MUST be optional, with C122 = `cmake-conditional-link`, and carry its conditions in a new row, `waybill:cmake-link-conditions`, with C232's shape and normalisation. Conditions open at an `include()` site apply to the included file. A component linked at least once unconditionally keeps its existing scope, and carries no conditions.
- **FR-006: No invention.** No edge, component or record is created for a link directory inside no record's prefix, or for a name matching nothing there.
- **FR-007: DVC pointers.** A DVC pointer (`lib<name>.a.dvc`, `lib<name>.so.dvc`) counts as the library when matching (FR-004). It gets no annotation of its own (research R6).
- **FR-008: Formats agree.** CycloneDX, SPDX 2.3 and SPDX 3 MUST agree on the new edges, their scope, their conditions and every C45, C104 and C105 value.
- **FR-009: Repeatable.** Spec 1542's repeat check MUST pass for every new contract fixture, and for the public corpus.
- **FR-010: No new dependencies.** Zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names.

### Key Entities

- **Workspace root**: a component a reader synthesizes for a workspace (`waybill:component-role` = `workspace-root`), whose members are the workspace's projects.
- **Installed-library record**: spec 1531's record of a pkg-config or CMake version file in a recognised install location, with its prefix.
- **Link-path correlation**: a link directory inside a record's prefix plus a linked name matching that record, giving an edge from the document root.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (trace first):** Before FR-002 and FR-004 are fixed in the plan, the code and measured runs answer FR-001's questions.
- **SC-002 (TheRock):** On TheRock's full source tree, 0 unreachable components without an I7 marker. `pkg:generic/tensile` and `pkg:generic/wkmi@1.0.0` are each reachable.
- **SC-003 (the wkmi edge is honest):** The `wkmi` edge comes from the `rocm-systems` submodule, and `wkmi` is optional with conditions `["WIN_SDK"]`, in all three formats.
- **SC-004 (formats and repeatability):** On every contract fixture and on TheRock, the formats agree on C45, C104 and C105, and repeated scans are identical after masking.
- **SC-005 (corpora):** The public corpus invariant gate stays green, and every golden change is traced to FR-002 or FR-004. Every quality-corpus movement is traced against a `main` control.
- **SC-006 (cost):** The median full-tree TheRock source scan is at most 1.1× `main`'s, by the interleaved `measure-source` workflow.
- **SC-007 (tests have teeth):** Each rule (workspace-root attachment, member edges preserved, the link-path match, conditions, no invention) is pinned by a test that fails when the rule is removed.

## Assumptions

- **Scope** is attaching synthesized workspace roots, and correlating CMake link paths with existing installed-library records. No new CMake target model is introduced: the edge comes from the linking file's owner, as fetch edges do, and from the document root only when there is none.
- **TheRock is measured in CI only**, through the fixture repository. Single public files are read through the GitHub API; TheRock is never cloned or scanned locally.
- **The Windows `win/lib/wkmi.lib` path gets no edge.** It is not inside any record's prefix: that variant has no package files.
- **DVC** is detected only by its pointer file's name (`<file>.dvc`) and its documented YAML shape (`outs:` with `path:`). Nothing is fetched.
