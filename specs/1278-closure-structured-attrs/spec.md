# Feature Specification: a closure member's attributes are read wherever nix put them

**Feature Branch**: `1278-closure-structured-attrs`
**Created**: 2026-11-04
**Status**: Draft
**Issue**: #1348
**Input**: The nix closure reader reads only a derivation's flat `env`, so every
attribute declared under `structuredAttrs` in Determinate Nix's format-v4 output
is invisible: 554 dependency entries across 421 of 1,749 derivations, which
inflates the `unreferenced` role and understates the dependency graph.

## Why this exists

`nix derivation show` has two places to put a derivation's attributes. Normally
they are strings in a flat `env` map. When nixpkgs sets `__structuredAttrs =
true`, they move into a typed `structuredAttrs` object — arrays stay arrays —
and `env` is left holding only the output paths.

waybill reads `env`. It has never read `structuredAttrs`; the identifier does
not appear anywhere in `waybill-cli/src/`. For a derivation that uses the
typed form, every dependency field the role classifier depends on reads as
absent, and absent is indistinguishable from "nothing depends on this".

This is the failure shape the project already has a name for: a check that did
not run looks identical to a check that passed. Here, an edge that was not read
looks identical to an edge that does not exist.

**The sharp consequence.** Milestone 1277 shipped three days ago to fix exactly
one defect: *a closure member that only builds the artifact must not read as
part of it.* It did that by asserting CycloneDX `scope: "excluded"` from the
member's role. Measured on the committed golden, **9 components carry
`scope: "excluded"` sourced from `waybill:scope-classification-source =
"closure-role"` — the role alone, with no independent lifecycle support — and
that role is wrong.** Their corrected role is `artifact-input` or `both`, which
m1277's own FR-002 forbids asserting any scope for, because that is precisely
the case where waybill cannot tell whether the member ships. `autoconf`,
`automake`, `docbook-xml`, `tzdata` and `util-linux-minimal` are claimed as
excluded while being referenced both ways; `bash-interactive`, `elfutils`,
`keyutils` and `libedit` are claimed as excluded while being artifact inputs.

So m1277's defect class is already back, through a different door. It is not a
regression in m1277 — the rule it encodes is right. The input to that rule is
wrong.

**And the damage is not confined to a `waybill:` property.** Planning found
that the same golden carries, in CycloneDX's own vocabulary,
`{"aggregate": "complete", "dependencies": ["nix-closure-moat@d069055"]}` — a
schema-blessed assertion that the root's dependency relationships are complete,
meaning "no further dependencies are known to exist", in a document whose
closure skipped 554 declarations. A consumer that has never heard of waybill
reads that field and believes it.

## Clarifications

### Session 2026-11-04

- Q: Where must the "a declaration was not interpretable" report be visible? → A: A new document-scope annotation. C190 `waybill:nix-closure-degraded` cannot carry it — it is emitted only when no closure was recorded at all, and this is a closure that *was* read with some declarations unread. Constitution XII.3 requires degraded enrichment be disclosed in the document, not only logged, and #1115 was that defect.
- Q: What should an unrecognized `nix derivation show` format version do? → A: Nothing to the reads — resolution stays version-agnostic, driven by which attribute shape is present. The observed format version is recorded in the new annotation instead, so the output shape a document was built from is in the document rather than inferred later.
- Note (Phase 0, R1): the first answer above is **refined, not reversed**, by
  the native-carrier audit. The *fact* of incompleteness has native carriers in
  two of the three formats — CycloneDX `compositions[].aggregate` and SPDX
  3.0.1 `Relationship.completeness`, both verified against the vendored
  schemas — so Principle V puts it there. The new annotation carries the
  *detail* no format expresses, and for SPDX 2.3, which has no completeness
  field at all, it carries the fact too. Choosing the annotation over a log
  line stands; letting it stand alone beside a native field still claiming
  `complete` would not.
- Q: How wide should the uninterpretable-declaration check look? → A: It reports **every** dependency-shaped field a derivation declares that classification does not consult, as a diagnostic. Classification stays on the seven fields, so this milestone's role census remains attributable to the shape fix alone, while the report measures #1346's field-coverage gap for free.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - the role reflects every way nix references a member (Priority: P1)

An operator scans a Nix flake and reads the `waybill:closure-role` annotation,
and the native scope markers derived from it, to decide whether a closure
member is part of what ships. Today a member whose referencing derivation used
the typed attribute form is reported as referenced by nothing, and is then
either dropped from the document or marked as excluded from the artifact.

