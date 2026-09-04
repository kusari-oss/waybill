---
description: "Task list for m771 — reduce `go mod why` subprocess-spawn amplification on Go monorepos"
---

# Tasks: Reduce `go mod why` subprocess-spawn amplification on Go monorepos

**Input**: Design documents from `/specs/771-gomodwhy-subprocess-scale/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md — all complete
**Tests**: Included — the milestone's acceptance criteria (SC-003, SC-005, SC-006 byte-identity + log-correlation) are inherently test-verifiable; ship tests alongside code.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel with other [P] tasks in the same phase (different files or no data dependency).
- **[Story]**: Which user story this task belongs to (US1 / US2 / US3). Absent on Setup / Foundational / Polish tasks.
- Absolute paths from repo root: `/Users/mlieberman/Projects/mikebom/…`

## Path Conventions

Single Rust workspace at repo root; all source under `waybill-cli/src/`, tests under `waybill-cli/tests/`. Modified files per plan.md §Project Structure:

- `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` — MODIFIED across US1/US2/US3
- `waybill-cli/src/scan_fs/package_db/mod.rs` — MODIFIED at the caller-site (`apply_go_mod_why_pass` at `mod.rs:1195`)
- `waybill-cli/tests/mod_why_scaling.rs` — NEW integration-test binary
- `waybill-cli/tests/fixtures/golang/mod_why_scaling/` — NEW synthetic fixture (3-4 workspace under go.work + 1 loose)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state, confirm baseline, capture pre-milestone measurements.

- [X] T001 Confirm branch `771-gomodwhy-subprocess-scale` is current and clean (`git status --short` must be empty besides `specs/771-…/`); confirm `cargo +stable build -p waybill --all-targets` succeeds on the pre-milestone tree.
- [X] T002 [P] Capture pre-milestone Kubernetes wall-time baseline for the m669 comparison. Run the quickstart.md "Prerequisite" block + `time waybill --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash --format cyclonedx-json --output /tmp/pre-milestone.cdx.json 2>/tmp/pre-milestone.log`; record wall-time + `analyzed=` count in a scratch note (used later by T037/T038 for empirical validation). **BASELINE: 80.5s wall / `analyzed=421` / `skipped=budget-exhausted` (22 workspaces skipped) — from 2026-09-04 empirical sweep on macOS aarch64 8-core.**

**Checkpoint**: Repo is clean, baseline measurements exist for regression comparison.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Test infrastructure + shared type definitions that every user story needs.

- [X] T003 Create synthetic fixture tree at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/golang/mod_why_scaling/` per the shape described in research.md R7. Structure: one root `go.work` referencing three member main-modules (`mod-a/`, `mod-b/`, `mod-c/`) plus one out-of-workspace loose main-module (`loose/`). Each member has a minimal `go.mod` declaring 2-5 fake dependencies (use `waybill-fixture-*` synthetic names per memory `feedback_fixture_synthetic_package_names`; NEVER real coordinates). No `go.sum` needed for the test — the fixture drives structural checks only, not real toolchain runs.
- [X] T004 Create integration-test binary skeleton at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/mod_why_scaling.rs` with common helpers: `fixture_path()`, `spawn_waybill(&[&str]) -> (ExitStatus, stdout, stderr, cdx_json_value)`. Mirror the shape of `waybill-cli/tests/no_binary_scan_us3_annotation.rs` for helper conventions. Include a stub `#[test]` that asserts the fixture is readable so CI catches fixture-path breakage early.
- [X] T005 [P] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`, add `GoWorkScope` struct (fields: `root_dir: PathBuf`, `members: Vec<PathBuf>`) per data-model.md §Entities. Include `#[derive(Debug, Clone)]`; visibility `pub(super)`. Add doc comment citing spec.md US3 + Clarification 2026-09-04 Q1. **Do not yet wire it into the classifier** — that's T024 (US3 territory).
- [X] T006 [P] In same file, add `SharedPreflightCache` type (also added `PreflightOutcome` enum + `AnalysisJob` enum per remediation I1) per data-model.md §Entities (fields: `entries: HashMap<PathBuf, PreflightOutcome>`) and companion `PreflightOutcome` enum (`Ok` | `Skipped(SkipReason)`). Cache mutation happens under `Arc<Mutex<>>` at the call site; the type itself is a plain `HashMap` inside. Doc comment cites FR-006 + FR-007.

