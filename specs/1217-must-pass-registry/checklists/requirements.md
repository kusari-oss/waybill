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
