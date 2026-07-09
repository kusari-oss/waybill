# Tasks: Graph-completeness reachability signal (m177)

**Input**: Design documents from `/specs/177-graph-reachability-signal/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/, quickstart.md

**Tests**: Integration + unit tests included per spec SC-001…SC-008. The classifier is small (one pure function, one call site) so unit tests are minimal; integration tests carry most of the acceptance coverage per US.

**Organization**: Tasks are grouped by user story. This milestone has a substantial Foundational phase because the classifier extension is the shared prerequisite every US downstream tests — the classifier fires the code uniformly; US1/US2/US3 differ only in test scenarios.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 / US2 / US3
- Include exact file paths in descriptions

## Path Conventions

Code lives under `mikebom-cli/src/generate/graph_completeness/`; tests under `mikebom-cli/tests/`; docs under `docs/reference/`. All paths absolute from repo root.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm branch state and zero-dep posture. This milestone's branch was created before alpha.56 landed, so verify sync state.

- [X] T001 Verify branch `177-graph-reachability-signal` is checked out. Check `git log --oneline main..HEAD` — the branch may be behind main by 4 commits (m174 + m176 + m175 + alpha.56 bump landed while this spec was authored). If behind, fast-forward via `git stash --include-untracked` + `git merge main --ff-only` + `git stash pop`; resolve any CLAUDE.md conflicts (add m177 entry alongside existing m174/m175/m176 entries). Untracked state acceptable (m176-era `image-baz.cdx.actual.json` scratch file).
- [X] T002 Verify zero-dep claim via `cargo tree -p mikebom --depth 1` — existing `std::collections::HashMap` / `HashSet` + `ResolvedComponent.sbom_tier` + `Purl::ecosystem()` / `Purl::name()` all in the tree. No new Cargo deps needed.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend the m158 `ReasonCode` enum with the new variant + wire the classifier into `compute_graph_completeness`. **BLOCKS all user stories** — every US integration test depends on the classifier firing correctly. Unit-level correctness is verified here so US phases can focus on end-to-end scenarios.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 In `mikebom-cli/src/generate/graph_completeness/reason_codes.rs`, add the new `TransitiveEdgesUnresolvable { ecosystems: Vec<String> }` enum variant per data-model.md §Entity 1. Insert alphabetically-adjacent to `MultiEcosystemPartialRoot` (which has the closest structural precedent — both are ecosystem-list-shaped). Include the full doc-comment from data-model.md §Entity 1 explaining reachability-consumer semantic + PURL-type-canonical name contract + non-empty precondition.
- [X] T004 In the same file's `to_reason_string` `match` block, add the arm per data-model.md §Entity 2:
  ```rust
  Self::TransitiveEdgesUnresolvable { ecosystems } => format!(
      "transitive-edges-unresolvable: {}",
      ecosystems.join(", ")
  ),
  ```
  Preserves the existing arm ordering (semantic-driven, not alphabetical) — insert adjacent to `MultiEcosystemPartialRoot` for reader clarity.
- [X] T005 In the same file's `#[cfg(test)] mod tests` block, add 3 unit tests: (a) `transitive_edges_unresolvable_single_ecosystem` — `ecosystems: vec!["pypi".to_string()]` produces exactly `"transitive-edges-unresolvable: pypi"`; (b) `transitive_edges_unresolvable_multi_ecosystem` — `ecosystems: vec!["composer".to_string(), "pypi".to_string()]` (already sorted) produces `"transitive-edges-unresolvable: composer, pypi"`; (c) `join_with_orphaned_composes` — a `Vec<ReasonCode>` containing `OrphanedComponentsDetected { orphan_count: 3 }` + `TransitiveEdgesUnresolvable { ecosystems: vec!["pypi".to_string()] }` produces the semicolon-joined value `"orphaned-components-detected: 3 component(s) not reachable from root; transitive-edges-unresolvable: pypi"` via `join_reason_codes`. Guard the `mod tests` item with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per Constitution IV.
- [X] T006 In `mikebom-cli/src/generate/graph_completeness/mod.rs`, add the private `classify_transitive_edges_unresolvable(components: &[ResolvedComponent]) -> Option<ReasonCode>` classifier function per data-model.md §Entity 3 algorithm. Two-pass:
  - Pass 1: build `HashMap<(String, String), bool>` where key = `(purl.ecosystem(), purl.name())` and value = `true` iff any component with that key has `sbom_tier ∈ {Some("source"), Some("deployed"), Some("build")}`.
  - Pass 2: iterate components filtered to `sbom_tier ∈ {Some("design"), Some("analyzed")}`; for each, check the safe-lookup table; if no safe counterpart exists, insert `c.purl.ecosystem().to_string()` into a `HashSet<String>`.
  - If the affected-ecosystems set is non-empty, return `Some(TransitiveEdgesUnresolvable { ecosystems: sorted_dedup })`; else `None`.
  Complexity contract: `O(N)` time, `O(N)` auxiliary space. Pure function, no I/O.
