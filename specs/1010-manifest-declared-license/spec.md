# Feature Specification: The scanned project's declared license reaches its SBOM

**Feature Branch**: `1010-manifest-declared-license`
**Created**: 2026-09-26
**Status**: Draft
**Input**: Extend the Haskell pattern from #957 so the declared license of the scanned project reaches its SBOM across the remaining main-module readers, reading the license from the manifest each reader already parses and canonicalising it before emission.

## Context

A project's declared license is the one fact it definitely knows about itself, and
it is stated plainly in the manifest every reader already parses. Today it does not
reach the emitted SBOM for eleven of the twelve ecosystems that produce a
main-module component.

This matters more for the scanned project than for its dependencies. A dependency's
license can be recovered later by any consumer holding its PURL, because the
dependency is a published package with a registry record. The scanned project
usually is not published — `hackage.haskell.org/package/moat` returns 404 — so
enrichment has no record to draw on. If the license is not read from the manifest
at scan time it is absent from the document permanently.

It also means an **offline scan carries no license data at all**, for any
component, because enrichment is currently the sole supplier.

### Measured baseline (committed corpus goldens, `main` @ `5453c6f2`)

| Target | Components carrying any license |
|---|---:|
| rust-ripgrep | 0 / 68 |
| python-flask | 0 / 108 |
| maven-guice | 0 / 61 |
| npm-express | 0 / 45 |
| haskell-aeson | 2 (after #957) |

Across all thirteen corpus targets, **zero** carry a license on the document's
root component.

### What #957 already established

The Haskell reader was fixed first and proves the approach end to end. `aeson`
emits:

```json
{ "name": "aeson", "purl": "pkg:hackage/aeson@2.3.2.0",
  "licenses": [ { "license": { "acknowledgement": "declared",
                               "id": "BSD-3-Clause" } } ] }
```

Three things follow from that. The first and third are settled and not reopened;
the second was deliberately revisited during clarification:

1. **No precedence rule is required.** Declared and concluded licenses occupy
   different wire slots. A manifest declaration is the project's own assertion and
   populates the declared slot; third-party enrichment continues to populate the
   concluded slot. They cannot overwrite one another.
2. **An unverified string is never presented as a valid identifier.** A manifest may
   carry a string that is not a valid license expression — a legacy spelling, a bare
   filename, a proprietary marker. #957 handles this by discarding the value.
   This feature **supersedes** that (FR-004): the raw declaration is preserved as a
   custom non-listed license reference instead, because the standard provides a
   construct for a license outside its own list, and a proprietary marker is often
   the most legally significant thing a manifest says. The invariant #957 got right
   and that is retained: an unrecognised string is never emitted *as though* it were
   a recognised license identifier.
3. **The information is already in hand.** Each reader parses the manifest that
   carries the field. In the cargo reader the parsed table sits three lines above
   the point where the empty license list is constructed.

### Two distinct components, and how each gets a license

A scan emits both a **main-module component** (the project as its ecosystem names
it — `aeson`, `ripgrep`) and a **synthetic scan-root** used as the document's
primary component (`pkg:generic/haskell-aeson@682162c`). They are different things:
the main-module derives from a manifest, the scan-root derives from the directory
name and revision and has no manifest behind it.

#957 fixed the main-module. The scan-root still carries no license and cannot read
one directly, because nothing declares a license for a directory. It therefore
**inherits** one, but only when that inheritance is unambiguous: when the scan
contains exactly one main-module component. A scan containing none, or several
that may disagree, leaves the scan-root without a license rather than asserting a
value no manifest declares.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An auditor asks what the scanned project is licensed under (Priority: P1)

Someone receives an SBOM for a project and needs to answer the first question any
SBOM is asked: under what terms is this thing licensed. They read the component
representing the project itself and expect to find the license its manifest
declares.

**Why this priority**: This is the unrecoverable case. Every other component's
license can be looked up later from a registry; this one cannot, because the
project may never be published. It is also the question asked most often.

**Independent Test**: Scan a project whose manifest declares a license, in any one
of the affected ecosystems, and confirm the main-module component carries that
license marked as declared. Delivers value with a single ecosystem implemented.

**Acceptance Scenarios**:

1. **Given** a project whose manifest declares a recognised license expression,
   **When** it is scanned, **Then** the main-module component carries that
   expression, attributed as declared by the project rather than concluded by a
   third party.
2. **Given** the same project, **When** it is scanned with no network access,
   **Then** the license is still present, because it came from the manifest rather
   than from enrichment.
3. **Given** a project whose manifest declares no license, **When** it is scanned,
   **Then** the main-module component carries no license and the scan succeeds
   without warning — absence of a declaration is not an error.

---

### User Story 2 - A manifest declares something that is not a valid license expression (Priority: P2)

A project declares a license string its ecosystem permits but that is not a valid
SPDX expression — a pre-SPDX spelling, a filename, or a proprietary marker. The
SBOM must not present that string as though it were a verified license identifier.

**Why this priority**: Correctness of what is emitted matters more than coverage.
A wrong license identifier is more damaging than a missing one, because downstream
compliance tooling will act on it. Ranked below P1 because it is the exception path.

**Independent Test**: Scan a project whose manifest declares an uncanonicalisable
string and confirm the raw text is preserved as a custom reference rather than
either dropped or passed off as a valid identifier, with a debug diagnostic naming
the value.

**Acceptance Scenarios**:

1. **Given** a manifest declaring a string that cannot be canonicalised,
   **When** it is scanned, **Then** the raw declaration is preserved as a custom
   non-listed license reference, its text is recorded at document level, and it is
   nowhere presented as a recognised identifier.
2. **Given** the same project, **When** the operator raises log verbosity,
   **Then** a diagnostic names the manifest, the rejected value, and the reason.
3. **Given** a manifest declaring a license in a form that differs from its
   canonical spelling but is recognisable, **When** it is scanned, **Then** the
   canonical form is emitted.

---

### User Story 3 - Coverage is consistent across ecosystems (Priority: P3)

An operator scanning a polyglot estate gets the same treatment of declared
licenses regardless of which ecosystem a project is written in, rather than one
ecosystem behaving differently from its neighbours.

**Why this priority**: Consistency is the reason this is one feature rather than
eleven. It delivers no new capability beyond P1 repeated, so it ranks last — but
uneven coverage is itself a defect, and currently the same field is read from every
dependency's manifest in one ecosystem while being ignored in the project's own.

**Independent Test**: Scan one project per affected ecosystem and confirm each
main-module component is treated identically with respect to its declared license.

**Acceptance Scenarios**:

1. **Given** one project per affected ecosystem, each declaring a license,
   **When** each is scanned, **Then** every main-module component carries its
   declared license.
2. **Given** an ecosystem whose manifest format carries no license field at all,
   **When** a project in it is scanned, **Then** no license is emitted and this is
   documented as expected rather than appearing as an inconsistency.

---

### Edge Cases

- A manifest declares **multiple** licenses (an array, or several entries). These
  are combined by the reader into one expression using the ecosystem's documented
  operator (FR-010). Getting this wrong inverts legal meaning: joining a
  choose-either list with a conjunction asserts a consumer must satisfy every
  license when the project said any one would do.
- A manifest declares a license **by file reference** rather than by identifier
  (`license-file`, `PackageLicenseFile`). Resolving it requires reading file
  content, which is out of scope; the reference alone is not a license identifier.
- A manifest declares a license and enrichment later concludes a **different** one.
  Both are retained in their respective slots; the disagreement is visible rather
  than resolved silently.
- A **workspace** declares a license at the root and its members inherit it without
  restating it. Resolved per FR-011a. This is the common case rather than an exotic
  one: both of waybill's own member crates declare `license.workspace = true`, so a
  scan of this repository would return nothing for either without resolution.
- A project declares a license the ecosystem treats as valid but that is
  proprietary or non-identifying (for example a marker meaning "all rights
  reserved"). Preserved as a custom reference per FR-004, not discarded — such a
  marker is often the single most legally significant thing the manifest says.
- The same project is discovered through **two** readers (a Pants repository is
  both a Pants target and a native-ecosystem project) and the two disagree. Out of
  scope: two discoveries of one project are already merged by the existing
  reconciliation pass, and deciding which declaration wins is that pass's concern,
  not an extraction concern. Recorded so the omission is deliberate.
- A manifest is **malformed or partially parseable** — license extraction must not
  turn a recoverable parse into a failed scan (FR-007, covered by a dedicated task).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST read the declared license from the manifest that
  already identifies the main-module component, for every affected ecosystem, and
  attach it to that component.
- **FR-002**: An emitted license MUST be attributed as **declared by the project**,
  distinct from a license **concluded by a third party**, in every output format
  that distinguishes the two.
- **FR-002a**: (Principle V native-construct audit) each target format was audited
  for an existing construct before any `waybill:*` property was considered, and one
  exists for every signal, so **none is introduced**. "Declared by the project" is
  `licenses[].license.acknowledgement: "declared"` in CycloneDX 1.6, `licenseDeclared`
  in SPDX 2.3, and the declared-attribution expression element in SPDX 3. "Concluded
  by a third party" is `acknowledgement: "concluded"` / `licenseConcluded`. "Not on
  the standard list" is `LicenseRef-<id>` plus its extracted-text record. This audit
  also discharges Principle XII.2, which requires externally-sourced data to carry
  provenance: the declared-versus-concluded distinction **is** that provenance,
  expressed natively, which is why a `waybill:license-source` annotation was declined.
- **FR-003**: Licenses supplied by external enrichment MUST continue to populate the
  concluded attribution unchanged. This feature MUST NOT alter enrichment behaviour.
- **FR-004**: A declared value that cannot be canonicalised into a valid license
  expression MUST still be preserved, as a custom non-listed license reference
  carrying the raw declared text, rather than discarded. The fact that the project
  declared *something* is itself information, and the standard provides a construct
  for a license outside its own list.
- **FR-004a**: Resolution MUST be a two-step ladder: attempt strict canonicalisation
  first and emit the canonical identifier on success; only on failure fall back to
  preserving the raw text as a custom reference. A value that canonicalises MUST
  NEVER be emitted as a custom reference.
- **FR-004b**: A fallback to a custom reference MUST produce a debug-level
  diagnostic naming the manifest, the value, and why canonicalisation failed, so the
  gap is visible without inspecting output.
- **FR-004c**: A custom reference MUST NOT be presented as though it were a
  recognised license identifier in any format, and MUST be accompanied by the
  document-level record of its raw text that each format requires.
- **FR-005**: A declared value that is valid but non-canonically spelled MUST be
  emitted in canonical form.
- **FR-006**: Absence of a license declaration MUST be treated as ordinary, not as
  an error or a warning.
- **FR-007**: License extraction MUST NOT cause a scan to fail. A manifest that
  parses for identity but not for its license field MUST still yield its component.
- **FR-008**: The affected ecosystems are those whose main-module manifest carries a
  license field: **cargo, npm, pip, gem, maven, composer, elixir, erlang, scala,
  cocoapods, nuget**.
- **FR-008a**: Haskell, already implemented by #957, MUST be brought into line with
  FR-004's preservation behaviour. #957 discards an uncanonicalisable value; leaving
  it that way would make the one completed ecosystem the only one that loses a
  declaration, which is precisely the unevenness User Story 3 exists to prevent.
- **FR-009**: Ecosystems whose manifest format carries **no** license field
  (Go modules, Swift packages, Dart pubspec) MUST be left unchanged, and the reason
  MUST be documented so the gap is not read as an oversight.
- **FR-010**: Where a manifest declares **more than one** license, the reader MUST
  combine them into a **single** expression using the operator its own ecosystem
  documents — disjunction where the ecosystem defines a list as a choice among
  licenses, conjunction where it defines a list as cumulative obligations. All
  declared licenses MUST be represented; none may be silently discarded.
- **FR-010a**: The operator chosen for each ecosystem MUST be traceable to that
  ecosystem's own documentation. Where an ecosystem documents no semantics for a
  multi-license list, the reader MUST join with **conjunction**, and this assumption
  MUST be recorded per ecosystem in the reader documentation rather than left
  implicit. Conjunction is chosen because it over-states the obligation rather than
  under-stating it: a consumer complying with more licenses than required cannot
  breach one, whereas a consumer told any single license suffices can.
- **FR-010b**: A reader MUST NOT push several separate license values and rely on a
  downstream default to combine them. The reader is the only layer that knows its
  ecosystem's semantics; combination decided anywhere else cannot be correct except
  by coincidence.
- **FR-011**: A license declared only by **file reference** MUST NOT be emitted as
  an identifier, since resolving it requires reading file content.
- **FR-011a**: A license a component **inherits** from a workspace root or parent
  manifest MUST be resolved and attached, following the inheritance rules the
  ecosystem itself defines. A component whose manifest declares inheritance rather
  than a literal value MUST NOT be treated as declaring nothing.
- **FR-011b**: Where an inherited value cannot be resolved — the referenced root is
  absent, unreadable, or declares no license — the component MUST carry no license
  and the scan MUST succeed. An unresolvable inheritance is a missing declaration,
  not an error.
- **FR-012**: Behaviour MUST be identical with and without network access — a
  declared license MUST be present in a fully offline scan.
- **FR-013**: Stale source comments deferring license detection to the closed issue
  #103 MUST be corrected. Five occurrences exist: `cargo.rs:634`,
  `pip/mod.rs:614`, `npm/walk.rs:506`, `golang/legacy.rs:965`, and
  `golang/legacy.rs:4185`.
- **FR-014**: The cargo test asserting that the main-module emits **no** license
  MUST be replaced by one asserting the declared license is present, so the
  regression guard points the right way.
- **FR-015**: Output MUST remain deterministic: repeated scans of identical input
  MUST produce identical licenses in identical order.
- **FR-016**: The synthetic scan-root component MUST inherit the declared license
  when the scan yields **exactly one** main-module component carrying one. When the
  scan yields none, or more than one main-module component, the scan-root MUST carry
  no license — inheritance must never combine or arbitrarily choose between
  declarations made by different projects.
- **FR-017**: An inherited scan-root license MUST carry the same declared
  attribution as the main-module it came from, since its origin is still a project's
  own manifest.

### Key Entities

- **Declared license**: a license expression stated by the project in its own
  manifest. Attributed to the project. Sourced at scan time; available offline.
- **Concluded license**: a license expression attributed to a third party that
  examined the artifact. Sourced from enrichment; requires network access.
- **Main-module component**: the component representing the scanned project as its
  own ecosystem names it, derived from a manifest.
- **Scan-root component**: the synthetic component used as the document's primary
  component, derived from directory name and revision, with no manifest behind it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For every corpus target whose manifest declares a license, the
  main-module component carries it. Baseline is **0 of 13** targets carrying a
  license on any project-representing component today.
- **SC-001a**: For every corpus target that resolves to exactly one main-module
  component with a declared license, the document's primary component carries that
  license. Baseline is **0 of 13**.
- **SC-002**: An offline scan of a project declaring a license yields that license.
  Baseline: an offline scan currently yields **no** license for any component,
  because enrichment is the only supplier.
- **SC-003**: Every affected ecosystem behaves identically for the same input shape:
  a declared valid license appears, a declared invalid one does not, and an absent
  one produces no diagnostic.
- **SC-004**: No manifest value that fails canonicalisation is presented as a
  recognised license identifier in any format; each instead appears as a custom
  reference with its raw text recorded at document level.
- **SC-004a**: No declared license value present in a manifest is absent from the
  output entirely — every declaration is either canonicalised or preserved verbatim
  as a custom reference. Measured as: count of declarations found equals count
  emitted.
- **SC-004b**: For every ecosystem permitting a multi-license declaration, a project
  declaring two licenses yields one expression whose operator matches that
  ecosystem's documented semantics. Verified against a fixture per such ecosystem.
- **SC-004c**: A scan of a workspace whose members inherit a root-declared license
  yields that license on every member. Verified against this repository, where two
  member crates inherit and the baseline yields nothing for either.
- **SC-005**: No component loses a license it carried before this change, in any
  format — coverage only increases.
- **SC-006**: Licenses attributed as concluded are unchanged in count and value
  against a pre-change scan of the same input.
- **SC-007**: Repeated scans of identical input produce byte-identical license data.
- **SC-008**: Zero source comments remain that defer license detection to the
  closed issue #103.

## Assumptions

- The declared-versus-concluded distinction already modelled is the correct home for
  this data, so no new field or annotation is required to carry it.
- The canonicalisation behaviour established by #957 is the precedent for the
  success path. Its drop-on-failure behaviour is **superseded** by FR-004, and #957
  must be updated to match (FR-008a).
- The custom-reference construct is already emitted in every output format and is
  already fed by the OS-package readers, so preserving a raw declaration is expected
  to require no change to emission. To be confirmed during planning.
- Most ecosystems do not define multi-license list semantics, so the conjunctive
  fallback of FR-010a is the common path rather than the exception. Verified in Phase
  0: **composer** documents disjunction for its array form; **gem** states explicitly
  that its array does not express how licenses combine; **maven** states nothing.
  **npm**, **pip**, **cargo**, **nuget** and **cocoapods** take a single expression,
  so no operator is chosen for them at all. Recorded as an assumption rather than a
  requirement because it describes the world, not an obligation on waybill.
- Every affected reader already parses the manifest carrying the license field, so
  no new file reads or parsers are needed. Verified for cargo; to be confirmed per
  reader during planning.
- Corpus goldens will need regenerating, since this changes emitted output for every
  affected ecosystem. The corpus lane is presently red for unrelated accumulated
  drift (#1008), which is expected to be resolved first so that this change's diff
  is readable in isolation.
- Dependency components are out of scope except where they share a construction path
  with the main-module, in which case improved coverage is an accepted side effect
  rather than a goal.
- No new third-party dependencies are required.

## Out of Scope

- **Detecting a license from LICENSE file content.** Ecosystems whose manifests
  carry no license field can only be served by matching license text, which is a
  materially different mechanism with its own accuracy and dependency questions.
  This is why Go, Swift and Dart are excluded.
- **Resolving licenses declared by file reference** — same reason.
- **Changing enrichment**, including which sources it consults and which slot it
  writes to.
- **Reconciling a declared license against a concluded one.** Both are retained;
  neither is suppressed.
- **Declared licenses for dependency components.** A dependency's license is
  recoverable from a registry by any consumer holding its PURL; the scanned
  project's is not. Where a reader shares one construction path between its
  main-module and its dependencies, incidental coverage is accepted rather than
  suppressed, but it is not a goal and no reader is to be restructured to achieve it.
- **The shared emitter's unconditional `AND` join.** Where several license values
  reach emission, they are joined with a conjunction regardless of their source's
  intent. This feature avoids relying on that behaviour (FR-010b) rather than
  changing it, because the OS-package readers already depend on it and re-deciding
  their semantics is separate work. **This is an observed, unaddressed issue**, not
  a judgement that the current join is correct for them.
- **Recording which manifest supplied a license.** The declared attribution already
  states that the value came from the project rather than from a third party. A
  finer-grained source annotation would add a cross-format field with
  parity-catalog obligations for no additional decision-making power.

## Clarifications

### Session 2026-09-26

- Q: Should the synthetic scan-root component carry a license? → A: Inherit it when
  the scan yields exactly one main-module component carrying one; leave it absent
  when there are none or several. Captured as FR-016 / FR-017 and SC-001a.
- Q: Should dependency components get declared licenses too? → A: Main-module only.
  Incidental coverage from a shared construction path is accepted but is not a goal.
  Captured under Out of Scope.
- Q: Should the emitted license record which manifest supplied it? → A: No. The
  declared attribution already carries the necessary provenance. Captured under Out
  of Scope.
- Q: When a manifest declares a license string that will not canonicalise, what
  should be emitted? → A: Preserve it as a custom non-listed license reference with
  its raw text, rather than dropping it. Captured as FR-004/FR-004a/FR-004b/FR-004c,
  SC-004/SC-004a, and FR-008a, which brings #957 into line.
- Q: Should a license inherited from a workspace or parent manifest be resolved? →
  A: Yes, following each ecosystem's own inheritance rules; an unresolvable
  inheritance counts as a missing declaration rather than an error. Captured as
  FR-011a/FR-011b and SC-004c.
- Q: When a manifest declares several licenses, how should they be combined? → A:
  The reader builds a single expression using its own ecosystem's documented
  operator, rather than pushing several values and letting a downstream default
  combine them. Captured as FR-010/FR-010a/FR-010b and SC-004b. The shared emitter's
  unconditional conjunction is left unchanged and recorded under Out of Scope as an
  observed issue affecting the OS-package readers.
- Q: What operator applies where an ecosystem documents no multi-license semantics?
  → A: Conjunction, recorded per ecosystem in the reader documentation, with no
  annotation. It over-states the obligation rather than under-stating it, and agrees
  with the single combining rule already in the emitter. Captured as FR-010a; the
  supporting finding that this case is the majority rather than the exception is
  recorded under Assumptions, since it describes the ecosystems and not an obligation
  on waybill.
