# Feature Specification: Pants resolves are owned and named across both language namespaces

**Feature Branch**: `1064-pants-resolve-namespaces`
**Created**: 2026-10-03
**Status**: Draft
**Input**: Issue #924 — `waybill:resolve-ownership` (catalogue row C161) names only Python resolves, so a JVM Pants repository gets no ownership statement and a polyglot one gets a statement describing half of it. Research on the issue (2026-10-03) found the gap is wider: JVM resolves also get no owning (anchor) component, unconfigured default resolves are named differently from how Pants names them, and the anchor identity is not namespace-qualified.

## Background *(measured)*

Measured on `main` @ 29f90262 against the committed public-corpus goldens, crate fixtures, and Pants 2.31.0 source:

| repository | ownership statement (C161) | resolve anchors | JVM packages without an owning anchor |
|---|---|---|---|
| `pants-example-python` | names `python-default` | 1 (Python) | — |
| `pants-example-jvm` | **absent** | **none** | 27 of 27 |
| `pants-clojure-polyglot` | names the 2 Python resolves only | 2 (Python only) | 22 of 22 |
| fixture `pants_namespace_collision` | names `default`, `lint`; the JVM `default` is invisible | Python only | all |

Consequences a consumer can observe today:

- A JVM Pants repository's SBOM makes **no statement** about its resolves, and the polyglot one makes an incomplete statement that reads as complete.
- JVM packages have no owner in the graph. Their top-level requirements reach the document root only through a fallback that two of the three output formats apply and one does not. That is the three-way root-edge disagreement pinned for `pants-clojure-polyglot` in milestone #925's corpus target (6 / 6 / 2).
- Pants names a resolve it was never told about `jvm-default` or `python-default` (its built-in defaults). waybill names it `default`, after the lockfile's filename. `pants-example-jvm` itself refers to its resolve as `jvm-default`, while waybill labels its 27 packages `default`.
- Anchor identity is the resolve name alone, so a Python `default` and a JVM `default` would be the same component. Adding JVM anchors without qualifying identity would repeat #919's merge one layer down.

## Clarifications

### Session 2026-10-03

- Q: How should anchor identity be qualified by namespace? → A: As a qualifier on the existing identity: `pkg:generic/<resolve-name>?pants-namespace=<namespace>` (option B). There is no settled community convention for identifying build-system grouping components in a PURL; that open question is tracked on #1106.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A JVM Pants repository states and anchors its resolves (Priority: P1)

An auditor scans a JVM-only Pants repository. The SBOM states which resolves the repository declares, gives each declared resolve an owning component, and connects that component to the resolve's declared top-level requirements, exactly as it already does for Python.

**Why this priority**: This is #924's core gap. A whole language namespace currently produces no ownership statement and no owner in the dependency graph, which Principle VIII (completeness) and Principle X (transparency) both cover.

**Independent Test**: Scan `pants-example-jvm`. The ownership statement is present and names its JVM resolve, an anchor component exists for it, and every JVM top-level requirement is reachable from the document root through that anchor in all three output formats.

**Acceptance Scenarios**:

1. **Given** a repository whose only Pants lockfiles are JVM lockfiles, **When** it is scanned, **Then** the document carries an ownership statement listing each declared JVM resolve.
2. **Given** a declared JVM resolve whose lockfile lists top-level requirements, **When** it is scanned, **Then** an owning component exists for that resolve and depends on exactly those requirements' packages from the same resolve.
3. **Given** the same scan emitted as CycloneDX, SPDX 2.3 and SPDX 3, **When** the root's direct dependencies are counted in each, **Then** the three counts are equal.

---

### User Story 2 - A polyglot repository's statement covers both namespaces without ambiguity (Priority: P1)

An auditor scans a repository that has Python and JVM resolves, including two resolves that share a name across the namespaces. The ownership statement lists every resolve once and unambiguously, and each resolve, same-named or not, has its own owning component.

**Why this priority**: Without this, a statement that names `default` cannot say which `default` it means. With JVM anchors added (User Story 1) and identity left unqualified, two resolves would merge into one component. Doing User Story 1 without this one would produce the wrong graph for a collision repository.

**Independent Test**: Scan the `pants_namespace_collision` fixture and the `pants-clojure-polyglot` corpus target. The statement lists all resolves qualified by namespace. Two same-named resolves yield two distinct owning components, each owning only its own packages.

**Acceptance Scenarios**:

1. **Given** a repository with a Python resolve and a JVM resolve both named `default`, **When** it is scanned, **Then** the statement lists both, each identified with its namespace, and two distinct owning components exist.
2. **Given** `pants-clojure-polyglot`, **When** it is scanned, **Then** the statement lists `pants-2.30`, `pants-2.31`, `java17` and `java21`, each qualified by namespace.
3. **Given** a split by resolve, **When** each split document is read, **Then** its ownership statement is still repository-wide and covers both namespaces (milestone 912 FR-007).

