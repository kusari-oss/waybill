---
description: "Task list for milestone 166 — SPDX 3 duplicate-Annotation-spdxId dedup fix"
---

# Tasks: SPDX 3 duplicate-Annotation-spdxId dedup fix

**Input**: Design documents from `/specs/166-spdx3-annotation-dedup/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: INCLUDED. SC-008 requires ≥5 unit tests; SC-009 requires a new integration test. All test surfaces are load-bearing SC evidence.

**Organization**: Tasks grouped by 3 user stories from spec.md (US1 P1 SPDX 3 conformance, US2 P2 uniqueness invariant, US3 P3 CDX+SPDX 2.3 byte-identity). US1 is the load-bearing MVP; US2 verifies the general invariant; US3 is the regression guard.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no dependencies on incomplete tasks — parallelizable.
- **[Story]**: US1 / US2 / US3 for user-story tasks. Setup, Foundational, Polish have NO story label.

## Path Conventions

Single-project workspace layout per plan.md §Project Structure. All paths absolute-relative to repo root `/Users/mlieberman/Projects/mikebom/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify prerequisites + baseline current state before making changes.

- [X] T001 Verify the current SPDX 3 emission code matches research.md R1 pinpoint: `grep -n 'annotations.sort_by\|graph.push(anno)' mikebom-cli/src/generate/spdx/v3_document.rs` — MUST show the merge-point sort at lines ~814-820 followed by the push loop. If not present (upstream drift), abort and re-baseline research.md R1.
- [X] T002 Verify existing SPDX 3 conformance test baseline: `cargo +stable test --test spdx3_conformance --no-fail-fast` — MUST pass clean before starting. Records the pre-166 test-count baseline. If this test already FAILS pre-166, that's a critical anomaly — the m165 audit found duplicates on K8s+ArgoCD but not on fixtures; if fixtures now fail, investigate first.

**Checkpoint**: Codebase state confirmed. Ready for foundational implementation.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the dedup helper + call-site update + FR-007 tracing log. All user stories depend on this phase.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Add `dedup_annotations_by_spdx_id` helper at end of `mikebom-cli/src/generate/spdx/v3_annotations.rs` per data-model.md E1. Signature: `pub(crate) fn dedup_annotations_by_spdx_id(annotations: Vec<serde_json::Value>) -> (Vec<serde_json::Value>, usize)`. Body: `BTreeMap<String, Value>` keyed by `anno["spdxId"].as_str().unwrap_or("").to_string()`; LAST-writer-wins via `map.insert(...).is_some() → dropped += 1`. Return `(map.into_values().collect(), dropped)`.
- [X] T004 Update the merge point at `mikebom-cli/src/generate/spdx/v3_document.rs:754-820` per data-model.md E2. Replace the existing `annotations.sort_by(...)` step with a call to the new helper: `let (annotations, spdx3_annotation_duplicates_dropped) = super::v3_annotations::dedup_annotations_by_spdx_id(annotations);`. Keep the subsequent `for anno in annotations { graph.push(anno); }` loop unchanged. Remove the redundant explicit `sort_by` block — BTreeMap iteration is naturally lex-sorted (research §R3).
- [X] T005 Add or extend the FR-007 tracing log per data-model.md E3. Run `grep -n 'tracing::info!\|tracing::warn!' mikebom-cli/src/generate/spdx/v3_document.rs` to check for existing logs. If one exists at end of `build_v3_document`, extend with new field `spdx3_annotation_duplicates_dropped = spdx3_annotation_duplicates_dropped`. If not, add a new one: `tracing::info!(doc_iri = %doc_iri, graph_element_count = graph.len(), spdx3_annotation_duplicates_dropped = spdx3_annotation_duplicates_dropped, "spdx3 document emitted");`. Fire unconditionally per research §R4 (grep-friendly).

