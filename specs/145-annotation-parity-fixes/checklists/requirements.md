# Specification Quality Checklist: Annotation-emission parity fixes from sbom-conformance audit (2026-06-26)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-27
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
  - Spec references specific files and line numbers as scope anchors (`file_tier/mod.rs:232-234`, `annotations.rs:227-236`, `v3_annotations.rs:267`) so the planner can locate the bug sites; these are scope-bounding pointers, not implementation prescriptions. No control flow, function bodies, or Rust syntax in the spec proper.
- [X] Focused on user value and business needs
  - US1 framed around downstream-consumer JSON parsing and jq queries; US2 framed around compliance gates for prod/dev distinction; US3 framed around cross-format SBOM consumer parity.
- [X] Written for non-technical stakeholders
  - Each user story has a plain-language journey + an explicit "Why this priority" paragraph + Given/When/Then scenarios that name observable behaviors.
- [X] All mandatory sections completed
  - Origin, User Scenarios & Testing, Requirements, Success Criteria, Assumptions, Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
  - Zero markers. All design decisions documented in Assumptions.
- [X] Requirements are testable and unambiguous
  - FR-001 to FR-011 each name a specific behavior with a verifiable assertion. Example: FR-001 specifies "MUST emit ... as a native JSON array of strings ... NOT as a JSON-string-encoding" — directly testable via type-check on the emitted value.
- [X] Success criteria are measurable
  - SC-001/SC-004/SC-008 name exact pre/post finding counts (3112→0, 261→0, 51→0). SC-002/SC-005/SC-009 name specific assertion types. SC-003/SC-006 name the golden-refresh scope. SC-010 names the pre-PR gate. SC-011 names cumulative finding-reduction floor. SC-012 names the no-regression invariant.
- [X] Success criteria are technology-agnostic (no implementation details)
  - SCs describe observable outcomes (harness findings counts, value type assertions, golden refresh scope, exit codes, byte-equivalent values). The only "technology" references are the three SBOM formats (CDX, SPDX 2.3, SPDX 3) which are spec-bounded artifact formats, not implementation choices.
- [X] All acceptance scenarios are defined
  - US1 has 4 scenarios; US2 has 4 scenarios; US3 has 4 scenarios. Each is Given/When/Then-shaped.
- [X] Edge cases are identified
  - 6 distinct edge cases covered: empty file-paths list, embedded quotes in paths, `LifecycleScope::Runtime` omission, `include_source_files = false` gating, empty `source_file_paths`, cross-US Maven/file-tier interaction.
- [X] Scope is clearly bounded
  - Out of Scope section lists 7 explicit exclusions (image-pseudo-component, sbom-tier disagreement, new mikebom:* annotations, BTreeMap restructuring, prior-run `<component-presence>` cluster, perf optimization, source-file-paths population-time changes).
- [X] Dependencies and assumptions identified
  - Assumptions section names 7 explicit assumptions including audit-count stability, file-paths shape-change as accepted wire-output break, additive-only SPDX 3 lifecycle-scope addition, shared `evidence.source_file_paths` field, canonical-value choice for source-files (with explicit conditional caveat), no new Cargo deps, and harness-as-oracle.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
  - FR-001 ↔ US1 scenarios 1+2 + SC-001/SC-002/SC-003. FR-002 ↔ Edge Cases §empty + §embedded quote. FR-003 ↔ US1 scenario 3. FR-004 ↔ US1 scenario 4 + SC-003. FR-005 ↔ US2 scenarios 1+4 + SC-004/SC-005. FR-006/FR-007 ↔ US2 + Edge Cases §Runtime + SC-005's second test. FR-008 ↔ SC-007 (research artifact). FR-009 ↔ US3 scenarios 1+2 + SC-008. FR-010 ↔ SC-003/SC-006. FR-011 ↔ explicit Constitution V audit-result statement.
- [X] User scenarios cover primary flows
  - US1 + US2 + US3 cover the three harness-flagged issues, each with their own Given/When/Then. P1/P1/P2 priority reflects audit-finding count + fix-complexity ratio.
- [X] Feature meets measurable outcomes defined in Success Criteria
  - Every SC is achievable purely via the FR set (no SC requires work outside the listed FRs).
- [X] No implementation details leak into specification
  - File paths and line numbers referenced are scope anchors, not implementation prescriptions. No function signatures, no Rust syntax, no library API calls in the spec body.

## Notes

- All checklist items pass on first iteration.
- The spec deliberately names specific file paths (`file_tier/mod.rs:232-234`, `annotations.rs:227-236`, `v3_annotations.rs:267`) because they bound the scope of the change — a planner reading this spec needs to know that the file-paths fix is in the file-tier constructor (not in any of the three emitters) and that the lifecycle-scope fix is in the SPDX 3 annotation emitter (not in the per-ecosystem readers). Without those anchors, the planner could mis-scope the work.
- The US3 investigation-first structure is deliberate: per FR-008 and SC-007, the research artifact MUST document the diagnosis before any code fix lands. This matches the spec's Assumption that the per-emitter drift is somewhere upstream of emission (both emitters consume the same `c.evidence.source_file_paths` field).
- Ready for `/speckit-clarify` (probably zero questions; the spec is fairly self-contained) or directly for `/speckit-plan`.
