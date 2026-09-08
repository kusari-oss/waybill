---

description: "Task list for m774 — parallel Go source-import collection"
---

# Tasks: Parallel Go Source-Import Collection (m774)

**Input**: Design documents from `/specs/774-parallel-source-imports/`
**Prerequisites**: plan.md (Phase 0/1 complete), spec.md (post-clarify), research.md (R1–R10), data-model.md, contracts/collect-imports-parallelism.md, quickstart.md

**Tests**: Included by design. FR-011 + SC-002 require byte-identity across every existing fixture; SC-004 requires two-run determinism verification; FR-007 requires panic-fail-fast test. One new integration test file at `waybill-cli/tests/collect_imports_parallel_774.rs` covers the milestone-specific contracts.

**Organization**: Single P1 user story (per spec.md). Phases 1–2 lay groundwork; Phase 3 is US1; Phase 4 polishes.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Story label required only in Phase 3 (US1)
- Every task cites an exact file path

## Path Conventions

Single Cargo workspace at repo root. Primary edit surface: `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`. Test file added under `waybill-cli/tests/`. No new modules, no new subdirectories.

---

## Phase 1: Setup

**Purpose**: Confirm branch state and pre-milestone baseline are captured.

- [X] T001 Confirm branch `774-parallel-source-imports` is checked out and clean via `git status --short && git branch --show-current`. If dirty, stash or commit local changes before starting.
- [X] T002 Capture pre-milestone baseline wall time for the SC-001 target fixture. Recorded: walker-isolated (`--no-go-mod-why`) = 22.99s, default = 39.48s on macOS aarch64 8-core warm cache. Binary at `/tmp/mikebom-main/target/release/waybill` (worktree). SC-001 targets: ≤ 10s walker-isolated (need -13s), ≤ 18s default (need -21s).
- [X] T003 [P] Verified `mod_why::worker_count()` at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:204` is `pub` (line 204: `pub fn worker_count(workspace_count: usize) -> usize`). No promotion needed.

---

## Phase 2: Foundational

**Purpose**: Groundwork every US1 task depends on — type definitions, test-file scaffolding.

**⚠️ CRITICAL**: No US1 implementation task begins until this phase completes.

- [X] T004 Added `WorkspaceImportJob<'a>` + `ImportCollectionResult` at file scope in `legacy.rs` (near `GoScanSignals`). `SharedImportState<'a>` was NOT introduced as a distinct struct — the three shared handles (`queue`, `tx`, `known_modules`-borrow) are captured directly by the worker closure, avoiding an unused-struct warning. `use std::sync::{mpsc, Arc, Mutex}` added. Reference: data-model.md § "New entities" — the struct is documented as an organizational aid, not a required type.
- [X] T005 **Deviation from spec**: create the 5 m774 tests as unit tests in an inline `#[cfg(test)] mod m774_tests` inside `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` (near the existing `#[cfg(test)] mod tests`) rather than a separate `waybill-cli/tests/collect_imports_parallel_774.rs` integration file. Reason: T011 (panic injection) needs library-scoped access via a `#[cfg(test)] static M774_INJECT_PANIC: AtomicBool` — integration tests link against `#[cfg(test)]=false` binary and can't reach it. Unit-test approach also gives faster iteration (no CLI subprocess build cost) and matches the pattern of the existing `collect_test_imports_records_...` tests at `legacy.rs:3887-3935`. Module scaffold added at end of `legacy.rs` after the existing test module; five `#[test]` fns with `unimplemented!()` bodies land in Phase 2 as placeholders and get filled in Phase 3c.

**Checkpoint**: Types compile in isolation; new test file compiles with `unimplemented!()` stubs. `cargo build -p waybill --all-targets` clean.

---

## Phase 3: User Story 1 - Parallel per-workspace source-import collection (Priority: P1) 🎯 MVP

