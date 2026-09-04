# Phase 0 Research — m771 `go mod why` subprocess scaling

**Feature**: 771-gomodwhy-subprocess-scale
**Status**: Complete
**Date**: 2026-09-04

The clarify step (see spec.md §Clarifications) resolved the only high-impact ambiguity (US3 preflight working directory = `go.work` file's parent). The remaining research items are technical-decision confirmations to lock in before implementation. Each entry follows the Decision / Rationale / Alternatives format.

---

## R1 — CHUNK_SIZE selection (US1)

**Decision**: Set `CHUNK_SIZE = 500`. Compile-time constant, no env-var override.

**Rationale**:
- Empirical shape check: 246 modules × ~50 char average path (typical `github.com/owner/repo/v2` shape) = ~12 KB argv. POSIX ARG_MAX minimum is 128 KiB (macOS + Linux both far above; Linux is 2 MiB, macOS is 1 MiB per `sysctl kern.argmax`). A chunk of 500 at 100-char paths = 50 KB argv, still 2.5× under the tightest supported floor with margin.
- Reduces Kubernetes-fixture per-workspace subprocess count from 13 → 1 (`ceil(246/500) = 1`); combined with the shared preflight (US3), each workspace collapses to 1 subprocess.
- No env-var override: keeps operator surface flat; the argv-length guard (R2) auto-splits if a projected chunk approaches ARG_MAX. Env-var configurability adds test surface and doc surface for zero user demand.
- Chunk-size does not change `go mod why -m` output semantics: `go mod why -m foo bar baz` returns the same three sections as three separate invocations, just faster. Verified against the existing `parse_go_mod_why` output shape (multi-section handling already covered by the `multi_section_output` test at `mod_why.rs:548+`).

**Alternatives considered**:
- **CHUNK_SIZE = 1000**: marginal further win on very-large single-workspace fixtures (not present in canonical corpus); rejected because 500 already reduces k8s to 1 chunk per workspace.
- **CHUNK_SIZE = 100** (5× current, mid-ground): rejected because it doesn't get k8s to 1-chunk-per-workspace, so US2 concurrency still contends with 2-3 chunks per worker.
- **Env-var override**: rejected as above; the argv guard is defense-in-depth without user surface.

---

## R2 — Argv-length guard algorithm (US1 support)

**Decision**: Before invoking `go mod why -m <paths...>`, compute the projected argv length as `strlen("go") + strlen("mod") + strlen("why") + strlen("-m") + strlen("-vendor") + sum(strlen(path) + 1) for path in batch`. If the projected length exceeds `ARG_MAX_SAFE = 96 KiB` (75% of the POSIX 128 KiB floor to leave headroom for env vars, working-dir, executable path), bisect the batch into two halves and recurse. Log a `debug!` line naming the split.

**Rationale**:
- 96 KiB is a conservative envelope. macOS `sysctl kern.argmax = 1048576` and Linux `ulimit -s / 4` (glibc computes ARG_MAX as `stack_size / 4`) both provide 6-8× more headroom in practice, but the operator's environment may impose lower limits (e.g., some Docker/podman configurations shrink `RLIMIT_STACK`). 96 KiB matches the safe envelope other Go tooling (goimports, gopls) uses internally.
- Bisection is stable and terminating: worst-case log₂(500) = 9 recursions before individual paths are argument-list-single. Individual paths cannot exceed POSIX PATH_MAX (4 KiB) so single-path recursion is always safe.
- Runtime cost is negligible: byte-length summation over 500 paths is O(500) ≈ microseconds.

**Alternatives considered**:
- **Query `sysconf(_SC_ARG_MAX)` at runtime**: adds a `libc` dependency (or hand-rolled FFI). Rejected — 96 KiB constant is both simpler and safer.
- **Trust `CHUNK_SIZE` unconditionally**: rejected because a future spec-drift (larger paths, or an operator override) could silently exceed ARG_MAX and hit `E2BIG` errno, which currently surfaces as an opaque `SpawnFailed` warning without a repair suggestion.

---

## R3 — Concurrency model for US2

**Decision**: Bounded thread pool using `std::thread::available_parallelism()` for the cap, coordinated via `std::sync::mpsc` for result collection. NO `rayon`, NO `tokio`, NO `crossbeam`.

**Rationale**:
- Zero new Cargo deps (FR-013). Rayon / tokio / crossbeam are not workspace deps for waybill's non-tracing paths.
- Matches the existing golang-resolver parallel-fetch pattern at `graph_resolver.rs:1001` (spawn-N-threads + mpsc channel + join). Consistent with what the codebase already does; reviewer familiarity is high.
- `available_parallelism()` was stabilized in Rust 1.59, well below waybill's MSRV. Returns `NonZeroUsize`, so no divide-by-zero handling needed.
- Concurrency shape: given N workspaces to analyze, spawn `min(N, cap)` worker threads. Each worker pulls the next workspace from a `Arc<Mutex<Vec<Workspace>>>` work queue. The mpsc collects per-workspace `MainModuleAnalysis` structs. Main thread joins all workers and merges verdicts.
- Subprocess concurrency: capped naturally by the worker count (one `go` subprocess in-flight per worker via `run_bounded` which is synchronous). No separate subprocess semaphore needed.

**Alternatives considered**:
- **`rayon::par_iter` on the workspace list**: cleaner code but adds a workspace-level Cargo dep. Rejected.
- **Async runtime (tokio)**: overkill for CPU-bound subprocess-orchestration; adds massive dep surface. Rejected.
- **Fixed 8-thread pool**: doesn't adapt to host CPU count; underuses 16-core hosts and oversubscribes 2-core CI runners.

---

## R4 — Budget-sharing under concurrency (FR-004)

**Decision**: Wrap the existing `BudgetTracker` (`Instant + Duration`, already naturally `Send + Sync` via `Copy`) in `Arc<BudgetTracker>` and pass to each worker. Every call to `budget.remaining()` recomputes from the shared `Instant::now() - started`, so all workers observe the same wall-clock reference.

**Rationale**:
- `BudgetTracker::remaining()` is a pure function of `self.started` and `self.budget` — no mutable state. Sharing via `Arc` is trivially correct without locks.
- The 60-second wall-clock cap is TOTAL across the scan, not per-worker. Two workers each starting a chunk with 5 seconds remaining will both be capped at 5 seconds by `run_bounded`'s `mpsc::recv_timeout`. When one worker returns `TimedOut`, its `SkipReason::BudgetExhausted` propagates; sibling workers who complete under-budget still emit verdicts.
- No changes to `BudgetTracker::from_env()` — the `WAYBILL_GO_MOD_WHY_BUDGET_MS` env var override remains as-is (spec Non-Goal: budget default unchanged).

**Alternatives considered**:
- **Per-worker budget = TOTAL / num_workers**: divides the pain evenly but under-uses budget when some workers finish quickly. Rejected because it doesn't preserve the "fair-race" semantics the current single-tracker model has.
- **AtomicU64 remaining-nanos counter**: introduces atomic-ordering concerns for zero win. `Instant` arithmetic is naturally consistent.

---

## R5 — `go.work` scope enumeration (US3)

**Decision**: Parse `go.work` files line-by-line in pure Rust (no `go env GOWORK` subprocess call). Extract `use <path>` directives, resolve each relative to the `go.work` file's parent directory, and canonicalize via `std::fs::canonicalize`. Build a `HashMap<PathBuf, Vec<PathBuf>>` mapping `go.work` parent-dir → member main-module dirs.

**Rationale**:
- `go.work` grammar is deliberately simple: `go X.Y[.Z]` directive + zero or more `use <dir>` + zero or more `replace ...` directives. `use` accepts single path or `use ( path1\npath2\n... )` block. All spec-defined at [go.dev/ref/mod#go-work-file](https://go.dev/ref/mod#go-work-file).
- Parsing in Rust avoids a subprocess spawn (spawning `go env GOWORK` costs ~50-100ms — the same overhead this milestone is trying to eliminate).
- Parser is ~40 lines, mirrors `parse_go_mod` at `legacy.rs:200` in structure.
- Malformed `go.work` (any parse error) → fall back to per-workspace preflight per FR-008. Warn once with the go.work path + parse-error detail.

**Alternatives considered**:
- **Shell out to `go env GOWORK`**: gives absolute path to the go.work but doesn't enumerate members; we'd still need to parse the file. Adds subprocess cost for no info gain. Rejected.
- **Shell out to `go list -m -json all` with `GOWORK=<path>`**: heavyweight; requires network fallback in some Go toolchain versions. Rejected as too broad for member enumeration.
- **Detect `go.work` presence but not membership**: treat _every_ main-module in the tree as a workspace member. Rejected because misclassifies out-of-workspace `hack/tools/*` sub-modules that k8s has.

---

## R6 — File organization (mod_why.rs vs mod_why/ submodule)

**Decision**: Keep everything in `mod_why.rs` as a single file for US1 + US2 + US3. Extract to `mod_why/` submodule only if the file exceeds ~1200 lines after implementation (currently ~700 lines; expected net add ~300 lines → ~1000).

**Rationale**:
- Context locality helps reviewers. Splitting a 1000-line file into 4 sub-files (`argv_guard.rs`, `concurrent.rs`, `shared_preflight.rs`, `mod.rs`) trades vertical scrolling for horizontal file-hopping. Neither is strictly better at ~1000 lines.
- The mod_why-adjacent workspace-mode helpers (`detect_workspace_mode`, `WorkspaceMode` enum) already live in `mod_why.rs` at ~700 lines and haven't been an issue.
- Extraction is a reversible refactor if code review flags it. Defer to review.

**Alternatives considered**:
- **Extract eagerly**: rejected — over-engineering ahead of demand.
- **Extract US3 only**: rejected — inconsistent organization; either it's all in one file or all decomposed.

---

## R7 — Small integration-test fixture (not full Kubernetes)

**Decision**: The three US integration tests use a small synthetic multi-workspace Go fixture under `waybill-cli/tests/fixtures/golang/mod_why_scaling/` — 3-4 `go.mod` files under a synthetic `go.work`, ~10 modules each. NOT the full 380 MB Kubernetes clone.

**Rationale**:
- Tests must be hermetic + fast + reproducible in CI. Cloning 380 MB × per-CI-run is unacceptable.
- The 3-4 workspace shape exercises every code path US1/US2/US3 introduces:
  - **US1**: any workspace with ≥1 module hits the chunk-selection path (argv-guard branch exercised via a targeted unit test with a synthetic 1000-path input, not the fixture).
  - **US2**: 3-4 workspaces + a 2-CPU CI runner exercises the "concurrent > 1" branch. The `available_parallelism()` mock isn't necessary — CI already reports ≥2.
  - **US3**: 3-4 workspaces under a single `go.work` exercises the shared-preflight path; +1 out-of-workspace `go.mod` exercises FR-008 fallback.
- The full Kubernetes benchmark (SC-001) is measured via the m669 benchmark harness at `xtask bench`, not as a `cargo test` case. Wall-time thresholds land in `docs/perf/baseline.json` per the m669 convention.

**Alternatives considered**:
- **Full Kubernetes as a test fixture**: rejected per above.
- **No fixture; unit-test only**: rejected because US2/US3 correctness properties (log correlation, shared preflight uniqueness) need end-to-end validation via the real `PackageDbEntry` → `analyze_main_module` call path.

---

## R8 — SC-001 empirical validation methodology

**Decision**: After each US ships, re-run the benchmark protocol from issue #745's re-benchmark (see the 2026-09-04 sweep results): fresh clone of `kusari-sandbox/test-kubernetes`, warm cache, macOS aarch64 8-core reference host, `time` command, release build. Update the `perf/baseline.json` file (m669) with the new baseline per US milestone.

**Rationale**:
- Reproducibility: the same command that populated the issue-#745 empirical data validates the fix.
- macOS aarch64 8-core is my dev machine — matches the constant reference class the issue used. Other host classes benefit proportionally but their exact wall-times are not pinned by SC-001 (per spec Assumption 1).
- Per-US measurement lets us confirm the tiered targets (US1 ≤ 30s, US1+US2 ≤ 15s, all three ≤ 10s) rather than only the terminal target.
- Regression pin: the m669 benchmark harness runs on GHA runners; even though those aren't the SC-001 reference class, they'll catch order-of-magnitude regressions (e.g., a future PR that accidentally re-introduces per-workspace preflight).

**Alternatives considered**:
- **CI-only measurement (no local reproduction)**: rejected — GHA runners have highly variable performance (2-4 vCPUs, shared physical hosts, filesystem contention). Not a stable reference class for absolute-time SC.
- **Statistical median of N runs**: reasonable but overkill for a 10s target where 3-5s of variance is well below the threshold. If regression variance becomes a problem post-ship, revisit.

---

## Findings summary

Every technical unknown that the spec surfaced is resolved:
- Chunk-size default + argv-guard algorithm (R1, R2)
- Concurrency primitive (R3)
- Budget-sharing semantics under concurrency (R4)
- go.work enumeration approach (R5)
- File organization (R6)
- Test fixture strategy (R7)
- Empirical validation methodology (R8)

Zero `NEEDS CLARIFICATION` markers remain. Ready for Phase 1.
