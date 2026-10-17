# Feature Specification: a marker for a version that was never there to find

**Feature Branch**: `1156-version-absence-marker`
**Created**: 2026-10-16
**Status**: Draft
**Input**: #1216 population 1 — Pants resolve anchors carry an empty version with nothing on the wire saying the absence is by construction.

## Context

An empty `version` field has two causes that a consumer cannot tell apart:
the component never had a version to have, or waybill looked for one and
failed. The first is complete information. The second is a gap somebody
should act on. Invariant I8 exists to report components where the document
does not say which.

C151 `waybill:unresolved-reason` (milestone 236) covers the second cause.
Nothing covers the first in the general case — there are three narrow
markers (`waybill:component-tier` = `file`, `waybill:closure-role`,
`waybill:haskell-version-unresolved-reason`) and a Pants resolve anchor
matches none of them.

A Pants resolve anchor stands for a lockfile. It is not a package: there is
no upstream, nothing to fetch, and no version to resolve. Seven such anchors
across four corpus targets are therefore correct data with an incomplete
account of itself, and they are the entire I8 failing population except one
Go binary (population 2, out of scope here).

`docs/reference/sbom-invariants.md` already states the shape of the fix in
I8's own row: *"The fix is a marker that states the anchor is versionless,
not a wider accepted set here."* This feature is that marker. Widening the
invariant's accepted set is a consequence of the wire becoming more honest,
never a substitute for it.

## Clarifications

### Session 2026-10-16

- Q: What are the annotation's key and value? → A: `waybill:version-absence` = `by-construction` — the key names what is being explained, the value names why.
- Q: How general is the positive control? → A: A committed must-pass registry (`invariant-must-pass.toml`), companion to `invariant-declarations.toml`, seeded with the four I8 cells and open to any cell proven load-bearing.
- Q: Delivery shape, given the stale-declaration rule? → A: Two independently-green changes — emission plus catalogue row plus golden refresh first (declarations stay accurate), then acceptance plus declaration deletion plus registry and tests.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A consumer can tell "no version exists" from "resolution failed" (Priority: P1)

Somebody reading a waybill SBOM of a Pants repository sees a component with
an empty version. Today they must guess. After this change the component
carries a marker whose single meaning is *no version exists to be found for
this component; the absence is not a resolution failure* — so the consumer
reads the absence as a fact about the thing rather than as a hole in the
scan, and can act on the real holes instead.

**Why this priority**: This is the defect. Everything else in this feature
exists to prove this story is true or to keep it true. Shipped alone it
delivers the whole user-visible value.

**Independent Test**: Scan a Pants repository with both a Python and a JVM
resolve, and read the three emitted documents. Each resolve anchor carries
the marker, with the same value in all three formats. No other component
gains it.

**Acceptance Scenarios**:

1. **Given** a Pants repository with a Python resolve, **When** waybill
   emits CycloneDX, SPDX 2.3 and SPDX 3, **Then** the anchor component for
   that resolve carries the version-absence marker in all three documents
   with an identical value.
2. **Given** a Pants repository with a JVM resolve, **When** waybill emits
   the three formats, **Then** the same holds for the JVM anchor — the
   marker is namespace-independent.
3. **Given** a component whose version resolution was attempted and failed,
   **When** waybill emits it, **Then** it carries `waybill:unresolved-reason`
   and **not** the version-absence marker: the two are mutually exclusive
   accounts of the same empty field.
4. **Given** the four Pants corpus targets with refreshed goldens but
   acceptance not yet widened, **When** the invariant matrix runs, **Then**
   their I8 cells still fail with the kind their declarations pin, and the
   lane is green — this story is shippable before any harness change. The
   cells passing and the declarations being deleted are US2's outcome, not
   this one's.
5. **Given** the marker is a closed enum, **When** a document carries an
   unrecognised value for it, **Then** that is a defect in the emitter, not
   a consumer-side variation to tolerate.

---

### User Story 2 - The explanation is proven real, not vacuously accepted (Priority: P2)

A maintainer changing the Pants readers, or refreshing the corpus goldens,
needs the lane to fail if the anchors stop being explained — including the
case where they stop being *emitted*. Today an I8 cell has two green
spellings: "versionless components present, all explained" (`Pass`) and
"no versionless components at all" (`NotApplicable`). Both are green. If
all seven anchors vanished from the goldens tomorrow, every existing test
would stay green and the marker would be dead code nobody noticed.

