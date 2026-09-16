# Feature Specification: Declared dependencies must resolve regardless of the requirer's PURL type

**Feature Branch**: `867-mainmod-depends-ecosystem`
**Created**: 2026-09-15
**Status**: Draft
**Input**: User description: "886"

## Context

A component that a reader emits carries a list of dependency *names* it
declared. Those names are turned into graph edges by looking them up in an
index keyed on `(ecosystem, name)`, where `ecosystem` comes from the
**requirer's own PURL type**.

When a reader emits a component whose PURL type does not match the ecosystem
its dependency names belong to, every one of those lookups misses and every
declared dependency is discarded. No diagnostic is emitted.

This is not hypothetical and not confined to one reader.

**Measured, `bitwarden/android` @ `d817f6b4bf7c17172a74fabca1e09e738c7ec6c9`**
(the m770 `gradle-bitwarden-android` target, scanned as the harness scans it):

| | outgoing edges on the application main module |
|---|---:|
| default | **0** |
| with the existing opt-in cross-ecosystem flag | **9** |

The 9 are exactly the `Gemfile.lock` `DEPENDENCIES` block. The reader read the
declaration, built the list, and the resolver discarded all of it. The
consequence is that the target's 103 gem components form a **disconnected
island** — 140 real internal edges, 2 graph roots, nothing reaching them from
the project — so reachable depth collapses to 1 and the document reports
itself flat.

**A second reader has the same shape by construction.** The NuGet reader's
documented version ladder falls back to `pkg:generic/<stem>@0.0.0` when no
version can be resolved, and separately populates that main module's declared
dependencies from the lockfile. Those names are NuGet names; the requirer is
`generic`. The existing test `main_module_version_ladder_falls_through_to_generic`
pins the generic PURL.

**The epistemic point that makes this a defect rather than a limitation.**
An opt-in cross-ecosystem *inference* capability already exists, and its own
implementation notes describe it as bridging generic main modules "and future
m216-alikes" to matching components. Inference across ecosystems is
speculative and is rightly opt-in and annotated as inference. But a
dependency a manifest **declares** is not inference. Dropping it is a false
negative in the primary graph, and the operator is given no signal that it
happened.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A declared dependency appears as a dependency (Priority: P1)

Someone generates an SBOM for a project whose tooling dependencies are
declared in a manifest the scanner already reads. They open the SBOM and look
at what the project depends on.

**Why this priority**: This is the whole defect. Everything else here is
either a generalisation of it or a diagnostic for when it cannot be done.
Without this the SBOM asserts that a project depends on nothing while listing
its dependencies as unattached components — a document that contradicts
itself.

**Independent Test**: Scan a project whose manifest declares dependencies in
an ecosystem different from the emitted project component's PURL type;
confirm each declared dependency is an outgoing edge of that component, with
no opt-in flag.

**Acceptance Scenarios**:

1. **Given** a project whose declared dependencies resolve to components
   present in the same scan, **When** an SBOM is generated with no optional
   flags, **Then** every declared dependency is an outgoing edge of the
   project component.
2. **Given** that same project, **When** the SBOM is generated, **Then** the
   components that were previously unreachable are reachable from the
   document root.
3. **Given** a project whose project component and dependencies share an
   ecosystem, **When** an SBOM is generated, **Then** its edges are unchanged
   from before this feature.

---

### User Story 2 - The same holds for every reader, not one (Priority: P2)

Someone scans a project in a different ecosystem whose project component also
ends up with a non-matching PURL type — for example because a version could
not be determined and the reader fell back to a generic identity.

**Why this priority**: The defect is a property of how declared dependencies
are resolved, not of any one reader. Fixing it for a single reader would
leave the same silent drop in every other reader that can emit a
non-matching identity, and would have to be re-fixed each time one is found.

**Independent Test**: Construct the fallback condition in a second reader and
confirm its declared dependencies resolve, using the same scenario shape as
User Story 1.

**Acceptance Scenarios**:

1. **Given** a project whose reader emits a fallback identity whose type does
   not match its dependencies' ecosystem, **When** an SBOM is generated,
   **Then** the declared dependencies are outgoing edges of that component.

---

### User Story 3 - A dependency that cannot be resolved is visible (Priority: P3)

Someone scans a project where a declared dependency names something no
component in the scan corresponds to.

**Why this priority**: Genuinely unresolvable names must still be dropped —
inventing a component for them would be a false positive. But the operator
should be able to tell the difference between "this project declares nothing"
and "this project's declarations went nowhere". Silence is what allowed the
original defect to survive undetected across multiple releases and multiple
readers.

