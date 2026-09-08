---
description: "Task list for m772 — parallelize the scan_fs shared-walker"
---

# Tasks: Parallelize the scan_fs shared-walker

**Input**: Design documents from `/specs/772-parallel-scan-walker/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md — all complete
**Tests**: Included — the milestone's acceptance criteria (SC-002 byte-identity + SC-004 determinism + SC-005 symlink safety) are inherently test-verifiable; ship tests alongside code.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel with other [P] tasks in the same phase (different files or no data dependency).
- **[Story]**: `[US1]` on user-story-phase tasks. Absent on Setup / Foundational / Polish.
- Absolute paths from repo root: `/Users/mlieberman/Projects/mikebom/…`

## Path Conventions

Single Rust workspace. Modified/new files per plan.md §Project Structure:

- `waybill-cli/src/scan_fs/walk_registry/walker.rs` — MODIFIED (split `walk_inner` + add parallel path)
- `waybill-cli/src/scan_fs/walk_registry/perf_metrics.rs` — MODIFIED (`WalkerMetrics` → atomics)
- `waybill-cli/src/scan_fs/walk_registry/mod.rs` — MODIFIED (re-exports if needed)
- `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` — MODIFIED (add any new `fn walk_*`)
- `waybill-cli/tests/walker_parallelism_772.rs` — NEW (integration tests)
- `waybill-cli/tests/fixtures/walk_registry/symlink_loop_cross_subtree/` — NEW (test fixture)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify branch state, capture pre-milestone Kubernetes baseline for regression comparison.

- [ ] T001 Confirm branch `772-parallel-scan-walker` is current and clean (`git status --short` empty besides `specs/772-…/`); confirm `cargo +stable build -p waybill --all-targets` succeeds on the pre-milestone tree.
- [ ] T002 [P] Capture pre-milestone Kubernetes wall-time baselines per quickstart.md Prerequisite block. Record BOTH `--offline sbom scan` (default) AND `--offline --no-go-mod-why sbom scan` (walker-isolated) wall-times, plus CPU utilization %. Baseline expected: default ≈ 33.4 s (738% CPU) / walker-isolated ≈ 18.7 s (99% CPU). Used later by T032/T033 for empirical validation.

**Checkpoint**: Repo is clean, baseline measurements exist for regression comparison.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type declarations + metrics-atomics migration + test fixture that US1 depends on. All independent of US1 implementation; can land as one prep PR OR bundled with US1.

- [ ] T003 [P] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/walk_registry/walker.rs`, add `SubtreeJob` struct per data-model.md §Entities (fields: `canonical: PathBuf`, `scope: Option<HashSet<ReaderId>>`). Include `#[derive(Debug, Clone)]`; visibility `pub(super)`. Add doc comment citing spec.md FR-001 + Clarification 2026-09-04 Q1 (work-stealing shape).
- [ ] T003b Restructure `SharedWalker` fields to `Arc`-wrapped shape (per analyze finding I1 — prerequisite for both T015 serial-fallback and T017 parallel path so they share field shape). Change: `visited: HashSet<PathBuf>` → `visited: Arc<Mutex<HashSet<PathBuf>>>`; `dir_index: DirIndex` → `dir_index: Arc<Mutex<DirIndex>>`; `output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>` → `output: Arc<HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>>`; `metrics: WalkerMetrics` → `metrics: Arc<WalkerMetrics>` (compatible with T004 atomic-fielded WalkerMetrics). Update `SharedWalker::new` constructor to `Arc::new(...)` each field. Update `walk_inner` (serial path) to lock through the mutex where needed — should be byte-identical semantically since the single-threaded path never contends. This restructure is depended on by T005 (metrics callers), T014 (walk_one_directory), T015 (run_serial), and T017 (run_parallel).
- [ ] T004 [P] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/walk_registry/perf_metrics.rs`, migrate `WalkerMetrics` from `&mut self` methods to `&self` methods over `AtomicU64` counter fields per research R7. Fields: `dirs_visited: AtomicU64`, `files_visited: AtomicU64`, `per_reader: HashMap<ReaderId, AtomicU64>` (pre-populated at construct so `tick_file` never needs `or_insert`). Use `Ordering::Relaxed` for `fetch_add`. Ensure `WalkerMetrics::new(&[ReaderId])` constructor initializes the per-reader map.
- [ ] T005 [US1-prep] Update every caller of `WalkerMetrics::tick_dir` / `tick_file` in the codebase to use the new `&self` signature. Expected sites: `walker.rs::walk_inner` (lines ~144, ~250-260 or wherever `self.metrics.tick_*` currently calls). This is a mechanical rename — `&mut self.metrics` becomes `&self.metrics` (Arc-shared later; for now still `SharedWalker::metrics` field which stays owned by SharedWalker).
- [ ] T006 [P] Create the cross-subtree symlink-loop fixture per research R8 + contract §5. Directory shape at `/Users/mlieberman/Projects/mikebom/waybill-cli/tests/fixtures/walk_registry/symlink_loop_cross_subtree/`:
  ```
  symlink_loop_cross_subtree/
    dir_a/
      link_to_b -> ../dir_b/    (symlink)
    dir_b/
      link_to_a -> ../dir_a/    (symlink)
  ```
  Add a README.md explaining the fixture's purpose. Symlink creation is platform-specific — use `std::os::unix::fs::symlink` in a `build.rs`-style setup script OR a `#[cfg(unix)]`-gated test-init helper that creates the symlinks at test time (avoids checking symlinks into git for cross-platform reproducibility). Windows: skip the test.