**Why this priority**: The issue names this as a requirement, and the repo
has twice shipped a feature that could never fire
(`feedback_a_passing_test_may_exercise_nothing`,
`feedback_zero_results_can_mean_broken`). Without it, US1 is a claim rather
than a verified property. It is P2 rather than P1 only because US1 is what
a consumer sees.

**Independent Test**: Declare the four cells in the must-pass registry and
assert each reports `Pass`. Then delete the marker from one anchor on one
target and watch that target's cell go red with the
versionless-and-unexplained kind.

**Acceptance Scenarios**:

1. **Given** the four Pants corpus targets, **When** I8 is evaluated on
   each, **Then** each reports `Pass` — not `NotApplicable`, which would
   mean the anchors are no longer in the document at all.
2. **Given** a Pants target's golden, **When** the version-absence marker
   is removed from exactly one anchor, **Then** I8 on that target fails and
   names the versionless-and-unexplained kind.
3. **Given** a Pants target's golden, **When** an anchor's marker value is
   replaced with an unrecognised one, **Then** I8 on that target fails —
   acceptance is keyed to the pinned value, not to the key's presence.
4. **Given** the existing I8 mutation tests select their subject with a
   "first target that passes" search, **When** the anchors are present,
   **Then** the new control does not rely on that search: it names its four
   targets, because a floating anchor cannot detect a target dropping out.

---

### User Story 3 - An anchor that loses its edges is still explained (Priority: P3)

Invariant I7 reports components with no edge and nothing saying why. Resolve
anchors are reachable from the subject today, so they do not fail I7. If a
future change to the Pants edge wiring leaves one unreachable, the document
should still account for it rather than producing a second defect class for
the same seven components.

