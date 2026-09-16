# Feature Specification: Lockfile resolve graphs must be anchored to an owning component

**Feature Branch**: `868-resolve-ownership`
**Created**: 2026-09-16
**Status**: Draft
**Input**: User description: "887"

## Context

A lockfile resolve is a pinned set of packages plus the dependency graph
between them. waybill reads those graphs correctly. Nothing connects them to
the project they belong to, so they float.

**Measured on `lablup/backend.ai` @ `809fcd394dd8e39456986dd742e7d51c6aedd647`,
against `main` at `3ad457ae`** — scanned as the quality harness scans it:

| | |
|---|---:|
| components | 331 (272 of them pypi) |
| dependency edges | **761** |
| components reachable from the document root | **1** of 331 |
| max depth from the root | **1** |
| top-level requirements of the resolve reaching nothing | **99** |
| document self-report | `partial` — `orphaned-components-detected: 271` |

760 of those 761 edges are a real, well-formed pypi graph. The 761st is the
root's single edge, to `pkg:pypi/backend.ai@0.0.0-unknown`, which has **zero
outgoing edges**. So an SBOM consumer walking from the root sees one
component and stops, while 760 correct edges sit in the same document
unreachable.

### Why the main module having no edges is not the bug

The root manifest genuinely declares nothing to depend on:

```toml
[project]
requires-python = "~=3.13.7"
name = "backend.ai"
dynamic = ["version"]
```

`dynamic` covers only `version`. This is a Pants monorepo: dependencies live
in per-package manifests and in lockfiles, not in the root manifest. Reading
that root as "no direct dependencies" is correct.

