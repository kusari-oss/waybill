# Feature Specification: A split document says which resolve it is

**Feature Branch**: `912-split-doc-self-describing`
**Created**: 2026-09-18
**Status**: Draft
**Input**: User description: "914"

Addresses [#914](https://github.com/kusari-oss/waybill/issues/914), filed
during [#902](https://github.com/kusari-oss/waybill/issues/902) item 4 and
deferred deliberately.

## Context

`--split=resolve` emits one SBOM per Pants resolve. A document produced that
way should be able to say which resolve it represents. Two of them cannot.

**A declared resolve's document does say so**, by accident of a different
mechanism: its anchor component is promoted to a main module inside the
projection, so the emitted root names the resolve.

**A discovered resolve has no anchor**, because a lockfile found by filename
convention carries a name that is a convention rather than a declaration of
ownership, and milestone 868 declined to assert otherwise. Its document
therefore names the repository. Observed on a two-resolve convention-only
fixture:

```
default.generic.cdx.json   root = pants_discovered_resolves
lint.generic.cdx.json      root = pants_discovered_resolves
```

The document-scope ownership statement does not close the gap either. It
describes the **repository**, not the document, and is byte-identical in both:

```
default.generic.cdx.json  ->  {"declared":[],"discovered":["default","lint"],…}
lint.generic.cdx.json     ->  {"declared":[],"discovered":["default","lint"],…}
```

So a reader holding one of these files sees two resolve names, a root naming
the repository, and nothing saying which of the two this file is.

Nothing is *unanswerable* today: the split manifest records `root_purl` per
entry, and the filename carries the resolve slug. Both are outside the
document. A file that has been renamed, copied into a ticket, uploaded to a
scanner, or otherwise separated from its manifest has lost the answer.

The obvious fix — synthesise an owning component for the discovered resolve —
was rejected when #902 item 4 shipped, and that rejection stands. A component
that owns a resolve's packages is an ownership claim; the repository never
made one. Naming a resolve is information. That distinction is the whole
reason milestone 868 left these unanchored, and it is not being reopened.

## Clarifications

### Session 2026-09-18

- Q: Is a resolve identified by its bare name? → A: **No — the identity must be unambiguous across Pants language namespaces**, and the separate grouping defect this exposes is filed against shipped code rather than fixed here. `[python.resolves]` and `[jvm.resolves]` are distinct namespaces in `pants.toml`, so one repository can legitimately declare `default` in both. A bare name would let a document claim to be `default` without saying which. The shape of this identity is precisely what this feature fixes, so settling it now avoids a second consumer-visible change to the same field later. Chosen over a bare name, which is simpler and matches what component membership already carries, but inherits an ambiguity into a field created specifically to remove one. Chosen over fixing the grouping here too, which would widen a metadata feature into a correctness fix — the same scope boundary that kept #902's correctness work from being held behind its design work.
- Q: Where should the identity live? → A: **A separate document-scope statement of its own**, not folded into the existing repository-wide ownership statement. The confusion this feature fixes is exactly that a document-scope field describes the repository rather than the document; putting a document-scoped fact in the same container reproduces it. A separate statement is absent entirely from an unsplit or non-resolve-split document, which is the correct shape rather than a field that has to be explained away. **Was conditional on a Principle V audit**, and the audit has since run (research R1). It did **not** come back empty: the document-subject carrier exists in all three formats and already works for a declared Python resolve. The answer stands anyway, on a different rationale than the one it was made on — that carrier names a component, and FR-006 forbids inventing the one a discovered resolve lacks, so the construct is unusable here rather than missing. The extension is justified by the parity gap around SPDX 3 `Bundle.context` instead. FR-001c carries both findings.
- Q: Does a declared-resolve document carry the identity too, when its root already names the resolve? → A: **Yes, every per-resolve document carries it.** A consumer that must first determine whether a resolve was declared, in order to know where to read its identity, is being asked the question it came to ask. The declared case therefore states its resolve twice — in the root component and in the identity — and FR-005 requires the two agree. Chosen over carrying it only where the root does not already say so, which removes the duplication but forces every consumer to check two places and branch on provenance. Chosen over dropping the root promotion so the identity is the single source: the root of a declared-resolve document is useful on its own terms — it is what makes the document read as an SBOM *of a thing* rather than a labelled bag of components — and trading that for tidiness is a bad exchange.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A document identifies itself (Priority: P1)

Someone holding a single split SBOM — with no manifest, no directory listing,
and no assumptions about its filename — can tell which resolve it describes.

**Why this priority**: it is the whole feature. Everything else here is a
consequence of doing it consistently.

**Independent Test**: take one document from a convention-only repository,
rename it, and read it on its own. The resolve it represents must be
recoverable from the content.

**Acceptance Scenarios**:

1. **Given** a split document for a resolve found by filename convention,
   **When** it is read in isolation, **Then** it states which resolve it
   represents.
2. **Given** that document, **When** its filename is changed, **Then** the
   answer is unchanged — the identity lives in the content, not the name.
3. **Given** a repository with several such resolves, **When** their documents
   are compared, **Then** each states a different resolve. Today they are
   indistinguishable on this point.
4. **Given** a split document, **When** a reader looks for the repository-wide
   picture, **Then** it is still there. Saying which resolve this is must not
   cost the reader what the repository contains overall.

---

### User Story 2 - Every split document answers the same way (Priority: P2)

A consumer reads the resolve identity the same way regardless of whether the
repository declared its resolves.

**Why this priority**: without it, a consumer writes two code paths — read the
root when the resolve was declared, read something else when it was not — and
must first determine which case it is in, which is the question it was trying
to answer. Correctness-wise US1 is sufficient; this is what makes it usable.

**Independent Test**: read the resolve identity from a declared-resolve
document and a discovered-resolve document using one procedure. Both answer.

**Acceptance Scenarios**:

1. **Given** a declared-resolve document and a discovered-resolve document,
   **When** the same procedure is applied, **Then** both yield that document's
   resolve.
2. **Given** a declared-resolve document, **When** its identity is read,
   **Then** it agrees with the root component, which already names the
   resolve. Two statements of one fact must not be able to disagree.

---

### Edge Cases

- A document produced by a split mode that is **not** per-resolve —
  `--split=workspace` or `--split=directory` — where "which resolve is this"
  has no answer.
- An unsplit document, which represents every resolve rather than one.
- A repository where a resolve name collides with the repository name.
- A repository declaring the same resolve name under two Pants language
  namespaces — `default` under both `[python.resolves]` and `[jvm.resolves]`.
  The identity must tell them apart. **Note**: the per-resolve split currently
  *merges* such resolves into one document, which is a defect in shipped code
  and is tracked separately; this feature must not depend on that being fixed,
  nor make it harder to fix.
- A resolve whose document contains components belonging to other resolves
  too, which is normal: membership is not narrowed per document, so a shared
  package names every resolve that pins it.
- A consumer reading a document produced before this feature, where the
  identity is absent.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A per-resolve split document MUST state which resolve it
  represents, readable from the document alone.
- **FR-001b**: The identity MUST be carried separately from the
  repository-wide ownership statement, so that one statement describes the
  repository and another describes this document. A reader must not have to
  know which parts of one value are document-scoped.
- **FR-001c**: Per Constitution Principle V, the audit was performed and
  **did not come back empty**. Two distinct native constructs bear on this
  semantic and both are cited here because the distinction is what justifies
  the extension:
  - The **document-subject carrier** — CycloneDX `metadata.component`, SPDX
    2.3 `documentDescribes`, SPDX 3 `rootElement` — exists in all three
    formats and already carries this semantic for a declared Python resolve.
    It is unusable for the rest because it names a **component**, and FR-006
    (Principle IX) forbids inventing the one a discovered resolve lacks. The
    native construct is therefore deliberately unused, not absent.
  - The one native construct that could carry a document's subject **without**
    a component is SPDX 3 `Bundle.context`. CycloneDX has no equivalent and
    SPDX 2.3 has none. That is the parity gap the extension bridges, and the
    catalogue row MUST name it as the justification clause.

  The existing resolve rows' KEEP-NO-NATIVE finding covers "which resolve owns
  this component", a different question that does not transfer.
- **FR-001a**: That statement MUST be unambiguous across Pants language
  namespaces. `[python.resolves]` and `[jvm.resolves]` are separate
  namespaces, so one repository can declare the same resolve name in both, and
  an identity that cannot tell them apart fails at the one job it has.
- **FR-002**: FR-001 MUST hold whether the resolve was declared by the
  repository or discovered by filename convention.
- **FR-003**: The statement MUST NOT depend on the document's filename, its
  location, or the presence of the split manifest.
- **FR-004**: Every per-resolve document MUST carry the identity and MUST
  answer by the same procedure, so a consumer needs one code path rather than
  one per provenance. This includes documents whose root component already
  names the resolve — a consumer must not have to establish provenance in
  order to learn where to read identity.
- **FR-005**: Where a document also has a root naming its resolve — the
  declared case — the two MUST agree, and that agreement MUST be asserted
  rather than assumed. Two fields stating one fact can drift, and a test is
  the only thing that keeps them honest.
- **FR-006**: A discovered resolve MUST NOT gain an owning component, in the
  split output or anywhere else. Identifying a resolve is information;
  asserting a component owns its packages is a claim the repository never
  made, and milestone 868's refusal stands.
- **FR-007**: The repository-wide ownership statement MUST remain
  repository-wide inside a split document. Narrowing it to the document's own
  resolve would remove the reader's view of what else exists, which is the
  same loss the per-resolve split already declines to inflict on component
  membership.
- **FR-008**: A document that is not a per-resolve split MUST NOT claim a
  resolve identity, because it does not have one.
- **FR-009**: A reader of a document produced before this feature MUST be able
  to tell that the identity is absent rather than empty.

### Key Entities

- **Split document**: one SBOM emitted per resolve. Carries that resolve's
  components, repository-wide context, and — after this feature — a statement
  of which resolve it is.
- **Resolve identity**: which resolve a document represents. Singular by
  definition, unlike component membership, which is plural. Must distinguish
  resolves that share a name across Pants language namespaces — the one thing
  a bare name cannot do.
- **Resolve provenance**: whether a resolve was declared or discovered.
  Already stated at document scope. Determines today whether the root happens
  to name the resolve, which is the inconsistency this feature removes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Given only the bytes of a per-resolve split document, a reader
  can name the resolve it represents. Today this fails for every
  convention-discovered resolve.
- **SC-002**: Two documents from one convention-only repository state
  different resolves. Today they are byte-identical on every doc-scope field
  that mentions resolves.
- **SC-002a**: Two resolves sharing a name across Pants language namespaces
  produce distinguishable identities.
- **SC-003**: Renaming a document does not change the answer.
- **SC-004**: One reading procedure answers for both declared and discovered
  resolves, with no branch on provenance.
- **SC-005**: For a declared resolve, the identity and the root component name
  the same resolve.
- **SC-006**: A split document still reports the repository's full set of
  resolves, unchanged from before this feature.
- **SC-007**: No document gains a component that was absent before — component
  counts per document are unchanged.
- **SC-008**: A `--split=workspace` or `--split=directory` document, and an
  unsplit document, carry no resolve identity — the field is absent, not
  present-and-empty.
- **SC-009**: The repository-wide ownership statement and the document
  identity are separately readable: removing either leaves the other intact
  and meaningful.

## Assumptions

- The rejection recorded when #902 item 4 shipped stands: no owning component
  is synthesised for a discovered resolve. This feature closes the gap the
  rejection left, rather than revisiting it.
- The split manifest continues to record `root_purl` per entry. This feature
  makes the document independently readable; it does not make the manifest
  redundant.
- Existing document-scope carriers are sufficient in mechanism — every format
  already carries document-scope statements — but a new statement is added
  rather than an existing one extended. No new emission channel is expected.
- Per-resolve documents keep full component membership, so a document may name
  resolves whose packages it does not contain. That remains correct and is
  unrelated to the document's own identity.
- Consumers reading documents produced before this feature keep working; the
  identity is additive.
- No change to which components, edges, or resolves are discovered. This is a
  statement about a document, not about a repository.
- The per-resolve split's grouping defect — same-named resolves from different
  Pants language namespaces merging into one document — is out of scope and
  filed separately. This feature defines an identity that will still be
  correct once that is fixed, rather than one shaped around the bug.