**Why this priority**: Forward-looking insurance, not a present failure. All
five current I7 declarations belong to a different population (#1214). It is
in scope because #1216 asks for it and it costs one line, and it is P3
because nothing observable changes today.

**Independent Test**: A component carrying the marker, with an empty version
and no edges, is accepted by I7 as explained — demonstrable without any
corpus target exhibiting it.

**Acceptance Scenarios**:

1. **Given** a versionless, edgeless component carrying the version-absence
   marker, **When** I7 is evaluated, **Then** the component is not reported.
2. **Given** the four Pants targets today, **When** I7 is evaluated,
   **Then** the outcome is unchanged from before this feature — the I7 arm
   adds acceptance without moving any current cell.

---

### Edge Cases

- **A golden refresh drops an anchor.** The target's I8 cell silently
  becomes `NotApplicable`, which is green. This is precisely what US2's
  control exists to catch, and the reason it must name its targets.
- **A filtered golden cannot show the marker.** `pants-example-javascript`'s
  golden is filtered to the npm surface (feature 675 FR-008), so a component
  outside that surface is absent from the hermetic view while present in the
  emitted document. The same asymmetry already makes that target's I5 cell
  visible only in the corpus lane. Any target whose goldens are filtered must
  be excluded from US2's named set rather than asserted and waived.
- **An anchor is both versionless and a failed resolution.** It cannot be:
  an anchor has nothing to resolve. Emitting both markers on one component
  is a contradiction and should be treated as an emitter defect.
- **Documents emitted before this change** carry no marker and remain in
  I8's failing population. This needs no migration path: the corpus goldens
  are refreshed by the emission change, and waybill does not read its own
  prior output for this property.
- **Post-dedup components.** An emitted anchor may represent several
  pre-dedup entries (resolve membership is singular on an entry and plural
  on an emitted component). The marker is a single-valued fact about the
  component and must not be multiplied or concatenated by dedup.
- **A fix lands without deleting its declarations.** The lane fails on a
  stale declaration by design (feature 1155 FR-011), so the deletions are
  not bookkeeping — they are part of making the acceptance change green.
  The same rule is what lets the emission change ship first: while the
  cells still fail with the pinned kind, the declarations are accurate
  rather than stale.
- **A cell is registered must-pass and then legitimately stops applying.**
  Removing a corpus target, or narrowing an invariant's precondition, can
  make a registered cell honestly `NotApplicable`. The lane fails, and the
  fix is to remove the entry with the reason recorded in the change — the
  same discipline the declarations file already imposes in the other
  direction. The failure is the point: it forces the question to be
  answered rather than absorbed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST emit the per-component annotation
  `waybill:version-absence` = `by-construction`, meaning *no version exists
  to be found for this component; the absence is not a resolution failure*,
  on every synthetic Pants resolve anchor.
- **FR-002**: The annotation MUST be emitted from both the Pants Python and
  the Pants JVM lockfile readers, with the same key and the same value — the
  fact is about anchors, not about a language namespace.
- **FR-003**: The annotation's value MUST be a closed enum. v1 recognises
  exactly one value, `by-construction`. Adding a value later MUST NOT
  require a new catalogue row, and the row MUST say so.
- **FR-004**: The annotation MUST appear in all three emitted formats
  (CycloneDX, SPDX 2.3, SPDX 3) carrying the same value, and MUST be
  registered in the format-mapping catalogue as a new row with a matching
  extractor per format.
- **FR-005**: The new catalogue row MUST carry a Principle V audit showing
  no standards-native carrier exists for "this component has no version by
  construction" in CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1. The row MAY cite
  the existing C160 audit for the "stands for a set, not acquirable"
  reasoning and MUST add the arm specific to the version field.
- **FR-005a**: **Principle V audit result, cited here as the Constitution
  requires** (full method and quotations in `research.md` R1, read against
  the schemas vendored at `waybill-cli/tests/fixtures/schemas/`):
  **KEEP-NO-NATIVE.** No construct in CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1
  expresses *this component has no version to have*. In all three formats
  the version field is the lone version-bearing property with no sibling
  that could explain an absence. The nearest candidates and why each fails:
  CDX `evidence.identity[field=version].confidence` grades confidence in a
  *concluded* value, so `0` means "unsure what the version is" — C151's
  semantic, which merging would defeat I8's purpose; SPDX 2.3 `NOASSERTION`
  is not a schema-permitted `versionInfo` value and states the author's
  posture rather than a fact about the component; SPDX 3
  `software_SoftwarePurpose` does offer `manifest` and `bom` but names what
  the component *is*, and exists in one format only, which is the parity
  gap Principle V's bridging clause covers. CDX `component.type`, CDX
  `compositions[].aggregate` and SPDX 2.3 `primaryPackagePurpose` were
  audited and rejected by C160 for the adjacent semantic and are cited, not
  re-argued.
- **FR-006**: Invariant I8 MUST accept the annotation as an explanation of
  an absent or empty version, keyed to the pinned value and not merely to
  the key's presence.
- **FR-007**: Invariant I7 MUST accept the annotation as an explanation of
  an edgeless component.
- **FR-008**: The written rationale in both checkers and in
  `docs/reference/sbom-invariants.md` MUST be rewritten rather than left
  standing. All three currently argue that resolve anchors are *not*
  explained, which was correct before this change and is wrong after it.
- **FR-009**: `waybill:component-kind` = `lockfile-resolve` MUST NOT become
  an accepted explanation for I7 or I8. It names what kind of thing a
  component is and reserves enum extension; binding "has no version" to its
  presence would make every future kind inherit a promise its name does not
  make. One fact per annotation.
- **FR-010**: The four I8 declarations for `pants-clojure-polyglot`,
  `pants-example-django`, `pants-example-jvm` and `pants-example-python`
  MUST be deleted in the same change that widens I8's accepted set — not in
  the change that lands the emission. Until acceptance widens, the cells
  still fail with the kind those declarations pin, so the declarations are
  accurate and deleting them early would fail the lane.
- **FR-011**: The I8 declaration for `image-postgres16` MUST remain. It is
  population 2 — a real resolution gap rather than a labelling one — and is
  out of scope.
- **FR-012**: The positive control MUST be a committed must-pass registry
  of `(invariant, target)` cells, companion to the existing declarations
  file and named `invariant-must-pass.toml`. A cell listed there MUST
  report `Pass`; `NotApplicable` and `Fail` both fail the lane. It records
  cells by name rather than searching for a target that happens to pass,
  because a search cannot notice a target dropping out of a population.
- **FR-012a**: The registry MUST be seeded with the four Pants I8 cells and
  MUST be open to any cell, under any invariant, later shown to be
  load-bearing. The mechanism is general even though this feature's use of
  it is not.
- **FR-012b**: A cell MUST NOT appear in both the must-pass registry and
  the declarations file. The two make contradictory claims about the same
  cell, so the lane MUST reject the overlap rather than let one silently
  win.
- **FR-012c**: The registry MUST carry, per entry, the reason that cell is
  load-bearing — what a `NotApplicable` there would mean. An entry whose
  reason is "it passes today" is the vacuity this registry exists to
  prevent, restated one level up.
- **FR-012d**: The registry MUST be sorted by `(invariant, target)` and
  line-stable, matching the declarations file, so an unrelated addition
  does not move an existing entry (the milestone 117 retrofit precedent).
- **FR-013**: A mutation test MUST show that removing the marker from one
  anchor turns that target's I8 cell red with the
  versionless-and-unexplained kind.
- **FR-014**: The corpus goldens for the four affected targets MUST be
  refreshed through the corpus workflow dispatch, never generated locally
  (`docs/development/refreshing-corpus-goldens.md`, rule zero).
- **FR-015**: The refreshed golden diff MUST be confined to the marker on
  the seven anchor components. Any other difference MUST be explained
  before merge rather than absorbed into the refresh.
- **FR-016**: The invariant matrix MUST remain hermetic — the new
  acceptance reads only annotations already present in committed goldens
  and MUST introduce no network or filesystem access.
- **FR-017**: The work MUST be delivered as two changes, each of which
  passes the lane on its own:
  1. **Emission** — the annotation on both readers, the C197 catalogue row
     with its Principle V audit, the three per-format extractors, and the
     12 refreshed goldens. The declarations are untouched and remain
     accurate, because acceptance has not widened yet.
  2. **Acceptance** — I7 and I8 accept the marker, the four I8
     declarations are deleted, the must-pass registry is introduced and
     seeded, and the positive control and mutation tests land. It is
     verified against goldens already on the default branch.
- **FR-018**: The two changes are ordered, not symmetric. The emission
  change MUST be green on its own, before the acceptance change exists.
  The acceptance change MUST be green against the emission change already
  merged — it depends on goldens carrying the marker and is not shippable
  first. What is forbidden is merging either in a state whose verdict
  cannot be read without the other: a reviewer MUST be able to run the gate
  on the change in front of them and get a real answer.

### Key Entities

- **Resolve anchor**: a synthetic component standing for one lockfile in a
  Pants repository. Has a name and a namespace, no version, no upstream and
  no vulnerability surface of its own. Seven exist across four corpus
  targets.
- **Version-absence marker**: `waybill:version-absence`, the new
  per-component annotation. Closed enum; the one value recognised in v1 is
  `by-construction`. Complement of C151 `waybill:unresolved-reason`: C151
  means *looked and failed*, this means *there was nothing to look for*.
  "Version-absence marker" is the canonical prose term for it throughout
  this spec.
- **Catalogue row**: the entry in the format-mapping reference binding one
  annotation to its three per-format carriers. Gated in both directions — a
  row without an extractor and an extractor without a row both fail.
- **Cell outcome**: the result of one invariant on one target. Four states;
  the two that matter here are `Pass` (the precondition was met and the
  property holds) and `NotApplicable` (the precondition was never met).
  Collapsing them is the vacuity trap the matrix exists to close.
- **Declaration**: a committed record of one non-passing cell, pinned by
  kind. A fixed cell with a surviving declaration fails the lane.
- **Must-pass entry**: a committed record of one cell that must report
  `Pass`, carrying the reason that cell is load-bearing. The declarations
  file's mirror image: declarations say "this is known to be broken",
  must-pass entries say "this is known to be meaningful". A cell in both is
  a contradiction.

## Success Criteria *(mandatory)*

### Measurable Outcomes

Unless a criterion names a change, it describes the end state after both
of FR-017's changes have landed.

- **SC-001**: All four Pants corpus targets report I8 as `Pass`. Zero of
  them report `NotApplicable`.
- **SC-002**: I8's failing cell count across the seventeen corpus targets
  drops from 5 to 1, and the one remaining is `image-postgres16`.
- **SC-003**: `invariant-declarations.toml` contains exactly one I8
  declaration after the change, down from five.
- **SC-004**: Removing the marker from one anchor on any one of the four
  targets makes that target's I8 cell fail. Verified by a test that fails
  if the mutation is reverted.
- **SC-005**: The marker is present on all 7 anchors × 3 formats = 21
  component records, with one distinct value across all 21.
- **SC-006**: The golden refresh touches 12 files (4 targets × 3 formats)
  and no others, and within them changes only the 7 anchor components.
- **SC-007**: The I7 cells of all seventeen targets are unchanged by this
  feature.
- **SC-008**: The matrix still runs in the default test lane with no
  network, and the existing no-new-I/O gate still passes.
- **SC-009**: The catalogue's both-direction gate passes: the new row has
  an extractor in each of the three formats and no extractor lacks a row.
- **SC-010** *(DEFERRED — see below)*: The must-pass registry contains the
  four Pants I8 cells, each with a stated reason, and no cell appears in
  both it and the declarations file.
- **SC-011** *(DEFERRED — see below)*: Forcing any one registered cell to
  report `NotApplicable` fails the lane and names the cell. Verified by a
  test, since a registry that cannot fail is the same vacuity one level up.

**SC-010 and SC-011 are deferred to a follow-up spec**, decided 2026-10-17.
The must-pass registry turned out to be general machinery for all 136 cells
carried inside a four-cell change, and it moves to its own spec rather than
being dropped. FR-012's literal requirement is still met here by the
positive control that names the four targets and asserts `Pass` rather than
merely not-fail. What the split gives up is stated plainly: nothing in this
feature stops the *same* vacuity on any other cell, and these four are
protected only by a test that names them. The two criteria must be reported
as deferred, never as met.
- **SC-012**: Each of the two changes passes `clippy --workspace
  --all-targets` and `cargo test --workspace --no-fail-fast` on its own,
  with the emission change leaving all five I8 declarations intact and
  accurate.

## Assumptions

- **The anchors are reachable today, so I7 does not currently fail on
  them.** Verified: all five current I7 declarations belong to #1214's
  population (Go, npm, Haskell, Bazel, uv-fork). The issue's "accepted by
  both I7 and I8" therefore describes insurance, not a second present
  failure, and this spec sizes US3 accordingly.
- **The catalogue's current high-water mark is C196**, so the new row is
  C197. Verified by reading the catalogue rather than assuming.
- **The annotation is `waybill:version-absence` = `by-construction`**
  (settled in Clarifications, 2026-10-16). The key names what is being
  explained and the value names why, which keeps the enum closed. Three
  alternatives were rejected: a `*-reason` spelling mirroring C151, which
  borrows the naming of an annotation whose values are free-text reason
  strings and so imports the wrong expectation for a closed enum; the value
  `not-a-package`, which states what the component *is* — a fact that
  already sits beside it as `component-kind` = `lockfile-resolve`, and one
  fact per annotation; and a bare `waybill:versionless` = `true` flag,
  against which `waybill:component-tier` is this repo's receipt — a
  flag-shaped annotation whose name promised a vocabulary, only ever
  written as one value, with docs still claiming a `binary` tier the source
  contradicts. The chosen key also generalises: `component-tier` = `file`
  and `closure-role` are the same *kind* of fact about different
  populations, so a later change could collapse them under this key. That
  collapse is explicitly **not** in scope.
- **Raising C160 `waybill:component-kind`'s contract was considered and
  rejected.** It would cost zero wire churn and would make existing
  documents interpretable, but C160 is a taxonomy slot that reserves enum
  extension; binding version absence to it makes every future kind inherit
  a promise, and the first versioned kind forces the split anyway plus a
  migration. FR-009 records this as a constraint rather than a preference.
- **Population 2 is out of scope.** The `gosu` Go binary on
  `image-postgres16` is a resolution gap, not a labelling one: the Go build
  info that carries a module version is plausibly present and unread.
  Settling it needs the real binary, not more reading.
- **Goldens are never generated locally.** Two receipts: #818 (an arm64
  macOS perf baseline against Linux CI — nine days of failures across five
  release tags) and #832 (fixture content depending on whether the host had
  git-lfs — three nights, two wrong diagnoses). The refresh is two workflow
  dispatches with a review between them.
