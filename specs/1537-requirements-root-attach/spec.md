# Feature Specification: Declared requirements attach to the project that declares them

**Feature Branch**: `1537-requirements-root-attach`
**Created**: 2026-11-16
**Status**: Draft
**Input**: Issues #1566 and #1538. Components declared by a `requirements.txt` are orphaned from the root whenever the root has any other declared edge.

## Background

A `requirements.txt` file declares the packages a project needs. Today, waybill attaches the components it declares to the document root only through a **fallback**. When the root has no dependency edge of its own, every component that nothing else depends on is attached to the root. When the root has even one edge, the fallback is off, and those components are left unreachable.

The fallback is all-or-nothing, so an unrelated edge decides whether a `requirements.txt` is honoured. #1515 made git submodules components, each with an edge from the root. Since then, a repository with one submodule loses the fallback for every other ecosystem.

**Measured at `main` `283e4c19`** with #1566's reproduction, a `requirements.txt` pinning two packages, with and without one git submodule:

| Format | No submodule | With one submodule | Graph completeness claimed |
|---|---|---|---|
| CycloneDX | 2 of 2 attached | **0 of 2**, no orphan reason | `complete` |
| SPDX 2.3 | 2 of 2 | **0 of 2** | `complete` |
| SPDX 3 | 2 of 2 | 2 of 2 | `complete` |

**What the table shows:**
- **The scan is wrong** in CycloneDX and SPDX 2.3. The declared packages are unreachable.
- **The formats disagree.** SPDX 3 attaches what the other two orphan.
- **Every document says `complete`** while two of its three components are unreachable from the root.