- [X] T007 In the same file's `compute_graph_completeness`, wire the new classifier per data-model.md §Entity 4: insert after the closing `}` of the `if orphan_count > 0 { ... }` block (line ~267) and BEFORE the final `value` computation (line ~276):
  ```rust
  // Milestone 177 — classify tier-based reachability gaps.
  // Orthogonal to BFS-orphan classification — can fire even when
  // orphan_count == 0.
  if let Some(code) = classify_transitive_edges_unresolvable(components) {
      reason_codes.push(code);
  }
  ```
  Do NOT modify `orphan_count`, `reachable_count`, `total_count`, or `reachable_set` — those remain BFS-derived and continue to reflect graph-reachability, not tier-based reachability.
- [X] T008 Run `cargo +stable test -p mikebom --bin mikebom generate::graph_completeness` and confirm the 3 new unit tests plus all pre-existing graph_completeness tests pass. If the pre-existing `test_orphaned_components_detected` or similar tests hit unexpected `TransitiveEdgesUnresolvable` firings on their synthetic fixtures, update those tests to reflect the new expected reason-code composition (the classifier is deterministic — its firing on existing test-support fixtures is expected behavior, not a bug).

**Checkpoint**: `TransitiveEdgesUnresolvable` variant + classifier + call-site all landed and unit-tested. `compute_graph_completeness` now composes tier-based reachability gaps into `reason_codes` uniformly. All 3 US phases can now begin.

---

## Phase 3: User Story 1 — Reachability tool machine-check (Priority: P1) 🎯 MVP-part-1

**Goal**: A downstream reachability tool can machine-check `mikebom:graph-completeness-reason` for the `transitive-edges-unresolvable` substring and make a gating decision (refuse/downgrade/filter). Integration tests verify the jq recipe from contracts/reason-code-wire-format.md works end-to-end.

**Independent Test**: scan a `requirements.txt`-only pip fixture; assert the emitted CDX SBOM's reason value contains `"transitive-edges-unresolvable: pypi"`; assert the jq `contains(...)` recipe returns `true`. Verified by T009.

### Implementation for User Story 1

- [X] T009 [US1] Create `mikebom-cli/tests/reachability_signal.rs` with shared `scan_cdx` helper (release-independent `Command::new(bin())` invocation with `apply_fake_home_env` — same shape as m175's `design_tier_advisory.rs` and m176's `workspace_visibility.rs`). Add 2 US1 tests:
  - `t001_us1_machine_check_true_on_design_tier_scan` — synthesize `requirements.txt` fixture (3 constraint-only entries), scan under `--offline`, parse emitted CDX, assert the value of `mikebom:graph-completeness-reason` contains the exact substring `"transitive-edges-unresolvable: pypi"`.
  - `t002_us1_partial_graph_completeness_value` — same fixture, assert `mikebom:graph-completeness` value is `"partial"` (not `"complete"`) — verifies US1's core semantic assumption.
- [X] T010 [US1] Run `cargo +stable test -p mikebom --test reachability_signal` and confirm t001+t002 pass.

**Checkpoint**: US1 machine-check contract is verifiable. Reachability tools can wire the jq recipe from `contracts/reason-code-wire-format.md`.

---

## Phase 4: User Story 2 — Constraint-only scans emit accurate signal (Priority: P1) 🎯 MVP-part-2

**Goal**: The mikebom-side behavior change — pre-177 constraint-only scans reported `"complete"` (misleading); post-177 they report `"partial"` + the new reason code. This is the specific fix for the reachability-consumer false-positive.

**Independent Test**: same as US1's fixture — but the assertions focus on the *behavior change* (pre-177 vs post-177 differentiation), not the machine-check use case. T011 covers the offline-orthogonality gate + the empty-scan silent case.

### Implementation for User Story 2

