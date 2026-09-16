# Specification Quality Checklist: Lockfile resolve graphs must be anchored to an owning component

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-16
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

All items pass as of the 2026-09-16 clarification session. Two questions
asked and answered, both scope/modelling, and **both answers were changed by
evidence gathered during the session rather than reasoned from the spec**:

1. **Ownership → one component per named resolve.** The spec's first draft
   called consuming-package ownership "most faithful but maybe not
   recoverable". The reverse is true: `waybill:pants-resolve` is already
   emitted on 248 of 272 pypi components and already names 8 distinct
   resolves, so per-resolve ownership is what the available data supports,
   while consuming-package ownership would need a mapping nothing recovers.
   The spec now records that correction rather than quietly adopting the
   answer.

2. **Tool resolves → anchored and marked build-time.** Writing this surfaced
   a factual error made earlier in the same session: `[python.resolves]` is
   not an application-only list, it is the full registry with tool lockfiles
   in it. The real signal is a tool section back-referencing a resolve via
   `install_from_resolve`, and that signal is **partial** — it covers five of
   the nine resolves, while `towncrier` and `pants-plugins` are tooling that
   nothing declares as such.

   Rather than hide the partiality behind name-matching, FR-003a forbids
   inferring from names or paths, FR-003b defaults undeclared resolves to
   runtime (over-report loudly rather than hide quietly), and FR-003c requires
   the runtime-by-default count be reported so the over-reporting is visible.

This feature entered clarification with the defect fully measured and the fix
undecided. Both decisions are now grounded in observations recorded in the
spec, and each names the assumption it overturned.

Ready for `/speckit.plan`.
