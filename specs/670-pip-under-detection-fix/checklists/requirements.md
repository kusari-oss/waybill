# Specification Quality Checklist: Fix critical Python under-detection

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-31
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)

  Assessment: The spec names format files (`pyproject.toml`, `uv.lock`, etc.) as *entities to be read* — these are user-visible artifacts of the Python ecosystem, not implementation choices. It references milestone IDs (m064, m179, m180, m191, m236) as *existing infrastructure to reuse* in Assumptions, which is appropriate scope framing.

- [X] Focused on user value and business needs

  Assessment: Each user story frames the value as "SBOM operator scans a project and gets a correct component list." Success criteria are cast in terms of component counts and % resolved.

- [X] Written for non-technical stakeholders

  Assessment: The Context table + fixture-driven user stories are readable without Rust or waybill-internals knowledge. Some FR clauses (e.g., FR-002's "matching the m179/m180 pattern used for npm") reference internal milestones — flagged as appropriate reference to shared vocabulary, not a stakeholder-facing gap.

- [X] All mandatory sections completed

  Assessment: User Scenarios, Requirements, Success Criteria present. Edge Cases, Assumptions, Out of Scope all filled.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain

  Assessment: Zero markers. All ambiguous scoping decisions were resolved via informed defaults (documented in Assumptions and Out of Scope).

- [X] Requirements are testable and unambiguous

  Assessment: Each FR names a specific file format + a specific parsing outcome. Each acceptance scenario is Given/When/Then with concrete input state.

- [X] Success criteria are measurable

  Assessment: SC-001 through SC-005 name specific fixtures + specific component-count thresholds. SC-007/SC-008 name specific wall-clock deltas.

- [X] Success criteria are technology-agnostic (no implementation details)

  Assessment: SC's speak to "emitted components" and "wall-clock time" — no mention of Rust crates, function names, or module paths.

- [X] All acceptance scenarios are defined

  Assessment: 4 scenarios in US1, 4 in US2, 3 in US3, all well-formed.

- [X] Edge cases are identified

  Assessment: 6 edge cases enumerated: multiple manifests, lockfile-vs-manifest disagreement, sub-project trees, cyclic references, syntax quirks, PEP 508 markers.

- [X] Scope is clearly bounded

  Assessment: Explicit Out of Scope section listing 6 deferrals with rationale.

- [X] Dependencies and assumptions identified

  Assessment: 8 assumptions cataloged, each naming the existing milestone/mechanism being reused.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria

  Assessment: FR-001–FR-011 map directly to user story acceptance scenarios (US1 for FR-001–FR-003, FR-008–FR-011; US2 for FR-004–FR-005; US3 for FR-006–FR-007). FR-012–FR-018 (correctness posture + reconciliation) map to edge cases.

- [X] User scenarios cover primary flows

  Assessment: The three P1/P2/P3 stories cover the three fixtures that under-detected, which are representative of the three Python-project shapes in the wild: modern lockfile-based, legacy requirements-based, legacy setup.py-based.

- [X] Feature meets measurable outcomes defined in Success Criteria

  Assessment: Every US has a matching SC-* (SC-001 ↔ US1, SC-002 ↔ US3, SC-003 ↔ US2, plus SC-004 through SC-008 for cross-cutting quality).

- [X] No implementation details leak into specification

  Assessment: FR-014 mentions `source_file_paths` — this is a data-shape concept (evidence provenance), part of the emitted SBOM contract, not an implementation detail. Acceptable.

## Notes

- All 15 items pass. Spec is ready for `/speckit.plan`.
- **Post-`/speckit.clarify` (2026-08-31)**: 5 clarifications recorded in `spec.md ## Clarifications`. Added FR-003a (Poetry-legacy), FR-005a (requirements-scope heuristic), FR-005b (PEP 508 direct-URL). Extended FR-004 (default-prune list) and FR-012 (multi-lockfile handling). Edge case "Multiple lockfiles in one directory" added.
- The spec deliberately leans on existing milestone infrastructure (m064, m179, m180, m191, m236) as its reuse substrate rather than proposing new mechanisms. This keeps the surface area small and matches the "zero new Cargo deps" convention of recent milestones.
- Sweep-fixture SBOM comparison (SC-005) presumes the sweep fixtures stay pinned; if `kusari-sandbox/test-*` HEADs move before this work ships, the SC-001/002/003 thresholds may need a re-baseline.
