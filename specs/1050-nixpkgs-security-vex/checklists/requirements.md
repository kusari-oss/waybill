# Specification Quality Checklist: nixpkgs security declarations as VEX

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-30
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

**Two markers remain, both deliberate.** Each is a decision with more than one
defensible answer and materially different consequences, which is what the
marker is for. Both are posed to the maintainer below rather than guessed.

**Every measured number in this spec is traceable** to
`measurements/README.md`, which carries the probes. Nothing here is derived
arithmetic presented as measurement — the repository rule that caught a
fourfold undercount in milestone 1035.

Two of the three questions the feature request raised were closed by informed
guess rather than marker, and both are recorded in Assumptions:

- *Does this need evaluation?* Yes. `meta.knownVulnerabilities` is a Nix
  attribute. A file-parsing route would silently miss computed cases while
  appearing to work on literal ones, which is a worse failure than not
  offering it.
- *Does a declaration warrant its own evidence grade?* Yes (FR-009). A
  maintainer asserting "this version is vulnerable" is stronger evidence than
  a filename containing a CVE identifier, and milestone 1035 built the grade
  as an enum with one variant precisely so a second could be distinguished.
