# Phase 1 Data Model — m774 parallel source-import collection

**Feature**: 774-parallel-source-imports
**Status**: Complete
**Date**: 2026-09-04

Per-scan in-process types. No persistence, no wire representation, no cross-scan cache.

## New entities

### `WorkspaceImportJob<'a>` (new)

Work-queue payload representing one workspace awaiting parallel source-import collection.

```rust
struct WorkspaceImportJob<'a> {
    /// Position of this workspace in the input `parsed_roots` slice.
    /// Preserved for diagnostic ordering in the FR-014 summary log
    /// and for the FR-007 error-log context on worker panic.
    workspace_index: usize,
    /// Borrowed reference to the workspace's project_root. Lifetime
    /// tied to the surrounding `std::thread::scope`. No clone.
    project_root: &'a PathBuf,
}
```

**Validation rules**:
- `workspace_index` MUST be a valid index into `parsed_roots` (0 ≤ index < parsed_roots.len()).
- `project_root` MUST be non-empty and canonicalizable (pre-loop validation already ensures this before the workspace enters `parsed_roots`).

**Relationships**:
- Produced once per workspace at parallel-phase entry (before spawning workers). N workspaces → N `WorkspaceImportJob`s in the initial queue.
- Consumed by workers via `queue.lock().pop()` — one job per worker per iteration.

**Lifecycle**: created at parallel-phase entry, dropped at worker exit. Never persisted beyond the parallel phase.

---

### `ImportCollectionResult` (new)

Per-workspace worker output sent from worker → main thread via mpsc.

```rust
struct ImportCollectionResult {
    /// Preserved from the input WorkspaceImportJob for diagnostic
    /// ordering + defense-in-depth "no gaps" check in the reduce.
    workspace_index: usize,
    /// Per-worker local: production imports discovered by
    /// `collect_production_imports(project_root, ...)`. Moved through
    /// mpsc to the reducer.
    production_imports: HashSet<String>,
    /// Per-worker local: test imports discovered by
    /// `collect_test_imports(project_root, ...)`. Moved through mpsc.
    /// Note: NOT test-only — the test_only_imports set is computed
    /// downstream as `test_imports_union - production_imports_union`
    /// at `legacy.rs:2281`, unchanged from pre-milestone.
    test_imports: HashSet<String>,
}
```