- [X] T011 [US2] Extend `mikebom-cli/tests/reachability_signal.rs` with 2 US2 tests:
  - `t003_us2_advisory_fires_under_offline` (SC-005 offline-orthogonality) — scan the same pip requirements.txt fixture under `--offline`, assert the reason code STILL fires. Verifies FR-005 (semantic is orthogonal to network state).
  - `t004_us2_empty_scan_target_silent` (edge case) — synthesize a directory containing only a `README.txt` (zero manifests → zero components → the graph-completeness annotation itself doesn't emit; the reason cannot appear standalone). Assert the emitted SBOM has NO `mikebom:graph-completeness-reason` property OR the property exists but does NOT contain `"transitive-edges-unresolvable"`.
- [X] T012 [US2] Run the extended test file; confirm t003+t004 pass alongside t001+t002.

**Checkpoint**: US2 fully verified. Pre-177 → post-177 behavior change is asserted; edge cases (offline, empty scan) covered.

---

## Phase 5: User Story 3 — Polyglot scans enumerate affected ecosystems (Priority: P2)

**Goal**: On a polyglot scan where cargo is source-tier-resolved and pip is design-tier-only, the reason code's ecosystem list names ONLY `pypi` (not `cargo`) — enabling reachability tools to safely analyze the cargo subgraph while filtering out pypi.

**Independent Test**: synthesized cargo-with-lockfile + pip-without-lockfile fixture. Assert reason value contains `"transitive-edges-unresolvable: pypi"` AND does NOT contain `"cargo"` in that specific code's detail. Verified by T013.

### Implementation for User Story 3

- [X] T013 [US3] Extend `mikebom-cli/tests/reachability_signal.rs` with 2 US3 tests:
  - `t005_us3_polyglot_pypi_only_not_cargo` — synthesize `Cargo.toml` + `Cargo.lock` at `/rust/` subdir + `requirements.txt` at root. Scan. Assert reason value contains `"transitive-edges-unresolvable: pypi"` AND does NOT contain `"transitive-edges-unresolvable: cargo"` (cargo is source-tier via lockfile, so safe).
  - `t006_us3_multi_ecosystem_sorted_dedup` — synthesize a fixture with BOTH `requirements.txt` (pypi design-tier) AND `composer.json` without `composer.lock` (composer design-tier). Scan. Assert reason value contains `"transitive-edges-unresolvable: composer, pypi"` (alphabetically sorted).
- [X] T014 [US3 SC-002] Add `t007_sc002_safe_fixture_stays_complete` — synthesize cargo-with-lockfile-only (no pip). Scan. Assert `mikebom:graph-completeness` value is `"complete"` AND the emitted reason value (if the annotation is present) does NOT contain `"transitive-edges-unresolvable"`. Verifies SC-002's negative case — fully-resolved scans stay complete.
- [X] T015 [US3 SC-004 composition] Add `t008_sc004_composition_with_orphaned` — synthesize a fixture that triggers BOTH the new code AND `OrphanedComponentsDetected` (e.g., a pip requirements.txt fixture where m127's root-selector produces a root that doesn't reach every component). Scan. Assert reason value contains BOTH substrings — `"orphaned-components-detected:"` (or equivalent existing code) AND `"transitive-edges-unresolvable:"` — semicolon-joined per `join_reason_codes`. If constructing such a fixture proves hard because the fixture would need m127 root-selector to produce an orphan-generating layout, the test can synthesize by creating two disjoint pip requirements.txt files at different subdirs so multi-ecosystem-partial-root fires.
- [X] T016 [US3] Run `cargo +stable test -p mikebom --test reachability_signal` and confirm all 8 tests pass. If t015's composition-fixture construction proves impractical, downgrade to a doctest of `join_reason_codes` covering the composition case (the wire-format contract still holds; the fixture just doesn't naturally trigger both codes).

**Checkpoint**: US3 fully verified — polyglot analysis is safe; composition with existing codes works.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Docs + golden regeneration + SC-006/SC-007 gate verification + pre-PR + quickstart walk.

