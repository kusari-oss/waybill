---

description: "Task list for m203 — Helm `--helm-render` Subprocess Implementation"
---

# Tasks: Helm `--helm-render` Subprocess Implementation

**Input**: Design documents from `/specs/203-helm-render-subprocess/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md (no `contracts/` — inherits from m188 `specs/188-helm-chart-scanning/contracts/extraction-pipeline.md §Phase C`)

**Tests**: Included — every FR requires an executable regression assertion (m194-m202 precedent).

**Organization**: Two P1 stories — US1 (successful rendered extraction via real `helm` subprocess, gated behind `MIKEBOM_HELM_INTEGRATION=1`) and US2 (graceful fallback across 4 failure classes, runs in default CI via stub shell scripts). Foundational phase adds the new types + subprocess helper that BOTH stories consume.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file / no dependency on incomplete task
- **[Story]**: US1 (rendered success) OR US2 (fallback classes)

## Path Conventions

- Rust workspace: `mikebom-cli/src/`, `mikebom-cli/tests/`, `mikebom-cli/tests/fixtures/`
- Absolute paths in every task per plan.md structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline pins + reconnaissance re-verification. No code changes.

- [X] T001 Verify pre-m203 baseline pre-PR is green by running `./scripts/pre-pr.sh` on branch `203-helm-render-subprocess` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m203-prepr-baseline.txt` for SC-006 delta measurement later.
- [X] T002 [P] Golden-drift baseline: `git diff --stat main -- mikebom-cli/tests/fixtures/` (expected: only branch-local fixture changes if any) — record to `/tmp/m203-golden-baseline.txt`. Post-implementation this baseline gets re-compared for regression scope.
- [X] T003 [P] Fixture-shape + helper-availability recon. Record ALL outputs to scratch files for downstream tasks to consume:
  (a) `ls mikebom-cli/tests/fixtures/helm/ > /tmp/m203-existing-fixtures.txt 2>&1` — enumerate existing m188 helm fixtures to inform T008/T011 layout decisions.
  (b) `grep -rn 'fn cap_stderr_lines\|fn cap_lines\|fn.*_lines_cap' mikebom-cli/src/ > /tmp/m203-cap-stderr-helper.txt 2>&1` — check whether the 20-line stderr cap helper already exists. If the file is non-empty (helper found): record the exact `<file>:<line>` for T006 to import from. If empty (helper NOT found): T006 will add a small inline `fn cap_stderr_lines(bytes: &[u8], max_lines: usize) -> String` inside helm.rs.
  (c) `grep -n 'pub fn read\|extract_image_refs_unrendered' mikebom-cli/src/scan_fs/package_db/helm.rs > /tmp/m203-branch-site.txt 2>&1` — record the exact line for T009's branch-site edit. Post-m188 the anchor is around helm.rs:300; verify via this recon that no intervening refactor has drifted it.
  (d) `grep -n 'serial_test' mikebom-cli/Cargo.toml > /tmp/m203-serial-test-availability.txt 2>&1` — pin whether the `serial_test` crate is available so T007 knows whether to use `#[serial_test]` attributes or fall back to `--test-threads=1` documentation.

---

## Phase 2: Foundational — New Types + Subprocess Helper (Blocking Prerequisites)

**Purpose**: Add `HelmRenderError` enum + `resolve_render_timeout` helper + `extract_image_refs_rendered` function per data-model E1-E3. Both US1 and US2 consume these; the branch wiring at helm.rs:300 (T009) depends on all three landing first.

**⚠️ CRITICAL**: T004 → T005 → T006 → T007 in order (each depends on the previous). Post-T007, verify the existing SPDX 2.3 + m188 helm tests still pass byte-identically before starting US1 work.

