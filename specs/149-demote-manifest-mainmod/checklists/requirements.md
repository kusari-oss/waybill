# Specification Quality Checklist: preserve manifest-derived main-module as demoted library entry when `--root-name` overrides it

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-29
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
  - Spec names existing module paths (`mikebom-cli/src/generate/root_selector.rs`) as scope-bounding anchors. No control flow, function signatures, or Rust syntax in the spec body.
- [X] Focused on user value and business needs
  - US1 framed around compliance engineer + manifest-identity preservation. US2 framed around cross-ecosystem operator uniformity.
- [X] Written for non-technical stakeholders
  - Origin & Context section walks through the milestone-077 history + before/after JSON examples in plain language. Concrete Cargo example anchors the abstract design.
- [X] All mandatory sections completed
  - Origin & Context, User Scenarios & Testing, Requirements, Success Criteria, Assumptions, Out of Scope, Constitution V parity-bridging audit all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
  - Zero markers. All open decisions from issue #151 ("Decide whether default vs flag", "Decide annotation name", etc.) resolved in Assumptions or FRs.
- [X] Requirements are testable and unambiguous
  - FR-001 to FR-013 each name a specific behavior with verifiable assertions. Example: FR-005 explicitly names the THREE differences between pre-override main-module and demoted entry — `type` changes, annotation added, position in `components[]` instead of `metadata.component`.
- [X] Success criteria are measurable
  - SC-001 names the six ecosystems. SC-002/SC-003 name byte-identity regression. SC-004 names the in-tree test path. SC-005 names the parity-catalog row + golden refresh scope. SC-006 names the pre-PR gate. SC-007 names the docs files.
- [X] Success criteria are technology-agnostic (no implementation details)
  - SCs describe observable outcomes (test assertions, byte-equality, doc presence). The only "technology" references are the three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3) which are spec-bounded artifact formats.
- [X] All acceptance scenarios are defined
  - US1 has 5 scenarios covering the opt-in trigger, all three formats, the no-flag regression case, and the no-override regression case. US2 has 3 scenarios for cross-ecosystem coverage.
- [X] Edge cases are identified
  - 6 distinct edge cases covered: flag-without-override, partial override, `--root-purl` instead of `--root-name`, multi-main-module scan, PURL collision with existing dep, `--no-root-purl` interaction.
- [X] Scope is clearly bounded
  - Out of Scope section lists 9 explicit exclusions including the milestone-077 default-change boycott, demote of other roles, suppression-flag rejection, milestone-134 collision-handling interaction.
- [X] Dependencies and assumptions identified
  - Assumptions section names 8 explicit assumptions including opt-in default, annotation name choice, no root-override annotation, bom-ref derivation, empty relationships on demoted entry, no new Cargo deps, root-selector placement, operator-cadence verification sufficiency.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
  - FR-001 ↔ US1 scenario 1. FR-002 ↔ US1 scenarios 1-3. FR-003 ↔ US1 scenarios 1-3. FR-004 ↔ Constitution V audit + US1 scenarios 1-3. FR-005 ↔ US1 scenario 1. FR-006 ↔ Edge Case 1. FR-007 ↔ US1 scenario 4 + SC-002. FR-008 ↔ Assumption 7. FR-009 ↔ Edge Case 5. FR-010 ↔ SC-005. FR-011 ↔ SC-007. FR-012 ↔ US2 scenarios 1-2. FR-013 ↔ Edge Case 4.
- [X] User scenarios cover primary flows
  - US1 covers the singular value-add (opt-in demote) + regression cases (no-flag, no-override). US2 covers cross-ecosystem coverage as the load-bearing implementation-placement constraint.
- [X] Feature meets measurable outcomes defined in Success Criteria
  - Every SC is achievable purely via the FR set.
- [X] No implementation details leak into specification
  - File paths and module names are scope anchors only. No function signatures or Rust syntax in spec body.

## Notes

- All checklist items pass on first iteration.
- The spec deliberately names `mikebom-cli/src/generate/root_selector.rs` (per Assumption 7) as the implementation site to enforce cross-ecosystem placement — locating the logic in any ecosystem reader would scatter it across six modules and require FR-012 to be split into six per-reader requirements.
- The Constitution V parity-bridging audit (dedicated section + FR-011 documentation requirement) deliberately enumerates the rejected native-field alternatives across all three formats — preventing future contributors from second-guessing the `mikebom:demoted-from-main-module` annotation as a Principle V violation.
- Issue #151's open decisions ("Decide whether default vs flag", "Decide annotation name", etc.) are resolved deliberately in Assumptions: opt-in flag for backward compat, `mikebom:demoted-from-main-module = "true"` for the annotation, root-selector pipeline placement for the code location.
- Ready for `/speckit-clarify` (probably zero questions; spec is self-contained) or directly for `/speckit-plan`.
