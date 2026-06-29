# Specification Quality Checklist: source-files cross-emitter divergence — union evidence across same-PURL entries

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-28
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
  - Spec names specific file paths + line numbers (`scan_fs/mod.rs:750`, `resolve/deduplicator.rs:34-46`, `maven.rs:3429-3457`) as scope-bounding anchors. No control flow, function signatures, or Rust syntax in the spec body.
- [X] Focused on user value and business needs
  - US1 framed around compliance engineer + cross-format reconciliation tooling. US2 framed around cross-ecosystem invariant for future ecosystem readers.
- [X] Written for non-technical stakeholders
  - Origin & Context section walks through the bug discovery + root-cause in plain language. Concrete numerical SC-001 (51 → 0) anchors the outcome.
- [X] All mandatory sections completed
  - Origin & Context, User Scenarios & Testing, Requirements, Success Criteria, Assumptions, Out of Scope, Constitution V audit all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
  - Zero markers. All design decisions documented in Assumptions.
- [X] Requirements are testable and unambiguous
  - FR-001 to FR-010 each name a specific behavior with verifiable assertions. Example: FR-005 lists every field that MUST be preserved; the negative-space contract is explicit.
- [X] Success criteria are measurable
  - SC-001 names a specific count delta (51 → 0). SC-002 names specific CI test names. SC-003/SC-004/SC-005 name specific unit-test placements + assertions. SC-006 names the pre-PR gate. SC-007 names the operator-cadence harness re-run.
- [X] Success criteria are technology-agnostic (no implementation details)
  - SCs describe observable outcomes (audit-finding counts, byte-equality assertions, test pass/fail). The only "technology" references are the three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3) which are spec-bounded artifact formats.
- [X] All acceptance scenarios are defined
  - US1 has 5 scenarios covering the union-with-self degenerate cases (top-level-only, nested-only) plus the multi-entry union case. US2 has 3 scenarios for cross-ecosystem coverage.
- [X] Edge cases are identified
  - 7 distinct edge cases covered: three-or-more-entry case, empty-vec case, single-entry case, file-tier case, all-empty case, cross-ecosystem collision case.
- [X] Scope is clearly bounded
  - Out of Scope section lists 9 explicit exclusions (CDX bom-ref uniqueness, per-emitter dedup, new annotations, milestone 145 US3 changes, deduplicator group-key changes, other evidence fields, issues #1 and #2 from the triage thread, file-tier components, new parity-catalog row).
- [X] Dependencies and assumptions identified
  - Assumptions section names 8 explicit assumptions including Maven-dominance, deduplicator-key-preservation, source_file_paths-only scope, CDX-bom-ref-out-of-scope, no-new-deps, no-new-annotations, operator-cadence-sufficient, Maven-reader-unchanged.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
  - FR-001 ↔ US1 scenario 5. FR-002 ↔ SC-004. FR-003 ↔ Edge Case 7. FR-004 ↔ SC-005. FR-005 ↔ SC-004. FR-006 ↔ US1 scenarios 3 + 4 + 5. FR-007 ↔ US1 scenarios 3 + 4 + US2 scenario 3. FR-008 ↔ Constitution V audit. FR-009 ↔ SC-002. FR-010 ↔ SC-002.
- [X] User scenarios cover primary flows
  - US1 covers the singular value-add (cross-format `mikebom:source-files` parity on Maven nested-coords) + degenerate cases (single-entry, top-level-only, nested-only). US2 covers cross-ecosystem coverage as a free side-effect.
- [X] Feature meets measurable outcomes defined in Success Criteria
  - Every SC is achievable purely via the FR set.
- [X] No implementation details leak into specification
  - File paths and line numbers are scope anchors only. No function signatures or Rust syntax in spec body.

## Notes

- All checklist items pass on first iteration.
- The spec deliberately names `mikebom-cli/src/scan_fs/mod.rs` and `mikebom-cli/src/resolve/deduplicator.rs` as the fix site. The fix is post-dedup and cross-cutting across all ecosystem readers; placing it elsewhere would scatter the logic across readers (unnecessary fanout).
- The Constitution V audit (FR-008 + Constitution V audit section) deliberately documents the no-new-annotation claim — preventing future contributors from second-guessing the absence of a `mikebom:*` carry-out.
- Ready for `/speckit-clarify` (probably zero questions; spec is self-contained) or directly for `/speckit-plan`.
