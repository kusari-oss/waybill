# Specification Quality Checklist: multi-main-module root override

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-13
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [ ] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

Two [NEEDS CLARIFICATION] markers remain, both deliberate — they are the
decisions this feature exists to make, and guessing them would defeat
the purpose of specifying it:

- **FR-005** — does the N=1 path converge on the new policy, or stay as
  it is? Converging means changing milestone-077 behaviour that has
  shipped for a long time; not converging means two behaviours depending
  on a count the operator cannot see.
- **FR-006** — what happens to `--preserve-manifest-main-module`? Under
  the chosen direction it may become the permanent behaviour, leaving
  the flag a no-op.

Both are scope-level and go to `/speckit.clarify`.

Two file references appear in the spec (`root_selector.rs:525` and the
milestone-149 quotation). They are evidence for the problem statement,
not implementation direction, and both were read at authoring time
rather than recalled.

Numbers in the Observed Impact table and Success Criteria were measured
on 2026-09-13 against the committed goldens and local scans, not
estimated. The measurement for `rust-ripgrep` used the corpus cache at
pin `0e8390a`; `maven-guice` at pin `b0e1d0fa`.