- [X] T017 [P] Extend `docs/reference/reading-a-mikebom-sbom.md` §3.4 subsection at line ~494 per data-model.md §Entity 5 shape: append a new sub-paragraph AFTER the existing `mikebom:graph-completeness + mikebom:graph-completeness-reason` reason-code enumeration explaining the milestone-177 `transitive-edges-unresolvable` code + its reachability-consumer contract + jq recipe for machine-checking + m175 compose-orthogonally cross-reference. ~40 lines.
- [X] T018 [P] Extend `docs/reference/sbom-format-mapping.md` C111 row per data-model.md §Entity 6 shape: update the closed-vocabulary enumeration in the Justification column to include the 9th code with the Constitution-Principle-X "closed vocabulary is additive" note. ~5 lines.
- [X] T019 Regenerate goldens: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression && MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression && MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression`. Then `git diff --stat mikebom-cli/tests/fixtures/golden/` and enumerate which fixtures flipped. Verify SC-006 gate: cargo/gem/npm/maven/apk/deb/rpm goldens show ZERO byte drift (except alpha.56→alpha.57 version bump if a release is also in this branch — but per plan this milestone isn't part of a release). Verify SC-007 gate: pip / composer / other design-tier-containing goldens show ONLY the two annotation deltas (`mikebom:graph-completeness` value + `mikebom:graph-completeness-reason` addition/extension); zero other bytes drift.
- [X] T020 Run `./scripts/pre-pr.sh` — verify both `cargo +stable clippy --workspace --all-targets -- -D warnings` (zero errors/warnings — matches CI 1.97 lint gates) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) pass. Preserve the full per-target output when confirming green per feedback_prepr_gate_full_output.md.
- [X] T021 Walk quickstart.md Path A (US2 constraint-only flip) + Path B (US1 machine-check) + Path C (US3 polyglot filter) + Path D (SC-002 safe fixture stays complete) + Path E (SC-004 composition) against the release build. Confirm each expected result per quickstart.md §Full success criteria table. Document any deviation in the PR body.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no blockers.
- **Foundational (Phase 2)**: depends on Setup. **BLOCKS ALL user stories** — the classifier is the substrate every US downstream tests.
- **User Story 1 (Phase 3)**: depends on Phase 2. Independent of US2/US3.
- **User Story 2 (Phase 4)**: depends on Phase 2. Extends the same test file US1 created but tests different scenarios.
- **User Story 3 (Phase 5)**: depends on Phase 2. Extends the same test file.
- **Polish (Phase 6)**: T017 + T018 (docs) can run in parallel and don't block on user-story code. T019 (golden regen) depends on Phase 2 landing. T020 + T021 depend on ALL prior phases.

### User Story Dependencies

- **US1 (P1) MVP-part-1** — independent given Phase 2.
- **US2 (P1) MVP-part-2** — extends US1's test file; landing US1 first avoids merge conflicts on the test file.
- **US3 (P2)** — extends US1+US2's test file; land after both.

### Within Each User Story

- Tests are additive to `mikebom-cli/tests/reachability_signal.rs`; land them in test-number order (t001…t008) to avoid file conflicts within the same PR.

### Parallel Opportunities

- **Phase 6 docs (T017 + T018)** [P] — two different files.
- Everything else is sequential or intra-Phase-2 lockstep (T003 → T004 → T005 → T006 → T007 → T008 must land in order — all touch the same two files).

---

## Parallel Example: Phase 6 docs

```bash
# Two independent doc edits can run in parallel:
Task: "Extend §3.4 subsection in docs/reference/reading-a-mikebom-sbom.md per data-model §Entity 5"
Task: "Extend C111 row in docs/reference/sbom-format-mapping.md per data-model §Entity 6"
```

---

## Implementation Strategy

### MVP First (US1 + US2 landed together — P1 milestone shape)

1. Complete Phase 1: Setup (T001–T002).
2. Complete Phase 2: Foundational (T003–T008) — new variant + classifier + call-site + unit tests.
3. Complete Phase 3: US1 tests (T009–T010).
4. Complete Phase 4: US2 tests (T011–T012).
5. **STOP and VALIDATE**: quickstart.md Path A + Path B walks. Reachability tools can machine-check the signal; constraint-only scans emit accurately. **Shippable as MVP.**
6. Optional: continue to US3 in a follow-up commit or bundle into the same PR.

### Incremental Delivery

1. Setup + Foundational → substrate ready.
2. Add US1 → reachability-consumer contract verified → shippable.
3. Add US2 in same PR → behavior change verified end-to-end.
4. Add US3 → polyglot scenarios verified → shippable.
5. Polish → docs + goldens + pre-PR.

### Solo Strategy (recommended for m177 given ~250 lines total)

1. Sequential through Phase 1 → 2 → 3 → 4 → 5 → 6.
2. Bundle into a single PR.
3. Estimated wall-clock: ~2-3 hours for a solo dev — implementation is minimal; test scaffolding + golden regen review is the bulk.

---

## Notes

- [P] tasks = different files, no dependencies.
- Every FR from spec.md maps to at least one task; every SC has a verifying task in Phase 5 or 6.
- **Golden regeneration is expected and bounded** per plan.md §Complexity Tracking — the SC-007 gate at T019 codifies "additions/extensions only" invariant. If a golden shows unrelated drift, investigate before proceeding.
- **First closed-vocabulary extension since m167**: T018's C111 row update formally records the 8 → 9 vocabulary bump. m158 governance protocol treats this as a CHANGELOG event on merge (no separate CHANGELOG file today, but the PR description carries this responsibility).
- Pre-PR gate (T020) is MANDATORY per project CLAUDE.md. Do not open PR without both clippy + tests clean.