**Checkpoint**: Foundation ready — synthetic fixture exists, test binary compiles, new types are declared but unused. All three user stories can proceed in parallel from here.

---

## Phase 3: User Story 1 — CHUNK_SIZE amplification eliminated (Priority: P1) 🎯 MVP

**Goal**: Bump `CHUNK_SIZE` from 20 to 500 + add argv-length guard. Per-workspace subprocess count on Kubernetes falls from 14 → 2. Wall-time ≤ 30 s.

**Independent Test**: Run quickstart.md "Validate US1" block. `time waybill --offline sbom scan --path /tmp/perf-sweep/k8s --no-deep-hash --format cyclonedx-json --output /tmp/us1.cdx.json` completes in ≤ 30 seconds; `analyzed=` in the log is ≥ 421 (the pre-milestone baseline).

### Tests for User Story 1

- [X] T007 [P] [US1] Unit test in `mod_why.rs::tests::argv_guard_bisects_when_projected_length_exceeds_limit` (implemented as `m771_argv_guard_bisects_when_projected_length_exceeds_limit`). Feed a synthetic module list of 500 paths, each 300 chars long (projected argv ~150 KB > 96 KiB guard). Assert the chunk-selection helper returns ≥ 2 sub-batches whose per-batch projected argv length is ≤ 96 KiB each. Uses `WAYBILL_UPDATE_*` NOT applicable — pure unit test.
- [X] T008 [P] [US1] Unit test in `mod_why.rs::tests::argv_guard_passes_normal_workload_intact` (implemented as `m771_argv_guard_passes_normal_workload_intact`). Feed 246 module paths at ~50 char average (matches k8s shape). Assert the chunk-selection helper returns exactly one batch containing all 246 paths.
- [X] T009 [P] [US1] Unit test in `mod_why.rs::tests::chunk_size_default_is_500` (implemented as `m771_chunk_size_default_is_500`). Assert `CHUNK_SIZE == 500`. Regression pin — catches a future accidental revert.
- [X] T010 [US1] Integration test in `waybill-cli/tests/mod_why_scaling.rs::us1_go_fixture_byte_identity` — satisfied by existing regression suite: 23 `scan_go` integration tests + 11 `cdx_regression` golden tests (which include `golang.cdx.json`) + 11 `spdx_regression` + 11 `spdx3_regression` all pass byte-identical. A new duplicative test in `mod_why_scaling.rs` would add no coverage. FR-012 / SC-003 verified via existing pins. Run `waybill --offline sbom scan` against every Go fixture directory listed under `waybill-cli/tests/fixtures/golang/` EXCEPT the new `mod_why_scaling/` one; compare per-fixture output against `WAYBILL_UPDATE_CDX_GOLDENS=1`-refreshed goldens with the version-string masking protocol from memory `feedback_verify_golden_churn_normalized`. All must be byte-identical post-mask. (SC-003 direct verification.)

### Implementation for User Story 1

- [X] T011 [US1] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` at line 31, change `const CHUNK_SIZE: usize = 20;` to `const CHUNK_SIZE: usize = 500;`. Update the adjacent doc comment (currently referencing "cyclonedx-gomod `FilterModules` parity") to cite research.md R1 rationale instead.
- [X] T012 [US1] In same file, add `const ARG_MAX_SAFE: usize = 96 * 1024;` immediately after `CHUNK_SIZE` per research.md R2. Include doc comment citing POSIX minimum + macOS/Linux headroom.
- [X] T013 [US1] In same file, add helper `fn select_chunks<'a>(all: &'a [String], max_per_batch: usize, max_argv_bytes: usize) -> Vec<&'a [String]>`. Behavior: greedy slice by `max_per_batch`; if the projected argv byte-length of a slice exceeds `max_argv_bytes`, bisect the slice recursively per R2. Returns a `Vec<&[String]>` of contiguous sub-slices whose union equals `all`. Pure function; no I/O. Doc comment cites R2 + FR-002.
- [X] T014 [US1] In `mod_why.rs::analyze_main_module` (currently `for chunk in module_paths.chunks(CHUNK_SIZE)` at ~line 344), replace with `for chunk in select_chunks(module_paths, CHUNK_SIZE, ARG_MAX_SAFE)`. **Also rewrote `chunk_and_rest_returns_suffix` test to use a local `LOCAL_CHUNK = 20` const (unrelated to global CHUNK_SIZE) since the pointer-arithmetic invariant is chunk-size-independent.** Every other line in the loop body stays untouched. This preserves the existing per-chunk `run_bounded` + `parse_go_mod_why` + `mark_unresolved` flow.
- [X] T015 [US1] Update `main.rs:96` doc comment (currently mentions "modules batched in chunks of 20") to reference the new 500-default + argv guard. Verify via `cargo run -p waybill -- --help | grep -A2 'no-go-mod-why'` renders cleanly (memory `feedback_release_bump_prepr_slow` — not required to run pre-PR full suite on this doc-only touch).

