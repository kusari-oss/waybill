# Feature Specification: A must-pass registry for invariant cells whose Pass is load-bearing

**Feature Branch**: `1217-must-pass-registry`
**Created**: 2026-10-17
**Status**: Draft
**Issue**: #1227 (Refs #1216, #1155)
**Input**: User description: "A must-pass registry for invariant cells whose
Pass is load-bearing: a committed file recording, per (invariant, target)
cell, that the Pass carries meaning, so a cell cannot quietly flip to
NotApplicable and keep reading green. The mirror image of
invariant-declarations.toml, seeded with the four Pants I8 cells, attached as
a sibling corpus-level pass, with its own mutation test. Closes #1227 and
carries the deferred SC-010 and SC-011 of feature 1156 plus its eleven
deferred tasks T026-T031, T035-T038 and T042."

## Why this exists

The invariant matrix grades each (invariant, target) cell with one of four
outcomes, and two of them are green:

| outcome | meaning | grid | lane |
|---|---|---|---|
| `Pass` | the invariant looked at the document and every case it found was sound | `.` | green |
| `NotApplicable` | the invariant found nothing to look at | `-` | green |

Both verdicts are correct. A cell whose precondition is unmet is not a
defect, and failing the lane on one would make the matrix unusable. But the
two say opposite things, and nothing distinguishes them after the fact.

There is already a guard for the invariant-level version of this problem: the
lane fails when an invariant is `NotApplicable` on **every** target, because
a whole row of dashes cannot be told from a working check. The gap this
feature closes is per-**cell**, and it sits between the two existing
mechanisms. A single cell can stop applying while its invariant stays
falsifiable on other targets, and no part of the harness notices.

**The instance that motivated it.** Feature 1156 made four Pants targets pass
invariant I8 by marking resolve anchors as versionless by construction. If the
Pants readers stop emitting anchors — a refactor, a golden refresh that drops
them, a discovery-path regression — the cell can stop applying, flip `.` to
`-`, and the lane stays green while the marker becomes code nobody reads. The
existing all-targets guard does not fire, because I8 would remain applicable
on ten other targets.

That number is measured, not reasoned: the hermetic matrix over the committed
goldens reports `I8: 14/16` applicable, and the four Pants cells all read `.`
today. Losing them leaves ten, which is nowhere near the zero the all-targets
guard needs in order to fire.

**How far that reaches, also measured** — and narrower than this spec first
claimed. A cell goes `NotApplicable` only when *every* versionless component
leaves, and three of the four targets have versionless components besides the
anchors:

| target | versionless | anchors | survivors if anchors leave | I8 then |
|---|---|---|---|---|
| `pants-clojure-polyglot` | 4 | 4 | 0 | `NotApplicable` |
| `pants-example-django` | 14 | 1 | 13 | `Pass` |
| `pants-example-jvm` | 2 | 1 | 1 | `Pass` |
| `pants-example-python` | 2 | 1 | 1 | `Pass` |

So the trap this feature builds fires on one of the four seeded cells. On the
other three an anchor could vanish with the cell still reading green, because
their remaining versionless components — twelve design-tier packages carrying
`waybill:unresolved-reason` on django, one file-tier component each on the
other two — keep the precondition met and are themselves explained. The
stopgap being replaced is blind in exactly the same way, for the same reason:
it also asserts only that the cell reports `Pass`. Nothing is lost by
replacing it.