**Why this priority**: it is the only story that corrects a false claim already
in shipped output. Every other story in this spec adds or completes
information; this one retracts an assertion the project's own rules forbid
making. It also establishes the single seam — the accessors on the derivation
record — that the remaining stories reuse.

**Independent Test**: scan the `nix-closure-moat` corpus target and compare the
`waybill:closure-role` value and native scope marker of the nine named
components against the committed goldens. Delivers value alone: the false
exclusions are gone whether or not anything else in this spec lands.

**Acceptance Scenarios**:

1. **Given** a derivation that names its dependencies in the typed attribute
   form, **When** the closure is classified, **Then** the members it names are
   credited to the artifact or tooling role exactly as if the names had been in
   the flat form.
2. **Given** a member referenced only through a typed-form dependency field,
   **When** the document is emitted, **Then** it does not carry a native marker
   asserting it is absent from the artifact.
3. **Given** a member whose corrected role is `artifact-input` or `both`,
   **When** the document is emitted, **Then** no scope is asserted for it,
   per the existing rule that waybill cannot tell whether such a member ships.
4. **Given** a dependency field present in the typed form but holding an empty
   list, **When** the closure is classified, **Then** no member is credited —
   an empty list is not a reference.

---

### User Story 2 - patch evidence is complete (Priority: P2)

An operator relies on the patch records a closure contributes, including the
CVE identifiers in patch filenames, as the evidence that a vulnerability was
addressed in the build rather than left open. A derivation that lists its
patches in the typed form contributes none of them today.

**Why this priority**: patch evidence is the reason unreferenced members are
kept at all — milestone 1035 measured `jq` and `lua` as unreferenced while
carrying 6 of one project's 18 CVEs. Under-reporting it weakens the one claim
the closure tier makes about vulnerability remediation. It is P2 rather than P1
because it adds missing evidence rather than retracting a false claim, and
because it shares US1's seam, so it is nearly free once US1 lands.

**Independent Test**: count the patch records and the distinct CVE identifiers
emitted for the corpus target, and confirm the 28 patch entries that 7
derivations declare in the typed form are present.

**Acceptance Scenarios**:

1. **Given** a derivation listing patches in the typed form, **When** patch
   evidence is collected, **Then** each listed patch appears against that
   component.
2. **Given** such a derivation whose patch filename names a CVE, **When** the
   document is emitted, **Then** that CVE appears in the distinct-CVE record.

---

### User Story 3 - a member that declares its identity typed still becomes a component (Priority: P2)

An operator expects every closure member that declares a package name and
version to appear as a component. A member that declares them in the typed
form has no name to build an identity from today, so it is emitted as nothing
at all — not a wrong component, no component.

**Why this priority**: a missing component is invisible to every downstream
consumer, and unlike a wrong role it leaves no trace to notice. It is P2
because the measured population on the pinned closure is small (11
derivations), and because it is a distinct accessor from US1's, so it neither
blocks nor is blocked by it.

**Independent Test**: count components emitted for the corpus target and
confirm the identities that only the typed form declares are present.

**Acceptance Scenarios**:

1. **Given** a derivation declaring its package name and version only in the
   typed form, **When** components are emitted, **Then** it appears as a
   component under that name and version.
2. **Given** a derivation declaring a package name in neither form, **When**
   components are emitted, **Then** it is skipped as it is today — this story
   adds a source for the name, it does not invent one.

---

### User Story 4 - the next shape change is loud, not silent (Priority: P3)

A maintainer upgrades the nix the corpus runs against, or nixpkgs changes
which derivations use the typed form, and needs the run to say so rather than
quietly reclassifying members as referenced by nothing.

**Why this priority**: this defect survived three closure milestones and a
committed census because its symptom — a large `unreferenced` population — was
indistinguishable from the truth, and was in fact written up as a finding about
nix rather than about the reader. Without a positive signal the next shape
change repeats it. P3 because the first three stories are correct without it.

**Independent Test**: remove the typed-form reads and confirm the guard fails;
restore them and confirm it passes. The guard must fail for the defect this
milestone fixes, asserted against the pre-fix behaviour, not merely pass after.

**Acceptance Scenarios**:

1. **Given** a closure in which some derivation declares a dependency field
   that classification does not consult, **When** the closure is read, **Then**
   the emitted document discloses that a declaration went unconsulted, naming
   the field, rather than the member being treated as unreferenced.
2. **Given** a closure every declaration of which is consulted, **When** the
   closure is read, **Then** no disclosure is emitted in any format.
3. **Given** any closure, **When** it is read, **Then** the disclosure records
   the output format version it was read from.
