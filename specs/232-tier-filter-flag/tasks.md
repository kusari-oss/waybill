---

description: "Task list for feature 232-tier-filter-flag: --tier=<mode> output-filter flag on waybill sbom scan"
---

# Tasks: `--tier=<mode>` output-filter flag

**Input**: Design documents from `/specs/232-tier-filter-flag/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/tier-filter-cli.md`, `quickstart.md`

**Tests**: Included. Colocated unit tests for the filter helper + one subprocess-based integration test file with one assertion per mode + FR-008 empty-result.

**Organization**: Three user stories (US1 vulnerability-scanner P1 = MVP; US2 compliance-attribution P2; US3 container-artifact P3). US2 and US3 are trivial variants of US1's plumbing — same enum extension, same filter path.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no cross-task dependencies
- **[Story]**: `[US1]` / `[US2]` / `[US3]` — maps back to spec.md user stories
- File paths absolute or repo-relative; every task cites exact file

---

## Phase 1: Setup

- [X] T001 Verify feature branch is `232-tier-filter-flag` (per `git branch --show-current`) and that `cargo +stable check -p waybill --lib` exits 0 against the untouched tree — locks in the pre-change baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Land the `TierMode` enum, the `--tier` CLI flag, and the `apply_tier_filter` helper. Same-file edits; sequential.

- [X] T002 In `waybill-cli/src/cli/scan_cmd.rs`, add a `TierMode` `clap::ValueEnum` next to the existing `EnrichSource` (line ~41) or `SbomSourceMode` (line ~77) enums, matching the 4-variant shape in `data-model.md § New enum`. Derives: `ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq`. `#[clap(rename_all = "kebab-case")]`. `#[default]` on `All`. Doc comment cites #660 + `specs/232-tier-filter-flag/spec.md`.
- [X] T003 In the same file, add a `pub tier: TierMode` field on `ScanArgs` (grep for `pub struct ScanArgs` — the top-level scan CLI struct). Use `#[arg(long, value_enum, default_value_t = TierMode::All)]`. Doc comment cites the FR-002 byte-parity guarantee: default value MUST be `TierMode::All` so pre-232 invocations produce identical output.
- [X] T004 In the same file, add a module-private helper `fn apply_tier_filter(components: &mut Vec<ResolvedComponent>, relationships: &mut Vec<Relationship>, mode: TierMode)` implementing `data-model.md § New helper`. Early-return on `TierMode::All`. For other modes: build a `HashSet<String>` of dropped-component PURLs via a `tier_matches` closure; `components.retain(...)`; `relationships.retain(|r| !dropped.contains(&r.from) && !dropped.contains(&r.to))`; emit `tracing::info!` with drop count + mode; emit an additional `tracing::warn!` when the post-filter `components` is empty (FR-008).
- [X] T005 In the same file, insert the `apply_tier_filter(&mut components, &mut relationships, args.tier)` call site AT `scan_cmd.rs:~3200` — immediately after the existing `--exclude-scope` filter block (lines 3175-3199) and BEFORE the format-builder dispatch. This ordering is REQUIRED by SC-004 (graph-completeness must reflect the filtered set) — the format builders' internal `compute_graph_completeness` runs over whatever slice they receive.

**Checkpoint**: Flag exists in `--help`; helper compiles; existing tests continue passing.

---

## Phase 3: User Story 1 — Vulnerability-scanner source-only pipeline (Priority: P1) 🎯 MVP

**Goal**: `waybill sbom scan --tier=source-only` emits an SBOM whose `components[]` is exclusively `sbom_tier: "source"`; edges with any filtered-out endpoint are dropped; document-scope annotations reflect the filtered graph.

**Independent Test**: Scan the m230 `packages_lock_present` fixture with `--tier=source-only`; assert every emitted NuGet component is source-tier and zero design-tier PURLs appear anywhere in the SBOM.

### Tests for User Story 1