---

### User Story 3 - Resolves Pants declares by default are named as Pants names them (Priority: P2)

A maintainer whose repository relies on Pants's built-in resolve configuration scans it. Its resolve is called what Pants and the repository's own configuration call it (`jvm-default`, `python-default`), and it is treated as declared, because Pants's default configuration declares it.

**Why this priority**: The name is what a consumer uses to match the SBOM against the repository, its build configuration and other tools. Today the name disagrees with Pants, and the resolve is counted as found by convention, which understates the repository's own declaration.

**Independent Test**: Scan `pants-example-jvm`, which configures no JVM resolves. Its packages' resolve membership reads `jvm-default`, and the ownership statement lists `jvm-default` as declared, not as discovered.

**Acceptance Scenarios**:

1. **Given** a repository whose configuration names no JVM resolves and has a lockfile at Pants's default JVM lockfile path, **When** it is scanned, **Then** that resolve is named `jvm-default` and counted as declared.
2. **Given** the same situation for Python, **When** it is scanned, **Then** the resolve is named `python-default` and counted as declared.
3. **Given** a repository that configures its resolves explicitly, **When** it is scanned, **Then** names come from the configuration, unchanged from today.

---

### User Story 4 - JVM tool lockfiles are classified by their declaration (Priority: P3)

