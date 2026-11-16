# Specification Quality Checklist: Attribute installed binaries through embedded build and source paths

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-16
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

- **Domain terms are the subject, not implementation.** DWARF, `.comment`, build-id and vendoring directory conventions are the evidence the feature reads, as CMake files were in specs 1531–1533.
- **Clarified 2026-11-16:** generic mapping only, with no TheRock manifest. Bundled trees become versionless components nested under the binary, with a measured exclusion list.
- **The measurements are committed.** `measurements/` holds the census script, its 708-file input list, its output (`census.json`, from run 43784750746) and the summariser.
