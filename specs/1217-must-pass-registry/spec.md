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
invariant I8 by marking resolve anchors as versionless by construction. Those
four targets have no other versionless components. If the Pants readers stop
emitting anchors — a refactor, a golden refresh that drops them, a
discovery-path regression — I8's precondition stops being met on all four at
once. The cells flip `.` to `-`, the lane stays green, and the marker becomes
code nobody reads. The existing all-targets guard does not fire, because I8
would remain applicable on ten other targets.

That last number is measured, not reasoned: the hermetic matrix over the
committed goldens reports `I8: 14/16` applicable, and the four Pants cells
all read `.` today. Losing them leaves ten, which is nowhere near the zero
the all-targets guard needs in order to fire.

Feature 1156 shipped a narrower stopgap: a test asserting `Pass` on those
four **named** targets. That covers exactly those four cells, leaving every
other cell in the grid able to go quiet unnoticed. This feature replaces the
stopgap with the general mechanism, and carries feature 1156's **SC-010** and
**SC-011**, which were recorded as deferred rather than met.

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
7. **Given** an invalid registry, **When** the lane runs, **Then** validation
   fails **before** any cell outcome is compared, so no entry is partly
   honoured.
8. **Given** the registry file is missing or unreadable, **When** the lane
   runs, **Then** it fails. A deleted gate must not read as a passing one.
9. **Given** any entry in the registry, **When** both lanes have run,
   **Then** at least one of them evaluated that entry's cell. An entry no
   lane ever checks is the same vacuity wearing the gate's clothes.

---

### User Story 3 - The operator sees which cell broke, in the matrix (Priority: P3)

Someone reading the matrix report — in CI output or in the written artifact —
sees a must-pass failure alongside the other lane failures, rather than only
as a test name in a panic message.

**Why this priority**: The mechanism works without it; this is the difference
between a diagnosable failure and a search. Lowest priority because P1's
failure message already carries the substance.

**Independent Test**: Force a registered cell to report `NotApplicable` and
confirm the rendered matrix report lists it among the lane failures.

**Acceptance Scenarios**:

1. **Given** an unmet must-pass entry, **When** the matrix report is
   rendered, **Then** the failure appears in the report's failure list with
   the other lane failures, sorted alongside them.

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
  hermetic lane never evaluates it. An entry for it would be checked only by
  the corpus lane, which needs network and a populated cache. This is
  permitted, and is exactly the case FR-010 exists to keep honest.
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
  that is not live; a target absent from the corpus manifest; an empty
  reason; a duplicate cell; entries out of ascending sorted order; and a cell
  that also appears in the known-failure declarations.
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
- **FR-009**: Entries whose target was not evaluated in a given run MUST be
  skipped rather than reported as unmet.
- **FR-010**: Every entry MUST be checked by at least one of the harness's
  two lanes, and the harness MUST assert this. Combined with FR-009 this is
  what stops an entry from being skipped everywhere and read as satisfied.
- **FR-011**: A missing or unreadable registry MUST fail the lane. An empty
  but present registry MUST be valid and green.
- **FR-012**: Must-pass failures MUST appear in the matrix report's lane
  failures and in its rendered output, alongside the other failure classes.
- **FR-013**: The registry MUST be seeded with exactly the four Pants I8
  cells — `pants-clojure-polyglot`, `pants-example-django`,
  `pants-example-jvm`, `pants-example-python` — each with a reason of the
  shape FR-003 requires. No other cell. Sweeping in whatever is green today
  would record no judgement and would make the file unreadable as a
  statement of intent.
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
- **FR-017**: The narrower stopgap feature 1156 shipped — the test naming the
  four Pants targets — MUST be reconciled with the registry in the same
  change: either kept with its overlap stated, or replaced. It MUST NOT be
  left as an unexplained duplicate of the registry's seed.
- **FR-018**: The deferred success criteria of feature 1156, SC-010 and
  SC-011, MUST be reported as met by this feature, and feature 1156's
  deferral note MUST be updated to point at this one.

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
- **SC-004**: Each of the six validation rules, violated on its own, fails
  the lane with a message naming that rule — six separate demonstrations, not
  one.
- **SC-005**: An invalid registry fails before any cell outcome is compared,
  demonstrated rather than asserted in prose.
- **SC-006**: Removing the registry file fails the lane; emptying it does
  not.
- **SC-007**: Inserting an entry places it at its sorted position and leaves
  every other entry's rendering identical; removing it restores the rendering
  exactly.
- **SC-008**: Every entry in the committed registry is evaluated by at least
  one lane, asserted by a test rather than by inspection.
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

## Assumptions

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
- **Both lanes participate.** The registry gates the hermetic golden matrix
  in the default test lane and the corpus lane that scans freshly emitted
  documents. All four seeded targets have unfiltered goldens today, so the
  seed has teeth with no network — but FR-010 does not depend on that
  remaining true.
- **Two failures for one cell is acceptable.** A registered cell that fails
  reports once as a failing cell and once as an unmet entry.
- **The draft implementation preserved in #1227 is a starting point, not a
  patch.** It compiles against the harness as of feature 1156's merge but was
  never run in the tree. It is to be re-read and re-derived, not applied.

## Out of Scope

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
