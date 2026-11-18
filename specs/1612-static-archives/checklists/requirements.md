# Specification Quality Checklist: Open static archives on installed trees

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-18
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

- **Format details are named.** The spec names the archive formats (BSD, GNU, thin, fat) and their structures (symbol index, long-name table). These are properties of the input that the requirements must cover, not implementation choices. It also names two existing modules (the ipk reader's `ar` parser, the Nix tier's `lib_links.rs`) as context only. This matches the project's spec convention (specs 1531–1543).
- **No clarification markers.** The five decisions the input called for are made, with stated defaults in Assumptions:
  - installed trees only;
  - one component per archive, findings nested;
  - existing per-member scanners;
  - thin archives not followed, fat archives deferred;
  - bounds set in the plan from the census.
- **SC-001 and SC-006 depend on the FR-001 census** for their target values. This is deliberate: the contents of the archives are unmeasured, and the project's rule is to measure before committing a number. The census is the plan's first task.
