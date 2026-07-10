# Specification Quality Checklist: yarn v1 + Berry optional-dependency classification

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-10
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references internal helpers as concepts for planning-phase mapping; Key Entities section is a design pointer, not code prescription.
- [X] Focused on user value and business needs — every user story anchors on the same pico filter-parity gap m179+m180 closed for Go/Cargo/npm/pnpm; m181 extends it to yarn v1 + Berry.
- [X] Written for non-technical stakeholders — reader can follow "when I declare foo as optional in yarn.lock/dependenciesMeta, mikebom marks it excluded in the SBOM" without opening code.
- [X] All mandatory sections completed — User Scenarios (3 stories), Requirements (13 FRs), Success Criteria (9 SCs), Assumptions, Constitution Alignment all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — all design decisions are pre-ratified: single derivation value from m180 design, peer-precedence rule from m179 FR-009 + m180 US4, workspace-member scope explicitly deferred as an Assumption.
- [X] Requirements are testable and unambiguous — FR-001 through FR-013 all specify observable emitted-document shapes or classifier-state transitions; FR-005 has explicit precedence contract; FR-011 is a byte-identity regression guard.
- [X] Success criteria are measurable — SC-001/002 are set-equality gates (CDX excluded = SPDX 2.3 typed-source); SC-003/004/005 are drift-counting gates; SC-009 is byte-identity across formats; SC-007 pins the peer-precedence regression.
- [X] Success criteria are technology-agnostic — reference SBOM format constructs (typed dep-scope verbs, `scope: "excluded"`) rather than mikebom-internal code paths.
- [X] All acceptance scenarios are defined — 3 user stories with 1-3 scenarios each; every scenario states GIVEN/WHEN/THEN with observable output shape.
- [X] Edge cases are identified — 7 cases: v1 dep-in-both-sub-blocks, diamond-shape (optional-by-one-parent + regular-by-another), Berry orphan `dependenciesMeta`, workspace-member scope, `--spdx2-relationship-compat=basic`, `--include-dev=false`, missing package.json.
- [X] Scope is clearly bounded — yarn v1 + Berry readers only, root package.json scope only (workspace-member deferred); all other ecosystems unaffected (FR-011).
- [X] Dependencies and assumptions identified — 8 assumptions covering yarn's own semantics, m180 helper reuse, single derivation value, workspace-scope deferral, zero new Cargo deps.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 + FR-002 ⇔ US1; FR-003 ⇔ US2; FR-005 ⇔ US3; FR-004 ⇔ package.json plumbing; FR-006 ⇔ build_entry extension (planning-phase decision documented); FR-007 ⇔ US1 edge case; FR-008/009/010 ⇔ format-emission constraints; FR-011/012/013 ⇔ regression guards; SC-001/002 measure primary US1/US2 delivery; SC-007 pins US3.
- [X] User scenarios cover primary flows — flagship pico-continuation for both yarn variants (US1+US2) + peer-precedence regression guard (US3).
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001+SC-002 directly measure US1+US2 delivery; SC-007 directly measures US3 delivery.
- [X] No implementation details leak into specification — spec names lockfile field paths (e.g., `optionalDependencies:`, `dependenciesMeta.<name>.optional`) but frames them as user-visible schema surfaces, not implementation prescriptions.

## Notes

- **All 16 checklist items PASS as of 2026-07-10**. Ready for `/speckit-implement`.
- /speckit-analyze findings applied (2026-07-10): C1 — T011 split into T011a (Full mode) + T011b (Basic mode) matching m180's t010/t011 pattern; I2 — plan.md project-structure section clarified that `read_yarn_lock` keeps signature but `parse_yarn_lock` gains `pkg_json` param. C2, I1, U1, U2 (LOW findings) left as-is per implementer-choice deferrals; documented as follow-up polish.
- Scope narrowed intentionally to ROOT package.json only — yarn workspaces are a legitimately bigger scope carve-out (multi-workspace fixture + per-workspace `dependenciesMeta` cross-reference) that fits a follow-up milestone better. Documented in Assumptions.
- The `is_peer_optional` helper is already in the tree marked `#[allow(dead_code)]` — m181's landing removes that marker (a small polish outcome).
- Delivery cadence: 3 P1 user stories fit a single-PR bundle if the code-cost per US stays reasonable. If v1 and Berry plumbing prove to have significantly different shapes, US1 and US2 could split into separate PRs on the same branch — deferred to plan/tasks.
