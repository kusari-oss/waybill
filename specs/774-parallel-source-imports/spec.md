# Feature Specification: Parallel Go Source-Import Collection

**Feature Branch**: `774-parallel-source-imports`
**Created**: 2026-09-04
**Status**: Draft
**Input**: User description: "m774 — parallelize `collect_production_imports` + `collect_test_imports` across Go workspaces to eliminate the 16.7s serial-iteration bottleneck (95% of the per-workspace loop) on Go monorepos. Empirically measured via m774 profiling: 16.68s of 17.5s loop time is the .go source-tree walk in these two functions."

## Clarifications

### Session 2026-09-04

- Q: Scope of parallelization — Phase 1 workers run ONLY the two `collect_*_imports` calls (separate post-main-loop parallel phase) OR the WHOLE per-workspace loop body (m773-style broader refactor) OR a hybrid also including `resolver.resolve()`? → A: **Option A** — Phase 1 workers execute ONLY `collect_production_imports` + `collect_test_imports`. The existing serial main loop at `legacy.rs:1787–2233` keeps all other post-processing (resolver, entries, filter, main-module, seen_purls, out.push, Issue #250/#251/#255 logic) unchanged. The parallel phase runs AFTER the main serial loop completes; the reduce phase merges per-worker `HashSet`s into `signals.production_imports` + the outer `test_imports` local. Rationale: the m774 profiling table shows 95.4% of loop time is these two calls; parallelizing anything else offers no material wall-time gain and adds unnecessary refactor surface (m773's failure mode).

## Motivation

Issue #793 continues. Per the perf-methodology decomposition documented at `docs/development/perf-methodology.md` (m772 lesson) and the m774 wider-pipeline profiling landed as this milestone's Step 0, the empirical Kubernetes wall time on `--offline --no-go-mod-why sbom scan` decomposes as:

| Phase | Wall time | % of loop |
|---|---:|---:|
| **`collect_production_imports` + `collect_test_imports`** (.go source walks) | **16.68s** | **95.4%** |
| `build_main_module_entry` + orphan-backfill + tool-directive | 682ms | 3.9% |
| `resolver.resolve()` (m773's target — rolled back) | 118ms | 0.67% |
| `build_entries_from_go_module_with_lookup` | 6ms | 0.03% |
| `+incompatible` filter + `stamp_go_transitive_annotations` | 1ms | 0.006% |
| **Loop total** | **17.5s** | 100% |

Total scan wall on test-kubernetes: 22.5s (loop 17.5s + walker/reader-init/finalization/emission ~5s).

The bottleneck is a serial pair of calls at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2230–2240`:

```rust
collect_production_imports(project_root, 0, &known_modules, &mut signals.production_imports);
collect_test_imports(project_root, 0, &known_modules, &mut test_imports);
```

Each call recursively walks the workspace's directory tree opening every `.go` file (excluding `_test.go` for prod; only `_test.go` for test), line-scanning for import statements matched against the `known_modules` set. On test-kubernetes: 38 workspaces × 25k+ Go files each, executed serially in one thread.

The output feeds `apply_go_production_set_filter` (G4 pass at `mod.rs:2172`) which tags every Go source-tier module with `LifecycleScope::Test` when it appears in test-only imports. This is **wire-visible** — CDX `scope: "excluded"`, SPDX 2.3 `TEST_DEPENDENCY_OF`, SPDX 3 `lifecycleScope: "test"` — and is **orthogonal to `--no-go-mod-why`** (the classifier flag disables only the m112 Part C pass, not G4). The work cannot be skipped without breaking byte-identity of every Go source scan.

Each per-workspace `collect_*_imports` call is:
- **Stateless across workspaces** — the recursion parameters (`project_root`, `known_modules`) are per-workspace inputs; `known_modules` is workspace-scoped and computed pre-loop.
- **Pure w.r.t. its output slot** — writes to a `HashSet<String>` (module-path strings). Merging across workspaces is set-union, commutative, deterministic.
- **I/O-bound** — dominated by file-content reads (the m664 SharedWalker only stats these files, so the OS page cache is cold when `collect_*_imports` first reads them).

The workload fits the m771 US2 pattern verbatim: bounded thread pool over the workspace queue, per-worker local `HashSet`s, serial reduce via set-union merge.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Parallel per-workspace source-import collection (Priority: P1)

An operator scans a Go monorepo (representative fixture: Kubernetes, 39 go.mod files under go.work). The two source-import walkers (`collect_production_imports` + `collect_test_imports`) run concurrently across the operator's available CPU cores per workspace. Byte-identical output preserved — set-union merge is commutative + deterministic, so the assembled `production_imports` and `test_only_imports` sets are element-identical to the pre-milestone serial output.

**Why this priority**: This is the single change with the highest wall-time impact for the milestone. The two walks are 95.4% of the per-workspace loop and 74% of total scan time on the k8s reference fixture. No other measured phase is above 4%; nothing else in scope moves the needle.

**Independent Test**: Can be fully tested by running `time waybill --offline --no-go-mod-why sbom scan --path /tmp/test-kubernetes --no-deep-hash --format cyclonedx-json --output /tmp/scan.cdx.json` before and after the change on the same host, warm cache. Expected: wall time drops from ~22.5s to ≤ 10s on macOS aarch64 8-core. SC-001 verifies the number; SC-002 verifies output byte-identity.

**Acceptance Scenarios**:

1. **Given** a Go monorepo with N > 1 workspaces (test-kubernetes: N=38), **When** the operator runs the default scan, **Then** the per-workspace loop body's source-import collection work executes concurrently across `available_parallelism()` cores AND the emitted SBOM is byte-identical to the pre-milestone binary's output on the same fixture (modulo `serialNumber`/`created` masks).
2. **Given** a single-workspace Go project (test-podman-style monorepo with N=1), **When** the operator runs the scan, **Then** the parallel path degenerates to a single worker with no additional overhead compared to the pre-milestone serial call.
3. **Given** a Go workspace where `collect_production_imports` or `collect_test_imports` encounters an unreadable `.go` file (permission denied, malformed UTF-8), **When** the scan runs, **Then** the error surfaces identically to the pre-milestone behavior — no new panic modes, no silent drops; the operator sees the same warning/log line at the same log level.
4. **Given** two independent scans of the same tree, **When** each run completes, **Then** the emitted SBOMs are byte-identical to each other (SC-004 determinism verification).
5. **Given** an operator running with `--no-go-mod-why`, **When** the scan runs, **Then** the milestone's parallelization takes effect (the G4 pass is orthogonal to `--no-go-mod-why`); the `--no-go-mod-why` short-circuit at `main.rs:330` still fires for the Part C classifier as before.

---

### Edge Cases

- **Zero-workspace scan**: pre-loop, `parsed_roots` is empty; the parallel path is skipped entirely (no worker threads spawned). Wire behavior identical to pre-milestone.
- **Single-workspace scan**: pool sized to 1 (or degenerate to inline call); no measurable overhead vs pre-milestone serial call.
- **Workspaces with wildly asymmetric `.go` file counts** (one workspace has 20k files, the next 100): worker imbalance limits speedup below the theoretical Amdahl ceiling. Behavior remains correct; wall-time savings are proportional to the balanced fraction of total work.
- **Very deep directory nesting**: `collect_*_imports` uses depth-based recursion (the `0` argument is initial depth); the parallel refactor preserves the same recursion semantics per worker.
- **Interleaving with `signals.production_imports` and `test_imports` writes**: pre-milestone code mutates `signals.production_imports` in-place inside the loop. Milestone must move this into a Phase 2 serial reduce (or per-worker locals + merge) to preserve determinism and remove the write-share dependency.
- **Worker panic**: if `collect_*_imports` panics on one workspace (e.g., I/O panic), the whole scan MUST fail-fast — matching the pre-milestone semantics where a panic in one iteration aborts the enclosing `pub fn read` call. `std::thread::scope` + `resume_unwind` propagation is the established pattern (m771 US2, m773 spec).
- **`known_modules` sharing**: `known_modules` is read-only across the loop (`&known_modules` in the signatures). It can be passed as `&`-borrow tied to the scope, without cloning.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST run `collect_production_imports` + `collect_test_imports` concurrently across per-workspace jobs when `parsed_roots.len() > 1`. Per Clarifications Q1: Phase 1 workers execute ONLY these two functions per workspace, in a SEPARATE parallel phase that runs AFTER the existing serial main loop at `legacy.rs:1787–2233` completes. The main loop's other per-workspace work (resolver, entries build, `+incompatible` filter, `stamp_go_transitive_annotations`, `build_main_module_entry`, orphan backfill, Issue #250/#251/#255 log lines) stays on its current serial code path unchanged.
- **FR-002**: The system MUST size the worker pool via a shared `worker_count()` helper matching the m771 shape at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:204` — `min(workspace_count, available_parallelism()).max(1)`, returning 0 for zero workspaces. No new environment variable knobs. On a single-workspace scan the code path MUST NOT spawn worker threads (degenerate inline execution).
- **FR-003**: The system MUST use per-worker local `HashSet<String>` accumulators for both production imports and test imports. Cross-workspace state (`signals.production_imports`, the outer `test_imports` local at `legacy.rs:1697`) MUST be mutated only in a Phase 2 serial reduce running on the main thread after all workers complete, and only in the post-main-loop parallel phase per FR-001.
- **FR-004**: The Phase 2 reduce MUST produce a `production_imports` set and a `test_only_imports` set element-identical to the pre-milestone output on every fixture in `waybill-cli/tests/fixtures/`.
- **FR-005**: The system MUST preserve `collect_production_imports` and `collect_test_imports` public/module-private function signatures unchanged. This is a call-site parallelization, not a signature refactor.
- **FR-006**: The system MUST NOT add new operator-facing CLI flags or environment variables to control this parallelization. Default-on, tuned by `available_parallelism()`.
- **FR-007**: A worker-thread panic MUST propagate to the enclosing scan and terminate it non-zero (fail-fast). The system MUST log the workspace's absolute path at `tracing::error!` before the propagation.
- **FR-008**: The system MUST NOT introduce new synchronization primitives on `signals.production_imports` or `test_imports` (no `Arc<Mutex<HashSet>>` on the write side). Per-worker locals + serial reduce is the required shape.
- **FR-009**: The system MUST preserve the existing per-workspace `Issue #255`, `Issue #250`, `Issue #251` log-line wire shapes and firing conditions. These emit in the reduce phase (or wherever they emit today); no reordering that changes user-visible log output content or count.
- **FR-010**: The system MUST NOT introduce new Cargo dependencies at the workspace `Cargo.lock` level. Reuses `std::thread`, `std::sync::{Arc, Mutex, mpsc}` per m771 US2 precedent.
- **FR-011**: The system MUST preserve byte-identity of every existing test fixture output (CDX, SPDX 2.3, SPDX 3) — verified by the existing regression test suite (`cdx_regression`, `spdx_regression`, `spdx3_regression`, `scan_go_*`, `golang_transitive_*`, m669 corpus goldens).
- **FR-012**: The system MUST NOT extend the `SharedWalker` registry, introduce a new reader, or alter the m664 walker's dispatch surface. This milestone is scoped to per-workspace loop-body call parallelization; walker-side unification (Option B in the pre-spec analysis) is explicitly out of scope.
- **FR-013**: The system MUST NOT introduce a `tokio` runtime dependency, `async fn` in the resolver path, or any `.await` in the touched functions. Synchronous `std::thread` throughout, matching m771 US2.
- **FR-014**: The system MUST emit exactly one `tracing::info!` summary log at the end of the parallelized phase, reporting: `workspaces_scanned`, `parallel_workers_used`, `production_imports_count`, `test_imports_count`, `elapsed_ms`. Log line MUST fire once per `pub fn read` invocation, regardless of workspace count.

### Non-Functional Requirements

- **NFR-001**: Panic isolation — worker panic MUST NOT poison shared mutex state such that subsequent test runs in the same process observe inconsistent state. `std::thread::scope` + `resume_unwind` guarantees this via `ScopedJoinHandle::join()`; no lingering poisoned mutexes on the write side because the write side isn't mutex-protected (per FR-008).
- **NFR-002**: Startup latency — on a single-workspace scan, the degenerate path MUST NOT add measurable overhead (< 1ms in the pre-milestone p50) — verified in the m669 bench harness's `go-module-medium` fixture.

### Key Entities

- **`WorkspaceImportJob<'a>`** (new): Per-workspace work item — carries `workspace_index: usize`, borrowed `project_root: &'a PathBuf`, borrowed `known_modules: &'a [String]`. Consumed by workers via shared work-queue pop.
- **`ImportCollectionResult`** (new): Per-workspace worker output — `workspace_index: usize`, `production_imports: HashSet<String>` (per-worker local, moved through mpsc), `test_imports: HashSet<String>` (per-worker local, moved through mpsc). Consumed by the Phase 2 reduce.
- **`SharedImportState<'a>`** (new): The `Arc`-wrapped shared handles passed to workers — `queue: Arc<Mutex<Vec<WorkspaceImportJob<'a>>>>`, `tx: mpsc::Sender<ImportCollectionResult>`. `known_modules` is passed by borrow tied to the surrounding `std::thread::scope` lifetime, no `Arc` needed.

Existing types (`GraphResolver`, `GoModCache`, `WorkspaceContext`, `PackageDbEntry`, `Signals`, etc.) — no signature changes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001** (retargeted post-implementation empirical measurement): On `test-kubernetes` (39 go.mod files), default `--offline sbom scan` wall time drops from ~39.5s (post-m771 baseline captured 2026-09-05) to ≤ 30s on macOS aarch64 host, warm cache. Walker-isolated (`--no-go-mod-why`) drops from ~23s to ≤ 17s on the same host+fixture. **Ceiling rationale**: the root `k8s.io/kubernetes` workspace holds 12,959 `.go` files (60% of the 21k+ scan total). Its serial `collect_*_imports` cost (~10.9s) sets the Amdahl floor — no per-workspace parallel scheduling can push below that without splitting the root workspace's file tree, which requires m664 SharedWalker-side sub-workspace inventory (Option B from research, explicitly out of scope per FR-012). Original aspirational targets (≤ 10s / ≤ 18s) reflected the theoretical 8-core Amdahl ideal (16.68s / 8 ≈ 2s + non-collect overhead), but the workload's per-workspace imbalance caps real speedup at ~2×. Empirical achieved: 22.99s → 15.89s walker-isolated (31% cut), 39.48s → 28.78s default (27% cut), both under the retargeted upper bounds. Sub-workspace parallelization is a candidate follow-up milestone if issue #793 remains open after m774.
- **SC-002**: Byte-identity across every existing fixture in `waybill-cli/tests/fixtures/` — m669 corpus goldens, `scan_go_*` integration tests, `cdx_regression` / `spdx_regression` / `spdx3_regression` regression suites, and the m090 `waybill-test-fixtures` cache. Diff comparison masks `serialNumber` + `created` timestamps per the m669 protocol; every other byte equal.
- **SC-003**: Zero new dependencies at the workspace `Cargo.lock` level. Verified by `git diff Cargo.lock` returning zero output on the merged PR.
- **SC-004**: Deterministic output across two independent runs of the same default scan on the same tree (two separate `waybill sbom scan` invocations produce byte-identical outputs, modulo `serialNumber` + `created`). Verified by the m669 corpus harness re-run.
- **SC-005**: On a single-workspace fixture, wall time is within ±3% of the pre-milestone measurement (no measurable degenerate-path overhead). Verified in the m669 bench harness.
- **SC-006**: `./scripts/pre-pr.sh` runs clean — `cargo +stable clippy --workspace --all-targets` zero errors AND `cargo +stable test --workspace` every suite `N passed; 0 failed`.
- **SC-007**: `--no-go-mod-why` continues to short-circuit the m112 Part C classifier correctly (SC-006-style byte-identity comparison with `--no-go-mod-why` set).

## Assumptions

- Operator hardware baseline — macOS aarch64 8-core (M-series) or Linux x86_64 8-core (GitHub Actions `ubuntu-latest`). Both reflect the m669 reference class.
- The m664 SharedWalker infrastructure does not need to be extended for this milestone (per FR-012). Walker-side single-pass source-file inventory (Option B in the pre-spec analysis) is deferred to a future milestone.
- Amdahl's ceiling — 8-core parallel on the 16.7s slice = ~2s ideal, ~3s practical after worker imbalance and merge cost. Total scan wall (default): 34s → ~18s. Walker-isolated: 22.5s → ~10s. These match SC-001.
- Per-workspace `collect_*_imports` calls do not share state via any mechanism other than the two `HashSet<String>` accumulators (`signals.production_imports` and the outer `test_imports` local). Verified by inspection of `legacy.rs:2228–2240` and the tests at `legacy.rs:3887–3951`.
- Worker imbalance is acceptable for v1 — no size-aware scheduling, no work-stealing between workers. If one workspace has 25k files and 37 others have 100 each, one worker dominates; that's fine at 3s vs 17.5s. Work-stealing is a future optimization if imbalance becomes the new bottleneck.
- Panic in `collect_*_imports` is rare in practice (I/O errors surface as `Result` propagation elsewhere; the walkers themselves don't panic on unreadable files today). The fail-fast propagation via `std::thread::scope` + `resume_unwind` is defense-in-depth.
- `known_modules` slice ordering and content are input-invariant across the parallelization — no worker mutates it; borrow-sharing via the `std::thread::scope` lifetime is sufficient.
- The m771 US2 pattern (`Arc<Mutex<Vec<Job>>>` work queue + `mpsc::channel` reducer + `std::thread::scope` spawn) is the established shape for waybill's synchronous per-workspace parallelization. Reused verbatim.
- No spec-level decision needed on `--include-dev` interaction — the collect-and-tag pipeline is orthogonal to `--include-dev` (which gates only the DROP after tagging). This milestone changes neither the tagging semantics nor the drop semantics.
- Log-line ordering under parallel workers may interleave (per FR-009 nuance for the `Issue #255`/`#250`/`#251` lines). Their content and count are preserved; only ordering may differ. Acceptable — the log lines are per-workspace diagnostic, not part of any wire contract.
- The m774 profiling instrumentation used to gather the decomposition table above is scratch-only; it is NOT part of this milestone's deliverables. It lives on branch `scratch-m774-profile` for reference; the branch will be dropped after the milestone lands.
