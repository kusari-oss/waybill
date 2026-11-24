# Specification Quality Checklist: Closure-resolved Haskell components are not labelled `design`

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

- **The reader is a maintainer or consumer of the SBOM.** Property names (`waybill:sbom-tier`, `waybill:closure-contents`) are the product's observable surface, not implementation detail. The same convention applies across this repository's specs.
- **FR-001 deliberately defers *where* the change is made to measurement.** The same compiler's boot libraries are `build` in some places and `design` in others, and the path that assigns each was not established at specify time.
- **The tier choice (`build` for both groups) is made here, with its reasoning (FR-002, FR-003), rather than left open.** The evidence supports it: the same libraries are already `build` without a manifest, and the closure's derivation graph is a build graph.
- **No separate must-pass registry exists in the tree.** Every invariant × target cell must pass unless declared, so FR-006's "no declaration" is the enforcement.
