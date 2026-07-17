---
description: "Task list for m204 — helm image-extraction-completeness doc-scope annotation"
---

# Tasks: Emit `mikebom:image-extraction-completeness` Document-Scope Annotation

**Input**: Design documents from `/specs/204-helm-completeness-annotation/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, quickstart.md ✓

**Tests**: Tests-included. Spec's 3 P1 stories each name an Independent Test criterion; every story gets one integration test.

**Organization**: 8 phases — setup (baseline recon), foundational (single method + field prerequisites for every story), then 3 P1 story phases, then parity registration, then polish. m204 has NO user-selectable dependency ordering — all P1 stories complete when the foundational plumbing + emit branches + parity row land together.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3 mapping to spec.md user stories
- **File paths**: absolute or repo-relative — every task cites exact target

## Phase 1: Setup (Baseline + Recon)

**Purpose**: Establish the pre-m204 baseline for SC-004 (byte-identity) and SC-006 (pre-PR delta) verification. Re-verify all quickstart.md `Empirical re-verification` grep results actually match the current tree.

- [ ] T001 Verify pre-m204 baseline pre-PR is green by running `./scripts/pre-pr.sh` on branch `204-helm-completeness-annotation` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m204-prepr-baseline.txt` for SC-006 delta measurement later.
- [ ] T002 [P] Golden-drift baseline: `git diff --stat main -- mikebom-cli/tests/fixtures/` (expected: empty — branch is spec+plan only) — record to `/tmp/m204-golden-baseline.txt`. Post-implementation this baseline gets re-compared for regression scope (SC-004 assertion).
- [ ] T003 [P] Recon: verify every line number cited in plan.md / data-model.md is still valid by running quickstart.md's `Empirical re-verification at implement time` block. Record grep outputs to `/tmp/m204-recon.txt` for downstream tasks to consume. Concretely:
  - `grep -n "pub go_workspace_mode\|pub go_transitive_coverage" mikebom-cli/src/scan_fs/mod.rs`
  - `grep -n "pub go_workspace_mode:" mikebom-cli/src/generate/mod.rs`
  - `grep -n "go_workspace_mode: go_workspace_mode" mikebom-cli/src/cli/scan_cmd.rs`
  - `grep -n "go_workspace_mode\|go-workspace-mode" mikebom-cli/src/generate/cyclonedx/metadata.rs | head`
  - `grep -n "go_workspace_mode\|go-workspace-mode" mikebom-cli/src/generate/spdx/annotations.rs | head`
  - `grep -n "go_workspace_mode\|go-workspace-mode" mikebom-cli/src/generate/spdx/v3_annotations.rs | head`
  - `grep -n "go-workspace-mode\|C112" mikebom-cli/src/parity/extractors/mod.rs`
  - `grep -c "build_metadata(" mikebom-cli/src/generate/cyclonedx/metadata.rs`

## Phase 2: Foundational (Prerequisites for ALL user stories)

**Purpose**: Wire the 8-hop plumbing chain end-to-end. NO annotation is emitted until Phase 2 completes AND at least one emit-branch task (T009/T010/T011) lands. Sequential dependency inside Phase 2 (T004 → T005 → T006 → T007), but T004 has no upstream deps within this milestone.

