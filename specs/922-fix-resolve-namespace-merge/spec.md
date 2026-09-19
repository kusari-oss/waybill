# Feature Specification: Same-named resolves in different Pants namespaces are different resolves

**Feature Branch**: `922-fix-resolve-namespace-merge`
**Created**: 2026-09-19
**Status**: Draft
**Input**: User description: "919"

Addresses [#919](https://github.com/kusari-oss/waybill/issues/919), a defect
in shipped code (v0.9.0), found while specifying #914.

## Context

`[python.resolves]` and `[jvm.resolves]` are **separate namespaces** in
`pants.toml`. A Python resolve named `default` and a JVM resolve named
`default` are different things that happen to share a label.

`--split=resolve` groups on the bare resolve name, so it merges them into one
document whose contents are the union of two unrelated resolves. Observed on
the collision fixture:

```
default.generic.cdx.json
  components = [ pkg:maven/dev.waybill.fixture/waybill-fixture-jvmside@1.0.0,
                 pkg:pypi/waybill-fixture-pyside@1.0.0 ]
```

A consumer asking for "the production Python resolve" silently receives
another namespace's packages.

### A second symptom, not in the issue

If the colliding pair are a repository's **only** resolves, the split sees one
group, decides the repository is not partitionable, and emits a single unsplit
SBOM with a warning:

```
WARN no partitionable Pants resolves detected — emitting a single SBOM per the
     --split fallback contract.  detected=1  mode="resolve"
```

So in the simplest case the defect does not merely merge two documents — it
produces **no split at all**, and hides behind the degenerate-split fallback.
The collision fixture carries a third, uncollided resolve purely so the split
runs and the merge is observable.

### The constraint that shapes this feature

The issue asks whether the grouping and the annotation should be fixed
together. They are not separable, and that is a finding rather than a
preference.

Both components in a merged document carry **identical** membership:

```
pkg:maven/…/waybill-fixture-jvmside@1.0.0   waybill:pants-resolve = ["default"]
pkg:pypi/waybill-fixture-pyside@1.0.0       waybill:pants-resolve = ["default"]
```

Nothing in the component set distinguishes them. Milestone 912 added a
resolve-name → namespace index, but it is document-scope: enough to say *this
document represents `python:default` and `jvm:default`*, not enough to say
*this component belongs to the Python one*. **Regrouping requires namespace
information per component, which does not exist today.**

That is why this is a correctness milestone with a consumer-visible surface,
rather than a one-line change to a grouping key.

### What milestone 912 already settled

A per-resolve document states which resolve it is, namespace-qualified
(`waybill:document-resolve`, catalogue row C163). On a merged document it
states **both** identities, because the document genuinely represents both and
naming either alone would be false.

That statement is defined to stay correct once this lands: a document that
represents one resolve states one identity. No consumer contract changes when
this ships — the plural case simply stops arising.

## Clarifications

### Session 2026-09-19

- Q: How should a consumer learn which namespace a resolve belongs to? → A: **A new per-component annotation carrying the namespace, leaving the existing membership annotation unchanged.** Additive: nothing that parses today stops parsing, and a consumer that does not care about namespaces is unaffected. Chosen over qualifying the existing membership values in place (`["python:default"]`), which is tidier — one field, no pairing question — but spends a **second** consumer-visible break on that same field within one release cycle: v0.9.0 just changed it from a bare string to an array, and asking consumers to absorb another change to the same key immediately afterwards is a worse trade than an extra catalogue row. Chosen over fixing only the grouping and keeping the namespace internal, which is the smallest milestone and leaves the emitted document unable to answer a question it demonstrably gets asked — the same class of gap #914 just closed, reintroduced one layer down. The costs accepted are a new catalogue row with its three extractors, and corpus-golden movement on every Pants target.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Two resolves that share a name produce two documents (Priority: P1)

A repository declares `default` under both `[python.resolves]` and
`[jvm.resolves]`. Splitting by resolve produces one document per resolve, each
containing only its own packages.

**Why this priority**: it is the defect. Everything else is either what makes
it possible or what stops it recurring.

**Independent Test**: split the collision fixture. Expect two documents for
the two `default` resolves; today expect one containing both.

**Acceptance Scenarios**:

1. **Given** a repository declaring one name in two namespaces, **When** it is
   split by resolve, **Then** each resolve gets its own document.
2. **Given** those documents, **When** their components are compared, **Then**
   no package from one namespace appears in the other's document.
3. **Given** those documents, **When** each is asked which resolve it is,
   **Then** each states exactly one resolve, and the two differ.
4. **Given** a repository whose resolve names do **not** collide, **When** it
   is split, **Then** the output is unchanged from before this feature.

---

### User Story 2 - A consumer partitioning on membership can tell them apart (Priority: P2)

A consumer that groups components by `waybill:pants-resolve` itself, rather
than using `--split=resolve`, can distinguish a Python `default` from a JVM
`default`.

**Why this priority**: the split is one consumer of membership. Fixing only
the split leaves everyone who partitions the unsplit document with the same
defect, and the emitted SBOM still cannot answer the question. Whether this
requires a wire change is the open decision below.

**Independent Test**: from a single unsplit document of the collision fixture,
partition components by resolve. Two partitions, not one.

**Acceptance Scenarios**:

1. **Given** an unsplit document from a repository with colliding names,
   **When** components are grouped by their resolve, **Then** packages from
   the two namespaces fall into different groups.
2. **Given** a document from a repository with no collision, **When** a
   pre-existing consumer reads membership, **Then** it keeps working.

---

### User Story 3 - A repository whose only resolves collide still splits (Priority: P3)

A repository declaring `default` in both namespaces and nothing else produces
two documents rather than falling back to one unsplit SBOM.

**Why this priority**: it is the simplest reproduction of the defect and the
one most likely to be a real repository. It is separated because it is a
consequence of US1 rather than additional work — but it needs its own test,
since a fix that regroups correctly while still counting one group would leave
this case silently unsplit.

**Independent Test**: a fixture with exactly the two colliding resolves. Two
documents, no fallback warning.

**Acceptance Scenarios**:

1. **Given** such a repository, **When** it is split by resolve, **Then** two
   documents are produced and the not-partitionable fallback does not fire.
2. **Given** a repository with genuinely one resolve, **When** it is split,
   **Then** the fallback still fires — the degenerate case is still degenerate.

---

### Edge Cases

- A component belonging to resolves in **two** namespaces at once. Across
  ecosystems this cannot arise from a package coordinate — a PyPI package
  cannot be pinned by a JVM resolve — but `pkg:generic/*` entries from
  non-PyPI Pex sources are not obviously bounded, and the answer determines
  whether per-component namespace is singular or plural.
- A repository using the conventional names (`python-default`, `default`),
  which does not collide and must stay byte-identical.
- A consumer reading a document produced before this feature, where the
  namespace is absent.
- Three or more resolves sharing one name, if Pants ever adds a third
  namespace.
- A resolve name that already contains the namespace separator.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Two resolves sharing a name across Pants language namespaces
  MUST produce two documents under `--split=resolve`, each containing only its
  own resolve's components.
- **FR-002**: The grouping MUST NOT depend on the bare resolve name alone.
- **FR-003**: A repository whose only resolves are a colliding pair MUST
  split, not fall back to a single unsplit document (FR-001's simplest case,
  which the fallback currently hides).
- **FR-004**: A repository with no name collision MUST produce byte-identical
  output to before this feature. The fix must be invisible where the defect
  was absent.
- **FR-005**: The namespace MUST be recorded per component, because the
  emitted component set is the only thing the grouping can consult and it
  cannot distinguish the two resolves today.
- **FR-006**: A consumer partitioning components by resolve MUST be able to
  distinguish resolves that share a name, from the emitted document alone. The
  namespace is carried as a **new per-component annotation**; the existing
  membership annotation keeps its current key and value shape.
- **FR-006a**: The addition MUST be additive. A consumer reading membership
  today MUST keep working unchanged, and a consumer that does not care about
  namespaces MUST be able to ignore the new annotation entirely.
- **FR-006b**: The new annotation MUST carry the same value across all three
  formats, and MUST be registered in the format-parity catalogue with its
  extractors. A row carried by one emitter and not the others fails the
  project's parity gate.
- **FR-006c**: Whether a component's namespace is singular MUST be established
  by measurement, not assumed. Across ecosystems it cannot be plural — a PyPI
  package cannot be pinned by a JVM resolve — but `pkg:generic/*` entries from
  non-PyPI Pex sources are not obviously bounded. If a component can belong to
  resolves in two namespaces, the annotation needs a plural shape and the
  pairing question returns.
- **FR-007**: A document produced before this feature MUST remain readable;
  the namespace's absence MUST be distinguishable from a namespace of empty.
- **FR-008**: The per-document resolve identity (C163) MUST continue to state
  exactly the resolves its document represents — one, once this lands. No
  change to its shape or meaning.
- **FR-009**: The fallback for a genuinely single-resolve repository MUST
  survive. Only the miscounting caused by the merge is being fixed.
- **FR-010**: A regression gate MUST exercise a namespace collision. The
  defect shipped because nothing in the corpus or the milestone suites has
  two namespaces with a shared name.

### Key Entities

- **Resolve**: a dependency-resolution boundary declared in `pants.toml`.
  Identified by a name **and** the namespace that declares it; the name alone
  is not an identifier, which is the whole defect.
- **Pants language namespace**: `[python.resolves]` or `[jvm.resolves]`.
  Already a closed set in the codebase after milestone 912, recorded at
  document scope only.
- **Resolve membership**: which resolves pin a component. Plural, and
  currently carrying bare names.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A repository declaring one name in two namespaces produces two
  documents. Today it produces one.
- **SC-002**: No component appears in a document for a resolve that does not
  pin it. Today the merged document contains packages from both namespaces.
- **SC-003**: Each resulting document states exactly one resolve identity.
  Today the merged document correctly states two.
- **SC-004**: A repository whose only resolves collide produces two documents
  rather than one unsplit SBOM.
- **SC-005**: For every non-colliding repository, split output is
  byte-identical to before — measured across the existing Pants fixtures and
  corpus targets, not asserted.
- **SC-006**: A consumer can partition an unsplit document's components by
  resolve and separate the two namespaces.
- **SC-007**: A pre-existing consumer of resolve membership keeps working on a
  document from a non-colliding repository, and the membership annotation's
  key and value shape are unchanged from v0.9.0.
- **SC-009**: The namespace annotation decodes to the same value in all three
  formats.
- **SC-008**: The regression suite fails if the grouping reverts to the bare
  name.

## Assumptions

- The namespace is determined by which reader produced a component, not
  inferred from its PURL ecosystem. Ecosystem inference happens to work on
  every current fixture because each fixture resolve is single-ecosystem; it
  would mis-qualify a polyglot resolve silently. This was already decided in
  milestone 912 and is not reopened.
- Pants has exactly two language namespaces that declare resolves today. The
  design should not assume exactly two, but no third is specified.
- The collision fixture added by milestone 912
  (`waybill-cli/tests/fixtures/pants_namespace_collision/`) is the starting
  regression gate. It carries a third, uncollided resolve so the split runs at
  all; a variant without that third resolve is needed for FR-003.
- Corpus goldens WILL move on every Pants target, because a new per-component
  annotation appears on every component carrying resolve membership. That is
  expected rather than a warning sign, and is now cheap to review: the corpus
  lane emits a readable masked diff as of #921.
- A polyglot Pants corpus target is worth adding, since the issue notes
  nothing in the corpus exercises this. Treated as in scope for the
  regression-gate requirement (FR-010) rather than as a separate feature.
- No change to which components, edges or resolves are discovered. This is
  about how discovered resolves are identified and grouped.
- Milestone 912's document identity is already correct for the post-fix world
  and needs no change.