**Checkpoint**: Foundational types + metrics-atomics migration in place; symlink-loop fixture exists. Cross-caller test suite still passes at this point (metrics migration is byte-identical externally).

---

## Phase 3: User Story 1 — Parallel walker over sub-trees (Priority: P1) 🎯 MVP

**Goal**: Replace `SharedWalker`'s single-threaded recursive DFS with a work-stealing thread pool that shares state via `Arc<Mutex<...>>` for the visited-set + queue + output. Wall-time on k8s drops from ~18.7s (walker-isolated) to ≤ 5s.

**Independent Test**: quickstart.md "Validate SC-001" block. Walker-isolated (`--no-go-mod-why`) wall-time ≤ 5s; default scan wall-time ≤ 22s; CPU utilization > 300% under parallel path; single-thread fallback on tiny trees observable via CPU util ≤ 100%.

### Tests for User Story 1

- [ ] T007 [P] [US1] Unit test in `walker.rs::tests::m772_should_parallelize_gates_correctly`. Case matrix: (rootfs with 0 subdirs + N cores) → false; (rootfs with 1 subdir + N cores) → false; (rootfs with 2+ subdirs + 1 core) → false; (rootfs with 2+ subdirs + 2+ cores) → true. Regression pin for FR-001 fallback trigger + SC-006 serial-fallback observability.
- [ ] T008 [P] [US1] Unit test in `walker.rs::tests::m772_visited_set_dedup_under_concurrent_insertion`. Mock: `Arc<Mutex<HashSet<PathBuf>>>` shared across 4 threads each attempting to insert the same 100 paths in different orders. Assert final set size = 100 (exactly), no path missing. Validates R3 check-and-insert atomicity.
- [ ] T009 [P] [US1] Unit test in `walker.rs::tests::m772_subtree_job_scope_propagates`. Construct a `SubtreeJob` with `Some(HashSet<ReaderId>)` scope; verify `walk_one_directory` restricts dispatch to the named readers (mock ReaderRegistry). Validates FR-005 (m664 Contract C10 preservation).
- [ ] T010 [P] [US1] Unit test in `walker.rs::tests::m772_metrics_atomic_under_concurrent_ticks`. Spawn 8 threads each calling `tick_dir()` 1000 times on a shared `Arc<WalkerMetrics>`; assert final `dirs_visited` counter = 8000 (validates R7 AtomicU64 + Ordering::Relaxed monotonicity).
- [ ] T011 [P] [US1] Integration test in `waybill-cli/tests/walker_parallelism_772.rs::m772_cross_subtree_symlink_loop_terminates`. Runs `waybill sbom scan --path` against the T006 fixture; asserts scan exits within 5 seconds (walker MUST NOT hang). SC-005 direct verification.
- [ ] T012 [P] [US1] Integration test in `waybill-cli/tests/walker_parallelism_772.rs::m772_deterministic_emit_order_across_runs`. Run waybill against the m771 mod_why_scaling fixture twice; mask serialNumber + created; diff outputs; assert byte-identical. SC-004 direct verification.
- [ ] T013 [P] [US1] Integration test in `waybill-cli/tests/walker_parallelism_772.rs::m772_worker_panic_fails_fast`. Register a synthetic reader that panics on a specific filename in the m771 fixture; assert `waybill sbom scan` exits with non-zero status containing "panic" + worker identifier. FR-008 direct verification.

