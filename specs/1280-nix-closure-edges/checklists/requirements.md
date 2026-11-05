# Specification Quality Checklist: the nix closure tier emits its dependency graph

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [ ] No [NEEDS CLARIFICATION] markers remain
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

**One marker remains, FR-010, and it is deliberate.** It is the single decision
that changes the output by an order of magnitude, it has three defensible
answers, and all three were measured before the question was asked rather than
after. Resolving it by guess would be the "shipping a guess into a requirement"
failure this project records. See Question 1 in the report.

Two figures were corrected during validation, both against the committed
golden rather than from memory:

- SC-005's role census read `269 / 118 / 42 / 1,315`; the golden says
  `268 / 121 / 45 / 1,315`. Three of the four were wrong, and they are
  milestone 1279's own post-merge figures.
- SC-003 asserted that `graph-completeness` stops being `partial`. It will not:
  the reason carries two independent causes and this feature addresses one, so
  the criterion is now worded against the `orphaned-components-detected`
  clause. As first written it would have failed for the milestone working
  perfectly.

**One judgement a reviewer should check.** FR-002 forbids deriving edges from
the declared attributes, which is what makes SC-005's "census unchanged"
meaningful. If a reviewer would rather have both sources feeding the graph,
FR-002 and SC-005 both change.
