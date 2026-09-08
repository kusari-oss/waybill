# Phase 1 Data Model — m773 parallel graph_resolver

**Feature**: 773-graph-resolver-parallel
**Status**: Complete
**Date**: 2026-09-05

Per-scan in-process types. No persistence, no wire representation, no cross-scan cache.

## Entities

### `WorkspaceJob` (new)

Work-queue payload representing one workspace awaiting parallel resolution.

```rust
struct WorkspaceJob<'a> {
    /// Position of this workspace in the input `parsed_roots` slice.
    /// Used by the Phase 2 reduce for deterministic-order iteration
    /// (per FR-004 + research R3).
    workspace_index: usize,
    /// The pre-milestone loop iterated `&parsed_roots` yielding
    /// `(project_root, doc, sums)`. Workers get borrowed references
    /// tied to the surrounding `std::thread::scope` lifetime — no
    /// clone needed.
    project_root: &'a PathBuf,
    doc: &'a GoModDocument,        // Or the actual type from parse_go_mod
    sums: &'a Vec<ParsedGoSumEntry>, // Or the actual type from parse_go_sum
}
```

**Validation rules**:
- `workspace_index` MUST be a valid index into `parsed_roots` (0 ≤ index < parsed_roots.len()).
- `project_root`, `doc`, `sums` are borrowed references (`'a` lifetime) tied to the parent scope's `parsed_roots` slice. Workers read these; no mutation.

**Relationships**:
- Produced once per workspace at loop entry (before spawning workers). N workspaces → N `WorkspaceJob`s in the initial queue.
- Consumed by workers via `queue.lock().pop()` — one job per worker per iteration.

**Lifecycle**: created at loop entry, dropped at worker exit. Never persisted beyond the parallel loop.

---

### `ResolveResult` (new)

Per-workspace worker output sent from worker → main thread via mpsc.

```rust
struct ResolveResult {
    /// Preserved from the input WorkspaceJob for deterministic-order
    /// reduce (FR-004 + research R3).
    workspace_index: usize,
    /// The WorkspaceContext the worker constructed (owned, moved).
    /// Phase 2 reduce reads its fields for the post-processing step.
    ctx: WorkspaceContext,
    /// Result of the per-workspace `resolver.resolve()` call. Preserves
    /// the existing Result-shape from graph_resolver.rs so the reduce
    /// can log-and-fallback identically to the pre-milestone code path.
    resolve_outcome: Result<ModuleGraphMap, GraphResolverError>,
}
```

**Validation rules**:
- `workspace_index` MUST match the input `WorkspaceJob.workspace_index` — enforced by construction (the worker copies it from the popped job).
- `resolve_outcome`: `Ok(graph_map)` on success; `Err(e)` on the same failure paths the pre-milestone loop already handled at `legacy.rs:1796-1804`.

**Relationships**:
- Produced by workers via `tx.send(ResolveResult { .. })`.
- Consumed by the main thread's reduce via `rx.recv()`.
- One `ResolveResult` per `WorkspaceJob` consumed.

**Lifecycle**: created in worker body immediately after `resolver.resolve()` returns, moved through mpsc, consumed + dropped in Phase 2 reduce.

---

### `ResolverSharedState` (new — a bundle for readability)

The `Arc`-wrapped shared state passed to each worker thread.

```rust
struct ResolverSharedState {
    resolver: Arc<GraphResolver>,
    cache: Arc<GoModCache>,
    queue: Arc<Mutex<Vec<WorkspaceJob<'a>>>>,
    tx: mpsc::Sender<ResolveResult>,
}
```

**Validation rules**:
- `resolver` and `cache` are read-only shared handles (their APIs are all `&self` per FR-009 + FR-010).
- `queue` is initialized with N `WorkspaceJob`s at scope entry; drains monotonically as workers pop.
- `tx` is per-worker-cloned; the parent drops its own `tx` before entering the reduce loop so `rx.recv()` returns `Err(_)` after the last worker's send completes.

**Relationships**:
- Constructed once per `pub fn read` invocation (the enclosing function at `legacy.rs:1615` where the parallelized loop lives).
- Each worker receives a `.clone()` of the Arc-wrapped fields at spawn time (`std::thread::scope` closure).
- Dropped after all workers join.

**Lifecycle**: transient — created at parallel-loop entry, dropped after Phase 2 reduce completes.

---

## Modifications to existing types

### `GraphResolver` (existing — no signature change)

Public API preserved verbatim (FR-009):
```rust
pub struct GraphResolver { config: GraphResolverConfig }
impl GraphResolver {
    pub fn new(config: GraphResolverConfig) -> Self;
    pub fn config(&self) -> &GraphResolverConfig;
    pub fn resolve(
        &self,
        ctx: &WorkspaceContext,
        cache: &GoModCache,
    ) -> Result<ModuleGraphMap, GraphResolverError>;
}
```

**Milestone change**: NONE. The type is already `Send + Sync` (contains only `GraphResolverConfig` which is `Clone + Debug`; verified at compile-time via `assert_send_sync::<GraphResolver>()` per research R7).