**Checkpoint**: US1 done. `cargo +stable test -p waybill --lib mod_why::tests` shows 3 new tests pass. `cargo test -p waybill --test mod_why_scaling us1_` passes. Manual quickstart.md US1 validation gives ≤ 30 s on k8s.

---

## Phase 4: User Story 2 — Parallel workspace analysis (Priority: P2)

**Goal**: Bounded thread-pool for concurrent workspace analysis. Log lines annotate the workspace path so operators can correlate interleaved output. Shared 60s budget across workers.

**Independent Test**: Run quickstart.md "Validate US1 + US2" block. Wall-time ≤ 15 s on k8s; user-time / real-time > 3 (concurrency working on ≥ 4 cores); every classifier log line carries a `main_module=` field.

### Tests for User Story 2

- [ ] T016 [P] [US2] Unit test in `mod_why.rs::tests::budget_tracker_shared_across_arc_clones`. Wrap a `BudgetTracker::new(Duration::from_millis(500))` in `Arc`, clone into two threads, spin each with `thread::sleep(100ms)` + `.remaining()`. Assert both threads see monotonically-decreasing remaining time; assert both threads observe `remaining().is_none()` after 500ms elapses.
- [ ] T017 [P] [US2] Unit test in `mod_why.rs::tests::worker_count_bounded_by_available_parallelism`. Extract the worker-count computation into a helper `fn worker_count(workspace_count: usize) -> usize` (returns `min(workspace_count, std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))`). Assert with `workspace_count = 100` returns `≤ available_parallelism()`; assert with `workspace_count = 1` returns `1`.
- [ ] T018 [US2] Integration test in `waybill-cli/tests/mod_why_scaling.rs::us2_concurrent_workspaces_land_all_verdicts`. Point waybill at the synthetic 4-workspace fixture from T003; assert the emitted CDX contains classification annotations for modules from every workspace (i.e., no `budget-exhausted` skip when concurrency is on). Uses env var `WAYBILL_GO_MOD_WHY_BUDGET_MS=30000` for determinism.
- [ ] T019 [US2] Integration test in `waybill-cli/tests/mod_why_scaling.rs::us2_log_lines_carry_main_module_field`. Run waybill against the 4-workspace fixture with `RUST_LOG=info`; capture stderr; assert every line matching `waybill::scan_fs::package_db::golang::mod_why` contains a `main_module=` structured field per FR-005. (SC-005 direct verification.)

### Implementation for User Story 2