**That residual gap is closed here too, because it turned out not to need new
machinery.** Detecting *the anchor's own disappearance* is a claim about one
target, which is what the harness's per-target expectation layer is for — and
that layer already asserts anchor presence for two of the seven seeded
anchors. The other five are unasserted, measured the same way: four anchors on
`pants-clojure-polyglot` with only `pants-2.31` named, and the single anchor on
each of django and `pants-example-python`. Filling those in is five assertions
of a shape the file already contains (#1230, FR-019). The near-miss is worth
naming because it reads like coverage and is not:
`pants-resolve-annotation-present` asserts that *some* component carries
`waybill:pants-resolve`, which 34 components satisfy on django.

Both findings come from
`specs/1217-must-pass-registry/measurements/versionless-populations.py`, which
prints the table above and the per-anchor coverage from the committed goldens
and the expectations source.

This does not weaken the case for the feature; it relocates it. The registry
is general machinery for all 136 cells, and its seed is where the mechanism is
first exercised — not the whole of what it protects.

Feature 1156 shipped a narrower stopgap: a test asserting `Pass` on those
four **named** targets. That covers exactly those four cells, leaving every
other cell in the grid able to go quiet unnoticed. This feature replaces the
stopgap with the general mechanism, and carries feature 1156's **SC-010** and
**SC-011**, which were recorded as deferred rather than met.

## Clarifications

### Session 2026-10-18

- Q: What makes FR-010 ("every entry is checked by at least one lane") a real assertion rather than a tautology, given the corpus lane is network-gated? → A: Validation rejects an entry whose target the always-run lane cannot see — a target with filtered goldens is not recordable. FR-010 becomes a property validation enforces, and the filtered-golden case is deferred to a change that needs it.
- Q: Does FR-012's "rendered output" mean the grid itself, and if so does the mark show the breach or the commitment? → A: The grid, and it marks the commitment — every registered cell renders distinctly whether met or not, so a load-bearing pass is legible as one before anything breaks.
- Q: Is feature 1156's stopgap kept or replaced (FR-017 left it open)? → A: Replaced. The registry's seed plus its mutation test cover the same four cells and more, so the stopgap becomes a duplicate; the declarations-file header comment naming it is updated in the same change, and the separate marker-removal mutation test survives untouched.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A load-bearing Pass cannot go quiet (Priority: P1)

A maintainer who fixes a defect so that a cell now passes can record that the
`Pass` carries meaning. If a later change makes that cell stop applying, the
lane fails and tells them which cell, what it reports now, and what they
wrote down about why it mattered.

**Why this priority**: This is the entire value of the feature. Without it
the other two stories protect a mechanism that does nothing.

**Independent Test**: Record the four Pants I8 cells, confirm the lane is
green, then force one of them to report `NotApplicable` and confirm the lane
fails and names it. That mutation is the only evidence the registry works —
delivered alone, this story already closes the hole for every cell anyone
chooses to record.

**Acceptance Scenarios**:

1. **Given** a cell recorded as must-pass, **When** the cell reports `Pass`,
   **Then** the lane is green and the cell prints as a pass in the grid.
2. **Given** a cell recorded as must-pass, **When** the cell reports
   `NotApplicable`, **Then** the lane fails and the failure names the
   invariant, the target, the observed outcome and the recorded reason.
3. **Given** a cell recorded as must-pass, **When** the cell reports `Fail`
   or `Error`, **Then** the lane fails. Those already fail for their own
   reasons; the registry must not be the thing that rescues them.
4. **Given** a run that evaluated only one target, **When** the registry
   holds entries for other targets, **Then** those entries are skipped rather
   than counted as unmet.

---

### User Story 2 - The registry cannot itself be the vacuity (Priority: P2)

A maintainer cannot leave the registry in a state where it looks like a gate
but checks nothing: not by naming a cell that does not exist, not by leaving
a reason blank, not by recording a cell that is simultaneously accepted as
broken, and not by deleting the file.

**Why this priority**: A registry that cannot fail reproduces the defect it
exists to prevent, one level up. P2 rather than P1 only because P1's mutation
test already proves the mechanism has teeth for a well-formed file; this story
closes the ways a malformed one could go unnoticed.

**Independent Test**: Violate each validation rule on its own and confirm each
produces a named lane failure; delete the file and confirm the lane fails
rather than reading as "no cells are load-bearing".

**Acceptance Scenarios**:

1. **Given** a registry entry naming an invariant that is not live, **When**
   the lane runs, **Then** it fails naming that rule — the entry is rejected,
   not skipped.
2. **Given** a registry entry naming a target absent from the corpus
   manifest, **When** the lane runs, **Then** it fails naming that rule.
3. **Given** a registry entry with an empty reason, **When** the lane runs,
   **Then** it fails naming that rule.
4. **Given** two entries for the same cell, **When** the lane runs, **Then**
   it fails naming that rule.
5. **Given** entries out of sorted order, **When** the lane runs, **Then** it
   fails naming that rule.
6. **Given** a cell present in both the registry and the known-failure
   declarations, **When** the lane runs, **Then** it fails naming that rule.
   One cell required to pass and accepted as failing is a half-finished
   change, and the lane must not pick which half to honour.
7. **Given** a registry entry naming a target the always-run lane cannot
   evaluate, **When** the lane runs, **Then** it fails naming that rule. A
   gate guarded only by a lane that may not run is not a gate.
8. **Given** an invalid registry, **When** the lane runs, **Then** validation
   fails **before** any cell outcome is compared, so no entry is partly
   honoured.
9. **Given** the registry file is missing or unreadable, **When** the lane
   runs, **Then** it fails. A deleted gate must not read as a passing one.
10. **Given** any entry in the registry, **When** the always-run lane has
    run, **Then** it evaluated that entry's cell. An entry no lane ever
    checks is the same vacuity wearing the gate's clothes.

---

### User Story 3 - The operator sees which cell broke, in the matrix (Priority: P3)

Someone reading the matrix report — in CI output or in the written artifact —
sees which cells carry a must-pass commitment, and sees a breach alongside
the other lane failures, rather than only as a test name in a panic message.

**Why this priority**: The mechanism works without it; this is the difference
between a diagnosable failure and a search. Lowest priority because P1's
failure message already carries the substance.

**Independent Test**: Read the rendered grid and see the registered cells
marked as such while green; then force one to report `NotApplicable` and
confirm the mark changes and the failure appears among the lane failures.

**Acceptance Scenarios**:

1. **Given** an unmet must-pass entry, **When** the matrix report is
   rendered, **Then** the failure appears in the report's failure list with
   the other lane failures, sorted alongside them.
2. **Given** a registered cell reporting `Pass`, **When** the grid is
   rendered, **Then** that cell is distinguishable from an unregistered
   passing cell, and the legend explains the mark.
3. **Given** a registered cell reporting anything other than `Pass`, **When**
   the grid is rendered, **Then** the cell remains legible as that outcome —
   the mark says the cell was committed to, not what it did.

---

### Edge Cases

- **A target this run did not evaluate.** The corpus lane evaluates one
  target at a time; the hermetic lane evaluates all but one. An entry whose
  target is not among the evaluated cells is skipped, not failed — otherwise
  every per-target run would fail on the other sixteen targets' entries. The
  skip is what makes FR-010's every-entry-is-checked-somewhere requirement
  necessary rather than redundant.
- **A target whose goldens are filtered.** One corpus target
  (`pants-example-javascript`) has deliberately filtered goldens, so the
  always-run lane never evaluates it. An entry for it would be guarded only
  by the network-gated lane, which may not run for months — so it is
  rejected at validation rather than permitted (FR-010). No seeded cell is
  affected: all four have unfiltered goldens. Recording such a cell is a
  coherent future want and is out of scope here; the rejection message is
  what tells that author so.
- **A cell that both fails and is registered.** It produces two lane failures
  for one cell — one from the existing per-cell verdict, one from the
  registry. Accepted: two messages about a real failure is noise, while
  suppressing either risks suppressing the wrong one.
- **An empty registry.** Structurally valid and green. An empty file says no
  cell has been judged load-bearing yet, which was true before this feature.
  Distinct from a *missing* file, which says the gate was removed.
- **A reason that records no judgement.** "It passes today" satisfies the
  non-empty rule and defeats the purpose. Not mechanically detectable; it is
  a review gate, stated in the file's own header so a reviewer knows to
  enforce it.
- **An invariant retired while entries reference it.** Caught by the live-
  invariant rule, which fails rather than skipping — the same posture the
  declarations file takes, for the same reason.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The harness MUST read a committed registry recording, per
  (invariant, target) cell, that the cell's `Pass` is load-bearing. Each
  entry carries the invariant, the target, a written reason, and optionally
  an issue reference.
- **FR-002**: The registry MUST be named for what it is — a registry of
  required outcomes — and MUST NOT borrow the word "expectation", which the
  harness reserves for target-specific checks and enforces at source level,
  nor the word "invariant" alone, which it reserves for universal properties.
- **FR-003**: Each entry's reason MUST answer one question: *what would a
  `NotApplicable` here mean?* A reason that merely observes the present
  outcome records no judgement, and the registry MUST state this requirement
  where an author and a reviewer will both read it.
- **FR-004**: The harness MUST validate the whole registry before comparing
  any cell outcome, and MUST fail the lane on an invalid registry rather than
  honouring the valid entries. A skipped entry is an invisible exemption.
- **FR-005**: Validation MUST reject, each as a named failure: an invariant
  that is not live; a target absent from the corpus manifest; a target the
  always-run lane cannot evaluate; an empty reason; a duplicate cell; entries
  out of ascending sorted order; and a cell that also appears in the
  known-failure declarations.
- **FR-006**: For each entry whose target this run evaluated, the harness
  MUST resolve that cell's outcome and MUST fail the lane on anything other
  than `Pass`.
- **FR-007**: A must-pass failure message MUST name the invariant, the
  target, the observed outcome **and** the entry's recorded reason. A bare
  "must-pass cell failed" sends the reader back to the file to learn what
  they broke.
- **FR-008**: The registry MUST attach as a sibling corpus-level pass. The
  existing per-cell verdict function MUST keep its two-input signature and
  its eight-row behaviour unchanged, and its own tests MUST continue to pin
  that behaviour. The precedent is the existing all-targets applicability
  guard, which contributes lane failures of its own without being threaded
  through the verdict.
- **FR-008a**: The pass MUST run under both report scopes — the whole-corpus
  scope and the single-target scope the per-target lane uses — and MUST NOT be
  gated on scope the way the all-targets applicability guard deliberately is.
  That guard is skipped for a single target because a row of dashes means
  nothing when only one target was looked at; this pass has no such excuse,
  because FR-009 already makes a per-target run correct by skipping the
  entries it did not evaluate. The asymmetry sits three lines from its
  counterexample in the same function, so a test MUST pin it: the lane that
  would notice its loss is the network-gated one, which may not run for
  months.
- **FR-009**: Entries whose target was not evaluated in a given run MUST be
  skipped rather than reported as unmet.
- **FR-010**: Every entry MUST be evaluated by the lane that always runs —
  the one needing no network — and validation MUST reject an entry whose
  target that lane cannot evaluate, naming the rule and saying what the
  author is asking for. Combined with FR-009 this is what stops an entry from
  being skipped everywhere and read as satisfied. Stating it as "some lane
  checks it" would be true by construction, since every recordable target is
  in the manifest by FR-005; a guard enforced only by a lane that may not run
  for months is the vacuity this feature exists to close, one level up.
- **FR-011**: A missing or unreadable registry MUST fail the lane. An empty
  but present registry MUST be valid and green.
- **FR-012**: Must-pass failures MUST appear in the matrix report's lane
  failures alongside the other failure classes. The rendered grid MUST also
  mark every registered cell — whether met or not — distinguishably from an
  unregistered cell and from the existing declared-failure annotation, with
  the legend extended to explain it, and MUST keep the cell's own outcome
  legible. Marking only a breach would leave a load-bearing pass
  indistinguishable from an incidental one, which is the distinction this
  feature exists to make.
- **FR-013**: The registry MUST be seeded with exactly the four Pants I8
  cells — `pants-clojure-polyglot`, `pants-example-django`,
  `pants-example-jvm`, `pants-example-python` — each with a reason of the
  shape FR-003 requires. No other cell. Sweeping in whatever is green today
  would record no judgement and would make the file unreadable as a
  statement of intent.
- **FR-013a**: Each seeded reason MUST state what would have to be true for a
  `NotApplicable` on **that** target, which is not the same sentence four
  times: on one target the anchors leaving is sufficient, and on the other
  three it would additionally require that target's other versionless
  components to leave. A reason that implies the anchors alone would trip the
  cell is wrong on three rows out of four, and a file that is wrong about its
  own reach is the kind a reader stops trusting.
- **FR-014**: The feature MUST include a test that forces one registered cell
  to report `NotApplicable` and asserts the lane fails and names the cell.
  This is not optional; it is the only evidence the registry does anything.
- **FR-015**: Entries MUST be shaped and ordered so that inserting or
  removing one leaves every other entry's rendering unchanged, and the
  harness MUST assert this. Line instability is what made a predecessor
  allowlist unreviewable and had to be retrofitted later.
- **FR-016**: The feature MUST NOT introduce network access or new
  filesystem reads beyond the registry file, and the existing no-new-I/O gate
  MUST still pass. The gate's enumeration of permitted repository-file reads
  MUST name the registry.
- **FR-017**: The narrower stopgap feature 1156 shipped — the test asserting
  `Pass` on the four named Pants targets — MUST be removed in the same change
  that lands the registry, because the seed plus FR-014's mutation test cover
  the same four cells and additionally cover `Fail`, `Error` and the recorded
  reason. The known-failure declarations file's I8 header, which names that
  test as what holds those four cells, MUST be updated to name the registry
  instead; a comment pointing at a deleted test is worse than no comment. The
  separate test that removes the marker from one anchor and asserts the cell
  fails MUST survive unchanged — it exercises the invariant's checker, not the
  registry, and nothing here replaces it.
- **FR-018**: The deferred success criteria of feature 1156, SC-010 and
  SC-011, MUST be reported as met by this feature, and feature 1156's
  deferral note MUST be updated to point at this one.

- **FR-019**: For every anchor component a seeded reason leans on, the
  harness MUST assert that component's **presence** in the emitted document,
  in the target-specific expectation layer — a claim about one target is an
  expectation, not an invariant, and the layer already carries two such
  assertions. Without this the registry would record a commitment it cannot
  keep: on three of the four seeded targets the anchor can leave while the
  cell still reports `Pass`, because the target's other versionless
  components keep the precondition met. Closes #1230.
- **FR-019a**: The five currently unasserted anchors MUST each gain one:
  `java17`, `java21` and `pants-2.30` on `pants-clojure-polyglot`, and the
  `python-default` anchor on `pants-example-django` and on
  `pants-example-python`. `pants-2.31` and `jvm-default` already have theirs
  and MUST NOT be duplicated. All four of `pants-clojure-polyglot`'s are
  required because that entry's reason cites the four-anchor case as the thing
  being exercised, so one assertion would guard a quarter of the claim.

### Key Entities

- **Must-pass entry**: A recorded judgement that one cell's `Pass` carries
  meaning. Identified by (invariant, target). Carries a reason addressed to
  the next person, and optionally the issue whose fix made the cell pass.
- **The registry**: The committed set of must-pass entries. The mirror image
  of the known-failure declarations: that file records cells known to be
  **broken**, this one records cells known to be **meaningful**. A cell may
  appear in at most one of them.
- **The lane pass**: The corpus-level check that resolves each entry against
  the evaluated cells and contributes failures. A sibling of the existing
  all-targets applicability guard, not a parameter of the per-cell verdict.
- **Cell outcome**: Unchanged. `Pass`, `NotApplicable`, `Fail`, `Error` — the
  registry reads these and changes none of them.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001** *(feature 1156's deferred SC-010, now met)*: The registry holds
  the four Pants I8 cells, each with a stated reason, and no cell appears in
  both the registry and the known-failure declarations.
- **SC-002** *(feature 1156's deferred SC-011, now met)*: Forcing any one
  registered cell to report `NotApplicable` fails the lane and names the
  cell. Verified by a test.
- **SC-003**: Forcing a registered cell to report `Fail`, and separately to
  report `Error`, also fails the lane.
- **SC-004**: Each of the seven validation rules, violated on its own, fails
  the lane with a message naming that rule — seven separate demonstrations,
  not one.
- **SC-005**: An invalid registry fails before any cell outcome is compared,
  demonstrated rather than asserted in prose.
- **SC-006**: Removing the registry file fails the lane; emptying it does
  not.
- **SC-007**: Inserting an entry places it at its sorted position and leaves
  every other entry's rendering identical; removing it restores the rendering
  exactly.
- **SC-008**: Every entry in the committed registry is evaluated by the
  always-run lane, asserted by a test rather than by inspection, and an entry
  naming a target that lane cannot evaluate is rejected at validation.
- **SC-009**: An unmet must-pass entry appears in the rendered matrix report
  among the lane failures.
- **SC-010**: All four seeded cells report `Pass` — not `NotApplicable` — in
  the hermetic lane as committed, and the lane is green. Measured before
  this spec was written, so the seed is known to be satisfiable rather than
  assumed to be: the grid shows `.` for all four and `no lane failures`.
- **SC-011**: The matrix still runs in the default test lane with no network,
  and the no-new-I/O gate still passes.
- **SC-012**: The per-cell verdict function's signature and its eight-row
  tests are unchanged by this feature.
- **SC-013**: The feature passes `clippy --workspace --all-targets` and
  `cargo test --workspace --no-fail-fast`.
- **SC-014**: Each of the seven seeded anchors is named by a per-target
  expectation, verified by re-running the measurement probe and finding no
  `NO` rows; and removing any one anchor from a golden fails that target's
  expectations rather than passing as it does today.
- **SC-015**: No test asserting `Pass` on the four named Pants targets
  remains outside the registry, and the declarations file's I8 header names
  the registry rather than a deleted test. The marker-removal mutation test
  still exists and still fails when the marker is removed.
- **SC-016**: In the rendered grid, each of the four seeded cells is
  distinguishable from a passing cell that carries no entry, the legend
  explains the mark, and a registered cell that is not `Pass` still renders
  its own outcome.
- **SC-017**: Building the report for a single target reports an unmet entry
  among the lane failures, not only building it for the whole corpus —
  verified by a test that needs no network, so the property survives a reader
  who tidies the scope asymmetry.

## Assumptions

- **The seed's value is the mechanism, not its reach.** Three of the four
  seeded cells cannot be tripped by the anchors alone, measured rather than
  assumed. They are still correctly recorded: their `Pass` is load-bearing,
  and the registry is what makes that statement checkable at all. The entry
  that can be tripped today is `pants-clojure-polyglot`'s — and FR-019 covers
  what the registry cannot, so the pair of mechanisms is what makes the seed
  honest rather than either alone.
- **The seed stays at four.** The registry is general machinery that would
  protect any of the grid's cells; seeding it with everything currently green
  would record no judgement. Growing it is the business of whichever change
  makes a new cell's `Pass` load-bearing.
- **A missing file fails; an empty file passes.** Not stated in the source
  issue. Chosen because the project's own rule is that a check which did not
  run must not look like a check that passed, and a deleted registry is a
  check that did not run. An empty registry, by contrast, is the honest state
  of the world before anything has been judged load-bearing.
- **Reason substance is a review gate, not a mechanical one.** Only
  non-emptiness can be enforced by code. The requirement is stated in the
  file's header so that the reviewer who can enforce it is the one reading it.
- **Both lanes enforce it; one lane is required to.** The registry gates the
  hermetic golden matrix in the default test lane and the corpus lane that
  scans freshly emitted documents. Every entry must be evaluable by the
  first, which needs no network, so the seed has teeth in `cargo test`;
  the second adds coverage it may not have. All four seeded targets have
  unfiltered goldens, which is what makes that requirement satisfiable today.
- **Two failures for one cell is acceptable.** A registered cell that fails
  reports once as a failing cell and once as an unmet entry.
- **The draft implementation preserved in #1227 is a starting point, not a
  patch.** It compiles against the harness as of feature 1156's merge but was
  never run in the tree. It is to be re-read and re-derived, not applied.

## Out of Scope

- **Recording a cell only the network-gated lane can see.** Rejected by
  FR-010 rather than supported. A change that genuinely needs it would have
  to say how such an entry stays honest when that lane does not run.
- **A general marker-presence mechanism.** FR-019 pins the seven seeded
  anchors by name in the expectation layer, which is what the measured gap
  needed. Deriving such assertions automatically for every C197-bearing
  component on every target — so a new anchor on a new target is guarded
  without anyone remembering — is a larger idea and not this feature.
- **A must-not-apply registry.** The symmetric case — recording that a cell
  is *expected* to be `NotApplicable`, so that it failing to be would be
  news — is a coherent idea and not this feature. Nothing here forecloses it.
- **Changing any cell outcome.** No invariant's checker, precondition or
  accepted-annotation set changes. The registry reads outcomes.
- **The remaining I8 defect.** `image-postgres16`'s Go binary keeps its
  known-failure declaration; it is #1216 population 2 and has its own fix
  shape recorded there.
- **Retiring the per-cell known-failure declarations.** The two files are
  siblings and both stay.
