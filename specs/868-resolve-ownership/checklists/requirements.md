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

Two open `[NEEDS CLARIFICATION]` markers remain, both deliberately left open
rather than defaulted, and both in the highest-priority class (scope /
modelling):

1. **What component owns a resolve.** The three candidates differ in whether a
   new component appears in the SBOM at all, which is consumer-visible and not
   reversible by a later change without moving identities. There is no
   defensible default: (a) invents a component no manifest describes, (b)
   asserts a direct dependency the root manifest does not state, (c) is the
   most faithful but may not be recoverable from a lockfile alone.
2. **Whether tool lockfiles are anchored like application resolves.** Both
   answers are defective in opposite directions — anchoring asserts a runtime
   relationship that does not exist, excluding leaves the contents unreachable.
   A third option (anchor, but mark build-time) exists. This is a scope
   boundary, which the spec guidance ranks highest.

This feature is unusual in that the *defect* is completely characterised and
measured while the *fix* is genuinely undecided. The evidence is not the
uncertain part; the modelling is.

Every figure is traceable to an observation taken against `main` at
`3ad457ae` — re-measured for this spec rather than carried over from issue
#887, because #885 and #888 landed in between and stale evidence is how a
spec comes to describe a product state that no longer exists. The re-measure
also produced a figure the issue did not have (778 declared names resolving
to nothing), which was investigated and found to be unselected extras —
explicitly placed Out of Scope rather than left as an unexplained number.

Resolve both clarifications via `/speckit.clarify` before `/speckit.plan`.