4. **Given** a declaration in a field classification does not consult, **When**
   the document is emitted, **Then** the member's role is identical to what it
   would be without the disclosure — the report observes, it does not classify.

---

### Edge Cases

- **The same field in both shapes.** Measured: it does not occur on either
  closure examined. The reader must still have a defined precedence and must
  report a disagreement rather than silently picking one.
- **A typed field that is not a list of strings.** `doCheck` is a boolean,
  `env` is a nested object, `outputChecks` is a map. Reading a dependency field
  must tolerate any JSON shape without failing the scan.
- **A typed field holding a single string** rather than a list.
- **An empty list.** `buildInputs: []` is present on all 56 `source`
  derivations and must not credit anything.
- **Format versions beyond 4.** A version the reader has not seen is read
  exactly as a known one, by whichever attribute shape is present, and is
  recorded in the disclosure (FR-012, FR-013). It is never read as "no
  attributes anywhere", and it never degrades the tier on its own.
- **Existing hand-written fixtures are all flat-form.** Every one of them must
  keep producing the roles it produces today; this feature widens what is read
  and changes nothing about the flat form.
- **The root derivation stays unreferenced.** Nothing in the closure depends on
  the thing being built, in either shape. A caller treating `unreferenced` as
  "drop" would drop the artifact, which is existing behaviour and unchanged.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The reader MUST resolve a derivation's dependency fields from the
  typed attribute object when the flat map does not carry them, for every field
  the role classifier already consults — the three artifact fields and the four
  tooling fields — with no change to which fields those are.
- **FR-002**: The reader MUST resolve a derivation's patch list from the typed
  attribute object when the flat map does not carry it.
- **FR-003**: The reader MUST resolve a derivation's package name and version
  from the typed attribute object when the flat map does not carry them, and
  MUST preserve the existing distinction between a declared name and the
  fallback to the derivation's own name.
- **FR-004**: A typed field whose value is not a string or a list of strings
  MUST yield no paths and MUST NOT fail the scan.
- **FR-005**: An empty typed list MUST credit no member, exactly as an absent
  field does.
- **FR-006**: When a field is present in both shapes and the two disagree, the
  reader MUST prefer the flat map and MUST report the disagreement.
- **FR-007**: Every consumer of a derivation's attributes — role
  classification, component emission, patch collection and the nixpkgs
  declaration pass — MUST observe one and the same resolution rule. A consumer
  that resolves attributes differently from another is a defect, because the
  two would then disagree about the same derivation.
- **FR-008**: Emitted output MUST NOT assert a native marker claiming a member
  is absent from the artifact when that member's corrected role is
  `artifact-input` or `both`, which follows from the existing rule rather than
  adding a new one.
- **FR-009**: The emitted document MUST carry a document-scope disclosure,
  once per scan, when the closure was read but some declaration in it was not
  consulted. The disclosure MUST name the fields involved and how many
  declarations and derivations were affected, MUST be absent when there is
  nothing to disclose, and MUST be present in all three output formats with
  equal content. A log line alone does not satisfy this: a consumer cannot
  otherwise tell a fully-read closure from a partially-read one, which is the
  defect #1115 fixed for total closure degradation and this requirement
  extends to partial.
- **FR-010**: The committed corpus goldens MUST be refreshed in this milestone,
  through the existing workflow dispatch, never generated locally.
- **FR-011**: The milestone MUST record the corrected role census against the
  committed one, so the figures in the earlier closure milestones' research can
  be read as what they were measured through. Where an existing figure states
  no system, it MUST gain one; where it already states its system, it MUST be
  left alone.
- **FR-012**: The disclosure of FR-009 MUST additionally record the
  `nix derivation show` output format version the closure was read from, so
  which attribute shapes a document was built from is a fact in the document
  rather than something a later reader has to infer.
- **FR-013**: Attribute resolution MUST NOT be gated on the format version. A
  version the reader has not seen MUST be read exactly as a known one — by
  whichever attribute shape is present — because a version list is an
  allowlist that falls out of date, and the per-declaration check of FR-009
  is what actually has teeth against a shape change.
- **FR-014**: The disclosure of FR-009 MUST report every field a derivation
  declares that holds store paths and that role classification does not
  consult, not only the seven it does. This is diagnostic: it MUST NOT change
  any member's role, so that this milestone's census remains attributable to
  the attribute-shape fix alone.