**Checkpoint**: `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. `cargo +stable check -p mikebom` succeeds. Existing tests either pass or fail per their pre-166 expectations of duplicates (Phase 4 will regenerate SPDX 3 goldens if needed).

---

## Phase 3: User Story 1 — SPDX 3 conformance validator passes (Priority: P1) 🎯 MVP

**Goal**: Verify the dedup produces conformance-clean SPDX 3 output on both synthesized fixtures and (opt-in) real upstream targets.

**Independent Test**: Every mikebom-emitted SPDX 3 document (fixtures + synthesized integration test) validates clean via `spdx3-validate==0.0.5`. K8s + ArgoCD re-scanned post-166 return exit code 0.

### Tests for User Story 1

- [X] T006 [P] [US1] Unit test `t006_dedup_two_identical_spdx_ids_dropped_to_one` in `mikebom-cli/src/generate/spdx/v3_annotations.rs` mod tests. Construct a `Vec<Value>` with two entries sharing the same `spdxId` and identical content. Call `dedup_annotations_by_spdx_id`. Assert output len == 1, dropped == 1, and the retained entry matches the LAST input entry per FR-004.
- [X] T007 [P] [US1] Unit test `t007_dedup_different_spdx_ids_all_preserved` — construct 3 entries with distinct `spdxId`s. Assert output len == 3, dropped == 0, output sorted by `spdxId` lex order (BTreeMap natural ordering per research §R3).
- [X] T008 [P] [US1] Unit test `t008_dedup_last_writer_wins_on_different_content` — construct 2 entries with same `spdxId` but DIFFERENT `statement` values (defensive test for the hypothetical edge case per spec.md Edge Cases). Assert output len == 1, dropped == 1, and the retained `statement` matches the LAST input's statement per FR-004.
- [X] T009 [P] [US1] Unit test `t009_dedup_empty_input_no_op` — call `dedup_annotations_by_spdx_id(vec![])`. Assert output len == 0, dropped == 0.
- [X] T010 [P] [US1] Unit test `t010_dedup_mixed_unique_and_duplicate` — construct 5 entries: 3 unique + 2 duplicates of one unique's spdxId. Assert output len == 3, dropped == 2. Guards against off-by-one errors in the counter.
- [X] T011 [P] [US1] Unit test `t011_dedup_malformed_missing_spdx_id` — construct 2 entries where one is missing the `spdxId` field. The `.unwrap_or("")` fallback should map both to empty-string key. Assert output len == 1, dropped == 1. Documents defensive behavior for malformed input (matches pre-166 sort-key coercion at v3_document.rs:815).
- [X] T012 [US1] Integration test at `mikebom-cli/tests/spdx3_annotation_dedup.rs` per SC-009 — synthesize a scan that produces duplicate Annotation spdxIds. Approach: build a `ResolvedComponent` set + call `build_v3_document` directly with a mocked graph-completeness result that triggers the same-subject-same-field duplicate emission path. Assert: (a) `[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length == 0`; (b) FR-007 tracing log fires with `spdx3_annotation_duplicates_dropped > 0`; (c) content of retained Annotation matches the LAST-writer per FR-004. If direct synthesis via `build_v3_document` is fragile, fall back to invoking the release binary via `env!("CARGO_BIN_EXE_mikebom")` against a synthesized tempdir fixture.

**Checkpoint**: US1 is fully functional. Running `cargo +stable test --bin mikebom generate::spdx::v3_annotations::tests::t0` shows all 6 unit tests pass. `cargo +stable test --test spdx3_annotation_dedup` shows T012 passes. All 6 unit tests + 1 integration test pass.

---

## Phase 4: User Story 2 — Emitted `@graph[]` has no duplicate `spdxId` values (Priority: P2)

**Goal**: Verify the universal uniqueness invariant across all existing SPDX 3 goldens + regenerate goldens that previously contained duplicates.

**Independent Test**: For every milestone-090 fixture's SPDX 3 golden post-166, run `jq '[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length'`. Result MUST be 0 for every fixture.

### Tasks for User Story 2

- [X] T013 [US2] Run `cargo +stable test --test spdx3_conformance --no-fail-fast` — the milestone-078 conformance gate. MUST pass on every existing milestone-090 fixture. If any golden regressed (pre-166 passed but post-166 fails), investigate whether the regen is needed OR the dedup broke something (unexpected — dedup drops don't affect conformance for well-formed inputs).
- [X] T014 [US2] Regenerate SPDX 3 goldens if any fixture legitimately drifted: `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test`. Inspect the diff for every regenerated golden. Verify diff is limited to: (a) REMOVED duplicate Annotation entries from `@graph[]`; (b) NEW `spdx3_annotation_duplicates_dropped` log field (test log capture only, not part of the golden). NO new content; NO changes to retained annotations; NO reordering of remaining entries (BTreeMap preserves lex order). If any fixture golden shows non-dedup-related changes, that's a bug — investigate. If NO fixture goldens changed post-166, that's expected (empirically the audit found duplicates on K8s+ArgoCD but not on synthesized fixtures per research §R5) — T014 is a no-op.

**Checkpoint**: US2 is fully functional. All milestone-090 fixture SPDX 3 goldens satisfy the uniqueness invariant. Any legitimate drift is confined to duplicate-removal only.

---

## Phase 5: User Story 3 — CDX + SPDX 2.3 output unchanged (Priority: P3)

**Goal**: Regression guard. Verify CDX + SPDX 2.3 goldens are byte-identical to pre-166.

**Independent Test**: `git diff HEAD~ -- mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3}/**` produces zero output.

### Tasks for User Story 3

- [X] T015 [US3] Run `cargo +stable test --workspace --no-fail-fast` — every existing CDX + SPDX 2.3 golden test MUST pass unchanged. Zero drift on 11 ecosystems × 2 formats = 22 goldens. Any diff on those goldens indicates an emission-leak bug — the dedup was supposed to be SPDX-3-only. If any CDX or SPDX 2.3 golden test fails, that's a critical regression.

**Checkpoint**: US3 is fully functional. SC-005 dual-side byte-identity verified.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: CHANGELOG, empirical audit closure, pre-PR gate, commit.

- [X] T016 [P] Add `CHANGELOG.md` entry per SC-010 documenting: (a) motivation (milestone-165 audit surfaced the duplicate-Annotation-spdxId bug on K8s + ArgoCD; small footprint 0.04% but breaks whole-document SPDX 3 validation); (b) fix summary (dedup by `spdxId` at merge point using `BTreeMap`, LAST-writer-wins); (c) empirical impact — pre/post `spdx3-validate` results on Kubernetes + ArgoCD SBOMs; (d) new FR-007 tracing field `spdx3_annotation_duplicates_dropped` for observability; (e) consumer jq recipe for verifying dedup: `jq '[.["@graph"][].spdxId] | group_by(.) | map(select(length > 1)) | length'` MUST return 0; (f) note that no new annotations, CLI flags, or parity-catalog rows were added — reuses existing SPDX 3 emission infrastructure per FR-010 + Constitution Principle V.
- [X] T017 Optional SC-001 + SC-002 empirical audit — re-scan Kubernetes + ArgoCD post-166 following milestone-165's quickstart methodology. Assert `spdx3-validate` returns exit code 0 (PASS) on both regenerated SBOMs. NOT blocking for the PR — matches milestone-160 T033 + milestone-161 T040 + milestone-162 T034 + milestone-163 T037 + milestone-164 T020 + milestone-165 T037 fixture-gated audit pattern. If a K8s + ArgoCD clone is available, run: `./target/release/mikebom --offline sbom scan --path <clone> --format spdx-3-json --output /tmp/post166.spdx3.json --no-deep-hash && .venv/spdx3-validate/bin/spdx3-validate --json /tmp/post166.spdx3.json --quiet` and expect exit code 0.
- [X] T018 Run `./scripts/pre-pr.sh` — both `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST pass clean (SC-007). Test count MUST match pre-166 baseline plus the new Phase-3 unit tests + Phase-3 integration test (+6 unit + 1 integration = +7 total). Any additional failure blocks PR opening.
- [X] T019 Include `implements milestone 166 — audit-surfaced fix from milestone 165` in the impl PR body per SC-011 (milestone 166 has no upstream GitHub issue — it's the top-1 follow-on from milestone 165's audit). PR body should also document: (a) empirical closure — pre-166 `spdx3-validate` FAILS on K8s + ArgoCD with `More than 1 values on ns1:statement`; post-166 PASSES; (b) SC-008 unit test coverage (6 unit tests + 1 integration test); (c) FR-010 zero-new-dependencies posture; (d) delivers milestone-165's #1 top-3 follow-on recommendation.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No prerequisites. Baseline verification.
- **Phase 2 (Foundational)**: Depends on Phase 1. Dedup helper + call-site update + tracing log. Blocks US1/US2/US3.
- **Phase 3 (US1)**: Depends on Phase 2. 6 unit tests + 1 integration test.
- **Phase 4 (US2)**: Depends on Phase 3 (needs the dedup wired). Regenerate goldens if needed.
- **Phase 5 (US3)**: Depends on Phase 4 completion. Byte-identity verification for CDX + SPDX 2.3.
- **Phase 6 (Polish)**: Depends on Phase 5.

### Within Each User Story

- **US1**: T006-T011 (6 unit tests, parallel) → T012 (integration test).
- **US2**: T013 (conformance test-run) → T014 (regen if needed).
- **US3**: T015 (byte-identity via workspace test).

### Parallel Opportunities

Genuine `[P]` (different files, no dependencies):

- **Phase 3 T006-T011** — 6 unit tests all in the same file (`v3_annotations.rs`) BUT non-conflicting append-only fn additions; can be authored in parallel.
- **Phase 6 T016** — CHANGELOG update in a different file from everything else.

**Foundational tasks T003-T005 are strictly sequential** — each touches `v3_annotations.rs` or `v3_document.rs` and depends on the prior task's compilation state.

---

## Parallel Example: Phase 3 unit tests

```bash
# T006-T011 all append tests to the same #[cfg(test)] mod tests block in v3_annotations.rs
# but are non-conflicting fn definitions — safe to author in parallel.
Task: "Add unit test t006_dedup_two_identical_spdx_ids_dropped_to_one in v3_annotations.rs"
Task: "Add unit test t007_dedup_different_spdx_ids_all_preserved in v3_annotations.rs"
Task: "Add unit test t008_dedup_last_writer_wins_on_different_content in v3_annotations.rs"
Task: "Add unit test t009_dedup_empty_input_no_op in v3_annotations.rs"
Task: "Add unit test t010_dedup_mixed_unique_and_duplicate in v3_annotations.rs"
Task: "Add unit test t011_dedup_malformed_missing_spdx_id in v3_annotations.rs"
```

---

## Implementation Strategy

### MVP scope

**US1 alone is the shippable MVP** — delivers the observable-bug fix + SC-008 unit test coverage. US2 verifies the general invariant against existing fixtures; US3 is a regression guard.

Ship order:

1. Phase 1 (Setup) — 5 min. Baseline verification.
2. Phase 2 (Foundational) — 30 min. Dedup helper + call-site + info log. Small diff (~30 lines).
3. Phase 3 (US1) — 1-2 hrs. 6 unit tests + 1 integration test.
4. **STOP + VALIDATE**: Run T012 integration test. Iterate if failures.
5. Phase 4 (US2) — 15-30 min. Conformance test + regen if needed.
6. Phase 5 (US3) — 15 min. Full workspace test.
7. Phase 6 (Polish) — 30 min. CHANGELOG + optional audit + pre-PR gate + commit prep.

### Total effort

~19 tasks. Estimated **3-4 focused hours** end-to-end. Smaller than milestone 164 (23 tasks) and much smaller than milestone 163 (40 tasks) — this is a targeted single-symptom fix with minimal test surface.

### Empirical revision escape hatch

Per spec.md SC-009 note, if the direct-synthesis integration test (T012) proves fragile, fall back to release-binary invocation against a synthesized tempdir fixture (matches milestone-163 T028 pattern).

### Parallel team strategy

Single-contributor scope. If needed:

- Contributor A: Phase 1 → Phase 2 → Phase 3 US1 core (T012) — the load-bearing path.
- Contributor B: Phase 3 unit tests (T006-T011, in parallel with A after T003 lands) + Phase 6 CHANGELOG.

---

## Notes

- All test tasks are load-bearing SC evidence (SC-008 requires ≥5 unit tests; SC-009 requires the integration test). Skipping tests fails milestone acceptance.
- No new Cargo dependencies. No new annotations. No new parity-catalog rows. No new CLI flags. **Only new observable output**: FR-007 tracing log field `spdx3_annotation_duplicates_dropped=<N>`.
- Constitution Principle IV (`no .unwrap()` in production): `BTreeMap::insert` returns `Option<Value>` — no unwrap needed. The `.as_str().unwrap_or("")` fallback matches the existing pre-166 pattern at `v3_document.rs:815`.
- Constitution Principle V (standards-native precedence): reuses existing SPDX 3 emission infrastructure end-to-end. FR-010 documents this.
- Constitution Principle X (Transparency): FR-007 tracing log surfaces dropped duplicates for CI-log analysis. Non-zero counts signal redundant emitter code paths — a candidate for follow-on milestone investigation per research §R7.
- Empirical baseline (2026-07-05 audit) is pinned to live `github.com/kubernetes/kubernetes` @ `688614f2` + `github.com/argoproj/argo-cd` @ `f02203d0`. Numbers may drift with upstream commits; re-measure at implementation time if the pre-166 baseline shifted materially.
- Follows the milestone-165 audit's #1 top-3 follow-on recommendation. Delivers on Constitution Principle IX (Accuracy — emitted SPDX 3 documents must satisfy their own schema).
