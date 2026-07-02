# Specification Quality Checklist: SPDX 3 CustomLicense for LicenseRef-* (milestone 154 / issue #487)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-02
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec cites `v3_licenses.rs` in Origin & context as reader-anchor only; FRs / SCs describe behavior/outcomes without prescribing Rust APIs.
- [X] Focused on user value and business needs — cross-format symmetry framing throughout; compliance-auditor + downstream-tool persona invoked.
- [X] Written for non-technical stakeholders — outcomes phrased as "consumer gets consistent LicenseRef-resolution across both formats" rather than "add CustomLicense element."
- [X] All mandatory sections completed — User Scenarios + Requirements + Success Criteria + Assumptions all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — placeholder text is byte-locked from milestone 153; SPDX 3 element type (`simplelicensing_CustomLicense`) taken from issue body; no other spec-shaping decisions require operator input.
- [X] Requirements are testable and unambiguous — FR-001 through FR-018 each name a concrete behavior; SC-001 through SC-008 each name a verification method (jq recipe, byte-diff, validator run).
- [X] Success criteria are measurable — SC-001 names the 3 exact LicenseRef- names expected; SC-002 = byte-identity via existing goldens; SC-003 = spdx3-validate exit 0; SC-006 = ≥5 unit tests.
- [X] Success criteria are technology-agnostic — outcomes phrased in consumer terms (validator behavior, jq recipe output); tools named only where empirical (e.g., `spdx3-validate`).
- [X] All acceptance scenarios are defined — US1 has 6 Given/When/Then scenarios covering positive + dedup + cross-field + cross-format-symmetry; US2 has 2 covering happy-path byte-identity.
- [X] Edge cases are identified — 8 edge cases enumerated: same-package declared+concluded dedup, nested compound structure, DocumentRef prefix (both SPDX-2.3-style and SPDX-3's cross-doc model), empty document, IRI construction, CreationInfo reference, milestone-012 hash-fallback interaction.
- [X] Scope is clearly bounded — FR-012 through FR-018 + Out of Scope section enumerate explicit exclusions (no SPDX 2.3, no CDX, no SpdxExpression, no rpm_file.rs, no new annotations, no real text extraction, no expandedlicensing, no DocumentRef emission, no retroactive re-emission).
- [X] Dependencies and assumptions identified — 9 Assumptions + explicit Dependencies on milestones 153, 152, 011, 078.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — each FR maps to a US acceptance scenario, an SC, or both.
- [X] User scenarios cover primary flows — US1 (cross-format symmetry) covers the issue-#487 use case end-to-end; US2 (byte-identity happy path) covers the regression guard.
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 (testbed cross-format symmetry), SC-002 (byte-identity), SC-003 (validator zero-errors), SC-004 (cross-format placeholder identity), SC-005 (pre-PR gate), SC-006 (≥5 unit tests), SC-007 (no wire-format changes), SC-008 (CHANGELOG entry).
- [X] No implementation details leak into specification — the `v3_licenses.rs` reference in Origin & context is a reader-anchor; the FRs/SCs describe outcomes independently.

## Notes

- All 16 checklist items pass on first authoring pass.
- The milestone-153 wire contract (`PLACEHOLDER_EXTRACTED_TEXT`) is byte-locked; this milestone reuses it. No clarification needed on placeholder wording.
- One planning-phase-deferrable question: whether to reuse milestone 153's const via `pub(crate)` visibility promotion or duplicate the string in `v3_licenses.rs` with a doc-comment reference. FR-018 says "the choice is a planning-phase decision; both preserve the invariant" — not a spec-shaping ambiguity.
- Ready for `/speckit-clarify` if the choice between `simplelicensing_CustomLicense` and `expandedlicensing_CustomLicense` needs maintainer confirmation; otherwise ready for `/speckit-plan`.