- **The golden comparison is byte-for-byte after masking and does not
  tolerate array reordering, while the review tool sorts arrays.** A diff
  the review tool calls clean can still fail the lane. If that happens the
  fix belongs in the masking, not in the review tool.
- **The registry is named `invariant-must-pass.toml`, deliberately not
  `invariant-expectations.toml`.** The harness reserves "expectation" for a
  target-specific check and "invariant" for a universal property, and
  enforces the split with a source-level terminology gate (feature 1155
  FR-020/FR-020a) precisely so the two can be told apart by name alone.
  Calling this file "expectations" would put a registry of required
  outcomes under the name of a category of checks. The name is cheap to
  change at plan time; the collision is not.
- **A resolve anchor is never a split document's subject**, so the marker
  needs no special handling in the per-resolve mode. Verified in the
  source: `--split=resolve` selects by membership filter on
  `waybill:pants-resolve`, and a resolve discovered by filename convention
  has no anchor component at all. An anchor that does exist appears in such
  a document as an ordinary member component and carries the marker like
  any other. The corpus matrix evaluates unsplit documents regardless.
- **The user stories map onto the two changes.** US1 is the emission
  change; US2 and US3 are the acceptance change. US1 is therefore shippable
  and reviewable before the harness work exists, which is what makes the
  split honest rather than cosmetic.
- **No new dependencies.** Annotation emission, the three extractors, the
  must-pass registry and the harness changes all use crates already in the
  workspace — the registry is TOML, which the harness already parses for
  the declarations file.