This is also **not** the defect fixed for `python-ansible` (dynamic
dependencies unhandled) nor the one fixed for `gradle-bitwarden-android` (a
requirer whose PURL type did not match its dependency names' ecosystem).
Both were checked and neither applies: the root declares no `dependencies`
key at all, and enabling cross-ecosystem inference changes nothing here.

### The gap

Nothing in the model says **who owns a resolve**. The resolve's 99 top-level
requirements — the packages nothing else in the resolve depends on, i.e. what
the resolve was asked to provide — are attached to nothing, so no path exists
from any project component into the graph.

An anchor identity does already exist in the build configuration. The repo
declares its resolves by name:

```toml
[python.resolves]
python-default = "python.lock"
```

alongside a second application lockfile (`python-kernel.lock`) and seven
per-tool lockfiles under `tools/` (`pytest`, `mypy`, `black`, `coverage-py`,
`setuptools`, `pants-plugins`, …). A resolve is therefore a **named thing in
the project's own configuration**, not something that would have to be
invented.

### Why it was invisible until now

The same masking that hid two other defects. With the main module edgeless
the root was edgeless too, so the CycloneDX primary-dependency fallback
attached the root to every component nothing depended on — here the 99
resolve roots plus 59 file-tier components. The corpus bound was authored
against that. Scanning **without** `--root-name` still reproduces the
authored figure exactly (918 edges), because the main module then becomes the
root and the fallback fires against it.

So `918` counted fabricated edges and `761` counts a graph no project
reaches. Neither is a target to aim at, and the bound cannot be authored
until this is decided.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A project's resolved dependencies are reachable from it (Priority: P1)

Someone generates an SBOM for a monorepo whose dependencies are pinned in a
lockfile, and asks their tooling "what does this project depend on?" by
walking the graph from the document root.

**Why this priority**: This is the whole defect. A dependency graph that
cannot be walked from the root is, for most consumers, not a dependency graph
— vulnerability and license tooling routinely starts at the root and
traverses. 760 correct edges deliver nothing while unreachable.

**Independent Test**: Scan a repository whose lockfile resolve is named in
its build configuration; confirm the resolve's contents are reachable from
the document root by following dependency edges.

**Acceptance Scenarios**:

1. **Given** a project with a named lockfile resolve, **When** an SBOM is
   generated, **Then** the resolve's top-level requirements are reachable
   from the document root.
2. **Given** that same project, **When** an SBOM is generated, **Then** the
   count of components reported unreachable falls to reflect the now-connected
   graph.
3. **Given** a project whose dependencies are already reachable from its root
   today, **When** an SBOM is generated, **Then** its graph is unchanged.

---

### User Story 2 - The anchor says what it is (Priority: P2)

Someone reading the SBOM encounters the component or edge that connects the
project to its resolve, and needs to tell whether it represents something
declared in the project or something waybill introduced to express ownership.

**Why this priority**: Whatever anchors a resolve is, to some degree, a
statement waybill is making rather than one a manifest made. A consumer who
cannot distinguish it from a declared dependency cannot weight it, and this
project treats that distinction as the difference between a fact and an
inference.

**Independent Test**: Generate an SBOM for a project with a resolve and
confirm the anchoring is identifiable as such from the document alone.

**Acceptance Scenarios**:

1. **Given** an SBOM containing an anchored resolve, **When** a consumer
   inspects the anchor, **Then** its nature is determinable from the document
   without reference to waybill's source.
2. **Given** an SBOM with no resolves, **When** it is generated, **Then** no
   anchoring artefact appears.

---

### User Story 3 - Multiple resolves stay distinguishable (Priority: P3)

Someone scans a repository with several lockfiles — an application resolve, a
second runtime resolve, and a set of per-tool lockfiles — and needs to tell
which packages came from which.

**Why this priority**: Collapsing several resolves into one undifferentiated
set would misrepresent the project: a package pinned only in a linter's
lockfile is not a runtime dependency of the application. The measured target
has nine lockfiles, so this is the normal case rather than an edge case.

**Independent Test**: Scan a repository with more than one resolve and
confirm each package's originating resolve is determinable from the document.

**Acceptance Scenarios**:

1. **Given** a repository with two or more resolves, **When** an SBOM is
   generated, **Then** the resolve a package came from is determinable.
2. **Given** a package present in two resolves, **When** an SBOM is
   generated, **Then** that fact is not silently flattened into one arbitrary
   attribution.

---

### Edge Cases

- A lockfile the build configuration does not name — discovered on disk but
  belonging to no declared resolve.
- A resolve named in configuration whose lockfile is absent or unreadable.
- A resolve whose contents are entirely unreachable for an unrelated reason,
  so anchoring it connects a graph that is itself incomplete.
- Two resolves sharing packages: the same package, same version, reached via
  two different resolves.
- A repository with exactly one resolve and one project, where any ownership
  model degenerates to the same answer — the case most likely to make a wrong
  model look right.
- A resolve belonging to tooling rather than to the shipped application, where
  anchoring it to the application would assert a runtime relationship that
  does not exist. Handled by FR-003a: classified build-time when a tool
  declares it.
- **A tooling resolve that nothing declares as one.** `towncrier` and
  `pants-plugins` on the measured target: registered like any other resolve,
  living under `tools/`, with no `install_from_resolve` back-reference. These
  fall to the FR-003b runtime default and are over-reported rather than
  hidden, and FR-003c makes that visible.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The contents of a lockfile resolve MUST be reachable from the
  document root by following dependency edges.
- **FR-002**: Each named resolve MUST be represented by its own component,
  derived from the name the project's configuration gives it. Ownership MUST
  NOT be guessed when the configuration names no resolve.
- **FR-002a**: A resolve component MUST be identifiable as representing a
  resolve rather than a package. It stands for a pinned set the project
  declared, not for something installable, and a consumer must not mistake it
  for a dependency that can be fetched or scanned for vulnerabilities.
- **FR-002b**: A package belonging to more than one resolve MUST be reachable
  through each of them, satisfying FR-006 structurally rather than by an
  attribution rule.
- **FR-003**: Where ownership cannot be determined, the resolve MUST remain
  unanchored rather than being attached to an arbitrary component, and the
  situation MUST be reported.
- **FR-003a**: A resolve MUST be classified build-time when the project's own
  configuration declares a tool installing from it. Such a declaration MUST
  take precedence over any name-based heuristic.
- **FR-003a-i**: Where nothing declares a resolve, a name-based heuristic MAY
  classify it, and every such use MUST be reported (FR-003c). The heuristic
  MUST NOT override a declaration, and MUST NOT be treated as equivalent
  evidence.
- **FR-003b**: A resolve with no such declaration MUST be treated as
  runtime. This direction is deliberate: mis-marking a runtime resolve as
  build-time would hide real packages from a consumer filtering for runtime
  risk, whereas mis-marking a build resolve as runtime over-reports. Only the
  first failure is silent, so the default takes the loud one.
- **FR-003c**: The count of resolves classified by anything weaker than a
  declaration — by name heuristic, or by the FR-003b runtime default — MUST be
  reported, so reliance on weak evidence is visible instead of invisible. The
  count MUST be emitted even when zero, so "nothing needed guessing" and "the
  field is missing" stay distinguishable.
- **FR-004**: An anchoring relationship introduced by waybill MUST be
  distinguishable, by a consumer reading the document, from a dependency a
  manifest declared.
- **FR-005**: Each package MUST remain attributable to the resolve it came
  from, including when several resolves are present.
- **FR-006**: A package appearing in more than one resolve MUST NOT be
  silently attributed to one of them arbitrarily.
- **FR-007**: Scans of projects with no lockfile resolve MUST produce output
  identical to before this feature.
- **FR-008**: No component may be fabricated to represent a package that the
  scan did not observe; this feature changes what is connected, never what
  exists.
- **FR-009**: The document's reachability self-report MUST agree with the
  graph after anchoring — a document MUST NOT report components unreachable
  that are in fact now reachable.

### Key Entities

- **Resolve**: a named, pinned set of packages together with the dependency
  graph between them, declared in the project's build configuration and
  materialised as a lockfile. Already read correctly; what is missing is its
  relationship to the project.
- **Resolve component**: a component standing for one named resolve, sitting
  between the document root and that resolve's top-level requirements. New to
  the model. It represents a pinned set the project named, not an installable
  package — which is why FR-002a requires it be distinguishable from one.
- **Top-level requirement**: a package in a resolve that nothing else in that
  resolve depends on — what the resolve was asked to provide. 99 of these on
  the measured target, all currently unreachable.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On `lablup/backend.ai` @ `809fcd3`, components reachable from
  the document root rise from **1 of 331** to cover the resolve's contents.
- **SC-002**: On that same target, max depth from the root rises from **1**,
  and the document no longer reports itself flat.
- **SC-003**: The document's unreachable-component count falls from **271**
  and agrees with the emitted graph.
- **SC-004**: Every scan of a project with no lockfile resolve produces
  output byte-identical to before the change, demonstrated across the
  committed corpus.
- **SC-005**: On a repository with more than one resolve, each package's
  originating resolve is determinable from the document alone.
- **SC-005a**: On the measured target, the five resolves a tool declares
  (`black`, `pytest`, `coverage-py`, `mypy`, `setuptools`) are classified
  build-time, and the count classified runtime-by-default is reported and
  non-zero — it is 4 there, covering the two genuine applications plus the two
  undeclared tooling resolves.
- **SC-006**: No **package** component is created by this feature on any
  corpus target. Resolve components are added by design — 8 on the measured
  target — and each MUST correspond to a resolve the project's own
  configuration names. Connecting a graph must not invent packages.
- **SC-006a**: Each package's originating resolve remains determinable after
  the change, and a package belonging to several resolves is reachable via
  each.
- **SC-007**: Every claim above is demonstrated to **fail** against the
  pre-change build before it is accepted as passing.

## Assumptions

- **The resolve graphs themselves are correct and are not re-derived.** 760
  edges on the measured target are well-formed; this feature connects that
  graph to a project and changes nothing inside it.
- **Anchoring is a relationship question, not a discovery question.** Every
  package involved is already emitted as a component. Nothing here reads a new
  file format or finds packages that were previously missed.
- **The build configuration is authoritative about what resolves exist.** A
  repository that names its resolves is stating the ownership structure; this
  feature reads that statement rather than inferring one from directory
  layout.
- **The corpus expectation is re-authored after this lands, not with it.**
  Its current bound was authored against fallback-fabricated edges, and its
  post-fix value is not knowable until the ownership model is chosen.

## Dependencies

- The existing Pants/uv/pex readers, which already parse both the lockfiles
  and the `[python.resolves]` name→lockfile mapping this feature would use.
- The m770 quality corpus, which holds the before/after evidence and whose
  `pants-backend-ai` expectation cannot be authored until this lands.

## Out of Scope

- Changing how lockfiles are parsed or which packages are discovered.
- The unselected-extras long tail. The measured target reports 778 declared
  names resolving to nothing, but sampling shows these are overwhelmingly
  optional extras a resolve legitimately did not select (`furo`, `flake8`,
  `ruff`, `myst-parser`, `twine`). Correctly dropped, correctly reported, and
  unrelated to ownership.
- Ecosystems other than Python. The same shape may exist elsewhere, but no
  measurement supports that claim yet and this feature does not assert it.

## Clarifications

### Session 2026-09-16

- Q: What component owns a resolve?
  → A: **One component per named resolve.** The document root points at each
  resolve, and the resolve points at its own top-level requirements.
  Attribution across several resolves falls out of the structure rather than
  needing separate machinery.

  Decided on evidence gathered during clarification: `waybill:pants-resolve`
  is already emitted on 248 of 272 pypi components and already names **8
  distinct resolves** on the measured target. That grouping is exactly what a
  per-resolve component expresses.

  It also corrects this spec's own first draft, which called the
  consuming-packages model "most faithful but maybe not recoverable". The
  reverse holds: the existing tag names the **resolve** a package belongs to,
  not the package that consumes it, so per-resolve ownership is the option the
  available data supports and consuming-package ownership is the one that
  would need a mapping nothing currently recovers.

- Q: Do tool lockfiles get anchored the same way as application resolves?
  → A: **Yes — every resolve is anchored, and tool resolves are marked
  build-time.** Leaving them unreachable would be the same defect at smaller
  scale, and the runtime/build distinction is carried explicitly so a consumer
  filtering for runtime risk excludes them correctly.

  The classification rule must come from what the project declares, not from
  matching names. Evidence gathered during clarification, which corrected an
  assumption made earlier in this same session:

  - `[python.resolves]` is **not** an application-only list. It is the full
    resolve registry, tool lockfiles included — all nine on the measured
    target are declared there.
  - The declarative signal is instead a tool's own configuration section
    back-referencing a resolve (`[black] install_from_resolve = "black"`, and
    likewise `pytest`, `coverage-py`, `mypy`, `setuptools`).
  - **That signal is partial.** It covers five resolves. `towncrier` and
    `pants-plugins` are tooling by any reading and live under `tools/`, but
    nothing declares them as such — only the path convention suggests it.

### Session 2026-09-16 (amendment, post-plan)

- Q: FR-003a forbade any name-based inference. Planning found a name-allowlist
  classifier already ships and is the only signal for repositories whose tools
  do not declare `install_from_resolve`. Keep the prohibition?
  → A: **No — softened to a precedence rule.** A declaration now takes
  precedence over the heuristic (FR-003a) and the heuristic survives as a
  reported fallback (FR-003a-i), rather than being banned outright.

  The prohibition was right about the evidence and wrong about the remedy.
  Banning the heuristic would delete the only classification signal available
  to repositories that declare nothing, regressing them, and no measurement
  supports that trade.

  The evidence that motivated the original wording still stands and is
  stronger than clarification knew: the shipped allowlist contains `coverage`
  and `coveragepy` while the resolve is named `coverage-py`, and `setuptools`
  is absent entirely — so **two of eight resolves on the measured target are
  misclassified today**. Declaration-first fixes both without removing the
  fallback. See research R1.
