# Specification Quality Checklist: Locally built patches are sources, not fragments

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-13
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

- **Wire-level names are deliberate.** The spec names catalog rows (C206, C184) and the nixpkgs attribute (`patches`) because those are the observable surface a consumer reads. It names no source file, function or crate.
- **Population is measured, not taken from the issue.** The figures come from the reader's own classifier on the captured closure (sha256 `898fcdd3…44ab`): 5 derivations and 4 names, against the issue's "four".
- **The edge delta is not predicted.** FR-005 requires measuring it, so no number is quoted ahead of the measurement.
- **One judgement is left open for clarify.** The rung placement in Assumptions makes no difference on the pinned closure (measured), so it is recorded as a default rather than a marker.
