# Specification Quality Checklist: Operator-supplied CPEs, and the curated CPE table as data

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-13
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

- **Wire contracts and code locations.** As in every waybill spec, the spec names:
  - CLI flags, wire fields and catalog rows (C209, C212), because a consumer observes them;
  - code locations, in the Context section only, because they are the evidence for the "half true" finding.

  The data file's text format is left to the plan (Assumptions).
- **Decisions confirmed in `/speckit-clarify` (2026-11-13):**
  - FR-003: reverse m119's `cpe` rule, so the operator wins.
  - FR-004: key the mapping file by versionless PURL.
  - FR-005: rank `--cpe-map` above nixpkgs-declared.
  - FR-006: hard errors on the new surfaces, WARN on the supplement.
  - FR-017: two stacked PRs.
