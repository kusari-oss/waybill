# Specification Quality Checklist: A must-pass registry for invariant cells whose Pass is load-bearing

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-17
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

**On "non-technical stakeholders" and "technology-agnostic".** The user of
this feature is a waybill maintainer reading a CI gate, so the vocabulary of
the domain — invariant, cell, target, outcome, lane — is the stakeholder's
own vocabulary, not implementation leakage.

Checked mechanically against the stricter reading: the spec body contains no
file path, no module or function name and no field name. The single match for
`.toml` in the file is inside the quoted Input, which is the user's own
wording. The committed contract the issue already carries holds those
decisions, and the spec states the *constraints* on them instead — FR-002
requires a name that does not borrow a reserved word without prescribing the
name; FR-008 requires the existing per-cell verdict to keep its shape without
naming it.

Two unavoidable exceptions, both kept deliberately:

- **FR-013 names the four seeded targets.** They are the content of the
  requirement, not an implementation of it. A requirement to seed "the cells
  feature 1156 made pass" would be an indirection a reader has to resolve.
- **SC-012 refers to the per-cell verdict function.** The criterion is that
  an existing component's contract is *unchanged*, which cannot be stated
  without identifying it. Stated as a property of the behaviour (signature
  and eight-row tests) rather than of the code.

**On measured claims.** Two numbers in the spec describe the current corpus
rather than a design intent, so per the project's measure-before-designing
rule both were measured before the spec was written, not inferred: `I8: 14/16`
applicable and `.` on all four seeded cells, from the hermetic matrix over the
committed goldens. The spec says which, and says they are measured.

**On the one judgement call with no stated default.** Whether a *missing*
registry file should fail the lane or read as empty is not settled by #1227.
The spec chose fail, recorded it in Assumptions with the reasoning, and gave
it its own criterion (SC-006) so the choice is visible rather than buried.
It was not raised as a clarification because the project's own stated
principle — a check that did not run must not look like one that passed —
settles it.

**On the stopgap.** FR-017 exists because feature 1156 shipped a test that
overlaps this feature's seed exactly. A spec that added the registry without
saying what happens to the narrower test would leave a reviewer unable to
tell duplication from redundancy-by-design.

**Post-plan (2026-10-18).** Plan-time measurement corrected one premise and
added one requirement. The spec had claimed all four seeded targets would flip
to `NotApplicable` if the anchors left; measured, that is true of one, and the
spec now carries the table, FR-013a and the probe. FR-019/FR-019a were added
after a second measurement found the complementary assertion already exists
for two of seven anchors, making the gap a five-line fill-in rather than the
separate mechanism the first draft deferred (#1230). Re-checked: all items
still pass, and both new requirements carry success criteria (SC-014) rather
than prose.

**Post-clarify (2026-10-18).** Three clarifications were integrated and all
items were re-checked against the amended spec; all still pass. FR-010 is now
enforced by validation rather than asserted by a test that could not fail,
FR-012 names the grid and marks the commitment rather than the breach, and
FR-017's open choice is closed as removal. Each added a demonstrable success
criterion (SC-004 counts seven rules, SC-008, SC-014, SC-015), so no
clarification landed as prose alone.

**Post-analyze (2026-10-18).** `/speckit.analyze` found one HIGH coverage gap
and three smaller ones; all are closed above rather than noted. The HIGH one
was the plan's own first risk row: that the must-pass pass must run under the
single-target report scope was stated in research, in the contract and in a
module header, and nowhere as a requirement or a test — while the lane that
would notice its loss is network-gated. That is this feature's own argument
turned on itself, so it became FR-008a and SC-017 with a task (T038). Also
added: the unreadable half of FR-011 (a malformed file, not only a missing
one), the removal proof for the two anchors nobody is editing, and the finding
that the `pants-2.31` anchor's existing guard is a *reachability* check whose
diagnostic sends a reader to #1100 rather than to a vanished anchor — recorded
in the task rather than left for whoever next reads a `yes` row in the probe's
table. One CRITICAL finding was raised against the **constitution**, not this
feature: its MANDATORY pre-PR table still prints the pre-#1189 `+stable` form.
It needs its own amendment; the plan's Constitution Check now carries the
stale-clause note so the deviation is visible rather than silent.
