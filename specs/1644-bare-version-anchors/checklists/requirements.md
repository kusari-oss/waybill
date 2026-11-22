# Specification Quality Checklist: bzip2, GMP and xz versioned from a bare version plus a second anchor

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-22
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

- **Code references are deliberate.** As in specs 1641–1643, the spec names files, the `CuratedLibrary` enum, `BinaryScan.symbol_names` and the census target. They locate the defect and the observed behaviour; they do not prescribe a design. The design questions (how anchors are gathered, where the verdict is computed, how definedness of a symbol is read) are left to the plan.
- **Decided in the spec rather than asked**, each with its reason, and each open to amendment by FR-001's census:
  - the reason for a missing version goes in the census, not the document (FR-011);
  - bzip2 still needs its anchor (FR-006);
  - xz is named `liblzma`, after upstream's `.pc` file (FR-007).
- **Conditional requirements.** FR-003 (the xz symbol list), FR-004 (xz's suffix grammar, GMP's `X.Y` form) and FR-008 (the CPE rows) are fixed by research or census, which the spec requires to be written back into it before implementation. This is a measure-first rule, not a gap.
- **A code fact found while writing the spec.** On ELF, `symbol_names` keeps imported `.dynsym` entries as well as defined ones (`scan.rs:309-315`), so a symbol anchor read from it would accept any program that links liblzma. FR-003 and User Story 2 scenario 3 require definedness. The importer fixture is the test of it.
