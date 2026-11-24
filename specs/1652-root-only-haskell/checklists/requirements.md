# Specification Quality Checklist: `root-only` keeps the packages the root builds, and records what it dropped

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-24
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

- **#1676's diagnosis is corrected in the Background, from the code.** `waybill:workspace-member` is stamped on every component by a generic pass, as the directory of the component's own manifest; it is not set per reader. A cargo workspace member is kept because its evidence is the root `Cargo.lock`. A nested Haskell package names its own subdirectory. FR-003 is therefore stated as behaviour ("treated as a member of the root project"), and the mechanism is left to the plan.
- **FR-001 requires the cause of the dropped dependency to be measured before the design is fixed.** Code reading suggests `project_for_root`'s sibling demotion, which `--split` shares, hence FR-007's `--split` clause.
- **Two decisions are deferred to the plan:** whether FR-002 also applies to `strict`, and the form of the drop record (Principle V audit).
