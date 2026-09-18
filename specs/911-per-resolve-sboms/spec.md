# Feature Specification: Per-resolve SBOMs for Pants monorepos

**Feature Branch**: `911-per-resolve-sboms`
**Created**: 2026-09-17
**Status**: Draft
**Input**: User description: "902 (1,3, 4)"

Addresses items 1, 3 and 4 of [#902](https://github.com/kusari-oss/waybill/issues/902).
Item 2 shipped separately as [#910](https://github.com/kusari-oss/waybill/issues/910).

## Context

A Pants repository is several dependency-resolution boundaries wearing one
coat. A resolve is a real boundary — its own lockfile, its own pinned
versions — so one component per resolve is the honest granularity. Today a
whole monorepo collapses into one root component, and a vulnerability in a
linting resolve is indistinguishable from one in the production resolve.

Milestone 868 took the first step: every *declared* resolve gets an owning
component its members hang from. Milestone 910 made dependency edges resolve
inside their own resolve rather than crossing between them. Walking the graph
from an anchor is now a coherent operation.

Three things still stand between that and a per-resolve SBOM.

**Resolve membership does not survive deduplication.** The relation is
many-to-many — the same package at the same version is routinely pinned by
several resolves — but the annotation carries one name, and deduplication
keeps the winner's value for any key both sides hold. Every losing resolve's
claim is dropped silently. As reported on a large Pants Python monorepo
(~1,300 components, 25 Pex lockfiles, 136 workspaces; figures from 0.7.0,
re-checked against `main` at `d57235e9` and unchanged there):

| | count |
|---|---:|
| components emitted by the reader | 2,466 |
| components after deduplication | 1,319 |
| Pants lockfiles cited in source-files | 24 |
| **distinct resolve names surviving on any component** | **20** |

At least 1,147 membership claims were merged away, and four resolves end up
with **zero** components naming them. A consumer that partitions on the
annotation emits four empty SBOMs and under-reports every shared package.
Nothing in the emitted document reveals this: the counts look plausible, and
the four empty resolves look like resolves that genuinely contain nothing.

**A repository that never declares its resolves gets no anchors at all.**
Milestone 868 deliberately refused to anchor a lockfile found by glob: its
name comes from a filename stem, which is a convention rather than a
declaration of ownership. That reasoning is sound and is not being revisited.
The consequence for a consumer is that a repository relying on the
`3rdparty/python/*.lock` convention — a large share of real Pants
repositories — yields components carrying resolve names but nothing to walk
from, and the document does not say which situation it is in. There is a
doc-scope count of unanchored lockfiles, but a count cannot tell you *which*
resolves are anchored, so a consumer cannot decide between splitting and
falling back to one component without guessing.

**Neither existing split mode partitions a Pants repository.**
`--split=workspace` and `--split=directory` both derive their subproject
roots from components marked as main modules, and a resolve anchor is not a
main module. So the partitioning rule a consumer needs has to be
reimplemented outside waybill, once per consumer.

## Clarifications

### Session 2026-09-17

- Q: How should plural resolve membership be expressed on the wire? → A: **JSON-array-in-string, lexically sorted** (`["app","tools"]`). This is the encoding the codebase already uses for every plural annotation — `waybill:source-files`, `waybill:file-paths`, `waybill:workspace-member` — so it needs no new convention; it parses unambiguously whatever a resolve name contains; and it is the shape the consumer said is easiest to consume. Chosen over the comma-separated form used by the sibling Pants annotation (`waybill:pants-target`), which would keep the two Pants ownership annotations spelled alike but introduces a parsing ambiguity the JSON form does not have. Chosen over a second parallel annotation, which would leave two keys that must agree and a consumer reading the wrong one silently wrong.
- Q: Does a component belonging to one resolve also switch to array form? → A: **Yes, always an array** — `["app"]` for one, `["app","tools"]` for two. A shape that varies with cardinality makes every consumer write two code paths, and the branch they exercise least is the shared-package case this feature exists to fix. Accepted cost: the value changes on every Pants component, not only shared ones, so every Pants golden churns and any existing reader of this key must update. Consistent with how 0.8.0 shipped the `pkg:generic` → `pkg:pypi` change — pre-1.0, no deprecation path. Chosen over retiring the key for a new one, which would fail loudly for un-updated consumers but costs a catalogue row change for a project that has not needed that ceremony before.
- Q: How should the document distinguish declared from discovered resolves? → A: **Name them at document scope** — extend the existing doc-scope ownership annotation, which already counts unanchored lockfiles, to also name the resolves in each category. Purely informational: the graph is unchanged and a discovered resolve still gets no anchor, so milestone 868's refusal to assert ownership the repository never declared is preserved rather than softened with a confidence qualifier. Chosen over anchoring discovered resolves because the reach that would buy is smaller than it appears: membership is plural and complete after the first clarification, so a consumer can partition by membership without an anchor at all. The plan MUST verify that partitioning-by-membership genuinely works without an anchor (see Assumptions); if it does not, this answer is the one to revisit.
- Q: In a per-resolve document, does a shared package keep its full membership? → A: **Yes, full membership is preserved.** The `app` document records `["app","tools"]` for a package both pin. Narrowing to `["app"]` would recreate exactly the under-reporting this feature exists to fix, moved from component scope to document scope and unrecoverable without the unsplit document. A consumer triaging one resolve's SBOM can therefore see that the same fix lands in another. Accepted cost: a per-resolve document references a resolve whose packages it does not contain, which reads oddly but is true.
- Q: What grammar should the document-scope ownership annotation use once it names resolves rather than only counting them? → A: **A JSON object** — `{"declared":[...],"discovered":[...],"weak_classification":N}`. The value stops being a flat scalar the moment it carries a list, and every other plural value in this codebase is JSON. Chosen over nesting lists inside the existing `key=value;key=value` grammar, which needs a second delimiter level and breaks on a resolve name containing the separator. Chosen over a second doc-scope row, which leaves two rows that must agree and a consumer reading one without the other getting a partial answer. An existing reader of the count form breaks loudly rather than silently, which is the right failure here.
- Q: A component belonging to several resolves declares a dependency by bare name. Which resolve does that name resolve in? → A: **All of them — one edge per resolve that resolves the name.** Resolves pin independently, so `shared` means `shared@1.0.0` inside `app` and `shared@2.0.0` inside `tools`; a component in both genuinely depends on both, and picking one asserts a dependency the requirer does not uniquely have while dropping one it does. For a consumer matching advisories, a dropped edge to the other version means a real vulnerability goes unattributed. Chosen over taking the first match (arbitrary, and silently incomplete) and over first-in-sorted-order (decided by the alphabet, which is presentation rather than semantics).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Every resolve that contains a package says so (Priority: P1)

A consumer partitions an SBOM by resolve and gets, for each one, the packages
that resolve actually pins — including packages several resolves share.

**Why this priority**: without it, partitioning is not merely incomplete but
misleading. Four resolves report as empty and every shared package is
attributed to one arbitrary winner, decided by scan ordering. The other two
stories are refinements of a partition that has to be correct first.

**Independent Test**: scan a repository where two resolves pin the same
package at the same version, and confirm both resolves name it. Today exactly
one does, and which one depends on the order entries were read.

**Acceptance Scenarios**:

1. **Given** two resolves that pin one package at one version, **When** the
   scan emits, **Then** the surviving component names both resolves.
2. **Given** the same repository scanned twice with entries read in a
   different order, **When** the two documents are compared, **Then** the
   membership recorded for that package is identical. Order-dependence is
   how the defect hid: any single run looks self-consistent.
3. **Given** a repository whose lockfiles declare N resolves, **When** the
   emitted document is examined, **Then** the number of distinct resolve
   names appearing on components equals the number of resolves that contain
   at least one package — with no resolve silently absent.
4. **Given** a package pinned by one resolve only, **When** the scan emits,
   **Then** its membership is unchanged from today. Widening a
   single-valued field must not disturb the case that was already right.

---

### User Story 2 - A document says whether its resolves were declared (Priority: P2)

A consumer reading an SBOM can tell whether the repository declared its
resolves or relied on a filename convention, and chooses to partition or to
fall back to one component accordingly — without inspecting the repository.

**Why this priority**: this decides how many repositories the feature reaches
rather than whether it is correct. A consumer can ship with Story 1 alone by
treating an absent anchor as "do not split"; Story 2 replaces that guess with
a statement.

**Independent Test**: scan one repository that declares its resolves and one
that relies on the glob convention, and confirm the two documents are
distinguishable on that point alone.

**Acceptance Scenarios**:

1. **Given** a repository that declares its resolves, **When** the document
   is examined, **Then** it states which resolves were declared.
2. **Given** a repository with no declarations and lockfiles found by
   convention, **When** the document is examined, **Then** it states that
   those resolves were discovered rather than declared, and names them.
3. **Given** a repository with some declared and some discovered resolves,
   **When** the document is examined, **Then** each resolve is attributable
   to one category or the other. A single aggregate count cannot answer
   this, which is the gap the existing count leaves.
4. **Given** either kind of repository, **When** a consumer decides whether
   to partition, **Then** the decision needs nothing beyond the document.

---

### User Story 3 - waybill can do the partitioning itself (Priority: P3)

An operator asks for one SBOM per resolve and gets it, instead of every
consumer reimplementing the walk.

**Why this priority**: a convenience, explicitly so. The consumer that
prompted this can do the walk itself and does not need this to be unblocked.
Its value is that the partitioning rule lives in one place and moves with the
data model instead of drifting per consumer.

**Independent Test**: run the split against a multi-resolve repository and
confirm one SBOM per resolve, each containing that resolve's packages.

**Acceptance Scenarios**:

1. **Given** a repository with several resolves, **When** an operator
   requests a per-resolve split, **Then** one document is produced per
   resolve, each rooted at that resolve.
2. **Given** a package belonging to several resolves, **When** the split
   runs, **Then** it appears in each document whose resolve pins it.
3. **Given** a repository whose resolves are discovered rather than declared,
   **When** a per-resolve split is requested, **Then** the operator is told
   what they will get rather than receiving an empty or partial result
   silently.
4. **Given** a repository with no resolves at all, **When** a per-resolve
   split is requested, **Then** the outcome is stated rather than being an
   empty directory.

---

### Edge Cases

- A package pinned by many resolves — the value grows with the count, and
  the array encoding must stay readable and parseable at the high end rather
  than being truncated or silently capped.
- Two resolves pinning one package at *different* versions: two components,
  each with its own membership. This must not be conflated with the
  same-version case, which is one component with two memberships.
- A component in two resolves depending on a name pinned at different versions
  in each — it reaches both, which is the FR-011b case and the reason edge
  count can rise for a repository whose packages did not change.
- A resolve that declares a lockfile which contains no packages — an empty
  resolve that is genuinely empty, which must remain distinguishable from one
  emptied by the defect in Story 1.
- A per-resolve document whose packages name resolves absent from that
  document. This is expected under FR-011a, not a dangling reference to be
  cleaned up, and any validation over split output must accept it.
- A repository with both declared and discovered resolves of the same name.
- Deduplication merging a component that carries membership with one that
  does not.
- A consumer reading an older document, produced before this feature, where
  membership is a bare string rather than an array.
- A consumer reading a NEW document with the OLD expectation — the failure is
  a mis-parse rather than an error, which is why FR-006b requires it be
  called out rather than left to be discovered.
- The same package reached by two resolves where one classifies as a
  development resolve and the other does not.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Resolve membership MUST carry every resolve that pins a
  component, expressed as a lexically sorted JSON array, because the
  underlying relation is many-to-many.
- **FR-001a**: The ordering MUST be deterministic and independent of read
  order, so two scans of one repository produce byte-identical membership.
- **FR-002**: Membership MUST survive deduplication. When two components
  describing one package merge, the result MUST name every resolve either
  side named.
- **FR-003**: Membership MUST NOT depend on the order in which components
  were read. Two scans of one repository MUST record identical membership.
- **FR-004**: The number of distinct resolves named across all components'
  membership MUST equal the number of resolves that contain at least one
  package. A resolve MUST NOT disappear because its packages were attributed
  elsewhere. Measured as SC-002 states it.
- **FR-005**: A component belonging to exactly one resolve MUST name that
  same one resolve after this change — the *set* is undisturbed even though
  the encoding widens. Widening a field is not licence to change which
  resolves a package belongs to, and a component that gains or loses a
  resolve here is a defect, not a consequence.
- **FR-006**: Membership MUST decode to the same value, in the same order, in
  every emitted format, so a consumer's partition does not depend on which
  format it reads. Identical *bytes* is not achievable and not required:
  CycloneDX spec'es a property value as a string, so an array is carried as
  JSON-in-string there, while SPDX 2.3 and SPDX 3 carry a real array inside
  their annotation envelope. The encoding is each format's business; the
  decoded value is the contract.
- **FR-006a**: A component belonging to one resolve MUST use the same array
  encoding as one belonging to several. A shape that changes with cardinality
  forces every consumer to handle two cases and gets the rare one wrong.
- **FR-006b**: The change to the common case MUST be stated in the release
  notes as a consumer-visible change, because a reader of the previous scalar
  will not fail — it will parse an array as an unexpected string and carry on.
  A silent mis-parse in a downstream security tool is worse than a loud one.
- **FR-007**: The document MUST name, at document scope, which resolves were
  declared by the repository and which were discovered by filename
  convention, as a JSON object. A count alone does not answer the question a
  consumer asks.
- **FR-008**: FR-007 MUST be answerable from the document alone, without
  access to the repository it describes.
- **FR-009**: A resolve discovered by convention MUST NOT be presented as
  though the repository declared it, and MUST NOT gain an anchor. The
  distinction milestone 868 drew is preserved, not softened.
- **FR-009a**: Partitioning MUST be possible for a repository whose resolves
  are all discovered, using membership alone. If an anchor turns out to be
  required somewhere in the emission path, FR-009's no-anchor rule is what
  has to give, and that is a decision to reopen rather than work around.
- **FR-010**: An operator MUST be able to request one SBOM per resolve.
- **FR-011**: A package belonging to several resolves MUST appear in each of
  those resolves' documents.
- **FR-011b**: When a component belongs to several resolves, a dependency it
  declares by bare name MUST resolve in **each** of those resolves, emitting
  one edge per resolve that resolves the name. Resolves pin independently, so
  one bare name can denote different versions in different resolves, and the
  component depends on each.
- **FR-011c**: Those edges MUST remain separable by resolve, so a per-resolve
  document contains only the edges belonging to its own resolve. This falls
  out of membership rather than needing the edge to be tagged: an edge belongs
  to resolve R when both its endpoints name R.
- **FR-011a**: A package's membership MUST NOT be narrowed to the document it
  appears in. A per-resolve document records the package's full membership,
  so a reader of one resolve's SBOM can tell the package is shared and with
  which resolves.
- **FR-012**: When a per-resolve split cannot produce a meaningful partition
  — no resolves, or none anchored — the operator MUST be told what happened
  rather than receiving an empty or partial result silently.
- **FR-013**: The per-resolve split MUST reuse the same membership the
  document exposes, so an operator's split and a consumer's own walk agree.

### Key Entities

- **Resolve**: a dependency-resolution boundary in a Pants repository, with
  its own lockfile and its own pinned versions. Either *declared* by the
  repository's configuration or *discovered* by filename convention; the two
  differ in how much the name can be trusted to mean ownership.
- **Membership**: the set of resolves that pin a given package. Plural by
  nature; singular in today's output, which is the defect. Carried as a
  lexically sorted JSON array so the value is stable across runs and
  unambiguous to parse.
- **Anchor**: the component a resolve's packages hang from, making the
  resolve walkable as a graph. Emitted for declared resolves only.
- **Partition**: the set of documents produced by splitting one scan per
  resolve. Correct only if membership is complete — an incomplete membership
  yields an under-reported partition that looks plausible.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a repository where two resolves pin one package at one
  version, both resolves name that package. Today exactly one does.
- **SC-002**: The count of distinct resolve names appearing on components
  equals the count of resolves containing at least one package. On the
  reported monorepo that is 20 against 24 today, with four resolves empty.
- **SC-003**: Two scans of one repository, with component read order
  perturbed, produce identical resolve membership.
- **SC-004**: For a repository of single-resolve packages only, every
  component's membership names the same one resolve it named before, and does
  so in the same encoding a multi-resolve component uses.
- **SC-005**: Every emitted format reports the same membership for the same
  package, in the same order.
- **SC-006**: Given only an emitted document, a reader can list which
  resolves were declared and which were discovered, and the two lists
  together account for every resolve mentioned on any component.
- **SC-006a**: A repository whose resolves are all discovered can still be
  partitioned by membership, and the partition contains the same packages an
  anchored repository's would.
- **SC-007**: A per-resolve split of a repository with N resolves containing
  packages produces N documents, and a package pinned by several resolves
  appears in each, carrying the same membership in every one.
- **SC-007a**: A component in two resolves whose declared dependency is pinned
  at different versions in each reaches **both** versions in the unsplit
  document, and exactly one of them in each per-resolve document.
- **SC-008**: A per-resolve split of a repository with no anchored resolves
  produces a stated outcome, not an empty directory.
- **SC-009**: Re-running the reported monorepo measurement shows every
  resolve that contains packages represented, and no resolve empty that is
  not genuinely empty.

## Assumptions

- The reported figures describe that repository at the stated commits and are
  treated as the problem statement, not as something reproduced here. Items
  #910 and #901 have landed since and do not touch membership or dedup, so
  the survival figures are expected to be unchanged; confirming that is a
  cheap first measurement rather than an assumption to build on.
- Milestone 868's refusal to anchor glob-discovered lockfiles stands. This
  feature makes the distinction visible; it does not relitigate it.
- Partitioning depends on membership, not on anchors — the anchor is a
  convenience for graph traversal. This is the assumption FR-009a exists to
  test, and it is load-bearing for the choice not to anchor discovered
  resolves. It is stated here because it has NOT been verified yet.
- Deduplication's existing rule — the winner is authoritative for a key both
  sides carry — remains correct for the single-valued annotations it was
  written for. Only genuinely plural values need different treatment.
- Existing per-component and doc-scope annotation carriers are sufficient;
  no new emission channel is expected. The annotation key itself is retained
  rather than retired, so the catalogue row widens rather than being replaced.
- Documents produced before this feature remain readable by whatever read
  them then; this feature changes what waybill emits going forward, not any
  document already written.
- The split machinery from milestones 215 and 219 is the substrate for
  Story 3; this feature adds a mode rather than a second mechanism.
- No change to which packages are discovered, only to how their membership is
  recorded and partitioned.
