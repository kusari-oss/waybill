# Specification Quality Checklist: Nightly SBOM Quality Regression Corpus

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-03
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

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.

### Deliberate deviations from "no implementation details"

Three requirements name workspace-level concepts (FR-031, FR-032, SC-007: "crate",
"workspace", "the shipped waybill binary"). This is intentional and not accidental leakage.
Constitution Principle VI caps the workspace at three crates and requires a constitution
amendment for a fourth; the spec must state on its face that this feature adds none, or a
reviewer is obliged to raise it. Stating the constraint in technology-neutral language would
defeat its purpose.

### Open items carried into planning

- **The corpus membership is not enumerated in this spec.** FR-001 requires the corpus to
  live in configuration; the specific repositories, their pins, and their authored ranges
  are data and belong in the configuration file produced during planning and implementation.
  Eighteen candidate repositories have been measured and agreed; that list must be carried
  into `data-model.md` or the configuration artifact so it is not lost.
- **Ranges are not yet authored.** FR-020 makes unranged measurements observe-only, so the
  feature is landable before ranges exist. Authoring them is a maintainer task informed by
  the baseline observations recorded in the spec's Assumptions section.
- **No notification path is specified beyond job failure.** FR-019 and FR-029 require a
  failed status and a retained report. Whether a failing nightly additionally notifies
  anyone is deliberately unspecified, since the existing nightly suites in this repository
  do not, and matching them is the reasonable default.