**Goal**: Extract the two `collect_*_imports` calls at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2224–2240` into a bounded parallel phase running AFTER the main serial loop. Per-worker local `HashSet<String>` accumulators merged by a Phase 2 serial reduce into `signals.production_imports` + the outer `test_imports` local.

**Independent Test**: Per quickstart.md § "Step 2" — `time ./target/release/waybill --offline --no-go-mod-why sbom scan --path /tmp/test-kubernetes ...` wall time ≤ 10s (from baseline ~22.5s). Per Step 3 — `./scripts/pre-pr.sh` clean (byte-identity across all existing fixtures preserved).

### Phase 3a — Extract the two calls from the serial loop

- [X] T006 [US1] In `waybill-cli/src/scan_fs/package_db/golang/legacy.rs` at the serial loop body around lines 2224–2240, DELETE the two `collect_production_imports(project_root, 0, &known_modules, &mut signals.production_imports);` and `collect_test_imports(project_root, 0, &known_modules, &mut test_imports);` call sites from inside the `for (project_root, doc, sums) in &parsed_roots {` loop. Preserve every other line of the loop body verbatim (resolver.resolve, entries build, +incompatible filter, stamp, main-module, orphan backfill, Issue #250/#251/#255 log lines, `out.push`, `seen_purls.insert`). Reference: data-model.md diagram.

### Phase 3b — Add the post-main-loop parallel phase (Option A per Clarifications Q1)

- [X] T007 [US1] AFTER the closing `}` of the main serial loop at approximately `legacy.rs:2233`, add the parallel phase. Structure (per research R1 + R2 + R9 + data-model.md diagram):
   1. `let m774_phase_start = std::time::Instant::now();`
   2. `let worker_count = crate::scan_fs::package_db::golang::mod_why::worker_count(parsed_roots.len());`
   3. **Degenerate short-circuit** (R9): if `parsed_roots.len() <= 1`, inline the two `collect_*_imports` calls for the single workspace directly onto `signals.production_imports` and the outer `test_imports` local, then skip to step 7.
   4. Build `Vec<WorkspaceImportJob<'a>>` from `parsed_roots.iter().enumerate().map(|(i, (pr, _, _))| WorkspaceImportJob { workspace_index: i, project_root: pr }).collect()`.
   5. `std::thread::scope(|s| { ... })` block, inside which:
      - `let queue = Arc::new(Mutex::new(jobs));`
      - `let (tx, rx) = mpsc::channel::<ImportCollectionResult>();`
      - Spawn `worker_count` workers. Each worker's closure body MUST wrap the per-job body in `std::panic::catch_unwind(AssertUnwindSafe(...))` so FR-007's workspace-path logging can fire before propagation (per remediation F1 + research R2 revised). Shape:
         ```rust
         for _ in 0..worker_count {
             let queue = Arc::clone(&queue);
             let tx = tx.clone();
             s.spawn(move || {
                 loop {
                     let job = match queue.lock().unwrap().pop() {
                         Some(j) => j,
                         None => break,
                     };
                     let project_root_display = job.project_root.display().to_string();
                     let workspace_index = job.workspace_index;
                     let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                         let mut prod = std::collections::HashSet::new();
                         let mut test = std::collections::HashSet::new();
                         collect_production_imports(job.project_root, 0, known_modules, &mut prod);
                         collect_test_imports(job.project_root, 0, known_modules, &mut test);
                         (prod, test)
                     }));
                     match result {
                         Ok((prod, test)) => {
                             let _ = tx.send(ImportCollectionResult {
                                 workspace_index,
                                 production_imports: prod,
                                 test_imports: test,
                             });
                         }
                         Err(payload) => {
                             tracing::error!(
                                 workspace_index,
                                 project_root = %project_root_display,
                                 "m774 worker panicked in collect_*_imports for this workspace; propagating to abort scan"
                             );
                             std::panic::resume_unwind(payload);
                         }
                     }
                 }
             });
         }
         ```
      - After the spawn loop, `drop(tx);` (main-side sender dropped BEFORE reduce so `rx.recv` termination unblocks when all worker tx clones are dropped by worker exit).
      - DRAIN the mpsc synchronously INSIDE the scope: `for result in rx { signals.production_imports.extend(result.production_imports); test_imports.extend(result.test_imports); }`. When a worker panicked (per catch_unwind Err arm above), that worker's `resume_unwind` propagates through `std::thread::scope`'s automatic join at scope-close — main thread's reduce completes (or is mid-iteration when the first worker panics; either way scope join re-raises). This preserves FR-007 fail-fast without needing explicit `ScopedJoinHandle::join()` handling on the main side.
   6. (Reduce happens inside the scope, per step 5.)
   7. `signals.test_only_imports = test_imports.difference(&signals.production_imports).cloned().collect();` — reuse the EXACT pre-milestone line at `legacy.rs:2281`. This line MUST already exist in the current code; do NOT duplicate. Verify with `grep -n "test_only_imports.*difference" waybill-cli/src/scan_fs/package_db/golang/legacy.rs` — should return the existing single occurrence.
   8. FR-014 summary log per contracts/collect-imports-parallelism.md § "Contract 8": `tracing::info!(workspaces_scanned = parsed_roots.len(), parallel_workers_used = worker_count, production_imports_count = signals.production_imports.len(), test_imports_count = test_imports.len(), elapsed_ms = m774_phase_start.elapsed().as_millis() as u64, "m774 parallel source-import collection complete");`.

