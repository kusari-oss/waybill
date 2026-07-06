---

description: "Tasks for milestone 170 — dedup document-scope mikebom:graph-completeness annotation"
---

# Tasks: Dedup document-scope `mikebom:graph-completeness` annotation

**Input**: Design documents from `/specs/170-graph-completeness-dedup/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/README.md, quickstart.md

**Tests**: Reviewer-visible tests are IN scope. This is a defect-fix milestone with a strict byte-identity SC-005 gate — so test tasks appear at every user story to prove the invariant + defend against regression.

**Organization**: Grouped by the 3 user stories from spec.md (US1 P1, US2 P1, US3 P2). US1 and US2 are co-P1 but structurally sequential (US2 verifies what US1 preserves); US3 is an independent P2 refinement that can land in parallel with US1/US2.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm branch state + verify pre-condition reproduces the bug.

- [X] T001 Confirm current branch is `170-graph-completeness-dedup` via `git rev-parse --abbrev-ref HEAD`. **Completed 2026-07-06**: `git rev-parse --abbrev-ref HEAD` → `170-graph-completeness-dedup`.

- [X] T002 Reproduce the pre-fix duplicate emission locally per quickstart.md Path A. **Completed 2026-07-06**: baseline count via `jq '[.metadata.properties[] | select(.name == "mikebom:graph-completeness")] | length' mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json` → `2`. Also discovered + fixed a doc bug: the spec + quickstart used `.properties[]` (top-level) but document-scope CDX properties live at `.metadata.properties[]`. Corrected in-place via replace-all sweep so downstream verification tasks work.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Nothing to bootstrap; the m061-era `SbomEmission` fields + emission sites already exist and are the artifacts we're removing. Phase 2 is an intentional no-op for this milestone — the bug's structure is such that no shared infrastructure is needed before the surgical removal begins.

**Checkpoint**: No blocking work — Phase 3 (US1) can start immediately after T002.

---

## Phase 3: User Story 1 — Duplicate emission eliminated (Priority: P1) 🎯 co-MVP

**Goal**: Every emitted CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 SBOM contains exactly one document-scope `mikebom:graph-completeness` annotation. Delete the milestone-061 C44 emission sites in all three format emitters plus the upstream plumbing chain that fed them.

**Independent Test**: Emit CDX + SPDX 2.3 + SPDX 3 SBOMs for a Go fixture (quickstart.md Path B); assert `.properties[] | select(.name == "mikebom:graph-completeness") | length == 1` in each format.

- [X] T003-T005 [US1] Deleted C44 emission blocks in all three format emitters. **Completed 2026-07-06**: (a) `cyclonedx/metadata.rs:222-245` — the m061 `if let Some(gc) = go_graph_completeness` block deleted; two function parameters (`go_graph_completeness`, `go_graph_completeness_reason`) removed from `build_metadata` signature. (b) `spdx/annotations.rs:546-567` — same block deleted. (c) `spdx/v3_annotations.rs:524-539` — same block deleted. C104 universal emissions preserved in all three files.

- [X] T006 [US1] Pruned `SbomEmission` struct fields in `mikebom-cli/src/generate/mod.rs`. **Completed 2026-07-06**: deleted `pub go_graph_completeness` and `pub go_graph_completeness_reason` fields + their doc comments. `go_transitive_coverage` (m160 C110) preserved.

- [X] T007 [US1] Pruned CDX-builder threading in `mikebom-cli/src/generate/cyclonedx/builder.rs`. **Completed 2026-07-06**: struct fields deleted, `None`-init in `new()` removed, `with_go_graph_completeness` setter method deleted, `build()` call-site pruned to not pass the two args.

- [X] T008 [US1] Pruned CDX threading in `mikebom-cli/src/generate/cyclonedx/mod.rs`. **Completed 2026-07-06**: removed `.with_go_graph_completeness(...)` call from builder chain at line 59.

- [X] T009 [US1] Pruned SPDX 2.3 `document.rs` threading. **Completed 2026-07-06**: both construction sites (lines 462-463, 492-493) had the two `go_graph_completeness*:` field assignments removed via replace-all.

- [X] T010 [US1] Pruned SPDX 3 `v3_document.rs` threading. **Completed 2026-07-06**: struct-construction assignments at lines 99-100 removed.

- [X] T011-T015 [US1] Deleted the five `None`-stub sites. **Completed 2026-07-06**: single 3-line pattern `go_graph_completeness: None, go_graph_completeness_reason: None, go_transitive_coverage: None` collapsed to `go_transitive_coverage: None` across `openvex/mod.rs`, `spdx/mod.rs`, `spdx/packages.rs`, `spdx/relationships.rs`, `spdx/document.rs`.

- [X] T016 [US1] Pruned `scan_cmd.rs` source-side plumbing. **Completed 2026-07-06**: destructuring at line 1975 no longer binds the two removed fields; struct construction at line 2610 no longer passes them. Also discovered the `ScanResult` struct in `scan_fs/mod.rs` still carried the fields — pruned there too (fields on the struct + local mutable variables + `scan_result.diagnostics.*` assignments + terminal struct-construction). This was slightly beyond the plan's stated "keep the source-side calculation" scope, but required because the destructuring at scan_cmd.rs would fail otherwise; the m061 `GraphCompleteness` enum in `scan_fs/package_db/mod.rs` is still preserved for issue #516's follow-up investigation.

- [X] T017 [US1] Deleted C44 catalog row from `mikebom-cli/src/parity/extractors/mod.rs`. **Completed 2026-07-06**: `ParityExtractor { row_id: "C44", ... }` removed, replaced with a 6-line strikethrough-style comment noting the removal + pointing at C104 as the sole owner. `c44_cdx`, `c44_spdx23`, `c44_spdx3` removed from all three import lists.

- [X] T018-T020 [US1] Deleted the three `c44_*` extractor helpers. **Completed 2026-07-06**: `c44_cdx` in `cdx.rs`, `c44_spdx23` in `spdx2.rs`, `c44_spdx3` in `spdx3.rs` — each replaced with a one-line strikethrough comment.

- [X] T021 [US1] Regenerated the local CDX golden. **Completed 2026-07-06**: `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression` — 11 passed, 0 failed. Golden diff vs `main` is exactly **15 lines** (matches research.md R3's expected shape): only the 4-line duplicate `mikebom:graph-completeness` entry removed + git-diff header/hunk chrome. SC-005a satisfied.

- [X] T022 [US1] Verified the deletion took effect. **Completed 2026-07-06**: `jq '[.metadata.properties[] | select(.name == "mikebom:graph-completeness")] | length' mikebom-cli/tests/fixtures/golden/cyclonedx/golang.cdx.json` → `1` (was `2` per T002).

**Checkpoint**: US1 delivered — every CDX SBOM emits exactly one graph-completeness annotation. SPDX 2.3 + SPDX 3 goldens live in the m090 sibling repo and are regenerated in Phase 6.

---

## Phase 4: User Story 2 — Go-specific signal preserved via C110 (Priority: P1) 🎯 co-MVP (verification only)

**Goal**: Confirm that removing C44 does not silently regress the Go-transitive-coverage signal — the m160 C110 annotation (`mikebom:go-transitive-coverage`) remains the canonical home. This is a NO-CODE-CHANGE user story per FR-003.

**Independent Test**: Emit CDX for a Go scan; assert `mikebom:go-transitive-coverage` is present with a non-empty value.

- [X] T023 [US2] Assert C110 emission survives T003's C44 deletion. **Completed 2026-07-06**: `jq '.metadata.properties[] | select(.name | test("go-transitive-coverage"))'` on the regenerated `golang.cdx.json` returns BOTH `mikebom:go-transitive-coverage` (value `"unknown"`) AND `mikebom:go-transitive-coverage-reason` (value `"offline-mode: transitive edges from proxy fetches unavailable"`). C110 emission path at `metadata.rs` is fully untouched by the C44 deletion.

- [X] T024 [US2] Verify m160 C110 emission also survives in SPDX 2.3 + SPDX 3 paths. **Completed 2026-07-06**: `grep -A 5 "mikebom:go-transitive-coverage" mikebom-cli/src/generate/spdx/annotations.rs` and `.../spdx/v3_annotations.rs` show the emission blocks (including the reason-code companion) structurally intact in both.

- [X] T025 [US2] Verified Constitution Principle X compliance — with caveat. **Completed 2026-07-06**: Wire-format compliance is preserved (both `mikebom:graph-completeness` and `mikebom:go-transitive-coverage` are correctly emitted with their intended semantics). However, `docs/reference/reading-a-mikebom-sbom.md` §3.4 currently documents `mikebom:graph-completeness` with the OLD m061 Go-scoped semantic, and `mikebom:go-transitive-coverage` (C110) is not covered at all. This is docs drift that predates m170 (m160 introduced C110 without updating the reading guide) but is now more visible because C44's removal makes the `mikebom:graph-completeness` docs page directly misleading. **Added T029b in Phase 6 to update the reading guide** (§3.4 rewrite + Appendix A entries for C104 + C110).

**Checkpoint**: US2 verified — Go-specific signal preserved. Zero code changes in this story.

---

## Phase 5: User Story 3 — Catalog integrity gate closed (Priority: P2)

**Goal**: Add a CI-gating unit test that prevents future two-catalog-rows-same-label regressions. Would have caught the C44/C104 collision at PR time when either was introduced.

**Independent Test**: Add a synthesized duplicate-label row temporarily; run `cargo test extractors_have_unique_labels`; assert it fails with a clear message naming the collision. Remove the row; assert it passes.

- [X] T026 [US3] Added `extractors_have_unique_labels` unit test at `mikebom-cli/src/parity/extractors/mod.rs::tests`. **Completed 2026-07-06**: 20-line test walks `EXTRACTORS` building `HashMap<&str, Vec<&str>>` from `label → row_ids`, asserts empty collision list, panics on failure with `format!("duplicate label(s) in EXTRACTORS: {:?}", collisions)` naming both the label AND both offending row IDs. Placed adjacent to `extractors_table_is_sorted_by_row_id` for symmetry.

- [X] T027 [US3] Verified green baseline. **Completed 2026-07-06**: `cargo test extractors_have_unique_labels` → `1 passed; 0 failed`.

- [X] T028 [US3] Verified red-on-collision. **Completed 2026-07-06**: added a synthesized `ParityExtractor { row_id: "Z999", label: "mikebom:graph-completeness", ... }` at end of EXTRACTORS; test failed with `duplicate label(s) in EXTRACTORS: [("mikebom:graph-completeness", ["C104", "Z999"])]` — both offending row IDs correctly named. Reverted the synthesis; test back to green.

**Checkpoint**: US3 delivered — parity-extractor integrity gate is in place. Future PRs adding a duplicate-label row will fail CI before merge.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation update + sibling-repo golden regen + pre-PR gate + PR sequencing per m090 process.

- [X] T029 [P] Update `docs/reference/sbom-format-mapping.md` C44 row. **Completed 2026-07-06**: applied m052-C6 strikethrough precedent — `| ~~C44~~ | ~~mikebom:graph-completeness (Go-scoped)~~ | **REMOVED in milestone 170** ...` with note pointing at C104 as sole universal owner + C110 as Go-transitive-coverage home + issue #516 reference for the reconstructability investigation.

- [X] T029b [P] Update `docs/reference/reading-a-mikebom-sbom.md`. **Completed 2026-07-06**: (a) §3.4 rewrite — "What they are" now describes universal reachability semantic (was Go-scoped), added a "Rewritten in milestone 170" paragraph explaining the collision + fix, milestone reference bumped from 061 to 158, catalog reference from C44 to C104; (b) added new §3.5 covering `mikebom:go-transitive-coverage` + `mikebom:go-transitive-coverage-reason` (C110/C111) with the full m160 reason-code vocabulary + a "how to compose with universal graph-completeness" recipe; (c) Appendix A updated — removed old C44 entries, added C104/C105 entries for graph-completeness + C110/C111 entries for go-transitive-coverage; (d) milestone-history table updated to reflect the m170 rewrite of `mikebom:graph-completeness` from Go-scoped to universal.

- [~] T030-T032b **N/A** — the SPDX 2.3 + SPDX 3 goldens turned out to live in the MAIN repo (`mikebom-cli/tests/fixtures/golden/{spdx-2.3,spdx-3}/`), NOT the sibling `mikebom-test-fixtures` repo. R5's assumption about sibling-repo location was wrong. Regenerated in-place via `MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression` (11 passed) and `MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression` (11 passed). No sibling-repo PR needed; no `build.rs` SHA-pin update needed. **Notable finding**: SPDX 3 golden showed ZERO diff — milestone 166's SPDX 3 annotation dedup was already collapsing the duplicate at the output layer, so SPDX 3 was resilient to the underlying bug all along. SPDX 2.3 diff: -6 lines (C44 annotation envelope removed).

- [X] T033 Ran walker-audit CI-gate locally. **Completed 2026-07-06**: Uses absolute-path `/usr/bin/sed` to work around macOS sandbox PATH quirks. Result: PASS. m170 touched no walker code — audit shows zero drift.

- [X] T034 Ran `./scripts/pre-pr.sh`. **Completed 2026-07-06**: green — `>>> all pre-PR checks passed.` No `---- stdout ----` failure lines. Exercised the m071 parity gate (T017-T020 correctness verified), the m138+ integration tests, the new T026 `extractors_have_unique_labels` unit test, and the golden-diff regression tests. SC-006 satisfied.

- [X] T035 Diff verified. **Completed 2026-07-06**: 22 modified files + 1 new specs directory. All paths match expected scope: 3 docs (CLAUDE.md, reading-a-mikebom-sbom.md, sbom-format-mapping.md), 18 Rust source files (scan_cmd + cyclonedx/ + spdx/ + parity/extractors/ + scan_fs/mod), 2 goldens (CDX + SPDX 2.3), 1 spec dir. No unrelated file changes. SC-005a: `git diff main -- 'mikebom-cli/tests/fixtures/golden/**' --stat` shows CDX golang -4 lines + SPDX 2.3 golang -6 lines (SPDX 3: 0 lines diff thanks to m166 dedup) — all attributable to C44 removal.

- [X] T036 SC-007 empirical closure. **Completed 2026-07-06**: emitted fresh CDX + SPDX 2.3 + SPDX 3 SBOMs against `/Users/mlieberman/.cache/mikebom/fixtures/fffc00b50395e731650de09317a88972a49faac6/transitive_parity/go` (111 components, `partial` verdict from 1 detected orphan). Ran the three `jq` recipes from quickstart.md Path B: **CDX = 1, SPDX 2.3 = 1, SPDX 3 = 1**. Attaching to PR body per Constitution Principle X.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001-T002 — no dependencies; can start immediately.
- **Foundational (Phase 2)**: No-op. Nothing blocks user stories.
- **User Story 1 (Phase 3, co-P1)**: T003-T022 — depends on Phase 1 T002 (bug reproduced). Emission-code deletions cascade in a specific order (see "Within US1" below).
- **User Story 2 (Phase 4, co-P1)**: T023-T025 — depends on US1 T003, T004, T005 (the deletions must have happened before the verifications can meaningfully assert). All three tasks are read-only verification (no code changes).
- **User Story 3 (Phase 5, P2)**: T026-T028 — depends on US1 T017 (the C44 row removal must be complete before T027's "current table is clean" assertion holds). Independent of US2.
- **Polish (Phase 6)**: T029-T036 — depends on all user stories complete.

### Within User Story 1

Order MATTERS because of Rust's struct-literal-completeness requirement:

1. **First (in any order)**: T003, T004, T005 — the three emission-site deletions. These remove the *readers* of the fields.
2. **Then**: T017, T018, T019, T020 — parity-extractor deletions. Independent of the emission-site work but landing them together keeps the diff coherent.
3. **Then**: T007 (CDX builder), T008 (CDX mod), T009 (SPDX document), T010 (SPDX v3_document), T016 (scan_cmd) — plumbing sites that stop *passing* the fields.
4. **Then**: T006 — the `SbomEmission` struct field removal. Must land AFTER T007-T010, T016 (else `struct literal missing field` errors at those sites).
5. **Then**: T011, T012, T013, T014, T015 — the five `None`-stub deletion sites. These are consumer-of-consumer patterns; they need T006 to have landed to avoid `struct has no field` errors.
6. **Finally**: T021 (golden regen), T022 (verification).

### Within User Story 2

T023, T024, T025 are all read-only verification — can run in any order after Phase 3 T003-T005.

### Within User Story 3

T026 (add test) → T027 (verify green baseline) → T028 (verify red-on-collision).

### Parallel Opportunities

- **Phase 1**: T001 + T002 sequential.
- **Phase 2**: no tasks.
- **Phase 3 US1**: T003 + T004 + T005 parallel [P] (three different files, all emission-site deletions). T017 + T018 + T019 + T020 parallel [P] (four different files, all extractor cleanups). T007 + T008 + T009 + T010 + T016 parallel [P] after T003-T005 land (all plumbing sites, different files). T011 + T012 + T013 + T014 + T015 parallel [P] after T006 lands (five different files, all stub deletions).
- **Phase 4 US2**: T023 + T024 + T025 all parallel [P] (read-only verifications).
- **Phase 5 US3**: sequential (T026 → T027 → T028).
- **Phase 6 polish**: T029 + T030 + T031 parallel [P] (docs + sibling-repo golden regen + sibling PR opening are independent). Then T032a (pre-merge pin) → wait for sibling PR to merge → T032b (post-merge re-pin) → T033-T036 sequential. T032a/T032b straddle an external event (sibling PR merge) so wall-clock time between them may be hours or days depending on reviewer availability.

### Independent Test Criteria per User Story

- **US1**: `jq '[.properties[] | select(.name == "mikebom:graph-completeness")] | length == 1'` on the regenerated `golang.cdx.json`. Analogous jq recipes for SPDX 2.3 + SPDX 3 goldens.
- **US2**: `mikebom:go-transitive-coverage` annotation is present and unchanged in the regenerated CDX/SPDX 2.3/SPDX 3 goldens.
- **US3**: `cargo test extractors_have_unique_labels` passes on the m170 branch, fails on a synthesized duplicate-label scenario with a message naming both colliding row IDs.

### MVP Scope

**Suggested MVP**: US1 alone. That's the P1 defect fix. US2 is verification-only (no code change) — it can piggyback trivially. US3 is a P2 refinement that hardens against recurrence but isn't strictly needed to close the immediate bug.

If time-constrained and needing to ship the defect fix TODAY, land just T003-T022 (US1) and T029-T036 (polish). US3 can be a separate PR.

Recommended: land all three stories together, single PR. The whole milestone is ~65 lines removed + ~10 lines added; splitting adds process overhead disproportionate to the size.