- [X] T004 Add `HelmRenderError` enum to `mikebom-cli/src/scan_fs/package_db/helm.rs` per data-model E1: `pub(super) enum HelmRenderError` with 4 variants (`BinaryNotFound`, `NonZeroExit { code, stderr_head }`, `Timeout { timeout_secs }`, `IoError(#[from] std::io::Error)`) and `#[derive(Debug, thiserror::Error)]` with the display strings from data-model E1's table. Place adjacent to the existing `HelmRenderMode` enum (near line 57).
- [X] T005 Add `resolve_render_timeout() -> Duration` helper function to `helm.rs` per data-model E2: reads `MIKEBOM_HELM_RENDER_TIMEOUT_SECS` env var, parses as `u64`, clamps to `[1, 3600]`, defaults to 60 on absent/parse-fail. Standalone module-private function; 4-8 LOC. Include doc comment explaining the silent-clamp semantics per research R4.
- [X] T006 Add `extract_image_refs_rendered(chart_dir: &Path, timeout: Duration) -> Result<Vec<ImageRef>, HelmRenderError>` to `helm.rs` per data-model E3 + research R1's verbatim m055 pattern. Structure:
  1. Probe `helm` availability via `Command::new("helm").arg("version").arg("--short").output()`. `ErrorKind::NotFound` → `Err(BinaryNotFound)`; other `io::Error` → `Err(IoError(e))`.
  2. Spawn worker thread that runs `Command::new("helm").args(["template", chart_dir_str]).output()` and sends via `mpsc::channel`.
  3. Main-thread `rx.recv_timeout(timeout)`: `Ok(Ok(o))` → check status; `Ok(Err(e))` → `Err(IoError(e))`; `Err(_)` → `Err(Timeout { timeout_secs })`.
  4. On success (`output.status.success()`): apply existing `IMAGE_REGEX` to `output.stdout` bytes → return `Ok(refs)` deduplicated + sorted per m188 convention.
  5. On non-zero exit: `Err(NonZeroExit { code: output.status.code().unwrap_or(-1), stderr_head: cap_stderr_lines(&output.stderr, 20) })`. Use the helper T003 confirmed OR add a small inline `fn cap_stderr_lines(bytes: &[u8], max_lines: usize) -> String` that String::from_utf8_lossy + `.lines().take(max_lines).collect::<Vec<_>>().join("\n")`.
- [X] T007 [P] Add unit tests for T004-T006 in `mikebom-cli/src/scan_fs/package_db/helm.rs::tests`:
  - `resolve_render_timeout_default_when_env_var_absent_m203` — unset env var → `Duration::from_secs(60)`.
  - `resolve_render_timeout_honors_env_var_m203` — set `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=42` → `Duration::from_secs(42)`.
  - `resolve_render_timeout_clamps_below_min_m203` — set `=0` → `Duration::from_secs(1)`.
  - `resolve_render_timeout_clamps_above_max_m203` — set `=99999` → `Duration::from_secs(3600)`.
  - `resolve_render_timeout_ignores_parse_error_m203` — set `=notanumber` → `Duration::from_secs(60)`.
  - `helm_render_error_display_formats_all_variants_m203` — construct all 4 `HelmRenderError` variants + format each, assert human-readable string content matches data-model E1's Display strings.
  Use `std::env::set_var`/`remove_var` inside `#[serial_test]` if the crate is available; otherwise document the tests may need `--test-threads=1` if run in parallel.

**Checkpoint**: New types + helpers exist. Run `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test helm_reader --no-fail-fast 2>&1 | tail -3` to confirm existing m188 helm tests still pass byte-identically.

---

## Phase 3: User Story 1 — Successful Rendered Extraction (Priority: P1) 🎯 MVP

**Goal**: Wire `HelmRenderMode::OptIn` at `helm.rs:300` to actually invoke `extract_image_refs_rendered` per data-model E4. On success, set `ScanDiagnostics.helm_extraction_mode = Some(HelmExtractionMode::Rendered)`. Closes m188 US3 stub.

**Independent Test**: Fixture chart with `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` + values.yaml supplying `nginx:1.27.0`. Scan with `--helm-render` (real `helm` binary required, gated behind `MIKEBOM_HELM_INTEGRATION=1`). Assert emitted SBOM component list contains `nginx:1.27.0` concrete PURL AND no `{{` characters appear in any component's PURL field.

### Tests for User Story 1

- [X] T008 [P] [US1] Create fixture at `mikebom-cli/tests/fixtures/helm/render_success_m203/` with:
  - `Chart.yaml` — minimal valid chart declaring `apiVersion: v2, name: test-chart, version: 0.1.0`.
  - `values.yaml` — `image: { repository: nginx, tag: "1.27.0" }`.
  - `templates/deployment.yaml` — Deployment resource with `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` (proper Helm template syntax with quoted string in list contexts if needed).
  Verify locally with `helm template mikebom-cli/tests/fixtures/helm/render_success_m203 | grep 'image:'` — expected output line: `image: nginx:1.27.0` (concrete, no placeholders).