- **FR-015**: The **fact** of incompleteness MUST be carried by each format's
  own completeness vocabulary where one exists, and a document with an
  unconsulted declaration MUST NOT carry a native claim that its dependency
  relationships are complete. The disclosure of FR-009 carries the detail
  those vocabularies cannot express. Its payload MUST be identical in all
  three formats; in SPDX 2.3, which has no completeness field, the
  disclosure's **presence** is the fact, so nothing is added to its payload to
  say so. A native field asserting `complete` beside an extension saying
  otherwise is worse than either alone, because every consumer that does not
  know waybill believes the native one.
- **FR-016**: The expected effect of the golden refresh MUST be recorded
  **before** the refresh is dispatched, and scored against the diff afterwards
  with any miss and what it corrected. A refresh whose expectations are
  written after the diff is a description of the diff, and cannot catch a
  change nobody anticipated.
- **FR-017**: The committed invariant declarations MUST be reconciled with the
  refreshed goldens: a declaration the fix makes stale is retired, and any new
  offender is declared with its reason and count. A stale declaration is a
  recorded expectation of a defect that no longer exists, which makes the next
  reader trust a passing run that proves nothing.

### Key Entities

Two names for one thing, used deliberately: this specification says **flat
map** and **typed attribute object** because they are what a reader of the
emitted document can reason about, while the plan, data model and tasks say
`env` and `structuredAttrs`, which are what nix calls them. They are the same
two carriers throughout.

- **Derivation record**: one closure member as nix reports it. Carries a flat
  attribute map, an optional typed attribute object, declared outputs, and a
  name. The two attribute carriers hold the same kinds of value in different
  encodings; which one is populated is nixpkgs' choice per derivation, not the
  operator's.
- **Dependency field**: a named attribute whose value is a list of store paths.
  Seven of them decide a member's role. The same resolution serves the patch
  list, which is also a list of store paths.
- **Closure role**: how nix references a member — into the artifact, as
  tooling, both, or neither. Derived entirely from the dependency fields, so a
  field the reader cannot see moves a member toward "neither".
- **Partial-read disclosure**: a new document-scope record, present only when
  something went unconsulted, naming the fields, the number of affected
  declarations and derivations, and the output format version the closure was
  read from. It is the consumer's only way to distinguish a fully-read closure
  from a partially-read one, which the existing total-failure disclosure
  cannot express because it is emitted only when no closure was recorded.
- **Scope classification source**: the existing published record of what a
  scope assertion rests on. The nine false exclusions all rest on the role
  alone, which is how they can be identified from emitted output without
  re-deriving anything.

## Success Criteria *(mandatory)*

All figures below are **measured** on the `nix-closure-moat` corpus target at
its pinned commit, nixpkgs `a799d3e3`, with the x86_64-linux attribute the
goldens pin, unless labelled otherwise. The probes are committed under
`measurements/`. Counts can move when nix or nixpkgs moves; SC-001 and SC-002
are the invariants, and the rest are the figures observed on 2026-11-04.

- **SC-001**: No component in any emitted document asserts, on the basis of its
  closure role alone, that it is absent from the artifact while nix references
  it into the artifact through any dependency field in either attribute shape.
  **Measured today: 9 components violate this.**
- **SC-002**: For every derivation in a closure, each dependency field that
  derivation declares is either consulted when deciding the referenced members'
  roles, or reported as uninterpretable. No declared field is silently skipped.
  **Measured today: 554 dependency entries across 421 of 1,749 derivations are
  silently skipped.**
- **SC-003**: The nine named components — `autoconf`, `automake`,
  `bash-interactive`, `docbook-xml`, `elfutils`, `keyutils`, `libedit`,
  `tzdata`, `util-linux-minimal` — carry the corrected role and no
  absent-from-artifact marker.
- **SC-004**: The derivation-level role census moves from 256 / 101 / 39 / 1,353
  (artifact-input / build-tooling / both / unreferenced) to 270 / 118 / 42 /
  1,319. The `unreferenced` population falls by 34 derivations and by no fewer.
- **SC-005**: 14 of the 362 role-annotated components in the committed golden
  change role, in the measured directions: 5 `build-tooling`→`both`, 4
  `unreferenced`→`artifact-input`, 3 `unreferenced`→`build-tooling`, 2
  `artifact-input`→`both`. No component moves toward `unreferenced`.
- **SC-006**: 13 identities that are absent from the committed golden are
  emitted, having been referenced all along. They include `hadrian`, `alex`,
  `hscolour`, `isl`, `gdbm` and `byacc`.
- **SC-007**: The 28 patch entries that 7 derivations declare in the typed form
  are present in the emitted patch evidence, and the distinct-CVE count does
  not fall.