### Implementation for User Story 1

- [ ] T014 [US1] In `/Users/mlieberman/Projects/mikebom/waybill-cli/src/scan_fs/walk_registry/walker.rs`, extract the per-directory step from `walk_inner` into a new method `walk_one_directory(&self, canonical: &Path, scope: Option<&HashSet<ReaderId>>) -> Vec<SubtreeJob>`. This method: (a) checks the shared visited-set + inserts (returns empty Vec if already visited); (b) checks the ExclusionSet gate; (c) `read_dir` + iterates entries; (d) dispatches reader `on_file` callbacks per file; (e) returns newly-discovered subdirectory jobs (with propagated scope). All state mutation goes through the shared `Arc<Mutex<>>` handles at this point. Add to walker-audit allowlist per FR-011.
- [ ] T015 [US1] Refactor the existing `SharedWalker::run` to call `walk_one_directory` from a serial recursion (`fn run_serial(&mut self)`). Preserves pre-milestone behavior exactly when parallelism is not warranted (worker_count == 1 OR rootfs subdirs < 2). This is the SC-006 fallback path.
- [ ] T016 [US1] Add `fn should_parallelize(&self, first_dir_subdir_count: usize) -> bool` helper. Returns `available_parallelism() > 1 && first_dir_subdir_count >= 2`. Per research R1.
- [ ] T017 [US1] Add `fn run_parallel(&mut self)` method that: (a) constructs `SharedWalkState` per data-model.md (Arc-wrapped queue + visited + output + metrics + active_workers); (b) seeds queue with rootfs `SubtreeJob { canonical: rootfs, scope: None }`; (c) spawns `available_parallelism()` worker threads; (d) each worker loops `pop → active++ → walk_one_directory → push new subdirs → active--`; (e) drain condition: queue empty AND active_workers == 0; (f) join all handles + propagate any panic; (g) sort per-reader outputs. Add to walker-audit allowlist per FR-011. **SharedWalker → SharedWalkState transition strategy (per analyze finding I1)**: restructure `SharedWalker` fields to be `Arc`-wrapped from `SharedWalker::new()` — `visited: Arc<Mutex<HashSet<PathBuf>>>` (was `HashSet<PathBuf>`), `dir_index: Arc<Mutex<DirIndex>>` (was `DirIndex`), `output: Arc<HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>>` (was `HashMap<...>`), `metrics: Arc<WalkerMetrics>` (was `WalkerMetrics` — already atomic-fielded per T004). This lets BOTH `run_serial` and `run_parallel` operate on the same field shape (workers just `.clone()` the `Arc`s into thread bodies), and avoids the naive `&mut self.field` borrow-checker rejection that would hit workers taking multiple field handles simultaneously. The `SharedWalkState` struct in data-model.md is then simply a bundle of `.clone()`s of these fields, constructed at the top of `run_parallel` and moved into workers.
- [ ] T018 [US1] Modify `SharedWalker::run(&mut self)` dispatcher: first read rootfs to count immediate subdirs; call `should_parallelize(count)`. If true → `run_parallel()`; if false → `run_serial()` + `tracing::info!("walker: serial fallback (reason={reason})")` for SC-006 observability.
- [ ] T019 [US1] Implement FR-007 sort-at-end. After `run_parallel` / `run_serial` completes, iterate `self.output` and sort each `Vec<PackageDbEntry>` by natural key: `entry.purl.as_str()` primary; `entry.source_path` tiebreak; `entry.name` final tiebreak. Sort happens in `SharedWalker::finalize` OR at the tail of `run()` — either way runs exactly once per scan.
- [ ] T020 [US1] Add `walk.audit-allowlist.txt` entries for any new `fn walk_*` symbols introduced (`fn walk_one_directory`, potentially `fn walk_workers_drain` if extracted). Per FR-011 — CI enforces this.