- [X] T009 [US1] Modify `helm::read` at `mikebom-cli/src/scan_fs/package_db/helm.rs:300+` per data-model E4: replace the pre-existing `let _ = render_mode; let image_refs = extract_image_refs_unrendered(rootfs);` with the match on `render_mode` that either calls `extract_image_refs_rendered(rootfs, resolve_render_timeout())` (OptIn branch) or `extract_image_refs_unrendered(rootfs)` (Off branch); on any `HelmRenderError` from OptIn, WARN-log + fall back to unrendered. Update the subsequent `diagnostics.helm_extraction_mode = Some(HelmExtractionMode::Unrendered);` line to `= Some(extraction_mode);` (using the local binding from the match). Import `tracing::warn` if not already imported at the file top.
- [X] T010 [US1] Add integration test `us1_helm_render_success_rendered_extraction_m203` to `mikebom-cli/tests/helm_reader.rs`, gated by `MIKEBOM_HELM_INTEGRATION=1` env var (skip cleanly if unset — matches m188 pattern). Test: shell out to mikebom binary via `env!("CARGO_BIN_EXE_mikebom")` with `--offline sbom scan --helm-render --path <T008-fixture> --format cyclonedx-json --output <tempfile>`. Parse output. Assert (a) at least one component's PURL contains `nginx:1.27.0` (concrete image ref, exact match against expected substring), (b) zero components have a PURL string containing `{{` (no unrendered template placeholders leaked).

### Implementation for User Story 1

*(T009 above IS the US1 implementation; T008 + T010 are the tests. No further US1 implementation tasks — the T009 branch delta is the entire fix.)*

**Checkpoint**: US1 fully functional (gated). Vaultwarden-style live verification via T020 in Phase 6 (quickstart Reproducer 1 with real helm binary).

---

## Phase 4: User Story 2 — Graceful Fallback Across 4 Failure Classes (Priority: P1)

**Goal**: Verify that each `HelmRenderError` variant triggers WARN-log + fall-back to unrendered extraction. Default CI, no real `helm` binary needed.

**Independent Test**: Scan any helm fixture with `--helm-render` under adverse conditions (empty PATH, stub script exiting 1, stub script sleeping past timeout). Assert scan exits 0, WARN log mentions the specific error class, output falls back to unrendered.

### Tests for User Story 2

- [X] T011 [P] [US2] Create stub-script fixture at `mikebom-cli/tests/fixtures/helm/render_stub_scripts_m203/`:
  - `helm-exit1.sh` — `#!/bin/sh\necho "chart error: broken template" >&2\necho "line 2 of stderr" >&2\nexit 1\n` — three lines of stderr + exit code 1 (tests both NonZeroExit and stderr_head cap).
  - `helm-sleep-forever.sh` — `#!/bin/sh\nsleep 3600\n` — for timeout tests.
  - `helm-sleep-3s.sh` — `#!/bin/sh\nsleep 3\n` — for env-var-timeout-override tests (short sleep to keep test fast).
  Each script `chmod 755`. Fixture directory MUST be added to PATH in the test invocation (via `env!("CARGO_MANIFEST_DIR")` + join).
