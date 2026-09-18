# Specification Quality Checklist: A split document says which resolve it is

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-18
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
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

All three mechanism decisions were resolved by `/speckit.clarify` on
2026-09-18 and are recorded in the spec's Clarifications section:

1. **Identity shape** — unambiguous across Pants language namespaces, not a
   bare name. `[python.resolves]` and `[jvm.resolves]` are separate
   namespaces.
2. **Where it lives** — a separate document-scope statement, not folded into
   the repository-wide ownership statement. **Conditional**: a Principle V
   audit for a standards-native carrier is the plan's first research task, and
   this answer stands only if it comes back empty.
3. **Which documents carry it** — every per-resolve document, including
   declared ones whose root already names the resolve. The duplication is
   accepted; FR-005 requires a test that the two agree.

## A defect found while clarifying

`--split=resolve` groups on the bare resolve name (`split.rs:219`), so two
resolves sharing a name across Pants language namespaces merge into one
document. That is shipped in v0.9.0 and is filed as **#919**, deliberately
not fixed here — this feature is a metadata question and widening it into a
correctness fix is the scope boundary that kept #902's correctness work
unblocked. The spec is written so the identity stays correct once #919 lands.

The corpus does not catch #919: its JVM target uses `default` and its Python
target `python-default`, so they never collide.