**On real projects** (#1566 and #1538, measured by the reporter):
- A C/C++ super-project scanned without its submodules went from 0 to **73** orphaned components between the previous build and `main`. None carried an orphan reason. The root's direct dependencies went from 160 to 11, the number of submodules.
- The same super-project scanned **with** its submodules checked out orphans **62 of 66** components from its own requirements files, also without a reason (#1538). *Inference, not established:* this is the same gate, switched off by Python projects inside the submodules. #1538's own analysis says that theory did not fully separate attached from orphaned.

**Why patching the gate again is the wrong fix.** The rule "attach graph-tops to the root when it has no edges" exists in four independent copies:
- the CycloneDX dependency builder;
- the SPDX 2.3 document builder;
- the SPDX 3 document builder;
- the graph-completeness classifier, which mirrors the emitters to grade reachability.

Each copy has been patched separately to exempt one kind of edge:
- lockfile resolve anchors (#892/#894, #1108/#1022);
- supplement anchors (#1009);
- tool-use relationships (#1438);
- over-attachment under `--root-name` (milestone 866).

The submodule edge would be a fifth exemption, applied four times. The table above shows the copies have already drifted apart.

## Clarifications

### Session 2026-11-16

- Q: Who owns a requirements file in a nested directory with no project of its own? → A: The nearest enclosing project, which is ultimately the root (FR-003).
- Q: Which unreachable components must carry an orphan reason? → A: Only those this feature's rule declines to attach. Other ecosystems remain #1214 (FR-008).
- Q: Is `complete` measured from the document root alone, or from today's multi-root seed set? → A: Keep the multi-root definition in this feature. The verdict must match the graph the emitters actually produce, under that definition. Measure in CI how many corpus documents a root-only definition would change, and file a follow-up with the numbers (FR-007, SC-003, SC-008).
- Q: What counts as a project that can own a nested requirements file? → A: Any project waybill emits for that directory, meaning a main module of any ecosystem or a git submodule. CMake emits none (research R3). Cross-ecosystem edges are expected (FR-003).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A repository's own requirements are its dependencies, whatever else it contains (Priority: P1)

An engineer scans a repository with a top-level `requirements.txt` and one git submodule. The SBOM shows the repository depending on the submodule **and** on every package its `requirements.txt` declares. Adding or removing the submodule does not change whether those packages are attached.

**Why this priority**: This is the reported regression (#1566). A consumer walking the graph from the root, such as a vulnerability scanner reporting direct dependencies or a licence review, silently misses the project's own declared Python dependencies.

**Independent Test**: Scan #1566's reproduction with and without the submodule. Every declared package has an edge from the root in both scans, in all three formats.

**Acceptance Scenarios**:

1. **Given** a repository with a top-level `requirements.txt` pinning two packages and one git submodule, **When** it is scanned, **Then** the root depends on the submodule and on both packages, in CycloneDX, SPDX 2.3 and SPDX 3.
2. **Given** the same repository without the submodule, **When** it is scanned, **Then** the root depends on both packages: today's result, unchanged.
3. **Given** a repository whose root already depends on something other than a submodule, **When** it is scanned, **Then** the root still depends on every package its top-level `requirements.txt` declares. Examples are a Conan, vcpkg or CMake `FetchContent` dependency, or a supplement.

---

### User Story 2 - Every format tells the same story, and the completeness verdict matches it (Priority: P1)

A consumer comparing the CycloneDX, SPDX 2.3 and SPDX 3 documents from one scan sees the same root edges in each. `waybill:graph-completeness` describes the graph the document actually contains.

**Why this priority**: A document that claims `complete` while components are unreachable is a false claim. Measured today, all three formats make it. The formats disagreeing means a consumer's answer depends on which file they opened.

**Independent Test**: For each fixture and each corpus target, compare the set of components the root directly depends on across the three formats. Then recompute reachability over the emitted graph from the verdict's seed set and compare it with the completeness verdict.

**Acceptance Scenarios**:

1. **Given** any scan, **When** it is emitted in all three formats, **Then** the components the root directly depends on are the same set in each.
2. **Given** a document in which any component is unreachable from the completeness seed set the verdict is defined over, **When** its completeness is read, **Then** it does not say `complete`. Today that seed set is the document root plus the per-ecosystem roots.

---

### User Story 3 - What stays unreachable says why (Priority: P2)

When a component is left unreachable from the root, it carries a `waybill:orphan-reason` saying why. A reviewer can then tell "waybill could not place this" apart from "this is not a dependency of the subject".

**Why this priority**: It is required for honesty when the rule correctly declines to attach something. Without a reason, an unreachable component looks like the defect this feature fixes.

**Independent Test**: In every fixture and corpus document, every component this feature's rule declines to attach has an orphan reason (FR-008).

**Acceptance Scenarios**:

1. **Given** a component this feature's rule does not attach to the root, **When** the document is emitted, **Then** it carries a `waybill:orphan-reason` naming why.

---

### User Story 4 - A super-project with its submodules checked out (Priority: P2)

An engineer scans a C/C++ super-project with its submodules checked out. The super-project's own requirements files' packages are attached to the super-project. Packages declared by a submodule's own project are attached to that project, not to the super-project and not to nothing (#1538).

**Why this priority**: This is the real-world shape behind #1538: 62 of 66 orphaned. It depends on the rule chosen in FR-002 and FR-003, and it is measured in CI only.

**Independent Test**: Run the fixture repository's super-project measurement in CI, with and without submodules. Count the requirements-declared components that are unreachable without a reason; it should be 0.

**Acceptance Scenarios**:

1. **Given** the super-project with submodules checked out, **When** it is scanned, **Then** no component from its own top-level requirements files is unreachable.
2. **Given** the same, **When** the reachability of components declared inside submodules is counted, **Then** each is reachable or carries an orphan reason.

---

### Edge Cases

- **Not a declaration.** A `constraints.txt`, or a file reached only through `-c`, constrains versions but declares nothing. Its components must not be attached as dependencies. The pip reader reads every file matching `requirements*.txt`, so a file named like `requirements-constraints.txt` is read as a declaration today. T001 measures what the reader does with it, and the result decides whether this feature must exclude it.
- **Includes (known limitation).** The pip reader does not follow `-r` (research R8). A file reached only through `-r`, such as `requirements/base.txt`, is not read, so its packages are not components and there is nothing to attach. An included file the reader also reads directly is attached by FR-003, like any other. Following includes is a reader-coverage follow-up, filed by T001.
- **Scoped files.** A `requirements-dev.txt` or `requirements-test.txt` keeps its existing development or test scope on the edge. Attaching does not make it runtime.
- **A project file next to the requirements file.** When a `pyproject.toml` or `setup.py` sits in the same directory, its main module already declares dependencies (milestone 670). A package declared by both appears once, with one edge from the owning project.
- **The same package from two files.** One component, with an edge from each declaring project.
- **Inside a vendored or excluded directory.** Whether such a file is read is unchanged: the walker's skip set and `--exclude-path` decide that, as today. A file the reader does not read yields no component. A file it does read is attached by FR-003, like any other. T001 measures one vendored layout.
- **A `cget` `requirements.txt`.** Not a pip file (#1522), so this feature does not touch it.
- **Scans with no declarations at all**, such as a container image or OS packages (#236). Which scans get the fallback, and what switches it off, are unchanged. Only the format-parity corrections in FR-005 apply.
- **A submodule that is also a pinned project dependency.** It keeps a single root edge (#1515), not two.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A package declared by a pip requirements file is a **direct dependency of the project that declares it**. The edge exists because of the declaration. Whether the root or that project has other edges must not matter.
- **FR-002**: A requirements file at the **scan root** is declared by the document's subject. Its packages are direct dependencies of the root in every format.
- **FR-003**: A requirements file in a **nested directory that contains a project** is declared by that project. A project is any component waybill emits for that directory: a main module of **any ecosystem** (Python, Cargo, npm, Go, Maven, …) or a git submodule. A `pkg:pypi` dependency of a Cargo or npm project is an expected cross-ecosystem edge. CMake, Conan and vcpkg emit no project component, so a directory with only a `CMakeLists.txt` owns nothing and the walk continues upward (research R3). When one directory holds several main modules, the plan settles the choice deterministically and records it in research.

  A requirements file in a **nested directory with no project of its own** is declared by the **nearest enclosing project**. Walk up the directory tree to the first directory that contains a project; at the top, that is the root. For example, `build_tools/requirements.txt` in a super-project with no project at `build_tools/` belongs to the super-project. A file under a submodule's tree belongs to that submodule.
- **FR-004**: **No root-to-everything star.** The root gains an edge only to a component that a declaration places there, or to one the existing no-declarations fallback covers (FR-005). Components with no declaration must not be attached to the root because something else made the graph non-empty. This is #870 Class B.
- **FR-005**: The fallback for scans whose root has no dependency edge of its own is kept. Examples are image scans, OS-package scans and placeholder roots (#236). It is defined once, for all formats (FR-006). Making the copies agree corrects three format-specific behaviours, and these are the only changes allowed for such scans:
  - SPDX applies the fallback to a promoted main-module root as CycloneDX does, not only to a synthetic root;
  - only dependency edges other than lockfile-resolve anchors switch it off, in every format;
  - a nested component (one with a container) is never attached to the root by the fallback, in every format.
- **FR-006**: CycloneDX, SPDX 2.3 and SPDX 3 emitted from one scan agree on the set of components the root directly depends on. They also agree on every edge this feature creates.
- **FR-007**: `waybill:graph-completeness` is computed against the graph the document emits, not against a separately maintained copy of the emitters' attachment rule.
  - The definition is unchanged: reachability from the existing multi-root seed set, which is the document root plus the per-ecosystem roots.
  - Under that definition, a document with a component unreachable from every seed does not report `complete`.
  - Redefining `complete` as reachability from the document root alone is out of scope. SC-008 measures its effect.
- **FR-008**: A component that this feature's rule declines to attach carries `waybill:orphan-reason`. That covers constraint-only files (Edge Cases) and anything FR-004 forbids attaching to the root. Explaining unreachable components in other ecosystems remains #1214's scope.
- **FR-009**: The submodule reader's own output (#1515) is unchanged: one component per submodule, with its edge from the root.
- **FR-010**: Edges created by this feature keep the lifecycle scope the declaring file already implies: runtime, development, test or optional.
- **FR-011**: No new dependencies, subprocesses or network access.

### Key Entities

- **Declaring project**: The component that a requirements file's packages are direct dependencies of. That is the document root, a main module of any ecosystem, or a git submodule, decided by the file's location (FR-002, FR-003).
- **Declared edge**: An edge that exists because a manifest names its target. This is distinct from a fallback edge, which exists because nothing else reached the target.
- **Orphan**: A component unreachable from every seed of the completeness verdict, which is the document root plus the per-ecosystem roots (FR-007). When this feature's rule leaves it unattached, it carries a reason (FR-008).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: #1566's reproduction attaches **2 of 2** declared packages to the root in all three formats, with and without the submodule. Today: 0 of 2 in CycloneDX and SPDX 2.3 with it.
- **SC-002**: For every synthetic fixture and every public-corpus target, the set of the root's direct dependencies is identical across the three formats.
- **SC-003**: Recomputing reachability from the verdict's seed set over each emitted document agrees with its `waybill:graph-completeness` verdict in every fixture and corpus document. #1566's reproduction, with its declared packages unreachable as at `main` today, must not be graded `complete`. No document is `complete` with a component unreachable from every seed.
- **SC-004**: Public-corpus goldens are regenerated in CI. Every changed file is explained by one of the causes research R9 lists:
  1. a declared requirement attached;
  2. a second declaring file recorded;
  3. the fallback running for a root whose only edges were anchors;
  4. a completeness verdict corrected;
  5–7. one of the three FR-005 format-parity corrections.

  The review records a count per cause. A component gains a root edge only through a declaration, an anchor, or the fallback of FR-005 (FR-004).
- **SC-005**: The SBOM quality corpus is run in CI. Every metric outside its authored range is explained by the same R9 causes, and is re-authored with the runs cited.
- **SC-006**: Measured in CI on the C/C++ super-project in the public fixture repository, with and without submodules checked out:
  - the super-project's own requirements-declared components unreachable without a reason go from **73** (without) and **62 of 66** (with) to **0**;
  - the root's direct dependencies are reported before and after, and each change is explained.
- **SC-007**: Every functional requirement has at least one test that fails when the requirement is broken (mutation-checked).
- **SC-008**: Measured in CI over the public corpus at the feature branch: the number of documents whose verdict would change if `complete` meant reachability from the document root alone, with each change's component counts. The numbers are recorded and filed as a follow-up issue. Nothing ships on them in this feature.

## Assumptions

- **Attachment is fixed at the declaration, not in the emitters.** The four copies of the fallback (Background) keep only their current role for scans with no declarations (FR-005). The plan decides where the shared rule lives. The requirement here is only that the formats and the completeness classifier cannot disagree (FR-006, FR-007).
- **Python first.** pip requirements files are the reported case. Other manifest kinds whose components reach the root only through the fallback are found and listed during research. Whether they move to declared edges in this feature or a follow-up is decided there, not assumed.
- **#1538's root cause is not established.** SC-006 measures it. If the attachment rule does not resolve it, the remainder is recorded with its cause, and the feature does not claim to close #1538.
- **Measurement constraints.** The super-project is measured only in CI, through the public fixture repository. TheRock is never cloned or scanned locally. Fixtures use synthetic `waybill-fixture-*` names.
- **#1214's wider classifier work is out of scope** (FR-008). SC-003 still holds for every component: an orphan in another ecosystem that is unreachable from every seed must still stop the document from reporting `complete`.