- [X] T006 [P] [US1] Add unit test `apply_tier_filter_source_only_drops_design` in `scan_cmd.rs` `mod tests` block (if none exists, create it). Fixture: build a `Vec<ResolvedComponent>` with 3 source-tier + 2 design-tier + 1 binary-tier components; call `apply_tier_filter(&mut components, &mut edges, TierMode::SourceOnly)`; assert `components.len() == 3` and every survivor has `sbom_tier == Some("source")`.
- [X] T007 [P] [US1] Add unit test `apply_tier_filter_drops_dangling_edges` in same block. Fixture: 2 source-tier components + 2 design-tier components; 4 edges (source→source, source→design, design→source, design→design); call `apply_tier_filter(_, _, TierMode::SourceOnly)`; assert only the source→source edge survives.
- [X] T008 [US1] Create integration test `waybill-cli/tests/tier_filter_flag.rs`. Reuse the `common::bin` + `apply_fake_home_env` scaffold from `waybill-cli/tests/nuget_main_module_parity.rs`. Add test `tier_source_only_drops_design_tier_components` — spawn `waybill sbom scan --tier=source-only` against `tests/fixtures/golden_inputs/nuget/packages_lock_present`; parse emitted CDX; assert every component with `waybill:sbom-tier` property has value `"source"` (or no `waybill:sbom-tier` property at all, though pre-232 emission tags source-tier explicitly); assert zero `pkg:generic/App@0.0.0`-shape design-tier PURLs remain.

### Implementation for User Story 1