- **SC-008**: The 11 derivations whose name and version are declared only in
  the typed form are emitted as components.
- **SC-009**: Every existing flat-form fixture produces byte-identical
  classification output. Every corpus target that does not scan a closure
  produces byte-identical goldens; targets that do scan one change only by
  gaining the FR-009 disclosure and, for `nix-closure-moat`, the corrections
  of SC-003 through SC-008.
- **SC-010**: Removing the typed-form reads makes the US4 guard fail, asserted
  by a test that holds the pre-fix behaviour. A guard that only passes after
  the fix is not evidence that it can fail.
- **SC-011**: The disclosure of FR-009 is absent from a document whose closure
  was read completely, and present with equal content in all three formats
  when it was not. Equal content across the three is checked, not assumed:
  documenting the field without emitting it, or emitting it in one format
  only, both fail.
- **SC-012**: The disclosure records the format version observed, asserted by a
  test rather than inferred from the emitted document. On the pinned corpus
  target today that value is `4`.
- **SC-013**: The disclosure names every store-path-holding field that
  classification does not consult. Measured on the pinned closure, the
  classifier consults 7 fields; the disclosure is expected to name the
  remainder that are actually declared, which is the first measured figure for
  #1346's coverage gap and is **not predicted here** — the gap's size is an
  output of this milestone, not an input to it.
- **SC-014**: Every member's role is identical whether or not the FR-014
  diagnostic is enabled. The census figures in SC-004 hold either way.
- **SC-015**: No document whose closure had an unconsulted declaration carries
  a native claim that its dependency relationships are complete. **Measured
  today: the `nix-closure-moat` golden carries exactly that claim**, as a
  CycloneDX `aggregate: "complete"` record over the root, while 554
  declarations went unconsulted. Both the CycloneDX and the SPDX 3 answers
  must move together, from the one place that decides them, so the two formats
  cannot disagree.

## Assumptions

- **The blind spot is live in CI, not an artifact of the local toolchain.**
  Established by control rather than assumed: a flat-map-only reimplementation
  of the classifier agrees with the committed golden on **347 of 348** shared
  components' roles, so the closure CI produced the goldens from has the same
  shape as the one measured here. `public-corpus.yml` installs Determinate Nix
  unpinned, which is the product family that emits the typed form. The one
  disagreement (`docbook-xml`) and the 9 golden names absent from the local
  closure are attributed to nix and nixpkgs drift since the goldens were cut,
  and are not explained further here.
- **The flat map wins a disagreement.** No disagreement occurs on either
  closure measured, so this is a defined default rather than an observed
  preference, and FR-006 requires it be reported if it ever happens.
- **There is no mislabelled census, and an earlier revision of this spec said
  there was.** The 1,275-derivation figure in the corpus manifest is an
  aarch64-darwin measurement, and the comment carrying it says so explicitly,
  warning that the counts differ from the CI closure the goldens pin. The
  claim that it was labelled x86_64-linux was wrong: it came from reading the
  sentence without its opening clause. Retained here rather than deleted
  because the retraction is the useful part — see `measurements/findings.md`
  F0. FR-011 is accordingly about *recording* the corrected census, not about
  fixing a label.
- **The identity scheme for unreferenced derivations is out of scope.** Keying
  `source`-named members on forge coordinates is decided and recorded, and is
  the subject of separate work. This milestone changes which derivations are
  unreferenced; it emits no derivation that is not emitted today beyond those
  its corrected roles admit.
- **Whether the corrected `unreferenced` population changes that separate
  work's premise is a question for it, not for this milestone.** The reason
  this feature is sequenced first is that it moves the population; quantifying
  the knock-on belongs to whatever spec consumes it.
- **No new dependency fields are *classified*.** The seven that decide a role
  are unchanged, so the census in SC-004 is attributable to the attribute-shape
  fix alone. That the classifier reads 7 of roughly 20 nixpkgs dependency
  attributes is a separate, already-filed gap (#1346); widening the classified
  list here would move the census for two independent reasons at once and make
  SC-004 uncheckable. The fields beyond the seven are *reported* under FR-014,
  which is why this milestone produces that gap's first measurement without
  acting on it.
- **Goldens are refreshed through workflow dispatch.** Local generation is
  never used.
- **The disclosure's documentation and its emission land together.** The
  format-mapping reference and the cross-format parity machinery are checked
  against each other, so a documented field that nothing emits is a failure
  rather than a to-do. The next free row number is C204; the plan confirms it
  rather than this spec reserving it.
- **Zero new dependencies.** Both attribute shapes are already present in the
  parsed document.
