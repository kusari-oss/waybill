# Specification Quality Checklist: pip / poetry / uv optional-dependency classification

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-10
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs) — spec references `pip/poetry.rs:67`, `pip/mod.rs:474`, `pip/uv_lock.rs:65` as file-path anchors so a planner can find the affected code, but the user-story text stays at "operator wants" level (SBOM filter-parity for pip-family manifests). Constitution Alignment cites internal types (`LifecycleScope::Optional`, `is_non_runtime()`) as design signals, not prescriptions
- [X] Focused on user value and business needs — three P1/P2 user stories each pin a distinct pico filter-parity gap (poetry.lock bug, PEP 621 gap, uv.lock gap); every FR traces to a user-visible false-positive Runtime edge that misleads SBOM consumers
- [X] Written for non-technical stakeholders — reader can follow "poetry package marked `optional = true` shouldn't appear as a Runtime dep, but today it does" without opening code
- [X] All mandatory sections completed — User Scenarios (3 stories + 12 edge cases), Requirements (13 FRs), Success Criteria (9 SCs), Assumptions, Constitution Alignment, Deferred to Future Milestones all present

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain — every design decision documented in Assumptions with cross-references to m179/m180/m181 precedent (uniform derivation-annotation value, diamond-shape Runtime-wins rule, lockfile-precedence, dev-wins-over-optional)
- [X] Requirements are testable and unambiguous — FR-001 through FR-013 all specify observable classifier decisions + emission behavior; FR-005/FR-006 pin exact precedence semantics; FR-011 pins the byte-identity regression guard
- [X] Success criteria are measurable — SC-001/002/003 are set-equality gates (pico filter-parity), SC-004 pins net-decrement-zero, SC-005/006 pin byte-identity on non-pip fixtures, SC-007 is the m228 basic-mode preservation gate, SC-009 is the C122 parity annotation gate
- [X] Success criteria are technology-agnostic — reference operator-visible behaviors (edge counts, filter-parity set-equality, byte-identity across formats) rather than specific parsing internals
- [X] All acceptance scenarios are defined — US1 has 4 scenarios (happy path, byte-identity annotation, dev-preservation, dev-wins-over-optional), US2 has 3, US3 has 3; every scenario uses GIVEN/WHEN/THEN
- [X] Edge cases are identified — 12 cases covering v1/v2 poetry dialects, dev+optional collision, multi-extra dep names, lockfile-vs-manifest precedence (2 cases), editable/workspace-member exclusion, `--spdx2-relationship-compat=basic`, `--include-dev=false`, setup.py exclusion, requirements.txt exclusion
- [X] Scope is clearly bounded — Deferred section explicitly lists setup.py, workspace-member cross-reference, per-extra classification, dev-name heuristic, and requirements.txt retrofit as OUT of m183 scope
- [X] Dependencies and assumptions identified — 7 assumptions covering shared-derivation-value reuse, poetry `optional` source-of-truth, PEP 621 uniform Optional classification, root-project-only scope, setup.py exclusion, requirements.txt exclusion, golden-fixture regeneration expectations, lockfile-precedence collision-avoidance

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria — FR-001 ⇔ US1 acceptance 1+2 + SC-001; FR-002 ⇔ US2 acceptance 1+2 + SC-002; FR-003 ⇔ US3 acceptance 1+2 + SC-003; FR-004 ⇔ SC-009 (single derivation-value byte-identity); FR-005 ⇔ US2 acceptance 3 (diamond-shape); FR-006 ⇔ Edge Cases lockfile-precedence; FR-007 ⇔ SC-007 (basic-mode preservation); FR-008 ⇔ Edge Cases `--include-dev=false`; FR-009/010 ⇔ SC-005/006 byte-identity guards; FR-011 ⇔ SC-005 regression pin; FR-012/013 ⇔ SC-008 test-continuity
- [X] User scenarios cover primary flows — three distinct filter-parity flows: poetry.lock classification fix (US1, the underlying bug — highest impact), PEP 621 pyproject.toml classifier (US2, growing PEP-621-native user base), uv.lock classifier (US3, fastest-growing Python tool). Combined they cover ~100% of the Python ecosystem lockfile landscape mikebom currently reads
- [X] Feature meets measurable outcomes defined in Success Criteria — SC-001/002/003 are the three individual gap-fix gates; SC-004 is the net-decrement-zero regression guard; SC-005/006 are byte-identity guards; SC-007 preserves m228 escape hatch; SC-009 is the C122 parity annotation gate
- [X] No implementation details leak into specification — spec names internal types (`LifecycleScope::Optional`, `is_non_runtime()`) and file-path anchors as design signals but frames them as planning-phase surfaces, not user-facing prescriptions

## Notes

- **All 16 checklist items PASS as of 2026-07-10**. Ready for `/speckit-plan`.
- **/speckit-analyze remediations applied (2026-07-10)**: R1 — extended T024 with an FR-008 sub-check (verifies `--include-dev=false` filters m183-Optional-classified entries via the T014 poetry-optional fixture); R2 — added T029 that runs `cargo tree -p mikebom | wc -l` pre/post to explicitly verify FR-013 (zero new production Cargo dep). Task count: 28 → 29. All /speckit-analyze findings were LOW severity; A1/D1/I1/I2/O1 legitimate acceptance-as-is (accepted per rationale in the analyze report).
- Delivery cadence: 3 user stories (US1 + US2 as P1 for the biggest installed base; US3 as P2 for uv). Fit a single-PR bundle per m180/m181 precedent, but US3 can defer to a follow-up if implementation surprises arise.
- **US1 (poetry.lock) is a SILENT BUG FIX** — not just a filter-parity extension. Any poetry-locked project scanned since m179 has been emitting `optional = true` packages as Runtime. This means the m183 poetry.rs regression fixture at line ~178 (which currently asserts `LifecycleScope::Runtime` for `optional = false, category = "main"` entries) needs a NEW fixture entry for `optional = true, category = "main"` to lock in the fix. Existing dev-path assertion continues unchanged.
- **US2 is the highest implementation-complexity slice** — the main-module extractor currently flattens both `[project.dependencies]` and `[project.optional-dependencies]` into one `depends: Vec<String>`. Splitting them requires either a data-model change (two lists) or a downstream classifier pass. Planning-phase decision per FR-002.
- **US3 (uv.lock) is the smallest slice** — the reader already parses the TOML shape; adding a `[[package]].optional-dependencies.<extra>` sub-table walk is additive to the existing loop.
- **Shared derivation value `"pip-optional-dependencies"`** — third flavor after m179's `"cargo-optional-feature"` and m180/m181's `"npm-optional-dependencies"`. The value-set stays SMALL and enumerable (three values total after m183) so the C122 parity extractor's validation stays simple.
- **Dev-wins-over-optional precedence (Edge Cases + US1 acceptance 4)**: matches poetry's own semantic (a dev-group + optional-flagged package is only installed under `poetry install --with dev --extras foo`; the outer gate is the dev-group filter). Simplifies the classifier + preserves `--include-dev=false` filtering behavior.
- **Lockfile-precedence (FR-006)**: prevents double-emission of the derivation annotation when both `poetry.lock` and `pyproject.toml` are present. Matches m179's Go-graph ladder-precedence philosophy.
- No clarifications needed. The m179+m180+m181 precedent + poetry.lock's own semantic specification resolve every "how should this behave?" question. Delivery follows the established m179+ cadence.
