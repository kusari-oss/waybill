# Specification Quality Checklist: File-tier surfacing for source-heavy trees

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-01
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)

  Assessment: The spec references milestone IDs (m133, m033, m054, m113, m665) as *existing infrastructure to reuse* — appropriate scope framing. Naming the m133 `ContentShape::classify` function is a design-substrate reference, acceptable at spec level.

- [X] Focused on user value and business needs

  Assessment: All three user stories frame around SBOM operators + downstream consumers. Explicit callout of Constitution Principle VIII (Completeness) grounds the value.

- [X] Written for non-technical stakeholders

  Assessment: The context section explains WHY the mis-scoped `pypi ≥ 50` framing produced the SC-003 gap, in terms a compliance operator can follow. Technical anchors (m133, m665) reference shared vocabulary, not internals.

- [X] All mandatory sections completed

  Assessment: User Scenarios, Requirements, Success Criteria present. Edge Cases, Assumptions, Out of Scope filled.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain

  Assessment: Zero markers. The mode's exact CLI surface (`--file-inventory=<new-value>` value name) is deliberately left as a plan-phase decision — documented in Assumptions.

- [X] Requirements are testable and unambiguous

  Assessment: Each FR names a specific behavior + observable outcome. FR-002's extension list is closed and enumerated; FR-004/FR-005/FR-006 name specific dedupe/exclude/skip semantics.

- [X] Success criteria are measurable

  Assessment: SC-001 (≥100 file-tier), SC-003 (± 1% envelope), SC-005 (2× wall-clock ceiling), SC-006 (~2000-2400 `.py`-only bound) all name specific thresholds.

- [X] Success criteria are technology-agnostic (no implementation details)

  Assessment: Metrics are component counts, wall-clock deltas, and JSON-path verifiability. No mention of Rust internals or specific function names.

- [X] All acceptance scenarios are defined

  Assessment: 4 scenarios in US1, 3 in US2, 3 in US3, all well-formed Given/When/Then.

- [X] Edge cases are identified

  Assessment: 8 edge cases enumerated: symlink loops, oversize, binary-with-source-extension, package-DB overlap, `__pycache__`, vendored 3rd-party in Modules/, empty files, `--exclude` interaction.

- [X] Scope is clearly bounded

  Assessment: Explicit Out of Scope section listing 7 deferrals with rationale.

- [X] Dependencies and assumptions identified

  Assessment: 8 assumptions cataloged, each naming the existing milestone / mechanism being reused.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria

  Assessment: FR-001–FR-003 map to US1's Given/When/Then; FR-004–FR-006 map to US1 AS3 + edge cases; FR-007–FR-008 map to US2; FR-009 maps to US3; FR-010–FR-013 map to transparency + performance SCs.

- [X] User scenarios cover primary flows

  Assessment: US1 (main win) + US2 (regression floor) + US3 (scoping) covers the three operator paths.

- [X] Feature meets measurable outcomes defined in Success Criteria

  Assessment: US1 → SC-001 + SC-002 + SC-005 + SC-006; US2 → SC-003 + SC-004; US3 → SC-006; transparency FRs → SC-007.

- [X] No implementation details leak into specification

  Assessment: The `ContentShape::classify` reference is scope-clarifying (names WHERE the change lives) not implementation-directive. No source-code snippets, no data-structure design.

## Notes

- All 15 items pass. Spec is ready for `/speckit.plan`.
- **Post-`/speckit.clarify` (2026-09-01)**: 3 clarifications recorded in `spec.md ## Clarifications`. Q1 locked shape-restriction semantics as restrictive-subset (updated FR-009). Q2 locked CLI-flag surface as `--file-inventory=source-tree` + companion `--file-inventory-source-shapes` (updated FR-001). Q3 locked path-based dedupe on hash-divergence (updated FR-004).
- The spec deliberately reframes the m670 SC-003 target: `cpython ≥ 50 pypi` was mis-scoped and cannot be met (cpython legitimately consumes ~11 unique pypi deps). Reframed as `cpython ≥ 100 file-tier components under a new opt-in mode`, which is a Principle-VIII-honest measure of source-tree completeness.
- Two important guardrails: US2 preserves byte-identity for default-mode users (SC-003 + SC-004), preventing accidental SBOM inflation. FR-007 makes this a hard invariant.