- [X] T008 [US1] Verify the compiled binary via `cargo +stable build -p waybill --all-targets`. Zero errors, zero warnings on the touched module. If unused-import warnings appear on `Arc`/`Mutex`/`mpsc` in the single-workspace degenerate arm, scope the `use` statements INSIDE the multi-workspace arm's block (per quickstart § "Troubleshooting").

### Phase 3c — Implement the 5 integration tests

- [X] T009 [US1] Fill `m774_multi_workspace_merge_correctness` in `waybill-cli/tests/collect_imports_parallel_774.rs`. Construct a `tempfile::tempdir()` with 3 synthetic go.mod workspaces per research R10: each has `go.mod` declaring `module github.com/kusari-oss/waybill-fixture-m774-wsN` (N=1,2,3), 10-20 `.go` production files importing 2-3 modules from a known-module list (`github.com/kusari-oss/waybill-fixture-m774-lib-{a,b,c,d,e}`), and 2-3 `_test.go` files importing 1-2 additional test-only modules (`github.com/kusari-oss/waybill-fixture-m774-testonly-{x,y}`). Invoke the reader path via a public entry point (probably `waybill::scan_fs::package_db::golang::read` or an internal `pub(crate)` helper — check what the existing `golang_transitive_*` tests use). Assert: `signals.production_imports` contains every prod library, `signals.test_only_imports` contains every testonly library minus any also in prod. Use synthetic package-name convention per memory `feedback_fixture_synthetic_package_names`.
- [X] T010 [US1] [P] Fill `m774_determinism_across_runs` in `waybill-cli/tests/collect_imports_parallel_774.rs`. Reuse the 3-workspace fixture from T009 (extract a `fn make_three_workspace_fixture() -> TempDir` helper at the top of the test file). Run the scan twice against the same fixture, capture the resulting `Vec<PackageDbEntry>` (or serialize to CDX and mask serialNumber+timestamp per the m669 protocol). Assert the two runs' outputs are byte-identical after masking.
- [X] T011 [US1] [P] Fill `m774_worker_panic_fails_fast` in `waybill-cli/tests/collect_imports_parallel_774.rs`. Per research R8: check whether `collect_*_imports` naturally panics on any input the test can construct. If NOT (likely), add a `#[cfg(test)]`-gated panic-injection point at the top of `collect_production_imports` in `legacy.rs` guarded by `thread_local! { pub static M774_INJECT_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }`. In the test: build a 3-workspace fixture, set the panic-injection flag, invoke the reader path, assert the result is `Err(_)` (or the invocation `panic!`s if the read path panics-through). Assert `tracing::error!` line with `worker_idx` is captured via `tracing_subscriber::fmt::Layer` + `tracing_subscriber::registry()::with(...)`. **Note**: prefer the natural-panic path if any exists; injection is fallback per R8.
- [X] T012 [US1] [P] Fill `m774_single_workspace_no_thread_spawn` in `waybill-cli/tests/collect_imports_parallel_774.rs`. Construct a 1-workspace fixture (single `go.mod` + 10 `.go` files). Invoke the reader path 5 times, capture wall time via `std::time::Instant`. Assert every invocation's wall time is under a permissive bound (say, 500ms) — the assertion's real value is preventing regression from a future refactor that accidentally enters the `std::thread::scope` block for `parsed_roots.len() == 1`. Use `debug_assert!(m774_phase_start.elapsed() < Duration::from_millis(50))` in the DEGENERATE arm of the production code (T007 step 3) as a paired invariant check.
- [X] T013 [US1] [P] Fill `m774_summary_log_fires_once_per_read` in `waybill-cli/tests/collect_imports_parallel_774.rs`. Reuse the 3-workspace fixture. Set up a `tracing_subscriber` with a `Vec<u8>` writer buffer. Run the scan once. Assert exactly one line in captured output matches the pattern `"m774 parallel source-import collection complete"` AND has non-zero `workspaces_scanned` + `parallel_workers_used` fields.