**Checkpoint**: US1 done. All new unit tests pass; all new integration tests pass; existing `walk_registry_integration.rs` + `scan_go*.rs` + `cdx_regression` byte-identity tests pass. Manual quickstart.md SC-001 validation gives ≤ 5s walker-isolated on k8s.

---

## Phase 4: Polish & Cross-Cutting Concerns

**Purpose**: Full regression sweep, m669 baseline refresh, docs update, pre-PR gate.

- [ ] T021 [P] Run the full byte-identity regression suite: `cargo test -p waybill --no-fail-fast --test scan_go --test scan_cargo --test scan_python --test scan_npm --test cdx_regression --test spdx_regression --test spdx3_regression --test walk_registry_integration --test exclude_path_walker_pilot`. Assert 0 failures across every binary. Any regression = SC-002 violation; fix before ship.
- [ ] T022 Empirical SC-001 validation per quickstart.md. Warmup + measure walker-isolated (`--no-go-mod-why`) + default scan on k8s. Compare against T002 baseline. Confirm: walker-isolated ≤ 5 s (from 18.7 s); default ≤ 22 s (from 33.4 s); CPU utilization > 300% under parallel path; deterministic across two consecutive runs (SC-004).
- [ ] T023 SC-005 empirical: run the existing `walks_symlink_loop_without_hanging` regression test AND the new `m772_cross_subtree_symlink_loop_terminates` test; both MUST terminate in < 1 second (within an order of magnitude of pre-milestone m054 baseline).
- [ ] T024 SC-006 empirical: scan the m771 mod_why_scaling fixture with `RUST_LOG=info`; grep stderr for "walker: serial fallback"; confirm CPU utilization stays ≤ 100% (single-threaded path).
- [ ] T025 [P] Update `docs/perf/baseline.json` via `cargo run -p xtask -- bench --update-baseline` (m669 harness). Commit the baseline change SEPARATELY from the code change so future regression bisects can attribute wall-time shifts cleanly.
- [ ] T026 [P] Update `/Users/mlieberman/Projects/mikebom/docs/user-guide/cli-reference.md` Performance tuning section — refresh the empirical table row for kubernetes with the new post-m772 wall time. Add a note that the walker is now parallel by default.
- [ ] T027 [P] Confirm SC-003 (zero new Cargo deps): `git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml` shows no additions to any `[dependencies]` block. Cargo.lock diff empty.
- [ ] T027b [P] Confirm FR-009 (zero new operator flags — per analyze finding C1): `waybill sbom scan --help` output has zero new flag entries vs pre-milestone. Diff the flag list explicitly: `cargo run -p waybill --release -- sbom scan --help 2>/dev/null | grep -E "^\s+--" | sort > /tmp/help-post.txt`; compare against a pre-milestone reference (either recorded during T001 or extracted from git via `git show main:waybill-cli/src/cli/scan_cmd.rs | grep 'long =' | sort > /tmp/help-pre.txt` then reconciled). Zero diff expected. Cheap regression pin.
- [ ] T028 Run the full pre-PR gate: `./scripts/pre-pr.sh` (per CLAUDE.md — clippy + `cargo test --workspace`). Both MUST land clean. Per memory `feedback_prepr_gate_bails_on_first_failure` — use `--no-fail-fast` and enumerate every `^---- .+ stdout ----` line if any test fails.

