---
description: "Tasks: Go reader go.sum-based transitive fallback (closes #174)"
---

# Tasks: Go reader go.sum-based transitive fallback

**Input**: Design documents from `/specs/091-go-sum-transitive-fallback/`
**Prerequisites**: spec.md ✅, plan.md ✅, research.md ✅, data-model.md ✅, contracts/go-sum-fallback.md ✅, quickstart.md ✅

**Organization**: Mid-sized internal-library extension (one new ladder step + per-format provenance discriminator). Phase 1 captures the pre-fix 31-edge baseline. Phase 2 adds the foundational enum variant + step-5 body + LadderSummary counter. Phase 3 = US2 (regression net for cache-populated path — must run FIRST among the user stories because if it breaks, the whole approach is wrong). Phase 4 = US1 (≥130-edge verification + baseline bump). Phase 5 = US3 (per-component provenance discriminator in CDX/SPDX 2.3/SPDX 3). Phase 6 = Polish.

## Path Conventions

Repository-relative paths from `/Users/mlieberman/Projects/mikebom/`:
- Resolver: `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`
- Per-format emission: `mikebom-cli/src/generate/{cyclonedx_v1_6,spdx_2_3,spdx_3_0_1}.rs` (exact filenames depend on existing layout)
- Audit fixture (resolved via `MIKEBOM_FIXTURES_DIR`): `<cache>/transitive_parity/go/`
- Regression test: `mikebom-cli/tests/transitive_parity_go.rs`
- Audit doc: `specs/083-transitive-correctness/research.md` §8 — Ecosystem: Go

---

## Phase 1: Setup (pre-fix evidence)

