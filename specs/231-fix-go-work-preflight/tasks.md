---

description: "Task list for feature 231-fix-go-work-preflight: fix go list all preflight failure in Go workspace mode"
---

# Tasks: Fix `go list all` preflight failure in Go workspace mode

**Input**: Design documents from `/specs/231-fix-go-work-preflight/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/go-work-detection.md`, `quickstart.md`

**Tests**: Included. Bug fix follows the m216/m230 pattern of unit tests colocated with the reader + one integration test scanning a synthetic fixture.

**Organization**: One user story (US1). Setup + foundational helpers + implementation land as a single conceptual change but broken into sequential tasks for clarity. Grafana verification (SC-002) is a manual polish step.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different files, no cross-task dependencies
- **[Story]**: `[US1]` — the only user story
- File paths absolute or repo-relative; every task cites exact file

---

## Phase 1: Setup

- [X] T001 Verify feature branch is `231-fix-go-work-preflight` (per `git branch --show-current`) and that `cargo +stable check -p waybill --lib` exits 0 against the untouched tree — locks in the pre-change baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Land the `WorkspaceMode` enum, the `detect_workspace_mode` helper, and the modified `apply_offline_env` signature. These are same-file edits and MUST be sequential.

- [X] T002 In `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`, add a module-private enum `WorkspaceMode` with variants `Off`, `Inactive`, `Active(PathBuf)`, `Explicit(PathBuf)` matching the 4-variant shape in `data-model.md § New enum`. Place it below the existing `SkipReason` enum (~line 60) and above `apply_offline_env`. Do NOT export via `pub`; the enum is an implementation detail.
- [X] T003 In the same file, add a module-private helper `fn detect_workspace_mode(main_module_dir: &Path) -> WorkspaceMode` implementing the algorithm in `contracts/go-work-detection.md § Detection algorithm`. Steps: (1) read `GOWORK` env; case-insensitively match `"off"`; treat unset/empty/`"auto"` as fall-through; treat any other non-empty as explicit path (return `Explicit` if `is_file`, else fall through). (2) Walk `main_module_dir.ancestors()` looking for `<ancestor>/go.work`; first hit returns `Active(<ancestor>/go.work)` (canonicalize before wrapping in the variant). (3) Filesystem-root reached without a hit → return `Inactive`. Any `fs::metadata`/`is_file` failure at any level is treated as "no `go.work` here, keep walking" — no error propagation.
- [X] T004 In the same file, modify `apply_offline_env` (currently `fn apply_offline_env(cmd: &mut Command, offline: bool)` at line 134) to `fn apply_offline_env(cmd: &mut Command, offline: bool, workspace_mode: &WorkspaceMode)`. Set `GOPROXY=off` + `GOTOOLCHAIN=local` unconditionally when `offline == true`. Set `GOFLAGS=-mod=mod` ONLY when `workspace_mode` matches `Off` or `Inactive`. When `workspace_mode` matches `Active(_)` or `Explicit(_)`, do NOT set `GOFLAGS` (Go's workspace default `-mod=readonly` will apply).
- [X] T005 In the same file, update `run_bounded` (line 154) — the caller of `apply_offline_env` — to compute the workspace mode from `cwd` (which IS the main-module directory for the preflight invocation) and pass it through. Add a `let workspace_mode = detect_workspace_mode(&cwd);` line right before the `apply_offline_env` call, then pass `&workspace_mode` to it.
- [X] T006 In the same file, extend `MainModuleAnalysis` struct (~existing definition near the module top; grep for `pub struct MainModuleAnalysis`) with a new `pub workspace_active: bool` field defaulting to `false`. `analyze_main_module` (line 182) MUST set it via `analysis.workspace_active = matches!(workspace_mode, WorkspaceMode::Active(_) | WorkspaceMode::Explicit(_));` immediately after calling `detect_workspace_mode` on the main-module dir.
- [X] T007 Extend the scan-level `INFO: go-mod-why classification:` log line (grep for `go-mod-why classification` in `waybill-cli/src/` — the emitter lives in the aggregation loop that consumes `MainModuleAnalysis` outputs, likely in `scan_fs/package_db/golang/mod.rs` or a sibling). Add a new key `workspace_modules=<N>` where N is the sum of `workspace_active` across every analyzed main-module. Placement: after `unknown_marked=<E>` and before `elapsed_ms=<T>`.

**Checkpoint**: All helpers land + wiring done. Workspace still builds. Existing NuGet + Go tests still pass (the modified `apply_offline_env` signature is invoked correctly from `run_bounded` — no other callers).

---

## Phase 3: User Story 1 — Workspace scan preserves build-inclusion classification (Priority: P1) 🎯 MVP

**Goal**: A workspace-mode scan produces definitive `waybill:build-inclusion` verdicts on every Go component (no `unknown` fallback triggered by the preflight failure). Verified via a synthetic fixture (SC-001) and manual Grafana re-scan (SC-002).

**Independent Test**: Run `cargo +stable test -p waybill --test golang_workspace_mode_preflight`. Expect all invariants from `contracts/go-work-detection.md § Behavior invariants` pass, plus SC-001 assertions (no `go-mod-why analysis skipped` warnings, ≥1 non-`unknown` build-inclusion, `analyzed ≥ 1`).

### Tests for User Story 1

- [X] T008 [P] [US1] Add unit test `detect_workspace_mode_returns_off_when_env_off` in `mod_why.rs::tests`. Set `std::env::set_var("GOWORK", "off")` (guard with `EnvGuard` per memory `reference_podman_test_flake`; see `waybill-cli/src/testing/env_guard.rs`), place a `go.work` in a tempdir, call `detect_workspace_mode(tempdir.path())`, assert result matches `WorkspaceMode::Off`. Contract invariant #1.
- [X] T009 [P] [US1] Add unit test `detect_workspace_mode_returns_inactive_when_no_go_work` in same block. `GOWORK` unset (env-guard); tempdir without `go.work`; assert `WorkspaceMode::Inactive`. Contract invariant #2.
- [X] T010 [P] [US1] Add unit test `detect_workspace_mode_active_from_immediate_parent` in same block. `GOWORK` unset; tempdir containing a nested `sub/` dir and a `go.work` at the tempdir root; call `detect_workspace_mode(tempdir.path().join("sub"))`; assert `WorkspaceMode::Active` with the tempdir's `go.work` path. Contract invariant #3.
- [X] T011 [P] [US1] Add unit test `detect_workspace_mode_active_from_two_levels_up` in same block. `GOWORK` unset; tempdir with `a/b/` structure and `go.work` at tempdir root; call detection on `tempdir/a/b`; assert `Active(tempdir/go.work)`. Contract invariant #4.
- [X] T012 [P] [US1] Add unit test `detect_workspace_mode_explicit_path_returns_explicit` in same block. Create a real `go.work` at a tempfile path; `GOWORK=<that-path>` (env-guard); call detection on any dir; assert `WorkspaceMode::Explicit(<that-path>)`. Contract invariant #5.
- [X] T013 [P] [US1] Add unit test `detect_workspace_mode_falls_through_when_explicit_missing` in same block. `GOWORK=/nonexistent-path` (env-guard); tempdir with a `go.work` at root; call detection on tempdir; assert `WorkspaceMode::Active` (explicit path missing → fall through to on-disk; contract invariant #6).
- [X] T014 [P] [US1] Add unit test `apply_offline_env_workspace_omits_goflags` in same block. Build a `Command::new("echo")`, call `apply_offline_env(&mut cmd, true, &WorkspaceMode::Active(PathBuf::from("/tmp/go.work")))`. Inspect the child-process env via `cmd.get_envs()` — assert `GOPROXY=off` present, `GOTOOLCHAIN=local` present, `GOFLAGS` **not present** (or empty).
- [X] T015 [P] [US1] Add unit test `apply_offline_env_non_workspace_keeps_mod_mod` in same block. Build a `Command`, call `apply_offline_env(&mut cmd, true, &WorkspaceMode::Inactive)`. Assert `GOFLAGS=-mod=mod` IS present (FR-003 byte-parity guarantee).
- [X] T016 [US1] Create the synthetic workspace fixture at `waybill-cli/tests/fixtures/golden_inputs/golang/workspace_mode/`. Files: `go.work` (Go 1.22 syntax with `use ./module-a` + `use ./module-b`), `module-a/go.mod` (module `example.com/mikebomfixture/a` with dep on `example.com/mikebomfixture/shared v1.0.0`), `module-a/main.go` (imports shared), `module-b/go.mod` (module `example.com/mikebomfixture/b` with same shared dep), `module-b/lib.go` (imports shared). Use `MikebomFixture.*`-style synthetic names per memory `feedback_fixture_synthetic_package_names` — no real coordinates. Include populated `go.sum` files if the offline scan requires them; otherwise the preflight will report the shared dep as unresolved which is still an acceptable outcome (verdict changes from `unknown` to `unresolved`).
- [X] T017 [US1] Create integration test at `waybill-cli/tests/golang_workspace_mode_preflight.rs`. Reuse the `common::bin`, `apply_fake_home_env`, `Command::new(bin())` subprocess scaffold from `waybill-cli/tests/nuget_main_module_parity.rs`. Add four tests: (1) `workspace_scan_produces_no_skip_warnings` — asserts stderr contains 0 `go-mod-why analysis skipped` lines (SC-001); (2) `workspace_scan_analyzes_at_least_one_module` — asserts stderr contains an `analyzed=N` where N ≥ 1 (SC-004); (3) `workspace_scan_emits_workspace_modules_counter` — asserts stderr contains `workspace_modules=` with a positive value (FR-006); (4) `workspace_scan_produces_no_unknown_markers` — asserts stderr's `build-inclusion pass: marked=` counter reports 0 (or ≤ small_residual per spec Assumptions) for the fixture (FR-004 explicit assertion that a successful preflight produces definitive verdicts, not the `unknown` fallback).

**Checkpoint**: Unit + integration tests pass locally via `cargo +stable test -p waybill mod_why::` and `cargo +stable test -p waybill --test golang_workspace_mode_preflight`.

---

## Phase 5: Polish & Cross-Cutting Concerns

- [X] T018 [P] **N/A** — no pre-existing Go-workspace audit doc exists at `docs/audits/`; there's nothing to append. Mark this task complete during implementation without touching any file. (If a future Go-workspace audit is authored, that milestone will add its own post-231 update note.)
- [X] T019 Run `./scripts/pre-pr.sh` locally — expect green. Per memory `feedback_prepr_gate_bails_on_first_failure`, if it fails, enumerate every `^---- .+ stdout ----` line in the failure output before triaging.
- [X] T020 **Manual verification (SC-002)**: With a local clone of `github.com/grafana/grafana`, run the milestone-231 binary against it in offline mode and confirm the `INFO: build-inclusion pass: marked=` counter drops from 469 to ≤ 5. Record the exact number in the PR body when opening the fix PR. This is a one-shot local check, not automated.
- [X] T021 Walk through `specs/231-fix-go-work-preflight/quickstart.md` end-to-end against a fresh `cargo build --release -p waybill` binary. Confirm SC-001, SC-003, SC-004, SC-005 assertions each return the expected shape. SC-002 was covered by T020.

---

## Dependencies & Execution Order

- **Phase 1 (Setup)**: T001 no code dependencies.
- **Phase 2 (Foundational)**: T002 → T003 → T004 → T005 → T006 → T007 all touch `mod_why.rs` (plus T007 touches the aggregation-loop caller); sequential. T007 depends on T006's new `workspace_active` field.
- **Phase 3 (US1)**: Requires Phase 2 complete. Unit tests T008–T015 all live in `mod_why.rs::tests` (same file, but each is a separate `#[test] fn`); can be authored in parallel. Fixture creation (T016) is independent of unit tests. Integration test (T017) requires T016 fixture + T007 log-line extension.
- **Phase 5**: All polish tasks require Phase 3 complete. T018–T021 are largely independent but T020's manual Grafana verification implicitly depends on T019 (pre-PR gate) being green first.

### Parallel Opportunities

- Unit tests T008–T015 — different test-function names in the same mod tests block. Parallelizable at authoring time; sequential at merge time.
- Fixture creation (T016) can proceed in parallel with unit-test authoring.

---

## Implementation Strategy

### MVP: Complete US1 (all sequential except unit-test authoring)

1. T001 setup verification.
2. T002 → T007 foundational (7 edits to mod_why.rs and its caller; ~150 LOC net).
3. T008 → T015 unit tests (colocated with helpers; ~200 LOC).
4. T016 synthetic fixture (~30 LOC across 5 files).
5. T017 integration test (~120 LOC).
6. T018 skip if no relevant audit.
7. T019 pre-PR gate.
8. T020 manual Grafana verification (one-shot).
9. T021 quickstart walkthrough.

### Not a parallel-team milestone

Single-file fix (mod_why.rs + one aggregation-loop tweak), ~500 LOC total including tests. One contributor completes linearly in one session.

---

## Notes

- Every task cites a concrete file path per the format requirement. Unit-test tasks name the test function so each checklist item maps to a specific `#[test] fn <name>` block.
- Env-var-mutating unit tests (T008, T009, T012, T013) MUST route through `crate::testing::EnvGuard::acquire()` per memory `reference_podman_test_flake` to prevent cross-test race conditions.
- Fixture module paths use `example.com/mikebomfixture/*` synthetic naming per memory `feedback_fixture_synthetic_package_names`.
- No new Cargo dependencies; no `Cargo.toml` edits.
- No changes to `scan_fs/mod.rs` — the fix is scoped to `mod_why.rs` + the classification-log aggregation site.
- `go_mod_graph.rs` is untouched (research §R3 confirmed it doesn't have the same bug).
- **FR-005 (residual failures → warn-and-skip preserved) is covered by latent regression**: the pre-231 `mod_why.rs::tests` block contains 13 existing `#[test]` blocks that exercise the `SkipReason::UnresolvablePackages` WARN path at `mod_why.rs:209-217`. Milestone 231 does NOT modify that path — it only adds a conditional gate (`workspace_mode` check) before setting `GOFLAGS`. Any regression on FR-005 would break those existing tests. No new task needed.