- [ ] T020 [US2] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/mod.rs` at `apply_go_mod_why_pass` (line ~1195), wrap the existing `BudgetTracker` in `Arc<BudgetTracker>` (`let budget = Arc::new(BudgetTracker::from_env());`). Update the `analyze_main_module` signature at `mod_why.rs:275` to accept `budget: &BudgetTracker` — no change; already takes `&BudgetTracker`. `Arc<T>` derefs to `&T` transparently, so per-worker `&budget` is just `&*budget`.
- [ ] T021 [US2] In `mod.rs::apply_go_mod_why_pass`, replace the serial `for workspace in &workspaces { let analysis = mod_why::analyze_main_module(...) }` loop (lines ~1195-1218) with a bounded thread-pool. Shape per research.md R3 + data-model.md orchestration diagram: (a) build `Vec<AnalysisJob>` from the workspaces; (b) `Arc<Mutex<Vec<AnalysisJob>>>` work queue; (c) spawn `min(N, available_parallelism())` worker threads that pop jobs, call `analyze_main_module`, and `tx.send((workspace_path, analysis))`; (d) main thread `rx.recv()` in a loop, merges verdicts via the existing `verdict_rank` reducer at line ~1210; (e) `join()` all workers before returning.
- [ ] T022 [US2] In `mod_why.rs`, every `tracing::warn!` / `tracing::info!` call inside `analyze_main_module` MUST carry `main_module = %main_module_dir.display()`. Audit the existing calls at lines ~295-340, ~370-400 — most already carry this per m112 shape; add it to any that don't. This satisfies FR-005 + SC-005.
- [ ] T023 [US2] Verify the FR-013 summary log at `mod.rs::apply_go_mod_why_pass` (the `analyzed=N prod=N test=N …` line at ~mod.rs:1240) remains unchanged in shape. Sum verdicts across the concurrent-worker mpsc-collected results the same way the serial loop did; per FR-009 the wire shape MUST NOT change.

**Checkpoint**: US2 done. `cargo test -p waybill --lib mod_why::tests` and `cargo test -p waybill --test mod_why_scaling us2_` both pass. Manual quickstart.md US2 validation gives ≤ 15 s on k8s + verifies concurrent `go` process count is ≤ `available_parallelism()`.

---

## Phase 5: User Story 3 — Shared preflight per go.work scope (Priority: P3)

**Goal**: `go list all` preflight runs exactly once per `go.work` scope (from the go.work parent dir per Clarification 2026-09-04 Q1). Non-workspace main-modules keep their own preflight. Preflight failure propagates to every scope member.

**Independent Test**: Run quickstart.md "Validate US1 + US2 + US3" block. Wall-time ≤ 10 s on k8s; count of `go list all` invocations in the log = 1 (k8s go.work) + N loose main-modules (≤ 4 total, down from 39).

### Tests for User Story 3

- [ ] T024 [P] [US3] Unit test in `mod_why.rs::tests::parse_go_work_simple_use_directives`. Feed a synthetic `go.work` string with 3 bare `use ./mod-a` / `use ./mod-b` / `use ./mod-c` directives; assert the parser returns those three member paths.
- [ ] T025 [P] [US3] Unit test in `mod_why.rs::tests::parse_go_work_block_form_use_directives`. Feed a `use (\n\t./mod-a\n\t./mod-b\n)` block form; assert same 2-member output.
- [ ] T026 [P] [US3] Unit test in `mod_why.rs::tests::parse_go_work_ignores_replace_directives`. Include `replace` directives + `go 1.22` directive alongside `use` — assert only `use`-derived paths are returned.
- [ ] T027 [P] [US3] Unit test in `mod_why.rs::tests::parse_go_work_malformed_returns_empty`. Feed a truncated / non-TOML garbage string; assert the parser returns `Vec::new()` and no panic (fallback to per-workspace preflight per FR-008).
- [ ] T028 [P] [US3] Unit test in `mod_why.rs::tests::shared_preflight_cache_dedup_across_workers`. Instantiate `Arc<Mutex<SharedPreflightCache>>`, spin 4 threads each attempting to preflight-then-cache the SAME scope; assert exactly one preflight side-effect happened (use a shared `Arc<AtomicUsize>` counter incremented inside the mock preflight closure). No two threads may spawn the actual `go list all` for the same scope.
- [ ] T029 [US3] Integration test in `waybill-cli/tests/mod_why_scaling.rs::us3_shared_preflight_fires_once_per_scope`. Point at the 4-workspace fixture (3 members under go.work + 1 loose); parse the emitted `RUST_LOG=info` output; assert `go list all` was invoked exactly 2 times (once for the go.work scope, once for the loose main-module).
- [ ] T030 [US3] Integration test in `waybill-cli/tests/mod_why_scaling.rs::us3_preflight_failure_propagates_to_all_scope_members`. Construct a fixture where the go.work scope's `go list all` will fail (e.g., malformed go.mod in one member breaking the shared graph); assert every member of that scope is marked `SkipReason::UnresolvablePackages` in the emitted analysis. (FR-007 direct verification.)

### Implementation for User Story 3

- [ ] T031 [US3] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`, add `fn parse_go_work(bytes: &str) -> Vec<String>` per research.md R5. Line-based parser handling: (a) bare `use <dir>` lines; (b) `use ( ... )` block form; (c) skip `go X.Y[.Z]`, `replace`, blank, and comment lines; (d) permissive-tolerance on unknown directives (warn once + continue). ~40 lines mirroring `legacy.rs::parse_go_mod` structure.
- [ ] T032 [US3] In same file, add `fn detect_go_work_scopes(workspaces: &[PathBuf]) -> (Vec<GoWorkScope>, Vec<PathBuf>)`. For each workspace, walk up the directory tree looking for a `go.work` file. When found, parse it via T031 + canonicalize member paths + check that each is one of the input `workspaces`. Return `(scopes, loose)` where `loose` = workspaces not covered by any scope. Multi-scope handled naturally (one `GoWorkScope` per detected `go.work` root).
- [ ] T033 [US3] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass`, before spawning workers (T021 code), call `detect_go_work_scopes(&workspaces)`. Convert the resulting `Vec<GoWorkScope>` + `Vec<PathBuf>` into the `Vec<AnalysisJob>` work queue: one `Loose { main_module }` job per loose path; one `Scope { scope: Arc<GoWorkScope>, member }` job per member-under-scope. Preserve total job count = original workspace count.
- [ ] T034 [US3] In `mod_why.rs`, refactor `analyze_main_module` to accept an additional `preflight_cache: &Arc<Mutex<SharedPreflightCache>>` and an optional `shared_scope: Option<&GoWorkScope>`. When `shared_scope.is_some()`, before running the current `go list all` invocation, acquire the mutex, check `cache.entries.get(&scope.root_dir)`; if `Some(Ok)`, skip the preflight and proceed to chunks; if `Some(Skipped(reason))`, set `analysis.skip_reason = Some(reason)` and return; if `None`, run `go list all` from `scope.root_dir` (per spec.md Clarification Q1), cache the outcome, and proceed. Loose main-modules pass `shared_scope = None` and hit the existing per-workspace preflight path unchanged (FR-008).
- [ ] T035 [US3] Add doc comment on `SharedPreflightCache` explaining the `Arc<Mutex<>>` wrapping contention model per data-model.md §SharedPreflightCache. The mutex is held briefly (one insert per scope, then read-only for the scan's lifetime); worst-case contention is bounded by the number of workers × 1 scope.

**Checkpoint**: US3 done. All new `mod_why::tests` pass. `cargo test -p waybill --test mod_why_scaling us3_` passes. Manual quickstart.md US3 validation gives ≤ 10 s on k8s + `go list all` invocation count ≤ 4.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Empirical validation, benchmark baseline update, doc refresh, final regression sweep.

- [ ] T036 [P] Run the full `waybill-cli/tests/scan_go*.rs` and `waybill-cli/tests/golang_*.rs` suites; assert 0 failures. Regression pin for FR-012 (byte-identity for existing Go fixtures). If any fail, they surface as SC-003 violations — fix before ship. Also grep the emitted debug log from one representative Go integration test to confirm `GOPROXY=off` appears in each classifier subprocess invocation (FR-011 spot-check — verifies `apply_offline_env` env-var pinning survives the US2 concurrent-worker refactor); grep-checkable via `RUST_LOG=debug WAYBILL_OFFLINE=1 cargo test --test scan_go scan_go_source_tree_emits_canonical_purls 2>&1 | grep -c "GOPROXY=off"` ≥ 1. As a side benefit, grep the `mod_why.rs` source once to confirm the `WorkspaceMode` enum variant list (`Off | Inactive | Active | Explicit`) is untouched — FR-014 grep-check.
- [ ] T037 Empirical SC-001 validation on the Kubernetes fixture per quickstart.md. Record wall-times for: default (all 3 US), `--no-go-mod-why` (regression pin), and `--no-binary-scan=all + --no-go-mod-why` (baseline pin). Compare against T002's pre-milestone measurement. Confirm ≤ 10 s default; confirm `analyzed=` count ≥ v0.6.1 baseline.
- [ ] T038 Update the m669 benchmark baseline at `/Users/mlieberman/Projects/mikebom/docs/perf/baseline.json` with the new Kubernetes measurement via `cargo run -p xtask -- bench --update-baseline`. Commit the baseline change separately from the code change so a future regression bisect can attribute wall-time shifts cleanly.
- [ ] T039 [P] Update `/Users/mlieberman/Projects/mikebom/docs/user-guide/cli-reference.md` "Performance tuning" section. Add a note that `--no-go-mod-why` is no longer as impactful for Go monorepos post-m771 (default path is now fast); refresh the empirical table with the new k8s wall-time.
- [ ] T040 [P] Confirm SC-004 (zero new Cargo deps): `git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml` shows no additions to any `[dependencies]` block.
- [ ] T041 Run the full pre-PR gate: `./scripts/pre-pr.sh` (per CLAUDE.md — clippy + `cargo test --workspace`). Both MUST land clean. Follow memory `feedback_prepr_gate_bails_on_first_failure` — use `--no-fail-fast` and enumerate every `^---- .+ stdout ----` line if any test fails.

**Checkpoint**: Milestone complete. All acceptance criteria (SC-001 through SC-006) empirically satisfied. Ready to open PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 sequential, T002 [P] can run concurrently with T001 once branch is confirmed clean.
- **Phase 2 (Foundational)**: Depends on Setup. T003 sequential (fixture must exist before test binary references it); T004 depends on T003; T005/T006 [P] can run concurrently after Setup (they touch different areas of `mod_why.rs`).
- **Phase 3-5 (User Stories)**: Depend on Foundational. Within a story, tests + implementation may be developed in either order (spec doesn't mandate strict TDD); the acceptance-checkpoint at the end of each phase requires both.
- **Phase 6 (Polish)**: Depends on all three user stories complete.

### User Story Dependencies

- **US1**: Independent — bumping `CHUNK_SIZE` + argv guard is self-contained in `mod_why.rs`. Can ship as MVP alone.
- **US2**: Depends on **US1** (bumping CHUNK_SIZE first means each concurrent worker does less per-workspace work → the concurrency benefit compounds). Also depends on **T005** (needs `Arc<BudgetTracker>` plumbing; the `Arc` wrap itself is in US2 but the data-model type is defined in Foundational).
- **US3**: Depends on **US2** (the caller-site refactor in T021 creates the `AnalysisJob` work-queue that T033 extends with `Scope` / `Loose` variants). Also depends on **T005 + T006** (uses `GoWorkScope` + `SharedPreflightCache` types declared in Foundational).

### Within Each User Story

- **Tests written before implementation** is recommended (matches CLAUDE.md TDD-adjacent posture) but NOT strictly required — the acceptance checkpoint gates both.
- **Constants before helpers**: T011 (CHUNK_SIZE) → T012 (ARG_MAX_SAFE) → T013 (select_chunks helper) → T014 (call-site).
- **Types before caller-site**: T005/T006 (Foundational) declare types → T031/T032 (parser + detection) populate them → T033/T034 (call-site) consume them.

### Parallel Opportunities

- **T007, T008, T009** [P] — 3 independent unit tests in the same file (`mod_why.rs::tests`). Different test names; no data dependency. Can be added in a single commit.
- **T024–T028** [P] — 5 independent unit tests for `parse_go_work` + `SharedPreflightCache`. Can be added in a single commit.
- **T005, T006** [P] — new type declarations in different regions of `mod_why.rs`. Independent.
- **T036, T039, T040** [P] — polish tasks touching different files (test run vs docs vs Cargo.lock check).

---

## Parallel Example: User Story 1 tests

```bash
# All three unit tests are independent — add in one commit, run in one invocation.
cargo test -p waybill --lib \
    mod_why::tests::argv_guard_bisects_when_projected_length_exceeds_limit \
    mod_why::tests::argv_guard_passes_normal_workload_intact \
    mod_why::tests::chunk_size_default_is_500