---

### `GoModCache` (existing — no signature change)

Public API preserved verbatim (FR-010). All methods `&self`:
```rust
impl GoModCache {
    pub fn discover(rootfs: &Path) -> Self;
    pub(crate) fn is_empty(&self) -> bool;
    pub(crate) fn read_mod_file(&self, module: &str, version: &str) -> Option<String>;
}
```

**Milestone change**: NONE. Type is already `Send + Sync` — verified at compile-time.

---

### `WorkspaceContext` (existing — no signature change)

Public API preserved. Constructed fresh per workspace inside the worker body (same as pre-milestone loop-body behavior). Fields are all `Send`-safe (`PathBuf`, `HashSet`, `HashMap` of owned data).

**Milestone change**: NONE.

---

## Diagram — orchestration

```text
                    ┌────────────────────────────────────┐
                    │   pub fn read entry (legacy.rs:1615)│
                    └────────────┬───────────────────────┘
                                 │
                     1. Build parsed_roots (pre-milestone)
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │  ResolverSharedState (all Arc-cloned into workers):    │
    │                                                         │
    │  resolver: Arc<GraphResolver>       ← &self read-only  │
    │  cache:    Arc<GoModCache>          ← &self read-only  │
    │  queue:    Arc<Mutex<Vec<WorkspaceJob>>>               │
    │  tx:       mpsc::Sender<ResolveResult>                 │
    │                                                         │
    │  results:  Vec<Option<ResolveResult>>                   │
    │            (sized to parsed_roots.len(), index-slotted) │
    └────────────────────────────────────────────────────────┘
                                 │
              2. Populate queue with N WorkspaceJobs
                                 │
                                 ▼
        std::thread::scope(|s| {                    ┌────────────────────────┐
          for i in 0..worker_count {                │ Phase 1 (parallel)     │
            s.spawn(|| loop {                       │                        │
              let job = queue.lock().pop()?;        │  worker pops job,      │
              let ctx = WorkspaceContext::from_...  │  builds ctx,           │
              let outcome = resolver.resolve(...);  │  calls resolve(),      │
              tx.send(ResolveResult { .. });        │  sends result          │
            });                                     │                        │
          }                                          └────────────────────────┘
          drop(tx);  // main-side sender dropped ─→ rx.recv() ends after workers
        });
                                 │
              3. Drain mpsc into results Vec by index
                                 │
                                 ▼
        for result in rx { results[result.workspace_index] = Some(result); }
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │  Phase 2 (serial reduce, workspace_index order):       │
    │                                                         │
    │  for i in 0..parsed_roots.len() {                       │
    │      let ResolveResult { ctx, resolve_outcome, .. } =   │
    │          results[i].take().expect("no gaps");           │
    │      // Existing pre-milestone post-processing:         │
    │      //   - signals aggregation (Coverage merge, etc.)  │
    │      //   - entries build via                           │
    │      //     build_entries_from_go_module_with_lookup    │
    │      //   - +incompatible filter                        │
    │      //   - out.push(entry), seen_purls.insert(purl)    │
    │      //   - build_main_module_entry + gosum augment     │
    │  }                                                      │
    └────────────────────────────────────────────────────────┘
```

**Coordination invariants**:
- Every popped `WorkspaceJob` corresponds to exactly one `ResolveResult` sent back via mpsc. No jobs silently dropped.
- `results[workspace_index]` is populated exactly once — deterministic slot assignment prevents duplicate writes.
- Phase 2 reduce iterates 0..N in order; missing slots (a worker panicked before send) surface via the `.take().expect("no gaps")` — but panic propagation (FR-006) makes the scope panic before the reduce ever runs, so the `expect` is a defense-in-depth belt-and-suspenders.
- Worker `join()` inside the scope block guarantees all workers have completed before Phase 2 begins.

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| Serial `for (project_root, doc, sums) in &parsed_roots` at `legacy.rs:1780` | Bounded thread pool + workspace_index-ordered reduce | US1 |
| `resolver: GraphResolver` owned on the stack | `Arc<GraphResolver>` cloned into each worker | R4 |
| `cache: GoModCache` owned on the stack | `Arc<GoModCache>` cloned into each worker | R4 |
| Post-loop state mutation interleaved with `resolver.resolve()` call | Post-loop state mutation happens only in Phase 2 reduce on the main thread | FR-005 |
| Worker panic = whole scan panics (no isolation needed — single thread) | Worker panic propagates via `ScopedJoinHandle::join()` `Err(_)` → `resume_unwind(payload)` | R5 + FR-006 |
| Per-workspace FR-013 summary log fires in loop order (which IS discovery order) | Per-workspace FR-013 summary log MAY fire in worker-completion order (interleaved) — still one line per workspace, wire-shape unchanged | FR-007 |

Every column-1 → column-2 transition preserves the `GraphResolver::resolve()` + `GoModCache` API surfaces (FR-009 + FR-010) and the pre-milestone post-processing logic (FR-005 — moves from loop body to reduce body, unchanged in semantics).
