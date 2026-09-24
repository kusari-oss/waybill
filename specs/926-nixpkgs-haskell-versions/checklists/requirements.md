# Specification Quality Checklist: Resolve Haskell dependency versions through the pinned nixpkgs

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-24
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

**Two [NEEDS CLARIFICATION] markers remain — FR-014 and FR-015.** Both are
carried deliberately from #947's own "What needs deciding" list. Each has
multiple defensible answers that change the shape of the work, and neither has
a default this spec can pick without either overstating what waybill knows
(FR-014, Principle IX) or silently changing default scan behaviour and network
posture (FR-015). The third item on that list — tier and provenance — *was*
defaulted, as FR-007, because Principle X and existing annotation precedent
settle it.

**Iteration 1 findings, all resolved in the spec as written:**

- An earlier draft carried "12 of 19 resolved" from #947 as a success
  criterion. The probe could not reproduce it (10 of 19) and established the
  delta is on the boot-library side. Neither source enumerates the target's
  dependencies, so the figure is unverified in both directions. Replaced with
  SC-001, expressed against the boot set as discovered from the pinned
  revision, which is measurable without that list. Recorded in
  `measurements/README.md` §M3 rather than dropped.
- An earlier draft assumed a fixed set of seven boot libraries, per #947's
  quoted snippet. Measurement M2 shows the nulled set is compiler-specific
  (40 attributes at GHC 9.4.x/9.6.x, 41 at 9.10.x, five-package symmetric
  difference), so a fixed list would be wrong. Became FR-004.

**Measurement status**: every nixpkgs figure in the spec is reproduced by
`measurements/probe_nixpkgs_haskell.py` against revision
`a799d3e3886da994fa307f817a6bc705ae538eeb`. Byte count and derivation count
match #947 exactly; the boot-library finding is new and contradicts the
issue's abridged snippet.