**Checkpoint**: Milestone complete. All acceptance criteria (SC-001 through SC-006) empirically satisfied. Ready to open PR.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 sequential, T002 [P] can run concurrently once branch confirmed clean.
- **Phase 2 (Foundational)**: T003–T006 can all run in parallel — they touch different regions of different files. T005 (metrics-caller updates) sequential after T004 (metrics migration).
- **Phase 3 (US1)**: Depends on Foundational. Tests T007–T013 can be written before OR after implementation T014–T020 — this codebase's convention accepts either order as long as the acceptance checkpoint passes both.
- **Phase 4 (Polish)**: Depends on US1 complete.

### Task Dependencies (within Phase 3)

- **T014 (extract `walk_one_directory`)** must land before T015 / T017 (both use it).
- **T015 (serial fallback)** and **T017 (parallel path)** are independent — both call `walk_one_directory`.
- **T016 (should_parallelize helper)** can land in parallel with T015 / T017.
- **T018 (dispatcher)** requires T015 + T016 + T017.
- **T019 (sort-at-end)** requires the parallel path to exist (needs `run_parallel` complete to have a place to attach the sort).
- **T020 (walker-audit allowlist)** — add entries as soon as the corresponding `fn walk_*` function is added.

### Parallel Opportunities

- **T003, T004, T006** — foundational, different files, safe to parallelize.
- **T007–T013** — 7 test-authoring tasks, mostly independent (T007/T008/T009/T010 in `walker.rs::tests`; T011/T012/T013 in new integration test file). Can commit as one PR.
- **T021, T025, T026, T027** — polish tasks touching different artifacts (regression sweep vs baseline JSON vs docs vs Cargo.lock check).

---

## Parallel Example: User Story 1 tests

```bash
# All 4 unit tests are file-local; can commit in one change.
cargo test -p waybill --lib \
    walker::tests::m772_should_parallelize_gates_correctly \
    walker::tests::m772_visited_set_dedup_under_concurrent_insertion \
    walker::tests::m772_subtree_job_scope_propagates \
    walker::tests::m772_metrics_atomic_under_concurrent_ticks

# All 3 integration tests live in the new test binary; can commit in one change.
cargo test -p waybill --test walker_parallelism_772
```

## Parallel Example: Foundational

```bash
# T003 (SubtreeJob) + T004 (WalkerMetrics migration) + T006 (fixture) touch
# 3 different files; can be authored in the same PR / commit.
```

---

## Implementation Strategy

**Single-PR delivery** — unlike m771's 3-tier split, this milestone has one user story and one shippable increment. Bundle Setup + Foundational + US1 + Polish into one PR (~500-line code delta including tests). Rationale:

- Byte-identity regression suite (SC-002) is the safety net; passes or fails atomically.
- Parallelization changes are tightly coupled — splitting foundational (types + metrics) from implementation (parallel path) would leave main in a half-migrated state where the atomics are added but no worker uses them.
- LOC delta is moderate (~250 code + ~200 tests + fixture + docs); reviewer cognitive load is manageable.

**PR sizing target** (post-CLAUDE.md `feedback_release_bump_prepr_slow` guidance):
- Total ~500 lines net add. Larger than m771 US1 (30 lines) but smaller than m771 US3 (180 lines source + 300 lines tests).
- Review focus: the drain-loop coordination (active_workers counter pairing) and the sort-at-end determinism logic. Everything else is mechanical.

**Total task count**: 28 tasks across 4 phases. Single US shape reflects the single-shippable-slice nature of the milestone.
