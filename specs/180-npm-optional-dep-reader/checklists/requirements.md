# Specification Quality Checklist: npm / yarn / pnpm optional-dependency classification

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-09
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references internal field names as concepts for planning-phase mapping; entities section is design pointer, not code prescription. Reader file paths appear only in Key Entities' audit-surface notes.
- [X] Focused on user value and business needs — every user story anchors on the pico filter-parity gap the m179 flagship closed for Go; m180 extends it to npm/yarn/pnpm/bun.
- [X] Written for non-technical stakeholders — reader can follow "when I declare foo in optionalDependencies, mikebom marks it excluded" without opening code.
- [X] All mandatory sections completed — User Scenarios (5 stories), Requirements (14 FRs), Success Criteria (8 SCs), Assumptions, Constitution Alignment all present.

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — the m178 peer-precedence rule was already ratified in m179's spec (FR-009), so no re-clarification needed; single-value derivation string (`"npm-optional-dependencies"`) documented in Assumptions.
- [X] Requirements are testable and unambiguous — FR-001 through FR-014 all specify observable emitted-document shapes; FR-006 has explicit precedence contract; FR-012 is a byte-identity regression guard.
- [X] Success criteria are measurable — SC-001 is set-equality (CDX excluded = SPDX 2.3 typed-source); SC-002/003/004 are drift-counting gates; SC-006 is byte-identity across formats; SC-007 pins the peer-precedence regression.
- [X] Success criteria are technology-agnostic — reference SBOM format constructs (typed dep-scope verbs, `scope: "excluded"`) rather than mikebom-internal code paths.
- [X] All acceptance scenarios are defined — 5 user stories with 1-3 scenarios each; every scenario states GIVEN/WHEN/THEN with observable output shape.
- [X] Edge cases are identified — 8 cases: diamond-shape (regular + optional), authoring-conflict (dependencies + optionalDependencies), devDependencies + optionalDependencies precedence, empty optionalDependencies, `--include-dev=false` filter interaction, peer-optional collision, basic-mode collapse, non-JavaScript ecosystem unaffected.
- [X] Scope is clearly bounded — npm / yarn / pnpm / bun readers only; other ecosystems unaffected (FR-012); no new format-emitter code (piggybacks on m179's classifier + emitter).
- [X] Dependencies and assumptions identified — 7 assumptions covering lockfile-as-authority, pnpm v9 baseline, yarn Berry's package.json source, single derivation value, peer-precedence rule (already ratified), no new Cargo deps, `include_dev` gating via `is_non_runtime()`.

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 ⇔ US1; FR-002 ⇔ US2; FR-003 ⇔ US3; FR-004 ⇔ US5; FR-006 ⇔ US4 (peer precedence); FR-005 ⇔ SC-006; FR-009 ⇔ SC-003; FR-010 ⇔ SC-004; FR-011 ⇔ SC-005; FR-012 ⇔ SC-002 + SC-003.
- [X] User scenarios cover primary flows — flagship pico-continuation flows for npm/pnpm (US1+US2), yarn plumbing story (US3), peer-precedence regression guard (US4), bun ecosystem coverage (US5).
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001 directly measures the flagship US1+US2 delivery.
- [X] No implementation details leak into specification — spec names lockfile field paths (e.g., `packages/node_modules/fsevents`, `importers/.:optionalDependencies:`) but frames them as user-visible schema surfaces, not implementation prescriptions.

## Notes

- **All 16 checklist items PASS as of 2026-07-09**. Ready for `/speckit-plan`.
- Scope is one milestone ahead of the m179 plan.md Decision 4 cadence — US4 becomes m180 exactly per the plan.
- The peer-precedence question (m178 vs m180 for peer-optional deps) is NOT a fresh clarification because m179's spec FR-009 already ratified the precedence at spec-authoring time. Re-litigating it in m180 would be a redundant clarification.
- The single `"npm-optional-dependencies"` derivation-value covering all four JavaScript lockfile variants is a design decision documented in Assumptions; if a future consumer needs finer granularity, a follow-up milestone can extend the value vocabulary without changing the annotation name (m179 FR-019 explicitly leaves this door open).
- Delivery cadence for m180's 5 user stories: expected single-PR bundle covering US1 + US2 + US4 (both P1 + the P1 regression guard) as the MVP; US3 (yarn plumbing) may split into a separate PR on the same branch depending on the code-churn size; US5 (bun) can defer to m181 if the reader-touch cost turns out too high vs the small bun user share.
