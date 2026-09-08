# Phase 1 Data Model — m771 `go mod why` subprocess scaling

**Feature**: 771-gomodwhy-subprocess-scale
**Status**: Complete
**Date**: 2026-09-04

Per-scan in-process types. No persistence, no wire representation, no cross-scan cache.

## Entities

### `GoWorkScope` (new — US3)

Groups sibling main-modules that share a single `go.work` file.

```rust
struct GoWorkScope {
    /// The absolute path to the go.work file's parent directory. This
    /// is the working-directory for the shared `go list all` preflight
    /// (per spec.md §Clarifications 2026-09-04 Q1).
    root_dir: PathBuf,
    /// Absolute paths to each member main-module directory (each
    /// contains a go.mod). Derived from `use` directives in the go.work
    /// file, canonicalized via std::fs::canonicalize.
    members: Vec<PathBuf>,
}
```

**Validation rules**:
- `root_dir` must be a directory containing a `go.work` file.
- Each `members[i]` must be a directory containing a `go.mod` file. Malformed `use` entries (missing dir, missing go.mod) are warn-and-skip per FR-008.
- `members.len() >= 1` — a `GoWorkScope` with zero valid members degrades to per-workspace preflight (FR-008 fallback).

**Relationships**:
- 0..* `GoWorkScope` instances per scan (typically 0 or 1; k8s has 1).
- Each `GoWorkScope` co-exists with 0..* stand-alone main-modules NOT covered by any scope (e.g., `hack/tools/*` in k8s that aren't in the root `go.work`).

**Lifecycle**: constructed at classifier entry, dropped at classifier exit. Never persisted.

---

### `AnalysisJob` (new — US2 introduces, US3 extends)

Work-queue unit for the concurrent classifier orchestrator.

```rust
enum AnalysisJob {
    /// Non-workspace main-module. Runs its own `go list all` preflight
    /// (per FR-008). Introduced by US2 as the initial (only) variant.
    Loose { main_module: PathBuf },
    /// Member of a detected go.work scope. Shares the preflight with
    /// other members via the SharedPreflightCache (per FR-006).
    /// Introduced by US3.
    Scope { scope: Arc<GoWorkScope>, member: PathBuf },
}
```

**Validation rules**:
- `Loose.main_module` and `Scope.member` must each be directories containing a `go.mod` file.
- A single `Loose { main_module: X }` and `Scope { member: X, .. }` MUST NOT co-exist for the same X — `detect_go_work_scopes` (T032) is the sole source of the partition.

**Relationships**:
- One `AnalysisJob` per detected main-module. Total job count = `|scopes.members.flatten()| + |loose|` = original workspace count.
- Workers pop jobs from an `Arc<Mutex<Vec<AnalysisJob>>>` queue (per data-model orchestration diagram below).

**Lifecycle**: constructed at classifier entry (`apply_go_mod_why_pass`), dropped after every job has been drained + `MainModuleAnalysis` reduced.

**Staging note**: US2 ships this enum with **only** the `Loose` variant (or an equivalent `Vec<PathBuf>` — implementer's choice as long as US3's extension is straightforward). US3 adds `Scope` and switches the caller-site partition to use both. Neither shape is user-visible; the enum lives entirely inside `apply_go_mod_why_pass`.

---

### `SharedPreflightCache` (new — US3)

In-memory cache of `go list all` preflight results, keyed by `GoWorkScope.root_dir`.

```rust
struct SharedPreflightCache {
    /// Map from go.work-scope root to preflight outcome.
    /// Populated lazily on first access per scope.
    /// Wrapped in Arc<Mutex<>> for concurrent-worker access (US2).
    entries: HashMap<PathBuf, PreflightOutcome>,
}

enum PreflightOutcome {
    /// `go list all` exited 0. Every member of this scope can proceed
    /// to per-member `go mod why -m` chunks.
    Ok,
    /// `go list all` failed (non-zero exit, spawn failure, or timeout).
    /// Every member of this scope is marked with
    /// SkipReason::UnresolvablePackages per FR-007.
    Skipped(SkipReason),
}
```

**Validation rules**:
- Cache is populated on first `analyze_workspace` call per scope; subsequent members skip the preflight and read the cached `PreflightOutcome`.
- Cache is thread-safe via `Arc<Mutex<HashMap<...>>>` — contention is bounded (one insert per scope, then read-only for the remainder of the scan).

**Relationships**:
- One `SharedPreflightCache` per scan (owned by the classifier's `read_all` scope).
- Cardinality: `|entries| ≤ |GoWorkScope|`.

**Lifecycle**: constructed at classifier entry, populated lazily during workspace analysis, dropped at classifier exit.

---

### `BudgetTracker` (existing — reused)

The shared 60-second wall-clock budget, unchanged from pre-milestone.

```rust
// Existing definition at mod_why.rs:164 — INCLUDED FOR REFERENCE, UNCHANGED.
pub struct BudgetTracker {
    started: Instant,
    budget: Duration,
}
```

**Milestone change**: wrapped in `Arc<BudgetTracker>` at the caller site (`scan_fs/package_db/mod.rs`) so it can be cheaply cloned into each concurrent worker. `BudgetTracker` is `Copy + Send + Sync` naturally; the `Arc` is a plumbing convenience, not a correctness requirement.

**Rationale**: see research R4.

---

### `MainModuleAnalysis` (existing — reused, unchanged shape)

Per-workspace verdict output from `analyze_main_module`. Unchanged wire shape (FR-009).

```rust
// Existing definition; INCLUDED FOR REFERENCE, UNCHANGED.
pub struct MainModuleAnalysis {
    pub verdicts: HashMap<String, GoModWhyVerdict>,
    pub skip_reason: Option<SkipReason>,
    pub workspace_active: bool,
}
```

**Milestone change**: none. The concurrent orchestrator produces one `MainModuleAnalysis` per workspace and merges them via the existing `verdict_rank` reducer at `scan_fs/package_db/mod.rs:1210`.

---

### `WorkspaceMode` (existing — reused, unchanged)

The m231 workspace-mode enum. Unchanged variants (FR-014).

```rust
// Existing definition at mod_why.rs:97; INCLUDED FOR REFERENCE, UNCHANGED.
pub(super) enum WorkspaceMode {
    Off,
    Inactive,
    Active(PathBuf),
    Explicit(PathBuf),
}
```

**Milestone change**: none to the enum. The `detect_workspace_mode` function is called once per main-module to determine per-subprocess env vars (unchanged from m231). US3 additionally uses the `Active(PathBuf)` / `Explicit(PathBuf)` payload path to group members into `GoWorkScope`s.

---

### `SkipReason` (existing — reused, unchanged)

Enumeration of why a main-module was skipped by the classifier.

```rust
// Existing definition; INCLUDED FOR REFERENCE, UNCHANGED.
pub enum SkipReason {
    BudgetExhausted,
    UnresolvablePackages,
    // ... (existing variants)
}
```

**Milestone change**: none. FR-007 uses the existing `UnresolvablePackages` variant to represent shared-preflight failure across every member of the affected `GoWorkScope`.

---

### `Invocation` (existing — reused, unchanged)

Result type from `run_bounded` subprocess helper.

```rust
// Existing definition at mod_why.rs; INCLUDED FOR REFERENCE, UNCHANGED.
enum Invocation {
    Completed(std::process::Output),
    SpawnFailed(String),
    TimedOut,
}
```

**Milestone change**: none. The concurrent workers each call `run_bounded` synchronously; the `Arc<BudgetTracker>` ensures they respect the shared deadline.

---

## Diagram — orchestration entities under US2 + US3

```text
                    ┌────────────────────────────┐
                    │   Classifier entry         │
                    │   (scan_fs::package_db     │
                    │    ::mod::apply_go_mod_    │
                    │    why_pass)               │
                    └────────────┬───────────────┘
                                 │
              1. Enumerate workspaces + build GoWorkScope[]
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │  scopes: Vec<GoWorkScope>   +   loose: Vec<PathBuf>    │
    │                                                         │
    │  shared: Arc<Mutex<SharedPreflightCache>>              │
    │  budget: Arc<BudgetTracker>                            │
    │  workqueue: Arc<Mutex<Vec<AnalysisJob>>>               │
    └────────────────────────────────────────────────────────┘
                                 │
              2. Spawn min(N, available_parallelism()) workers
                                 │
                                 ▼
       ┌───────────┐    ┌───────────┐    ┌───────────┐
       │ Worker 1  │    │ Worker 2  │... │ Worker P  │
       │           │    │           │    │           │
       │ pop job   │    │ pop job   │    │ pop job   │
       │ analyze() │    │ analyze() │    │ analyze() │
       │ mpsc.send │    │ mpsc.send │    │ mpsc.send │
       └─────┬─────┘    └─────┬─────┘    └─────┬─────┘
             │                 │                 │
             └─────────────────┼─────────────────┘
                               │
                               ▼
              ┌────────────────────────────┐
              │  Merge verdicts via        │
              │  verdict_rank reducer      │
              │  (unchanged from m231)     │
              └────────────────────────────┘

     AnalysisJob:
       Scope { scope: &GoWorkScope, member: &PathBuf }  -- go.work member; shares preflight
       Loose { main_module: PathBuf }                    -- non-workspace; own preflight
```

**Coordination invariants**:
- Each scope's preflight runs exactly once: the worker that pops the first job for a scope acquires the cache mutex, checks if the scope has been preflighted, and if not, runs `go list all` from `scope.root_dir` and populates the cache.
- Subsequent workers for the same scope read the cached result (blocking on the mutex if the first worker is still preflighting — bounded wait, mutex is released after the preflight subprocess returns).
- The workqueue mutex is only held for `pop()`; work is done outside the lock.
- The budget is checked at the top of every worker iteration; workers exit their loop when `budget.remaining()` returns `None`.

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| Serial `for workspace in &workspaces` loop at `mod.rs:1195` | Bounded thread-pool + mpsc reducer | US2 |
| Per-workspace `go list all` preflight | Cached per-`GoWorkScope` preflight (or per-workspace fallback for loose main-modules) | US3 |
| `CHUNK_SIZE = 20` in `mod_why.rs:31` | `CHUNK_SIZE = 500` + argv-length guard | US1 + R2 |
| `BudgetTracker` owned by `read_all` scope | `Arc<BudgetTracker>` shared across concurrent workers | US2 (R4) |
| `SharedPreflightCache` doesn't exist | `Arc<Mutex<SharedPreflightCache>>` shared across concurrent workers | US3 |
| `GoWorkScope` doesn't exist | `Vec<GoWorkScope>` built at classifier entry from detected `WorkspaceMode::Active/Explicit(root)` main-modules | US3 |

Every column-1 → column-2 transition preserves the `MainModuleAnalysis` output shape (FR-009) and the merge-loop's `verdict_rank` reducer (unchanged from pre-milestone).