### Phase 3d — Empirical validation against the SC-001 target

- [X] T014 [US1] Run `cargo build -p waybill --release` to produce the post-milestone binary. Then execute quickstart.md § "Step 2" verbatim: `time ./target/release/waybill --offline --no-go-mod-why sbom scan --path /tmp/test-kubernetes --no-deep-hash --format cyclonedx-json --output /tmp/m774-scan.cdx.json`. Record wall time; assert ≤ 10s (SC-001 walker-isolated). Then run the default variant (Step 2b): assert ≤ 18s (SC-001 default). If either target is missed, escalate per quickstart § "Rollback triggers".
- [X] T014b [US1] SC-007 empirical byte-identity check for `--no-go-mod-why`. Using the pre-milestone binary at `/tmp/mikebom-main/target/release/waybill` (from T002's worktree) and the post-milestone binary at `./target/release/waybill`, run BOTH against `/tmp/test-kubernetes` with `--no-go-mod-why` set, capture CDX + SPDX 2.3 + SPDX 3 outputs to distinct paths. Mask `serialNumber` + `metadata.timestamp` (CDX) / `creationInfo.created` + `documentDescribes` order-sensitive fields (SPDX 2.3) / `creationInfo` (SPDX 3) per the m669 protocol (memory: `feedback_cross_host_goldens`; `feedback_verify_golden_churn_normalized`). Diff the masked outputs. Zero diff = SC-007 satisfied. If diff: investigate — the `--no-go-mod-why` short-circuit path is now interacting with the parallel imports phase incorrectly. Reference: spec.md SC-007 + Contract 9.
- [X] T015 [US1] Run quickstart.md § "Step 4" — SC-004 determinism test on the same fixture. Two independent scans, masked diff should be zero.
- [X] T016 [US1] Run quickstart.md § "Step 5" — SC-005 single-workspace overhead test. `hyperfine` p50 must be within ±3% of pre-milestone baseline (captured in T002 or via the T002 worktree). If overhead exceeds 3%, the R9 degenerate short-circuit isn't firing correctly; debug the arm-shape.

**Checkpoint**: US1 delivers the milestone's shipped value. `./scripts/pre-pr.sh` MUST run clean before merging; SC-001/SC-004/SC-005 empirical checks pass.

---

## Phase 4: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, memory updates, final gates.

- [X] T017 [P] `./scripts/pre-pr.sh` background run completed clean (`>>> all pre-PR checks passed.`). Interactive foreground run hit the documented m203 helm-timing flake (`m203_us2_5_env_var_override_shortens_timeout` — memory `reference_m203_helm_test_flake`); background run raced through it cleanly. No m774-attributable failures.
- [ ] T018 [P] Update the memory index at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/MEMORY.md` with a new one-line entry: `- [m774 parallel source-imports collection](reference_m774_parallel_source_imports.md) — extracts collect_*_imports pair from per-workspace loop into post-main-loop parallel phase; m771 US2 pattern; k8s ~22.5s→~10s (walker-isolated), ~34s→~18s (default)`. Then create the memory file `reference_m774_parallel_source_imports.md` with type: reference, describing the parallelization site, the m771 pattern reuse, the R6 rationale (why resolver stays serial), and the R9 degenerate short-circuit. Content per memory-format spec in `~/.claude/CLAUDE.md`.
- [ ] T019 [P] Delete the scratch profiling branch `scratch-m774-profile` via `git branch -D scratch-m774-profile` (safe: no shipped code depends on it; instrumentation was reference-only per spec Assumption "The m774 profiling instrumentation ... is scratch-only").
- [ ] T020 Commit the milestone changes with message `wip(m774): parallel Go source-import collection (US1 T001-T020)` (matches the current WIP naming convention on this branch per `git log` for m771/m772/m773). Do NOT push yet — user reviews before push.
- [ ] T021 [P] Open the draft PR against `main` with title `perf(m774): parallel Go source-import collection` and body citing SC-001 (before/after wall time), SC-002 (regression suites green), SC-003 (zero new deps), SC-004 (determinism verified), SC-005 (no single-workspace overhead), SC-006 (pre-pr clean), SC-007 (--no-go-mod-why byte-identity). Cite the m774 profiling table verbatim in the "Motivation" section. Reference issues #793 + PR #794 (perf-methodology extension).

---

## Dependencies

Phase 1 → Phase 2 → Phase 3 → Phase 4 (strict sequencing).

Within Phase 3:
- T006 blocks T007 (extraction must precede parallel-phase build)
- T007 blocks T008 (need working code to verify build)
- T008 blocks T009–T013 (need working binary + working types to write tests)
- T009 blocks T010, T012, T013 (fixture helper `make_three_workspace_fixture` reused)
- T010, T011, T012, T013 can run in parallel [P] once T009's helper is in place
- T014, T014b, T015, T016 depend on T008 + T009 (need working binary AND passing base test); T014b additionally depends on T002 (needs pre-milestone binary from worktree)

Within Phase 4:
- T017, T018, T019 can run in parallel [P]
- T020 depends on T017 passing (don't commit if pre-pr is red)
- T021 depends on T020 (need commit to open PR)

## Parallel execution examples

**During Phase 3c**, after T009 lands the fixture helper:
```
Task T010, T011, T012, T013 in parallel — 4 tests, 4 test bodies, one file
```
(All four modify `waybill-cli/tests/collect_imports_parallel_774.rs`, so serialize the actual EDIT operations if using a single-file constraint. If they can be co-located in a single commit, batch them.)

**During Phase 4**:
```
Task T017 (pre-pr) + T018 (memory update) + T019 (branch cleanup) in parallel
```

## Implementation strategy (MVP-first)

**MVP scope**: US1 completed = m774 shipped. There is exactly one user story; no partial-MVP option exists at the spec level.

Recommended order for the implementer:
1. Phases 1–2 (T001–T005): ~30 min. Sets up baselines + types + test stubs.
2. Phase 3a–3b (T006–T008): ~2 hours. The load-bearing production edits + build.
3. Phase 3c (T009–T013): ~3 hours. Test bodies. Slower because the fixture-builder helper needs care.
4. Phase 3d (T014–T016): ~30 min. Empirical validation on the k8s fixture. If SC-001 misses, escalate per quickstart § "Rollback triggers".
5. Phase 4 (T017–T021): ~30 min. Docs + PR.

Total estimate: ~6.5 hours of focused work. First iteration MIGHT surface an R8 gotcha (natural-panic-path availability) or an R9 overhead surprise; budget +1 hour for iteration.
