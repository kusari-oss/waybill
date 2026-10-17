# Specification Quality Checklist: a marker for a version that was never there to find

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-16
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

Two items pass with a qualification worth recording rather than silently
ticking:

- **"No implementation details" / "non-technical stakeholders".** The spec
  names annotation keys, the three SBOM formats, the declarations file and
  the invariants reference. For this project those are the deliverable and
  the user-facing contract, not implementation choices — an annotation key
  is what an SBOM consumer reads. No Rust, no source paths, no function
  names appear. Judged to pass on the standard the m1155 spec set.
- **FR-005 and FR-015 are review-gated, not machine-gated.** "The row
  carries a Principle V audit" and "any other golden difference is
  explained before merge" are checkable by a reader but not by a test. They
  are written as requirements because leaving them implicit is how a
  `waybill:*` annotation ships without its audit and how an unrelated
  golden change gets absorbed into a refresh.

Updated after `/speckit.clarify` (session 2026-10-16). Three decisions
that the first draft carried as assumptions or left to the plan phase are
now settled in the spec:

- the annotation is `waybill:version-absence` = `by-construction`;
- the positive control is a committed must-pass registry
  (`invariant-must-pass.toml`), general across the matrix rather than
  scoped to I8's four cells;
- delivery is two independently-green changes, emission then acceptance,
  which the stale-declaration rule permits because the cells keep failing
  with their pinned kind until acceptance widens.

Two consequential edits fell out of the third: US1's fourth acceptance
scenario previously asserted an outcome belonging to the acceptance change,
and FR-010 previously required the declarations to be deleted alongside the
emission, which would have failed the lane. Both are corrected.