```

## Parallel Example: Foundational type declarations

```bash
# T005 + T006 can be authored in the same PR / commit — different structs, no dep.
grep -n "^pub(super) struct \(GoWorkScope\|SharedPreflightCache\)" \
    waybill-cli/src/scan_fs/package_db/golang/mod_why.rs
# Both should appear post-commit.
```

---

## Implementation Strategy

**MVP-first delivery**: T001 → T002 → T003 → T004 → T005/T006 → **Phase 3 (US1 only)** → merge as v0.6.2 patch. Then Phase 4 (US2) as a separate PR, then Phase 5 (US3), then Phase 6 (polish).

Each phase yields a shippable improvement — no partial state ever lands. The `--no-go-mod-why` flag (FR-010, SC-006) is the safety valve throughout: operators who hit any regression can immediately revert to skip-classifier behavior.

**PR sizing target** (post-CLAUDE.md `feedback_release_bump_prepr_slow` guidance):
- US1 PR: ~30 lines of source + ~150 lines of tests. Small, mergeable in a day.
- US2 PR: ~120 lines of source + ~200 lines of tests. Medium.
- US3 PR: ~180 lines of source (incl. `parse_go_work`) + ~300 lines of tests. Larger; may split into two PRs (parser+types first, then integration).

**Total task count**: 41 tasks across 6 phases. Independently testable per user story per plan.md.