- [X] T009 [US1] After tests fail with the expected "flag not recognized" clap error (pre-T002-T005), complete Phase 2 helpers. Then re-run tests T006-T008; expect green.
- [X] T010 [US1] Additional unit test `apply_tier_filter_empty_result_emits_warn` in `mod tests`. Fixture: `Vec<ResolvedComponent>` of 2 design-tier components; call with `TierMode::SourceOnly`. Assert `components.is_empty()` and `relationships.is_empty()`. (WARN log emission is tested at integration-test tier via T012 below since `tracing::warn!` in a unit-test context requires more scaffolding than it's worth.)

**Checkpoint**: `--tier=source-only` end-to-end produces the expected filter shape on the m230 fixture. Unit + integration tests green.

---

## Phase 4: User Story 2 — Compliance/attribution design-only pipeline (Priority: P2)

**Goal**: `--tier=design-only` emits an SBOM with only design-tier components. Same plumbing as US1 with a different `TierMode` variant.

**Independent Test**: Scan the m230 fixture with `--tier=design-only`; assert every emitted component has `sbom_tier == "design"`.

- [X] T011 [P] [US2] Add unit test `apply_tier_filter_design_only_keeps_only_design` in `scan_cmd.rs mod tests`. Same fixture shape as T006; call with `TierMode::DesignOnly`; assert 2 design-tier survivors.
- [X] T012 [US2] Extend `waybill-cli/tests/tier_filter_flag.rs` with `tier_design_only_keeps_only_design_components` — spawn scan with `--tier=design-only` against `packages_lock_present`; assert the emitted SBOM contains the single design-tier main-module component (`pkg:generic/App@0.0.0` per m230's version-ladder fallback) and no source-tier NuGet PURLs.

---

## Phase 5: User Story 3 — Container/artifact source-and-binary pipeline (Priority: P3)

**Goal**: `--tier=source-and-binary` retains source-tier AND binary-tier components; drops everything else.

**Independent Test**: Scan a fixture with source-tier + binary-tier components; assert both survive; design-tier drops.

- [X] T013 [P] [US3] Add unit test `apply_tier_filter_source_and_binary_keeps_both` in `scan_cmd.rs mod tests`. Fixture from T006; call with `TierMode::SourceAndBinary`; assert 4 survivors (3 source + 1 binary), 0 design.
- [X] T014 [US3] Extend `waybill-cli/tests/tier_filter_flag.rs` with `tier_source_and_binary_keeps_source_only_when_no_binary` — the m230 fixture has no binary-tier components, so this test verifies `--tier=source-and-binary` degenerates cleanly to `--tier=source-only` output on this fixture. Assert the same source-tier PURL set survives. **FR-005's "binary retention" clause is exercised at the unit level via T013 (`apply_tier_filter_source_and_binary_keeps_both`) which builds a hand-crafted `Vec<ResolvedComponent>` with a binary-tier entry — no fixture change is needed to prove the semantic** (closes analyze-report F2 gap). Deferred to a follow-up milestone: if a binary-tier integration fixture is added, extend this test to assert both classes coexist in emitted output.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T015 [P] Add byte-parity integration test `tier_all_is_byte_identical_to_default` in `waybill-cli/tests/tier_filter_flag.rs`. Run two scans of the m230 fixture: one with no `--tier` flag, one with `--tier=all`. Extract `components[].purl` and `dependencies[]` from each; assert set-equality. This is the SC-003 assertion.
- [X] T016 [P] Add graph-completeness re-evaluation test `tier_filter_recomputes_graph_completeness` in the same integration file. Scan m230 fixture twice: once no filter, once `--tier=source-only`. Extract `waybill:graph-completeness-reason` from each. Assert the two values differ OR the source-only value is absent (design-tier orphans were dropped → no orphan-reason emitted). This is the SC-004 assertion.
- [X] T017 [P] Add cross-format consistency test `tier_filter_produces_same_purl_set_across_formats` — scan the m230 fixture with `--tier=source-only` and `--format cyclonedx-json,spdx-2.3-json,spdx-3-json`; extract PURLs from each format; assert set-equality across all three. SC-005 assertion.
- [X] T017b [P] Add integration test `tier_empty_result_emits_warn` in `waybill-cli/tests/tier_filter_flag.rs`. Spawn `waybill sbom scan --tier=design-only --path tests/fixtures/golden_inputs/nuget/csproj_legacy` (fixture has zero design-tier components). Capture stderr. Assert stderr contains the substring `"tier filter dropped all components"`; assert the emitted CDX has `.components | length == 0`; assert the process exit code is 0 (FR-008 explicit assertion at integration tier — closes analyze-report C2 gap).
- [X] T018 Run `./scripts/pre-pr.sh` locally. Expect green. Per memory `feedback_prepr_gate_bails_on_first_failure`, if it fails, enumerate every `^---- .+ stdout ----` line in the failure output before triaging.
- [X] T019 Walk through `specs/232-tier-filter-flag/quickstart.md` end-to-end against a fresh `cargo build --release -p waybill` binary. Confirm every SC-001..SC-005 + FR-008 assertion. Any deviation returns to Phase 3–5.

---

## Dependencies & Execution Order

- **Phase 1**: T001 no code dependencies.
- **Phase 2**: T002 → T003 → T004 → T005 all same file (`scan_cmd.rs`); sequential.
- **Phase 3 (US1)**: Requires Phase 2. Tests T006–T008 authored in parallel; T009 verifies green; T010 adds one more unit.
- **Phase 4 (US2)**: Requires Phase 2 (same enum extension). T011 + T012 parallel with US3.
- **Phase 5 (US3)**: Requires Phase 2. T013 + T014 parallel with US2.
- **Phase 6**: Requires Phase 5 complete. T015 + T016 + T017 parallel; T018 → T019 sequential.

### Parallel Opportunities

- Unit tests T006, T007, T011, T013 — different `#[test] fn` names in same tests block. Parallelizable at authoring time.
- Integration-test additions T008, T012, T014, T015, T016, T017 — all in same file (`tier_filter_flag.rs`) but different `#[test] fn` names. Parallelizable at authoring time.
- Polish tests T015 + T016 + T017 truly parallel across authors.

---

## Implementation Strategy

### MVP: Complete US1 only

1. Setup + Foundational (T001–T005).
2. US1 tests + impl (T006–T010).
3. Ship as MVP. US2/US3 are one-line enum-variant additions plus mirror integration tests — trivially added in a follow-up commit or the same PR.

### Incremental delivery

MVP → Add US2 tests (T011–T012) → Add US3 tests (T013–T014) → Polish tests (T015–T017) → pre-PR + quickstart → PR.

Given all three US phases share the same plumbing, bundling all three in one PR is the natural granularity.

### Not a parallel-team milestone

Small enough (~150 LOC net addition including tests) for one contributor in one session.

---

## Notes

- Every task cites a concrete file path. Test tasks additionally name the `#[test] fn` block so the checklist item maps to a specific test.
- Unit tests colocate in `scan_cmd.rs::tests` block. Add `#[cfg(test)] mod tests { ... }` if the file doesn't already have one; grep first.
- Integration tests colocate in `waybill-cli/tests/tier_filter_flag.rs`. Reuses m230's subprocess scaffold; no new `common::` helpers needed.
- Fixture reuse: everything runs against `waybill-cli/tests/fixtures/golden_inputs/nuget/packages_lock_present` (m230-authored) and `csproj_legacy` for the FR-008 empty-result path.
- No new Cargo dependencies; no `Cargo.toml` edits.
- No changes under `waybill-cli/src/generate/`. The format builders naturally consume the filtered slice — SC-004 is satisfied structurally by ordering the filter before dispatch.
- FR-011 (composition with other flags) is a compile-time property of clap — no runtime enforcement needed. No task for it because the absence of mutual-exclusion attributes IS the implementation.