- [X] T012 [US2] Add integration test `us2_helm_render_missing_binary_falls_back_m203` to `helm_reader.rs`: scan the m188 helm fixture (or T008 fixture) with `--helm-render` set + `PATH=""` (empty). Assert (a) scan exits 0, (b) stderr contains the WARN line with `BinaryNotFound` OR the substring "helm-render failed", (c) the emitted SBOM's component list is byte-identical (or at least equivalent) to a fallback-unrendered scan. Skip on Windows via `#[cfg(unix)]`.
- [X] T013 [US2] Add integration test `us2_helm_render_non_zero_exit_falls_back_m203`: rename/symlink `helm-exit1.sh` to `helm` in a temp directory, prepend that temp dir to PATH, then scan with `--helm-render`. Assert (a) scan exits 0, (b) stderr contains WARN line with `NonZeroExit` AND the first line of the stub's stderr (`"chart error: broken template"`), (c) `stderr_head` field truncated to at most 20 lines (verify by checking the stub's 3-line stderr appears in full). `#[cfg(unix)]`.
- [X] T014 [US2] Add integration test `us2_helm_render_timeout_falls_back_m203`: use `helm-sleep-forever.sh` renamed as `helm`, set `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=1` in the scan environment, then scan with `--helm-render`. Assert (a) scan exits 0 within ~5 seconds (1s timeout + generous cleanup budget), (b) stderr contains WARN line with `Timeout`, (c) SBOM component list matches unrendered fallback shape. `#[cfg(unix)]`.
- [X] T015 [US2] Add integration test `us2_helm_render_env_var_timeout_override_m203`: use `helm-sleep-3s.sh` renamed as `helm`, set `MIKEBOM_HELM_RENDER_TIMEOUT_SECS=1`, scan with `--helm-render`. Assert (a) scan exits 0 within ~4 seconds (1s timeout + cleanup, NOT the 3s the stub would sleep), (b) WARN mentions `Timeout` referencing 1s (not the default 60s). Verifies the env-var override plumbing. `#[cfg(unix)]`.
- [X] T016 [US2] Add integration test `us2_helm_render_off_never_invokes_subprocess_m203`: scan a helm fixture WITHOUT `--helm-render` set + PATH containing the `helm-sleep-forever.sh` stub. Assert scan completes within ~2 seconds (proving the stub was NEVER invoked — if it had been, the scan would hang for 3600s). This is the FR-006 regression guard.
- [X] T016a [P] [US2] Add unit test `helm_render_error_io_error_variant_displays_and_falls_back_m203` to `mikebom-cli/src/scan_fs/package_db/helm.rs::tests` — the FR-007 `IoError` variant that the 4 integration tests (T012-T015) do NOT exercise (they cover BinaryNotFound, NonZeroExit, and Timeout only). Test body:
  1. Construct a synthetic `HelmRenderError::IoError(std::io::Error::from(std::io::ErrorKind::PermissionDenied))`.
  2. Assert the Display impl produces a non-empty string containing "I/O error" or similar wording per data-model E1.
  3. Assert the error implements `std::error::Error` (via `let _: &dyn std::error::Error = &err;` compile-time check).
  This closes CG1 from /speckit-analyze: without this test, all 4 fallback classes appear in FR-007 but only 3 have executable coverage. The IoError class fires rarely (permission-denied on `helm` binary, disk-full on stdout capture, etc.); a unit-level Display test is proportionate to its rarity vs the integration-test overhead of forcing the runtime condition.

**Checkpoint**: All 5 US2 fallback + regression tests pass in default CI (no real helm binary). Both US1 and US2 done.

---

## Phase 5: Cross-Cutting Golden Drift Re-Verification

**Purpose**: Follow the m199-m202 empirical-verification lesson. Research R3/plan claim 0 golden drifts, but that's an unverified claim until implement time. Explicitly re-audit post-implementation.

- [X] T017 [P] Re-run T002 audit post-implementation: `git diff --stat mikebom-cli/tests/fixtures/`. Expected: only the two new fixture directories (`helm/render_success_m203/` + `helm/render_stub_scripts_m203/`). Any existing golden JSON in the diff means unexpected drift.
- [X] T018 [P] Run every existing helm-related test to confirm zero regression: `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test helm_reader --no-fail-fast 2>&1 | tail -3` (expected: `ok. N passed; 0 failed`; N includes both m188's pre-existing tests + m203's 5 new US2 tests + T007's 6 new unit tests via `bin mikebom` path).

---

## Phase 6: Polish & Verification

- [X] T019 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 5 seconds per SC-006. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per feedback_prepr_gate_bails_on_first_failure memory).
- [ ] T020 [P] (Optional, requires local `helm` binary) Manually execute quickstart.md Reproducer 1 (successful rendered extraction) against `/tmp/m203-chart/`. Confirm `jq '.components[]? | .purl' /tmp/m203-rendered.cdx.json` returns a concrete `nginx:1.27.0` PURL (no `{{` characters). SC-001 verified.
- [X] T021 [P] Manually execute quickstart.md Reproducer 4 (FR-009 non-Helm byte-identity) against any non-Helm project. Confirm `diff` shows byte-identical output pre/post-fix for scans WITHOUT any Chart.yaml. Regression guard.
- [ ] T022 Draft PR body with `Closes #553` per SC-007. Include: (a) 1-paragraph summary of the subprocess pattern + 4 fallback classes, (b) research R3 empirical-verification outcome (T017 result — expected zero fixture drift beyond the 2 new directories), (c) code-diff LOC + files touched (~300 LOC, 1 source + 1 test + 2 fixtures), (d) test coverage summary (1 gated US1 integration + 5 US2 integration + 6 unit tests), (e) note that follow-up #554 (m204 candidate) will surface `helm_extraction_mode` via document-scope annotation.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. T001 sequential; T002 + T003 parallel.
- **Phase 2 (Foundational)**: Depends on Phase 1. T004 → T005 → T006 → T007 sequential (types + helper + subprocess function + tests build on each other). Post-T007 verify existing helm tests still pass.
- **Phase 3 (US1)**: Depends on Phase 2 completion. Fixture (T008) [P]; branch wire (T009) after T007; integration test (T010) after T009.
- **Phase 4 (US2)**: Depends on Phase 2 T006 landing + Phase 3 T009 branch wiring. Stub fixture (T011) [P]; tests T012-T016 all parallel after T011.
- **Phase 5 (Golden Drift Re-Verification)**: Depends on Phase 4 completion.
- **Phase 6 (Polish)**: Depends on Phase 5 completion.