- [X] T001 [P] Capture the pre-fix 31-edge baseline per quickstart Recipe 1: `target/release/mikebom --offline sbom scan --path "$MIKEBOM_FIXTURES_DIR/transitive_parity/go" --format spdx-2.3-json --output /tmp/pre-091.spdx.json --no-deep-hash`. Run `jq '[.relationships[]? | select(.relationshipType == "DEPENDS_ON")] | length' /tmp/pre-091.spdx.json` and confirm 31. Records baseline for FR-001 / SC-001 verification.
- [X] T002 [P] Confirm research §2 dispatch decision matches the actual code at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs:322` (`GraphResolver::resolve()`). Verify the existing call chain `step1_go_mod_graph → step2_cache_walk → step3_proxy_fetch → step4_empty_fallthrough` is sequential as expected. No code edits in this task.

## Phase 2: Foundational (new variant + step body + summary counter)

- [X] T003 Add `ResolutionStep::GoSumFallback` variant to the enum at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs:64`, BEFORE the existing `None` variant per VR-091-001.
- [X] T004 Add `gosum_fallback_count: usize` field to `LadderSummary` at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs:154`. Update `Default` impl + `Display` impl + the `tracing::info!` line at line ~368 to include the new counter per VR-091-003 + VR-091-004.
- [X] T005 Implement `step5_go_sum_fallback` method on `impl GraphResolver` per quickstart Recipe 2 step 4 + data-model.md "step5_go_sum_fallback method (new)" contract. Iterate `ctx.go_sum_modules`, skip modules already in `map`, insert `ModuleGraphEntry { module, requires: vec![], source: GoSumFallback }` for each new one. Increment `summary.gosum_fallback_count`. Then insert a synthetic root-module entry with `requires = ctx.go_sum_modules.iter().cloned().collect()` and `source = GoSumFallback`. Verifies VR-091-006 + VR-091-007 + VR-091-009.
- [X] T006 Wire step 5 into `GraphResolver::resolve` at `graph_resolver.rs:322`: insert `self.step5_go_sum_fallback(&mut map, ctx);` between the existing step 3 call and the existing step 4 call. Verifies VR-091-008.
- [X] T007 Verify `cargo +stable build --workspace` compiles cleanly. If `WorkspaceContext` lacks a public `root_module_id()` helper, add a minimal one (private to the module is fine — only step 5 needs it). Record any helper-additions in the PR description's "What changed" list.

## Phase 3: US2 — Cache-populated path still works at milestone-055 fidelity (Priority: P1) — regression net

**Goal**: Confirm `cargo +stable test --workspace` reports zero failures in the milestone-055 cache-populated tests post-step-5 addition.

**Independent Test**: Run `cargo +stable test -p mikebom --test scan_go` AND `cargo +stable test -p mikebom --test cdx_ref_closure_invariant`. Every test reports `0 failed`.

**Why first among user-story phases**: if cache-populated tests break, the step-5 implementation has a bug (probably its `if map.contains(module) { continue; }` guard isn't catching cache-populated entries). US1's edge-count verification is moot if the regression net fails.

### Implementation for User Story 2

- [X] T008 [US2] Run `cargo +stable test -p mikebom --test scan_go` and confirm every milestone-055-era test passes. Specifically the `scan_go_source_tree_emits_transitive_edges_when_cache_present` and any `*_cache_*` tests in that file MUST pass with zero behavioral changes. Verifies FR-005.
- [X] T009 [US2] Run `cargo +stable test -p mikebom --test cdx_ref_closure_invariant` and confirm 5/5 pass. Verifies that step 5 doesn't introduce orphan refs in any ecosystem (the closure-invariant test runs across all 6 post-053 ecosystems including go).
- [X] T010 [US2] Run `cargo +stable test --workspace` (full suite) and confirm zero failures across all test crates. The full suite catches any cross-test interference.

## Phase 4: US1 — Operator on CI sees ≥130 edges (Priority: P1)

**Goal**: trivy fs scan against post-091 mikebom output for the cri-tools fixture flags ≥130 components in the dep edge set, up from 31 today. Edge count regression test bumps + new representative edge captures step-5-only transitives.

**Independent Test**: `target/release/mikebom --offline sbom scan` against the cri-tools fixture emits ≥130 DEPENDS_ON edges; the milestone-083 `transitive_parity_go.rs` test passes with the bumped baseline.

### Implementation for User Story 1

- [X] T011 [US1] Re-run the smoke test post-step-5 per quickstart Recipe 3: `target/release/mikebom --offline sbom scan --path "$MIKEBOM_FIXTURES_DIR/transitive_parity/go" --format spdx-2.3-json --output /tmp/post-091.spdx.json --no-deep-hash`. Confirm DEPENDS_ON count ≥130 via the same `jq` query as T001. Record the exact post-fix count for T012's baseline.
- [X] T012 [US1] Update `mikebom-cli/tests/transitive_parity_go.rs` `EXPECTED_MIKEBOM_EDGE_COUNT` constant from 31 to T011's recorded count. Per the milestone-083 quickstart Recipe 3 baseline-bump pattern (used in milestones 087/088). Verifies VR-091-013 + FR-006.
- [X] T013 [US1] Add at least one new entry to `EXPECTED_REPRESENTATIVE_EDGES` in `transitive_parity_go.rs` that exercises step 5: pick a `pkg:golang/<root> → pkg:golang/<go-sum-only-transitive>` pair from T011's emitted SBOM where the target wasn't reachable pre-091. Update the surrounding doc-comment to add a "Closed by milestone 091" subsection mirroring milestones 087/088. Verifies VR-091-014.
- [X] T014 [US1] Run `cargo +stable test -p mikebom --test transitive_parity_go` and confirm all 4 tests pass with the bumped baseline + new representative edge.

## Phase 5: US3 — Per-component provenance discriminator (Priority: P2)

**Goal**: Every step-5 component carries a per-format provenance annotation distinguishing it from step-1/2/3 components. Operators can read a single field per emitted CDX `Component` / SPDX 2.3 `Package` / SPDX 3 `software_Package` to determine the discovery step.

**Independent Test**: Inspect post-fix emitted SBOMs against the cri-tools fixture. Verify CDX components reached via step 5 have `confidence: 0.50` + `mikebom:resolver-step = go-sum-fallback` property; SPDX 2.3 packages have `annotations[].comment` containing `mikebom:resolver-step=go-sum-fallback`; SPDX 3 has the equivalent `Annotation.statement`.

### Implementation for User Story 3

- [X] T015 [P] [US3] Update CDX 1.6 emission code (`mikebom-cli/src/generate/cyclonedx_v1_6.rs` or wherever the `mikebom:resolver-step` property is set per milestone 084) to recognize `ResolutionStep::GoSumFallback` and emit (a) `Component.evidence.identity[].methods[].confidence = 0.50` AND (b) `Component.properties[]` entry `mikebom:resolver-step = go-sum-fallback`. Verifies VR-091-010 + VR-091-011 (CDX side).
- [X] T016 [P] [US3] Update SPDX 2.3 emission code (`mikebom-cli/src/generate/spdx_2_3.rs`) to recognize `GoSumFallback` and emit `package.annotations[]` entry with `comment = "mikebom:resolver-step=go-sum-fallback"` (annotationType `OTHER`). Verifies VR-091-010 (SPDX 2.3 side).
- [X] T017 [P] [US3] Update SPDX 3 emission code (`mikebom-cli/src/generate/spdx_3_0_1.rs`) to recognize `GoSumFallback` and emit an `Annotation` element with `statement = "mikebom:resolver-step=go-sum-fallback"` and `subject` cross-referencing the package. Verifies VR-091-010 (SPDX 3 side).
- [X] T018 [US3] Spot-check the post-fix SBOM has the annotations: `jq '.packages[] | select((.annotations // []) | any(.comment | contains("go-sum-fallback")))' /tmp/post-091.spdx.json | head -20` returns ≥10 entries. Verifies VR-091-012 (the discriminator's presence is meaningful — non-step-5 components don't have it).

## Phase 6: Polish

- [X] T019 Regenerate goldens per quickstart Recipe 5: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression`, repeat for SPDX 2.3 + SPDX 3. Expected: at most 3 modified files (the 3 `golang.*` goldens) IF the milestone-013 `golang/simple-module` fixture's go.sum populates step 5; possibly 0 modifications if simple-module's tests still flow through steps 1/2.
- [X] T020 Audit golden diff scope: `git diff mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json` shows ONLY (a) confidence 0.85 → 0.50 changes for step-5 components AND (b) new `mikebom:resolver-step` property values. Zero PURL changes, zero component-count changes, zero dep-edge endpoint changes. Verifies the per-format scope contract.
- [X] T021 [P] Update `specs/083-transitive-correctness/research.md` §8 — Ecosystem: Go audit row to mark the gap closed. Strikethrough the original observation; add "Closed by milestone 091" annotation block referencing `transitive_parity_go.rs::EXPECTED_REPRESENTATIVE_EDGES` as the lock-down test. Mirror the milestone-087/088 closure pattern.
- [X] T022 Run `./scripts/pre-pr.sh`: zero clippy warnings + every test suite reports `0 failed`. Verifies SC-003 + the standard CLAUDE.md mandatory gate.
- [X] T023 Update CLAUDE.md "Recent Changes" if the speckit infrastructure didn't auto-update it (verify with `grep "091-go-sum-transitive-fallback" CLAUDE.md`).