- [ ] T004 Add `pub fn as_wire_str(&self) -> &'static str` inherent method to `HelmExtractionMode` in `mikebom-cli/src/scan_fs/package_db/mod.rs` per data-model E1 (impl block adjacent to the enum at line 427). Return `"partial"` for `Unrendered`, `"full"` for `Rendered`. Add doc-comment citing m204 (#554) + m071 parity catalog row C123. ALSO drop the `#[allow(dead_code)]` on `HelmExtractionMode::Rendered` at line 435 — m204 makes it live.
- [ ] T005 Add `pub helm_extraction_mode: Option<crate::scan_fs::package_db::HelmExtractionMode>` field to `ScanResult` struct in `mikebom-cli/src/scan_fs/mod.rs:76+` per data-model E2 (insert immediately after `go_workspace_mode` at line 131 with matching doc-comment shape). Add a `let mut helm_extraction_mode: Option<...> = None;` binding near line 313 (alongside `go_workspace_mode`). Add `helm_extraction_mode = scan_result.diagnostics.helm_extraction_mode;` mirror line near line 352 (immediately after `go_workspace_mode = ...`). Add `helm_extraction_mode,` to the returned `ScanResult { ... }` struct literal — locate the construction site deterministically via `awk '/^pub fn scan_path/,/^}$/' mikebom-cli/src/scan_fs/mod.rs | grep -n "^\s*ScanResult {"` (expected: exactly 1 match) rather than relying on line numbers that drift across milestones.
- [ ] T006 Add `pub helm_extraction_mode: Option<&'a crate::scan_fs::package_db::HelmExtractionMode>` field to `ScanArtifacts` struct in `mikebom-cli/src/generate/mod.rs:51+` per data-model E3 (insert immediately after `go_workspace_mode` at line 106-108 with matching doc-comment shape).
- [ ] T007 Wire `scan_cmd.rs` + peripheral construction sites. (a) Add `helm_extraction_mode,` to the `let scan_fs::ScanResult { ... } = scan_fs::scan_path(...)` destructure at `mikebom-cli/src/cli/scan_cmd.rs:2507-2517` (immediately after `go_workspace_mode,`). (b) Add `helm_extraction_mode: helm_extraction_mode.as_ref(),` to the `ScanArtifacts { ... }` construction at line 3187+ (immediately after `go_workspace_mode: go_workspace_mode.as_ref(),`). (c) Run this authoritative sweep for every `ScanArtifacts` / `go_workspace_mode: None,` site — the failure mode is a missed site causing a compile error, so make the count explicit: `grep -c "go_workspace_mode: None" mikebom-cli/src/generate/spdx/document.rs mikebom-cli/src/generate/spdx/mod.rs mikebom-cli/src/generate/spdx/packages.rs mikebom-cli/src/generate/spdx/relationships.rs mikebom-cli/src/cli/scan_cmd.rs` — record the per-file counts to `/tmp/m204-workspace-mode-sites.txt`; add `helm_extraction_mode: None,` at EVERY site (the expected match set is: `document.rs` 2 sites, `mod.rs` 1 site, `packages.rs` 1 site, `relationships.rs` 1 site — total 5; adjust if recon differs).
- [ ] T008 Post-T007 sanity: run `cargo +stable check --workspace 2>&1 | tail -20`. Expected: clean compile (no missing-field errors, no unused-field warnings — the emit branches in Phase 3-5 will consume the field but this checkpoint proves the plumbing is complete).

## Phase 3: User Story 1 — Helm operator sees "partial" on default unrendered scan (Priority: P1)

**Story Goal**: Every default helm-chart scan emits `mikebom:image-extraction-completeness = "partial"` at document scope across all three formats.

**Independent Test Criterion**: `mikebom sbom scan --path <chart-dir> --format <fmt>` for each of `cyclonedx-json`, `spdx-json`, `spdx-3-json` produces output containing a doc-scope `mikebom:image-extraction-completeness = "partial"` annotation.

- [ ] T009 [US1] CDX emit branch: add the m204 (#554) block to `mikebom-cli/src/generate/cyclonedx/metadata.rs::build_metadata` immediately after the C112 `go_workspace_mode` branch at line 569 per data-model E4. Also: append `helm_extraction_mode: Option<&crate::scan_fs::package_db::HelmExtractionMode>` as the LAST argument of `pub fn build_metadata`. Update the production callsite in `mikebom-cli/src/generate/cyclonedx/builder.rs` (find via `grep -n "build_metadata(" builder.rs`) to pass `scan_artifacts.helm_extraction_mode`. Update the ~7 test callsites within `metadata.rs` (lines 1198, 1219, 1228, 1240, 1251, 1260, 1296 — verified in plan recon) to append `None` as the final positional arg (each test doesn't need to exercise this branch).
- [ ] T010 [P] [US1] SPDX 2.3 emit branch: add the m204 block to `mikebom-cli/src/generate/spdx/annotations.rs::annotate_document` immediately after the C112 `go_workspace_mode` branch at line 630-639 per data-model E5. Consumes `artifacts.helm_extraction_mode` — no function-signature change (already in ScanArtifacts).
- [ ] T011 [P] [US1] SPDX 3 emit branch: add the analogous m204 block to `mikebom-cli/src/generate/spdx/v3_annotations.rs` immediately after the C112 `go_workspace_mode` branch at line 599-601 per data-model E6.
- [ ] T012 [US1] Integration test `m204_us1_partial_annotation_present_on_unrendered_helm_scan` in `mikebom-cli/tests/helm_reader.rs`: build a chart inline via `tempfile::tempdir()` (Chart.yaml with name/version + one templates/*.yaml file — reuse `write_chart_yaml` + `write_template` helpers). Scan in all three formats (extend `scan_dir` for the SPDX formats or introduce `scan_dir_multi` returning `(cdx, spdx23, spdx3)`). Assert each output contains a doc-scope annotation whose name is `mikebom:image-extraction-completeness` with value `"partial"`. **BEFORE writing the SPDX 3 extractor** (U1 remediation), recon the actual `push()` output shape via `grep -A 8 "mikebom:go-workspace-mode" mikebom-cli/src/generate/spdx/v3_annotations.rs` — the `push()` helper may emit `.statement` as either a native JSON object or a JSON-in-string; the m145/m166 convention is JSON-in-string but verify. Adjust the SPDX 3 extractor accordingly. Extractors:
  - **CDX**: `.metadata.properties[]` with `.name == "mikebom:image-extraction-completeness"` and `.value == "partial"`.
  - **SPDX 2.3**: `.annotations[]` with `.comment` decoding to a `MikebomAnnotationCommentV1` envelope where `k == "mikebom:image-extraction-completeness"` and `v == "partial"`. Use `serde_json::from_str` on the `.comment` field.
  - **SPDX 3**: `."@graph"[]` where `.type == "Annotation"` and `.statement` (verify shape per recon above; likely JSON-in-string) decodes to the envelope with same `k`/`v`.

## Phase 4: User Story 2 — Helm operator sees "full" on rendered scan (Priority: P1)

**Story Goal**: `--helm-render` + successful helm shell-out produces `"full"` across all three formats. Fallback (any m203 error class) produces `"partial"` — matching m203's HelmExtractionMode setting.

**Independent Test Criterion**: `mikebom sbom scan --helm-render --path <chart-dir> --format <fmt>` with real helm binary produces `"full"`. Gated behind `MIKEBOM_HELM_INTEGRATION=1` per m188/m203 precedent.

- [ ] T013 [US2] Integration test `m204_us2_full_annotation_present_on_rendered_helm_scan` in `mikebom-cli/tests/helm_reader.rs`, gated by `MIKEBOM_HELM_INTEGRATION=1` env var (skip cleanly if unset — matches m188/m203 pattern). Same chart shape as T012 + `--helm-render` flag on the scan invocation. Assert the value is `"full"` in all three formats using the same extractors as T012.
- [ ] T014 [P] [US2] Regression guard test `m204_us2_fallback_still_partial_when_helm_render_fails` in `mikebom-cli/tests/helm_reader.rs` (DEFAULT CI, `#[cfg(unix)]`): reuse the m203-added `scan_dir_with_env(scan_root, helm_render, path_env, render_timeout_secs)` helper with `helm_render=true` + `path_env=Some("")` (empty, forces `BinaryNotFound` fallback) + `render_timeout_secs=None`. Assert the emitted CDX contains `metadata.properties[]` with `mikebom:image-extraction-completeness` = `"partial"` (NOT `"full"`) — proves FR-005 (value reflects actual extraction path, not requested mode). SPDX 2.3/3 verification is redundant here since T012's US1 coverage already exercises `"partial"` in all three formats.

## Phase 5: User Story 3 — Non-Helm scan sees no annotation (byte-identity guard) (Priority: P1)

**Story Goal**: `mikebom:image-extraction-completeness` string does not appear in any format output when the scanned directory contains no `Chart.yaml`.

**Independent Test Criterion**: `grep -c "image-extraction-completeness" <emitted-output>` returns 0 for every format when scanning a directory without any helm chart.

- [ ] T015 [US3] Integration test `m204_us3_annotation_absent_on_non_helm_scan` in `mikebom-cli/tests/helm_reader.rs`: scan a `tempfile::tempdir()` containing only `readme.txt` (no `Chart.yaml` at any depth). For each of the three formats, assert the emitted document contains NO property/annotation whose name is `mikebom:image-extraction-completeness`. Concretely:
  - **CDX**: assert `.metadata.properties[]` has no element with `.name == "mikebom:image-extraction-completeness"`.
  - **SPDX 2.3**: assert no `.annotations[]` element's `.comment` decodes to an envelope with `k == "mikebom:image-extraction-completeness"`.
  - **SPDX 3**: assert no `.@graph[]` element with `.type == "Annotation"` has `.statement` decoding to the target `k`.
  - As a cheap upper-bound sanity check: assert the substring `"image-extraction-completeness"` does not appear anywhere in the raw JSON string of any format (helper: `assert!(!raw.contains("image-extraction-completeness"))`).

## Phase 6: Parity Catalog Registration

**Purpose**: Register C123 across the 4 parity infrastructure files. Once registered, existing m071 parity tests (`tests/holistic_parity.rs`, `tests/parity_synthetic_drift.rs`) automatically exercise C123's three-format equivalence.

- [ ] T016 [P] Add `cdx_anno!(c123_cdx, "mikebom:image-extraction-completeness", document);` to `mikebom-cli/src/parity/extractors/cdx.rs` immediately after the C112 `cdx_anno!` at line 814 per data-model E7 (item 1).
- [ ] T017 [P] Add `spdx23_anno!(c123_spdx23, "mikebom:image-extraction-completeness", document);` to `mikebom-cli/src/parity/extractors/spdx2.rs` immediately after the C112 `spdx23_anno!` at line 583 per data-model E7 (item 2).
- [ ] T018 [P] Add `spdx3_anno!(c123_spdx3, "mikebom:image-extraction-completeness", document);` to `mikebom-cli/src/parity/extractors/spdx3.rs` immediately after the C112 `spdx3_anno!` at line 643 per data-model E7 (item 3).
- [ ] T019 Register the `ParityExtractor` entry with `row_id: "C123"` + `label: "mikebom:image-extraction-completeness"` + `directional: Directionality::SymmetricEqual` + `order_sensitive: false` in `mikebom-cli/src/parity/extractors/mod.rs` immediately after the C112 entry at line 472 per data-model E7 (item 4). MUST be sequential with T016/T017/T018 — the module-level `ParityExtractor` references the `c123_*` symbols from those files.

## Phase 7: Golden Regen (Helm Scans Only)

**Purpose**: Every fixture that scans a helm chart will get a new `mikebom:image-extraction-completeness` = `"partial"` entry in its emitted output. Regenerate goldens ONLY for helm-scanning fixtures. FR-004 / SC-004: NON-helm goldens MUST remain byte-identical.

- [ ] T020 Identify helm-scanning goldens by finding tests that use `write_chart_yaml` / `scan_helm_chart_tgz` / any chart-directory fixture: `grep -rl "Chart.yaml\|helm_reader\|helm-chart" mikebom-cli/tests/fixtures/ | head` and also `grep -rl "MIKEBOM_UPDATE_" mikebom-cli/tests/ | xargs grep -l "helm"`. Record the fixture set to `/tmp/m204-helm-goldens.txt`.
- [ ] T021 Regenerate JUST those fixtures via their respective `MIKEBOM_UPDATE_*` env-var patterns (per `docs/dev/regen-goldens.md`). Verify per-file diff shows ONLY the new `mikebom:image-extraction-completeness` addition — no other drift.
- [ ] T022 Sanity check: `git diff --stat mikebom-cli/tests/fixtures/` vs T002 baseline. Assert only files in T020's list appear. If any non-helm fixture drifts, STOP and diagnose (indicates a plumbing bug: some non-Helm scan is triggering the annotation, violating FR-004).

## Phase 8: Polish & Delivery

**Purpose**: Verification, quickstart re-verify, PR body draft.

- [ ] T023 Re-run T002 audit post-implementation: `git diff --stat mikebom-cli/tests/fixtures/`. Compare to /tmp/m204-golden-baseline.txt. Assert delta is limited to files from T020.
- [ ] T024 [P] Run the full test suite for helm coverage: `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test helm_reader --no-fail-fast 2>&1 | tail -3` (expected `ok. N passed; 0 failed`; N includes all m188/m203 tests + the 3-4 m204 tests: T012, T014, T015 always + T013 gated). Also verify parity: `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test parity_synthetic_drift --no-fail-fast 2>&1 | tail -3` and `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test holistic_parity --no-fail-fast 2>&1 | tail -3` — both green means C123's three-format extraction produces byte-identical values (SymmetricEqual satisfied).
- [ ] T025 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 5 seconds per SC-006. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per feedback_prepr_gate_bails_on_first_failure memory).
- [ ] T026 [P] (Optional, requires local `helm` binary) Manually execute quickstart.md Reproducer 2 against `/tmp/m204-chart/`. Confirm CDX `.metadata.properties[]` contains `mikebom:image-extraction-completeness = "full"`. SC-002 verified.
- [ ] T027 [P] Manually execute quickstart.md Reproducer 3 (US3 byte-identity) against `/tmp/m204-nonhelm/`. Confirm `grep -c "image-extraction-completeness" <output>` returns 0 for every format. SC-003 verified independently of the automated test.
- [ ] T028 Draft PR body with `Closes #554` per SC-007. Include: (a) 1-paragraph summary of the m161 8-hop pattern reuse, (b) recon empirical verification outcome (T003 result + T023 fixture-drift assertion — expected drift limited to helm goldens), (b') A1 recon assertion: `grep -c "diagnostics.helm_extraction_mode = " mikebom-cli/src/scan_fs/package_db/helm.rs` returns exactly 1 (confirms the m188/m203 single-assignment "last-wins" semantic that spec Edge Cases relies on), (c) code-diff LOC + files touched (~200 LOC across ~7 source files + 4 parity infra files + 1 test file + 1 doc file per T028a), (d) test coverage summary (US1 default-CI + US2 gated + US3 default-CI + US2b fallback-still-partial regression guard + C123 parity via m071 test suite), (e) golden-regen scope (helm fixtures only per FR-004 / SC-004).
- [ ] T028a [P] (CG1 remediation) Add C123 row to `docs/reference/sbom-format-mapping.md` following the C108 shape (label, per-emitter mapping for CDX 1.6 + SPDX 2.3 + SPDX 3, `**KEEP-NO-NATIVE**` verdict, rejected-alternatives list per Constitution Principle V). Reference plan.md Constitution Check §V for the audit reasoning:
  - **CDX 1.6**: no native construct for "coverage confidence at document scope for image-ref extraction fidelity". `metadata.properties[]` is the m071-approved carrier for `mikebom:*` document-scope signals. Same slot as C110, C111, C112, C118, C119, C122.
  - **SPDX 2.3**: no native construct. Doc-scope `Annotation` with `MikebomAnnotationCommentV1` envelope.
  - **SPDX 3.0.1**: no native construct. `Annotation` element in JSON-LD graph.
  - Rejected alternatives per Principle V audit convention: (1) CDX `metadata.lifecycles[]` — lifecycle-phase enum, semantic mismatch (this is a fidelity signal, not a phase); (2) SPDX 2.3 `SpdxDocument.comment` — free-text, opaque to parsers; (3) SPDX 3 `SoftwareArtifact.software_primaryPurpose` — component-scope, semantic mismatch. Standards-native precedence explicitly acknowledged: if either standard adopts a doc-scope "extraction-fidelity" enum, migration TBD.

---

## Dependencies

Sequential within phases; phases mostly sequential across the milestone:

```
Phase 1 (Setup) ── T001, T002, T003 in parallel
     ↓
Phase 2 (Foundational) ── T004 → T005 → T006 → T007 → T008 (all sequential)
     ↓
Phase 3 (US1) ── T009 → T010, T011 in parallel → T012
     ↓ (US1 completes independently once emitters land)
Phase 4 (US2) ── T013, T014 in parallel (independent of US3)
     ↓
Phase 5 (US3) ── T015 (single test, sequential wrt T012's helper additions)
     ↓
Phase 6 (Parity) ── T016, T017, T018 in parallel → T019
     ↓
Phase 7 (Regen) ── T020 → T021 → T022 (all sequential)
     ↓
Phase 8 (Polish) ── T023 → T024, T026, T027 in parallel → T025 → T028
```

**MVP** = Phase 1 + Phase 2 + Phase 3 (US1 only). Delivers: default helm scans in all three formats surface the `"partial"` annotation. US2 + US3 are incremental increments on top.

## Parallel opportunities

- **Setup** (T002, T003): both are read-only recon, no ordering.
- **US1 emit branches** (T010, T011): different files, no shared state — SPDX 2.3 and SPDX 3 emit branches can land in either order.
- **US2 tests** (T013, T014): different `#[test]` fns, one gated one default.
- **Parity emit rows** (T016, T017, T018): three different files — can land in parallel.
- **Polish audits** (T024, T026, T027): read-only assertions.

## Implementation strategy

Ship as a single PR — the plumbing chain (T004-T007) and emit branches (T009-T011) are cheap and don't compose usefully in isolation. Parity row (T016-T019) is the last-piece that lights up automated three-format equivalence testing.

**Total task count**: 28 tasks.
**By story**: US1 = 4 tasks (T009-T012), US2 = 2 tasks (T013-T014), US3 = 1 task (T015). Phase 1 = 3, Phase 2 = 5, Phase 6 = 4, Phase 7 = 3, Phase 8 = 6.
