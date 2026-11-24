# Specification Quality Checklist: Read a CMake build tree to settle what a build actually used

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

- **Repository convention over template wording.** Like specs 1538–1647, the spec names CMake file kinds, catalog rows and the source files whose current behaviour it changes. These describe the inputs and outputs the feature is about (CMake's own artifacts, waybill's emitted fields), not how waybill will implement it, so they are kept. The Background's code references are labelled "to be re-verified in research".
- **No [NEEDS CLARIFICATION] markers, by design.** The three open decisions from the feature description (one or several build trees; whether compile evidence changes scope; how build-only components are marked) were written as explicit defaults and confirmed in the 2026-11-24 clarify session; see the spec's Clarifications and FR-001/FR-002, FR-009, FR-007.
- **File lists are provisional.** FR-003 and FR-013 name the CMake files assumed to carry each signal; FR-003 requires the observed files from SC-001 to replace them. This is the CLAUDE.md measure-first rule, not an open ambiguity.
- **SC-001(b) may fail.** TheRock build feasibility on a hosted runner is unverified; SC-001 states the fallback.