**Validation rules**:
- `workspace_index` MUST match the input `WorkspaceImportJob.workspace_index` — enforced by construction.
- `production_imports` and `test_imports` MAY be empty (workspace with zero `.go` files, or workspace whose imports don't intersect `known_modules`). Empty sets are valid outputs.

**Relationships**:
- Produced by workers via `tx.send(ImportCollectionResult { .. })`.
- Consumed by the main thread's reduce via `rx.recv()`.
- One `ImportCollectionResult` per `WorkspaceImportJob` consumed.

**Lifecycle**: created in worker body immediately after both `collect_*_imports` calls return, moved through mpsc, consumed + dropped in Phase 2 reduce.

---

### `SharedImportState<'a>` (new — a bundle for readability)

The shared-handle bundle passed to worker closures.

```rust
struct SharedImportState<'a> {
    /// Shared work queue drained by workers.
    queue: Arc<Mutex<Vec<WorkspaceImportJob<'a>>>>,
    /// Per-worker sender clone; the parent drops its own tx before
    /// entering the reduce loop so `rx.recv()` returns `Err(_)` after
    /// the last worker's send completes.
    tx: mpsc::Sender<ImportCollectionResult>,
    /// Borrowed reference to the workspace-scoped known_modules slice.
    /// Tied to the `std::thread::scope` lifetime; no clone, no Arc.
    known_modules: &'a [String],
}
```

**Validation rules**:
- `queue` is initialized with N `WorkspaceImportJob`s at scope entry; drains monotonically as workers pop.
- `tx` is per-worker-cloned; the parent's `tx` is dropped BEFORE entering the reduce loop.
- `known_modules` is read-only across the parallel phase (see research R4).

**Relationships**:
- Constructed once per `pub fn read` invocation at the entry of the post-main-loop parallel phase.
- Each worker receives an `Arc`-clone of `queue` + a `tx.clone()` + the `&known_modules` borrow at spawn time (`std::thread::scope` closure capture).
- Dropped after all workers join.

**Lifecycle**: transient — created at parallel-phase entry, dropped after Phase 2 reduce completes.

---

## Modifications to existing types

### `collect_production_imports` / `collect_test_imports` (existing — no signature change)

Public/module-private signatures preserved per FR-005:

```rust
fn collect_production_imports(
    project_root: &Path,
    depth: usize,
    known_modules: &[String],
    into: &mut HashSet<String>,
);

fn collect_test_imports(
    project_root: &Path,
    depth: usize,
    known_modules: &[String],
    into: &mut HashSet<String>,
);
```

**Milestone change**: NONE. Callers change (from serial inline mutation of `signals.production_imports` to per-worker local `HashSet`), but the function signatures themselves are byte-identical pre and post milestone.

---

### `Signals::production_imports` (existing — no field change)

```rust
pub struct Signals {
    // ... other fields ...
    pub production_imports: HashSet<String>,
    pub test_only_imports: HashSet<String>,
    // ... other fields ...
}
```

**Milestone change**: NONE. Field types + names preserved. Post-parallel-phase write is now a single `extend()` batch from the reducer instead of N incremental mutations in the loop; content is identical (set-union commutativity per research R5).

---

## Diagram — orchestration

```text
                    ┌────────────────────────────────────────┐
                    │ pub fn read entry (legacy.rs:1615)     │
                    └────────────┬───────────────────────────┘
                                 │
                     1. Build parsed_roots (pre-milestone)
                                 │
                                 ▼
                    ┌────────────────────────────────────────┐
                    │ SERIAL MAIN LOOP (unchanged from       │
                    │ pre-milestone EXCEPT the two           │
                    │ collect_*_imports calls at :2224–2240  │
                    │ are extracted):                        │
                    │  for (project_root, doc, sums) in ..:  │
                    │    - resolver.resolve(&ctx, &cache)    │
                    │    - build_entries_from_go_module...   │
                    │    - +incompatible filter              │
                    │    - stamp_go_transitive_annotations   │
                    │    - build_main_module_entry           │
                    │    - orphan backfill (Issue #251)      │
                    │    - tool-directive (Issue #250)       │
                    │    - +incompatible warn (Issue #255)   │
                    │    - out.push, seen_purls.insert       │
                    │  (collect_*_imports NOT called here)   │
                    └────────────┬───────────────────────────┘
                                 │
                     2. Build worker pool over parsed_roots
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │ SharedImportState (all Arc-cloned/borrow-shared into   │
    │ workers):                                              │
    │                                                         │
    │  queue:         Arc<Mutex<Vec<WorkspaceImportJob>>>    │
    │  tx:            mpsc::Sender<ImportCollectionResult>   │
    │  known_modules: &[String] via scope lifetime           │
    │                                                         │
    │  results:       Vec<ImportCollectionResult> drained    │
    │                 from rx by the reducer                 │
    └────────────────────────────────────────────────────────┘
                                 │
                     3. Populate queue with N jobs;
                        spawn worker_count() workers
                                 │
                                 ▼
        std::thread::scope(|s| {                    ┌────────────────────────┐
          for _ in 0..worker_count() {              │ Phase 1 (parallel)     │
            s.spawn(|| loop {                       │                        │
              let job = queue.lock().pop()?;        │  worker pops job,      │
              let mut prod = HashSet::new();        │  runs both             │
              let mut test = HashSet::new();        │  collect_*_imports,    │
              collect_production_imports(           │  sends result          │
                job.project_root, 0,                │                        │
                known_modules, &mut prod);          │                        │
              collect_test_imports(                 │                        │
                job.project_root, 0,                │                        │
                known_modules, &mut test);          │                        │
              tx.send(ImportCollectionResult {      │                        │
                workspace_index: job.workspace_index,│                       │
                production_imports: prod,           │                        │
                test_imports: test,                 │                        │
              });                                   │                        │
            });                                     │                        │
          }                                          └────────────────────────┘
          drop(tx);  // main-side sender dropped ─→ rx.recv() ends after workers
        });
                                 │
                     4. Drain mpsc into results Vec (any order)
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │ Phase 2 (serial reduce, main thread):                  │
    │                                                         │
    │  let mut test_imports = HashSet::new();                 │
    │  for result in results {                                │
    │      signals.production_imports.extend(                 │
    │          result.production_imports);                    │
    │      test_imports.extend(result.test_imports);          │
    │  }                                                      │
    │  // Then the pre-milestone difference at :2281:         │
    │  signals.test_only_imports = test_imports               │
    │      .difference(&signals.production_imports)           │
    │      .cloned().collect();                               │
    │                                                         │
    │  tracing::info!(                                        │
    │      workspaces_scanned = N,                            │
    │      parallel_workers_used = worker_count(),            │
    │      production_imports_count = ...,                    │
    │      test_imports_count = ...,                          │
    │      elapsed_ms = ...,                                  │
    │      "m774 parallel source-import collection complete"  │
    │  );                                                     │
    └────────────────────────────────────────────────────────┘
```

**Coordination invariants**:
- Every popped `WorkspaceImportJob` corresponds to exactly one `ImportCollectionResult` sent back via mpsc. No jobs silently dropped.
- Phase 2 reduce sees exactly N results; missing results surface via a defense-in-depth `debug_assert!(results.len() == parsed_roots.len(), ...)` — the FR-007 fail-fast propagation makes this an unreachable check under normal operation.
- Worker `join()` inside the scope block guarantees all workers have completed before Phase 2 begins.
- `known_modules` is never mutated by workers (borrow is `&`, not `&mut`).

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| Serial `for` at `legacy.rs:1787` calls both `collect_*_imports` inline per iteration, mutating `signals.production_imports` + outer `test_imports` local directly | Serial main loop skips `collect_*_imports`; a new post-main-loop parallel phase runs them across all workspaces via bounded thread pool + Phase 2 reduce | Clarifications Q1 → Option A |
| `signals.production_imports` mutated N times inside the loop | `signals.production_imports.extend(...)` called N times in the Phase 2 reduce on the main thread | FR-003 |
| `test_imports` local mutated N times inside the loop | Local built up via `extend()` in the reduce; then `signals.test_only_imports` computed via `.difference(...)` unchanged | FR-004 |
| Worker panic = whole scan panics (no isolation needed — single thread) | Worker panic captured by `ScopedJoinHandle::join()` `Err(_)` → `resume_unwind(payload)` → whole scan panics identically | R2 + FR-007 |
| Zero new log lines related to this phase | One new `tracing::info!` summary line at end of parallel phase per FR-014 | FR-014 |

Every column-1 → column-2 transition preserves the `collect_production_imports` + `collect_test_imports` function signatures (FR-005), the `signals.production_imports` field shape (Signals struct unchanged), and the pre-milestone content of both sets (byte-identity per FR-004 + FR-011 + SC-002).
