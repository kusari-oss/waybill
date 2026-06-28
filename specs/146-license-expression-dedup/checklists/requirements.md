# Specification Quality Checklist: SPDX license expression operand dedup

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-28
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
  - Spec names specific file paths (`mikebom_common::types::license::SpdxExpression`, `spdx/packages.rs::reduce_license_vec`, `cyclonedx/builder.rs`, `spdx/v3_licenses.rs`) and the `spdx = "0.10"` crate as scope-bounding references — these are scope anchors, not implementation prescriptions. The spec doesn't specify control flow, function signatures, or Rust syntax in the spec body proper.
- [X] Focused on user value and business needs
  - US1 framed around compliance engineer + license-policy gates / SBOM diffing / sbomqs scoring. US2 framed around symmetric idempotency closure.
- [X] Written for non-technical stakeholders
  - Plain-language user journeys + explicit "Why this priority" + Given/When/Then scenarios.
- [X] All mandatory sections completed
  - Origin, User Scenarios & Testing, Requirements, Success Criteria, Assumptions, Out of Scope all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
  - Zero markers. All design decisions documented in Assumptions.
- [X] Requirements are testable and unambiguous
  - FR-001 to FR-009 each name a specific behavior with a verifiable assertion. Example: FR-001 specifies "MUST gain a deterministic dedup pass that collapses byte-identical top-level operands" — directly testable via per-input/per-output unit assertion.
- [X] Success criteria are measurable
  - SC-001 names a specific jq query returning empty. SC-002/SC-005 name specific unit-test assertions. SC-003 names the golden-refresh scope. SC-004 names an integration-test shape. SC-006 names the pre-PR gate. SC-007 names the harness-finding-count metric.
- [X] Success criteria are technology-agnostic (no implementation details)
  - SCs describe observable outcomes (jq returns empty, value-equality assertions, golden file states, pre-PR exit code). The only "technology" references are the three SBOM formats (CDX 1.6, SPDX 2.3, SPDX 3) which are spec-bounded artifact formats.
- [X] All acceptance scenarios are defined
  - US1 has 5 scenarios; US2 has 2 scenarios. Each is Given/When/Then-shaped.
- [X] Edge cases are identified
  - 7 distinct edge cases covered: WITH clauses preserved, case-only differences, whitespace-only differences, parenthesized sub-expressions, substring-but-not-identical operands, parse-failure no-op, single-operand no-op, already-canonical goldens.
- [X] Scope is clearly bounded
  - Out of Scope section lists 7 explicit exclusions (recursive dedup into parens, algebraic simplification beyond operand dedup, cross-tier license merging, reader-side parsing changes, manual emitter dedup method, license-policy gates, CDX expression-shape dedup specifically, new `mikebom:*` annotations).
- [X] Dependencies and assumptions identified
  - Assumptions section names 7 explicit assumptions including spdx-crate API reliance with string-split fallback, AND/OR idempotency safety, fix-at-try_canonical architectural choice, no semantic loss, no new Cargo deps, Yocto-side root cause out of scope, operator semantics + precedence handling.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
  - FR-001 ↔ US1 scenarios 1-3 + SC-002. FR-002 ↔ US1 scenarios 2, 3. FR-003 ↔ Edge Cases §WITH + SC-005. FR-004 ↔ US1 scenario 4 + Edge Cases §single-operand. FR-005 ↔ SC-002 (via `try_canonical` API). FR-006 ↔ Assumption 3 ("preserves existing best-effort raw storage contract"). FR-007 ↔ SC-003. FR-008 ↔ US1 scenario 5. FR-009 ↔ Constitution V audit in Out of Scope §7.
- [X] User scenarios cover primary flows
  - US1 + US2 cover both idempotent operators (AND + OR). The 7 acceptance scenarios + 7 edge cases span the realistic input space.
- [X] Feature meets measurable outcomes defined in Success Criteria
  - Every SC is achievable purely via the FR set.
- [X] No implementation details leak into specification
  - File paths and line numbers are scope anchors only. No function signatures or Rust syntax in spec body.

## Notes

- All checklist items pass on first iteration.
- The spec deliberately names `mikebom_common::types::license::SpdxExpression` because it bounds the scope of the change — a planner reading this spec needs to know that the fix lives ONE LEVEL DEEPER than the emitters (in the type, not in each emitter), so the cross-format invariance is achieved by a single code change. Without that anchor, the planner could mis-scope as a 3-site emission-layer change.
- The spec deliberately includes US2 (OR dedup) even though the audit corpus doesn't surface OR cases — the fix shape is identical and shipping both at once closes a symmetric gap.
- Ready for `/speckit-clarify` (probably zero questions; spec is self-contained) or directly for `/speckit-plan`.