A JVM repository declares a tool's lockfile through that tool's own configuration (for example a test framework's lockfile option). The packages in that lockfile are classified as development-scope because the configuration says so, not because the resolve's name happens to match a heuristic, and the statement's count of heuristically classified resolves reflects that.

**Why this priority**: It strengthens evidence the statement already reports for Python and removes a name-based guess. It is valuable but not needed for the statement to exist.

**Independent Test**: A fixture declares a JVM tool lockfile through the tool's lockfile option, under a name the heuristic would not recognise. Its packages are development-scope and the resolve is not counted as heuristically classified.

**Acceptance Scenarios**:

1. **Given** a tool's configuration names a lockfile path, **When** that lockfile is scanned, **Then** its resolve is classified from that declaration.
2. **Given** a JVM resolve no tool declares, **When** it is scanned, **Then** it is classified by the existing name heuristic and counted as heuristically classified.

---

### Edge Cases

- A JVM lockfile with no recorded top-level requirements: its anchor exists and depends on nothing; the resolve is still listed. Nothing is invented.
- A top-level requirement whose package is absent from its own lockfile (a malformed lock): the edge is dropped and counted under the existing unresolved-dependency signal, not attached to another resolve's package.
- A lockfile found only by filename convention when the configuration *does* configure resolves for that language: it stays unanchored and is listed as discovered (milestone 868 FR-003 unchanged). The Pants built-in default applies only when the language's resolves are not configured at all.
- A configured resolve whose lockfile path does not exist: it is not listed, as today.
- Lockfiles at a default path with no `pants.toml` in the repository: not a Pants repository, so Pants's built-in default does not apply. They keep filename-stem names and are discovered, not declared.
- Both namespaces absent (a non-Pants repository): no statement, no anchors, output byte-identical to before.
- A tool declares a lockfile path that `[jvm.resolves]` also names: one resolve, the configured name, classified from the tool declaration.
- The same resolve name in both namespaces with an identical lockfile basename: still two resolves, two anchors, two statement entries.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The repository's ownership statement MUST include JVM resolves, with the same meanings it already gives Python resolves: declared resolves, resolves found only by convention, the count classified by heuristic, and the count of lockfiles left unanchored.
- **FR-002**: Every resolve named in the ownership statement MUST carry its language namespace, in the form the per-document resolve identity (C163) already uses (`<namespace>:<name>`).
- **FR-003**: There MUST be one ownership statement per repository covering all namespaces. It is present when either namespace found a Pants lockfile, and absent when neither did.
- **FR-004**: Every declared JVM resolve MUST have an owning component of the same nature as the Python one: marked as a lockfile resolve rather than a package, carrying its resolve membership and its namespace.
- **FR-005**: A JVM owning component MUST depend on exactly the packages named as top-level requirements in its own lockfile, resolved within that same resolve.
- **FR-006**: Owning-component identity MUST be unique across namespaces, so that two resolves sharing a name in different namespaces produce two components that never merge. The identity is the resolve name qualified by namespace, `pkg:generic/<resolve-name>?pants-namespace=<namespace>` (for example `pkg:generic/python-default?pants-namespace=python`). It applies to every owning component, Python and JVM, whether or not a name collides.
- **FR-007**: When the repository has a `pants.toml` and that configuration names no resolves for a language, a lockfile at Pants's default lockfile path for that language MUST be named with Pants's default resolve name (`jvm-default`, `python-default`) and counted as declared. Without a `pants.toml` there is no Pants repository and no built-in default; lockfiles keep their filename-stem names and stay discovered.
- **FR-008**: Each package's resolve-membership annotation MUST use the same resolve name the ownership statement uses.
- **FR-009**: A JVM resolve whose lockfile a tool declares through its own lockfile option MUST be classified from that declaration. Only resolves no tool declares fall back to the name heuristic, and only those are counted as heuristically classified.
- **FR-010**: In a document split by resolve, the ownership statement MUST remain repository-wide (milestone 912 FR-007 is preserved).
- **FR-011**: Every list in the ownership statement MUST be deterministically ordered, so two scans of one repository produce identical values.
- **FR-012**: The ownership statement MUST carry the same value in all three output formats, enforced by the existing format-parity catalogue.
- **FR-013**: For a Pants repository, every root edge that exists because of an owning component MUST be present in all three output formats. A repository whose only root-edge disagreement came from resolves without owning components therefore has equal root direct-dependency counts across formats. Disagreement with other causes (#1022, e.g. unversioned declared requirements in `pants-example-python` and `pants-example-django`) is outside this feature and unchanged.
- **FR-014**: A repository with no Pants lockfiles MUST produce output byte-identical to before this feature.
- **FR-015**: These changes are consumer-visible and MUST be recorded as such: the ownership statement's value changes for every Pants repository, membership names change for repositories using Pants defaults, and owning-component identity changes per FR-006. This feature deliberately supersedes milestone 912 SC-006 ("unchanged from before this feature") and milestone 868's Python-only scope.

### Key Entities

- **Resolve**: A named, locked dependency set in one language namespace (Python or JVM), backed by one lockfile. Identified by namespace and name together, never by name alone.
- **Ownership statement**: The repository-wide, document-scope summary of which resolves exist, which the repository declares, which were found only by convention, and how much classification rests on heuristics.
- **Owning (anchor) component**: The component that represents a declared resolve in the graph. It is not a package, and it depends on that resolve's declared top-level requirements.
- **Declaration source**: What establishes that a resolve is declared: the language's resolve configuration, Pants's built-in default when that configuration is absent, or a tool's lockfile option.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning `pants-example-jvm` yields an ownership statement naming exactly one declared resolve, `jvm:jvm-default`, and 0 of its 27 JVM packages without an owning component (today: no statement, 27 of 27).
- **SC-002**: Scanning `pants-clojure-polyglot` yields a statement listing all four resolves, each namespace-qualified. The root's direct-dependency counts are equal across the three formats, so the target leaves the known root-edge divergence list (today 6 / 6 / 2).
- **SC-003**: Scanning the namespace-collision fixture yields two distinct owning components for the two `default` resolves, each owning only its own namespace's packages, and two distinct statement entries.
- **SC-004**: For every repository in the public corpus that has no Pants lockfiles, output is byte-identical to before.
- **SC-005**: Two scans of each Pants corpus target produce byte-identical documents.
- **SC-006**: In a fixture whose JVM tool lockfile is declared by the tool under a name the heuristic does not match, 100% of that lockfile's packages are development-scope, and the heuristic-classification count excludes that resolve.

## Assumptions

- Pants semantics are those of Pants 2.31.0 as measured from its source. The built-in defaults are `[jvm].resolves = {"jvm-default": "3rdparty/jvm/default.lock"}` and `[python].resolves = {"python-default": "3rdparty/python/default.lock"}`, and JVM tools declare lockfiles through a per-tool `lockfile` option rather than `install_from_resolve`.
- The Pants built-in default applies only when a language's resolve configuration is absent. An explicit, non-default configuration keeps today's behaviour for unconfigured lockfiles (discovered, unanchored).
- Pants-generated coursier lockfiles record their top-level requirements in the lockfile metadata header, as the two measured JVM repositories do.
- Owning components keep the existing "lockfile resolve, not a package" marking. Only their identity and coverage change.
- Resolve-scoped edge resolution for Maven packages (#1103) is in place, so anchor edges resolve within their own resolve.
- Corpus goldens for the four Pants targets are regenerated through the CI lane per `docs/development/refreshing-corpus-goldens.md`, and every change is attributed in review.
- The `pants-namespace` qualifier is waybill's own; the purl-spec `generic` type registers only `download_url` and `checksum`. A consumer that compares PURLs with qualifiers removed sees two same-named resolves as one again. This is a known limitation of the chosen identity, accepted because there is no community convention to follow yet (#1106). Within waybill's own documents, uniqueness holds, because identity comparison includes qualifiers.
- Unifying the root-edge fallback across formats (#1022) is out of scope. `pants-example-python` and `pants-example-django` keep their known root-edge disagreement, whose cause (unversioned declared requirements) this feature does not touch. This feature removes the need for the fallback on Pants repositories but does not change the fallback itself.