**Independent Test**: Scan a project declaring a dependency that matches no
component and confirm the outcome is discoverable without reading source.

**Acceptance Scenarios**:

1. **Given** a declared dependency matching no component in the scan, **When**
   an SBOM is generated, **Then** no edge and no fabricated component is
   emitted for it, **And** the emitted document carries a non-zero unresolved
   count.
2. **Given** a component whose entire declared dependency list resolves to
   nothing, **When** an SBOM is generated, **Then** reading the document
   alone distinguishes that from a component that declared nothing at all.
3. **Given** a scan in which every declared dependency resolved, **When** an
   SBOM is generated, **Then** the same count field is present with value
   zero rather than absent.

---

### Edge Cases

- A declared name matches a component in a **different** ecosystem from the
  recorded one, and nothing in the recorded one. It stays unresolved: a
  same-named package in another ecosystem is a different package.
- A declared name corresponds to **several components within the recorded
  ecosystem** — for example one package present at two versions in a single
  scan, which does occur in practice. This is collapsed when the resolution
  index is built, one identity per name per ecosystem, and is unchanged by
  this feature. Noted so it is not mistaken for something introduced here;
  out of scope.
- A declared name matches the requirer itself — must not produce a self-edge.
- The recorded ecosystem is itself wrong or stale, so correct-looking
  candidates elsewhere are deliberately not reached. Accepted: FR-001a makes
  the recording the reader's assertion, and a wrong assertion is a reader
  defect to fix at the source rather than to paper over during resolution.
- A project declares dependencies but the scan captured none of them (for
  example a lockfile was absent), so every name is unresolvable.
- Components whose dependency names were *inferred* rather than read from a
  manifest must not gain edges they did not previously have.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A dependency that a reader read from a manifest MUST be
  resolvable to an edge whether or not the requirer's PURL type matches the
  dependency's ecosystem. This applies to **any** component, not only
  project/main-module components.
- **FR-001a**: The behaviour in FR-001 MUST apply only where the reader has
  explicitly recorded the ecosystem its dependency names belong to. Where a
  reader has recorded nothing, resolution MUST behave exactly as it does
  today. The recorded ecosystem MUST NOT be inferred or guessed on the
  reader's behalf — an absent recording means "unchanged", never "work it
  out".
- **FR-002**: Where a reader has recorded the ecosystem of its dependency
  names, that ecosystem — and only that ecosystem — MUST be searched.
  Candidates in other ecosystems MUST NOT be considered. Where nothing was
  recorded, the requirer's own PURL type MUST be used, exactly as today.
- **FR-003**: This behaviour MUST be on by default and MUST NOT require an
  optional flag.
- **FR-004**: A declared dependency matching no component MUST NOT produce an
  edge, and MUST NOT cause a component to be fabricated for it.
- **FR-005**: The system MUST report declared dependencies that resolved to
  nothing, both as an operator-visible log line and as a **document-scope
  count** carried in the emitted SBOM, so the signal survives the scan and is
  machine-readable.
- **FR-005a**: The count MUST be present and zero when every declared
  dependency resolved, so that "declared nothing" and "declarations all
  resolved" and "declarations went nowhere" are three distinguishable states
  rather than two.
- **FR-005b**: The count MUST be carried in a standards-native field where
  one exists for this meaning; a project-specific property is permitted only
  with a recorded justification naming the native field that was missing.
- **FR-006**: Cross-ecosystem ambiguity MUST NOT be resolvable by guessing.
  FR-002 removes the class outright by confining lookup to the recorded
  ecosystem: a name is either matched there or left unresolved. A candidate
  in a different ecosystem is not a weaker match for the same package, it is
  a different package, and emitting it would be a false positive.
- **FR-007**: Edges produced under this feature MUST be distinguishable, by a
  consumer reading the document, from edges produced by the existing opt-in
  cross-ecosystem *inference* capability, because the two carry different
  confidence.
- **FR-008**: Enabling the existing opt-in inference capability MUST NOT
  change any edge this feature already produces by default.
- **FR-009**: Scans of projects whose components and dependencies share an
  ecosystem MUST produce byte-identical output to before this feature.
- **FR-010**: Self-edges MUST NOT be emitted.

### Key Entities

- **Declared dependency**: a dependency name a reader read from a manifest,
  together with the ecosystem that manifest's names belong to. The second
  half is what is currently unavailable at resolution time, and recording it
  is what opts a reader into FR-001. It is a property of the manifest the
  reader parsed, not of the requirer's identity.