### Within US1

- T008 [P] → T009 → T010.

### Within US2

- T011 [P] → T012 + T013 + T014 + T015 + T016 all parallel.

### Parallel Opportunities

- **Phase 1**: T002 + T003 parallel.
- **Phase 2**: T007 unit tests can run in parallel once T004-T006 complete (same file — use one batched Edit for all 6 tests).
- **Phase 3**: T008 fixture creation parallel with reading existing test file.
- **Phase 4**: T012-T016 all parallel (5 independent test cases; add all to `helm_reader.rs` in one batched Edit).
- **Phase 5**: T017 + T018 parallel.
- **Phase 6**: T020 + T021 parallel.

---

## Parallel Example: Phase 4 Fallback Test Batch

```bash
# Kick off all 5 US2 integration tests as one batched Edit to helm_reader.rs.
# All 5 test bodies added simultaneously in a single tool call.
Task: "Add us2_helm_render_missing_binary_falls_back_m203 test"
Task: "Add us2_helm_render_non_zero_exit_falls_back_m203 test"
Task: "Add us2_helm_render_timeout_falls_back_m203 test"
Task: "Add us2_helm_render_env_var_timeout_override_m203 test"
Task: "Add us2_helm_render_off_never_invokes_subprocess_m203 test"
```

---

## Implementation Strategy

### MVP First (US2 First, US1 Optional)

Unlike the typical US1-is-MVP flow, m203 might reasonably ship US2 FIRST because:
- US2 (fallback tests) runs in default CI without a real helm binary.
- US1 (success test) requires `MIKEBOM_HELM_INTEGRATION=1` — nightly-only, doesn't gate PR merge.
- US2 alone proves the fix's graceful-degradation contract; US1's success path is a natural corollary if the plumbing works.

Recommended order:
1. Phase 1 (Setup) → Phase 2 (Foundational) → foundation ready.
2. Phase 3 (US1) → land the branch wire + fixture. T010 (success integration test) is nightly-gated.
3. Phase 4 (US2) → land all 5 fallback tests in default CI. This is the LOAD-BEARING regression guard.
4. STOP + VALIDATE: T012-T016 pass, T017-T018 zero fixture drift.

### Full-Bundle Delivery (Preferred)

1. Phases 1 → 2 → 3 → 4 → 5 → 6 in order.
2. Single PR closes #553 with US1 + US2 coverage.

---

## Notes

- [P] tasks = different files, no cross-dependency on incomplete task.
- Every FR has ≥1 executable test: FR-001 via T009 code + T010/T013 integration; FR-002 via T005 helper + T007 unit tests + T014/T015 integration; FR-003 via T012 integration (BinaryNotFound); FR-004 via T013 integration (NonZeroExit); FR-005 via T009 code + T010 integration; FR-006 via T016 regression guard; FR-007 via T012 (BinaryNotFound) + T013 (NonZeroExit) + T014 (Timeout) + T016a unit (IoError — the runtime-rare class); FR-008 via T019 wall-clock; FR-009 via T017 fixture drift + T021 manual verify.
- Empirical R3 claim (0 pre-existing goldens require regen) is re-verified at implement time via T017.
- Zero new Cargo dependencies.
- Zero new user-facing `mikebom:*` annotations (m203 sets `HelmExtractionMode::Rendered` on internal `ScanDiagnostics`; #554/m204 surfaces it later).
- US1 success test (T010) is nightly-gated behind `MIKEBOM_HELM_INTEGRATION=1` — default CI lane runs 5 US2 tests + 6 unit tests without a real helm binary.
- Total ~300 LOC across 1 source file + 1 test file + 2 fixture directories (per plan.md scope estimate).