---

## Dependencies & Execution Order

- T001 + T002 (Phase 1, parallel) — pre-fix scan + dispatch-confirm are independent.
- T003 → T004 → T005 → T006 → T007 (Phase 2, sequential — same files, additive edits).
- **Phase 2 MUST complete before Phase 3+** — no user-story tests run cleanly until the new variant + step body compile.
- T008 → T009 → T010 (Phase 3 US2, sequential by dependency depth — scan_go is the narrowest signal, closure-invariant cross-cutting, full workspace catches everything).
- **Phase 3 MUST complete before Phase 4** — if cache-populated tests fail, the step-5 implementation is wrong and US1's verification is moot.
- T011 → T012 → T013 → T014 (Phase 4 US1, sequential — record count → bump baseline → add edge → verify).
- T015 + T016 + T017 (Phase 5 US3, parallel — different files).
- T018 (Phase 5 verification) sequential after T015+T016+T017.
- T019 → T020 (Polish goldens + diff audit, sequential — diff must run AFTER regen).
- T021 (Polish audit doc) parallel with T019/T020 (different file).
- T022 → T023 (Polish gate + CLAUDE.md, sequential — gate runs last).

## Parallel Opportunities

- T001 + T002 (Phase 1).
- T015 + T016 + T017 (Phase 5 — different generation files).
- T021 in parallel with T019/T020 (different files; no inter-task dependency within Polish).

## Notes

- **No new Cargo dependencies** at the lockfile level — `parse_go_sum` already exists at `legacy.rs:353`.
- **Goldens MAY regenerate** for the `golang.*` fixtures only (CDX + SPDX 2.3 + SPDX 3). All non-golang goldens MUST stay byte-identical (FR-007 invariant).
- **PR diff target**: ~70 LOC in `graph_resolver.rs` + ~15 LOC across the 3 generation files + 5 LOC test-baseline bump + ~20 LOC audit-doc updates. Total ~110 LOC source diff.
- **Suggested MVP scope**: Phases 1+2+3+4 (the step-5 implementation + regression + edge-count verification). Phase 5 (US3 provenance) and Polish ship in the same PR for atomicity but are independently testable.
- **Backward-compatibility**: cache-populated paths emit byte-identical output (FR-005). Offline-cache-empty paths emit strictly MORE components in the dep edges, with the new ones tagged `go-sum-fallback`. Pre-091 SBOMs that passed downstream consumers continue to pass.