- **Resolution index**: the mapping from `(ecosystem, name)` to component
  identity, built once per scan from everything the scan found.
- **Requirer**: the component carrying the declared dependency list. Its PURL
  type is currently, and incorrectly, used as the ecosystem of its
  dependencies' names.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On `bitwarden/android` @ `d817f6b`, the project component's
  declared dependencies resolve to **9 of 9** edges with no optional flag,
  against **0 of 9** before.
- **SC-002**: On that same target, the gem components are reachable from the
  document root, and the document no longer reports itself flat.
- **SC-003**: A second reader, exercised through its non-matching-identity
  path, resolves its declared dependencies to edges with no optional flag.
- **SC-004**: Every scan whose components and dependencies share an ecosystem
  produces output byte-identical to before the change, demonstrated across
  the committed corpus.
- **SC-004a**: Every scan involving only readers that have not recorded a
  dependency ecosystem produces output byte-identical to before the change.
  This follows from FR-001a by construction and is the property that makes
  per-reader adoption safe, so it is asserted rather than assumed.
- **SC-005**: A declared dependency that matches no component produces no
  edge and no fabricated component, and the emitted document carries a
  non-zero unresolved count naming how many were dropped.
- **SC-005a**: A scan in which every declared dependency resolved emits the
  same count field with value zero, so a consumer can distinguish it from a
  document that never had declarations to resolve.
- **SC-006**: Turning on the existing opt-in inference capability changes no
  edge this feature produces by default.
- **SC-007**: Every claim above is demonstrated to **fail** against the
  pre-change build before it is accepted as passing, so no check is trusted
  that has not been observed failing.

## Assumptions

- **Component identity does not change.** Altering a project component's PURL
  type to match its dependencies would also resolve the mismatch, but PURL is
  the identity consumers key on. Changing it would move dedup, document
  identity and every golden, for a benefit this feature obtains without it.
  Out of scope; if it is ever wanted it belongs in its own change.
- **The reader knows the ecosystem of the names it read.** A reader parsing a
  Ruby lockfile knows those are gem names. This is treated as available
  information rather than something to be inferred, and recording it is the
  mechanism by which a reader adopts this feature (FR-001a). Adoption is
  therefore incremental and each reader's change is separately demonstrable.
- **"Declared" means read from a manifest.** Dependency lists a reader
  inferred by other means are out of scope, so this feature cannot turn an
  inference into a default edge.
- **Unresolvable stays unresolvable.** This feature changes which candidates
  are considered, not whether a component may be invented. Nothing here
  creates a component for a name the scan never saw.
- **The existing opt-in inference capability stays opt-in** and keeps its
  current annotations. This feature narrows what that flag has left to do; it
  does not replace or disable it.

## Dependencies

- The existing opt-in cross-ecosystem inference capability, whose scope this
  feature narrows and whose annotations must remain distinguishable (FR-007,
  FR-008).
- The m770 quality corpus, which holds the measured before/after evidence for
  SC-001 and SC-002 and whose `gradle-bitwarden-android` expectations cannot
  be re-authored until this lands.

## Out of Scope

- Changing any component's PURL type or identity.
- The separate, unrelated gap where a lockfile resolve's top-level
  requirements are attached to no owning component. Same visible symptom, a
  different cause, tracked on its own.
- Re-authoring the quality-corpus expectations for targets whose numbers were
  authored against fabricated edges. That follows this change rather than
  accompanying it.

## Clarifications

### Session 2026-09-15

- Q: Does this apply only to project/main-module components, or to any
  component carrying reader-declared dependency names?
  → A: Any component, but only where the reader has **explicitly recorded**
  the ecosystem of its dependency names. Readers adopt this one at a time; a
  reader that has not recorded it produces byte-identical output by
  construction.
- Q: What happens when a declared name matches components in more than one
  ecosystem?
  → A: It cannot. Lookup happens **only** within the recorded ecosystem, so
  cross-ecosystem candidates are never considered. A name matching nothing
  there is unresolved and reported, not resolved to another ecosystem's
  component.
- Q: In what form must an unresolved declared dependency be reported?
  → A: A log line **and** a document-scope count of declared dependencies
  that resolved to nothing, so the signal survives into the SBOM itself and
  is machine-readable. Which native or bridged field carries the count is a
  planning decision, subject to the standards-native-fields-first rule.
