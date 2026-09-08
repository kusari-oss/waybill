# Phase 1 Data Model — m772 parallel scan_fs walker

**Feature**: 772-parallel-scan-walker
**Status**: Complete
**Date**: 2026-09-04

Per-scan in-process types. No persistence, no wire representation, no cross-scan cache.

## Entities

### `SubtreeJob` (new)

Work-queue payload representing one directory awaiting parallel walk.

```rust
#[derive(Debug, Clone)]
pub(super) struct SubtreeJob {
    /// Canonicalized absolute path to the directory to walk. Guaranteed
    /// distinct across queue entries via the shared visited-set gate
    /// (research R3 + FR-003).
    canonical: PathBuf,
    /// C10 (m664) descend-into scope restriction inherited from the
    /// parent frame. `None` = no restriction (all readers active).
    /// `Some(set)` = dispatch limited to the named readers when inside
    /// a normally-skipped-but-opted-in dir.
    scope: Option<HashSet<ReaderId>>,
}
```

**Validation rules**:
- `canonical` MUST be an absolute canonicalized path (result of `std::fs::canonicalize`). The visited-set is keyed on this exact form.
- `scope`: `None` for jobs derived from the rootfs or normal-descent subdirs; `Some(set)` only for jobs derived from a `descend_into` opt-in.

**Relationships**:
- Produced at the queue-seed site (`ParallelWalker::run`) with rootfs as the sole initial entry.
- Produced by every worker's per-directory step when new subdirs are discovered — pushed back onto the shared queue for peers to steal.

**Lifecycle**: created at push time, dropped at pop time. Never persisted beyond `SharedWalker::run`.

---

### `SharedWalkState` (new)

Bundle of the three `Arc`-wrapped shared resources passed to each worker thread.

```rust
#[derive(Clone)]
struct SharedWalkState {
    /// FIFO seed + LIFO work-stealing pool per research R2.
    queue: Arc<Mutex<Vec<SubtreeJob>>>,
    /// Shared symlink-loop guard per research R3 + FR-003.
    visited: Arc<Mutex<HashSet<PathBuf>>>,
    /// Per-reader output collectors — already Mutex-per-reader per m664.
    /// Reused as-is per FR-006.
    output: Arc<HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>>,
    /// Atomic counters per research R7. Migrates the existing
    /// WalkerMetrics scalar fields to `AtomicU64`.
    metrics: Arc<WalkerMetrics>,
    /// Number of workers actively processing a job. When zero AND the
    /// queue is empty, the drain phase is complete.
    active_workers: Arc<AtomicUsize>,
}
```

**Validation rules**:
- `queue` is populated at seed time with exactly one `SubtreeJob` (rootfs); grows dynamically as workers discover subdirs.
- `visited` is empty at seed time; grows monotonically as workers canonicalize + insert.
- `output` is pre-populated with an empty `Vec` per registered `ReaderId` at construct time.
- `metrics` is initialized with all counters at 0.
- `active_workers` starts at 0; incremented atomically before a worker begins processing a popped job, decremented after the job completes.

**Relationships**:
- Constructed once per `ParallelWalker::run` invocation, cloned into each spawned worker thread.
- Dropped after all worker threads have joined and metrics have been read.

**Lifecycle**: created at `ParallelWalker::run` entry, dropped at exit. Never persisted.

---

### `ParallelWalker` (new — modest wrapper around existing state)

The parallel-execution entry point. Extracts the existing `SharedWalker::run` recursion into a per-directory step (`walk_one_directory`) that workers call in parallel, gated by the shared-queue drain loop.

```rust
struct ParallelWalker<'reg, 'ex> {
    rootfs: PathBuf,
    registry: &'reg ReaderRegistry,
    exclude_set: &'ex ExclusionSet,
    max_depth: usize,
    /// Worker count = min(available_parallelism(), 2..). Serial
    /// fallback when this is 1 (see FR-001 + SC-006).
    worker_count: usize,
}
```

**Validation rules**:
- `worker_count >= 1`. When `1`, `run()` calls the pre-milestone serial `SharedWalker::run` unchanged (SC-006 fallback path).
- `worker_count > 1` triggers the parallel path: build `SharedWalkState`, spawn N workers, wait for drain, sort per-reader outputs, return.

**Relationships**:
- Constructed as an alternative to `SharedWalker` OR as an internal detail called by `SharedWalker::run` when parallelism is warranted. Implementation choice; interface remains `SharedWalker::run`.

**Lifecycle**: transient — constructed at classifier-caller entry, dropped after `run()` returns.

---

## Modifications to existing types

### `WalkerMetrics` (existing — migrated to atomics per R7)

Pre-milestone shape (approximate):
```rust
pub struct WalkerMetrics {
    dirs_visited: u64,
    files_visited: u64,
    // per-reader tick counters as HashMap<ReaderId, u64>
}
impl WalkerMetrics {
    fn tick_dir(&mut self) { self.dirs_visited += 1; }
    fn tick_file(&mut self, id: ReaderId) { *self.per_reader.entry(id).or_default() += 1; }
}
```

Post-milestone shape:
```rust
pub struct WalkerMetrics {
    dirs_visited: AtomicU64,
    files_visited: AtomicU64,
    per_reader: HashMap<ReaderId, AtomicU64>, // pre-populated at construct
}
impl WalkerMetrics {
    fn tick_dir(&self) { self.dirs_visited.fetch_add(1, Ordering::Relaxed); }
    fn tick_file(&self, id: ReaderId) {
        if let Some(counter) = self.per_reader.get(&id) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}
```

Callers change `&mut self` → `&self`; the semantic contract (monotone per-scan counters, emitted at scan end) is unchanged.

**Milestone change**: interior mutability via atomics; per-reader map pre-populated at construct so `tick_file` never needs to `.or_insert`.

---

### `SharedWalker` (existing — no signature change, internal logic split)

The public API of `SharedWalker::run` is unchanged. Internally, `walk_inner` is split into:
- `walk_one_directory(canonical, scope) -> Vec<SubtreeJob>` — pure per-directory step: check visited-set, read_dir, dispatch reader callbacks, return list of newly-discovered subdirs. Called by BOTH the serial fallback recursion AND the parallel workers.
- `run_serial()` — pre-milestone recursive descent; still calls the same `walk_one_directory` step but drives it via `for job in returned_jobs { run_serial(job.canonical) }`.
- `run_parallel()` — new: seeds queue, spawns workers, waits for drain, sorts outputs.

Entry `SharedWalker::run` decides which path based on `ParallelWalker::should_parallelize()` — a helper checking `available_parallelism() > 1` AND (post-first-dir) `first_dir_subdirs.len() > 1`.

**Milestone change**: internal refactor; public API preserved (FR-002).

---

## Diagram — orchestration under work-stealing

```text
                    ┌────────────────────────────────┐
                    │   ParallelWalker::run entry    │
                    └────────────┬───────────────────┘
                                 │
              1. Seed queue with rootfs SubtreeJob
                                 │
                                 ▼
    ┌────────────────────────────────────────────────────────┐
    │   SharedWalkState (all fields Arc-shared):             │
    │                                                         │
    │   queue: Arc<Mutex<Vec<SubtreeJob>>>       ← LIFO      │
    │   visited: Arc<Mutex<HashSet<PathBuf>>>    ← R3 guard  │
    │   output: Arc<HashMap<ReaderId,             ← per-reader│
    │              Mutex<Vec<PackageDbEntry>>>>     mutexes  │
    │   metrics: Arc<WalkerMetrics>              ← atomics   │
    │   active_workers: Arc<AtomicUsize>         ← drain     │
    └────────────────────────────────────────────────────────┘
                                 │
              2. Spawn N = available_parallelism() workers
                                 │
                                 ▼
       ┌───────────┐    ┌───────────┐    ┌───────────┐
       │ Worker 1  │    │ Worker 2  │... │ Worker N  │
       │           │    │           │    │           │
       │ loop {    │    │ loop {    │    │ loop {    │
       │   pop     │    │   pop     │    │   pop     │
       │   active++│    │   active++│    │   active++│
       │   walk_1  │    │   walk_1  │    │   walk_1  │
       │   push[]  │    │   push[]  │    │   push[]  │
       │   active--│    │   active--│    │   active--│
       │ }         │    │ }         │    │ }         │
       └─────┬─────┘    └─────┬─────┘    └─────┬─────┘
             │                 │                 │
             └─────────────────┼─────────────────┘
                               │
              3. Drain condition: queue empty AND active_workers == 0
                               │
                               ▼
              ┌────────────────────────────┐
              │  join() every worker;      │
              │  sort per-reader outputs;  │
              │  return.                   │
              └────────────────────────────┘

     Worker loop pseudocode:
       loop {
         let job = queue.lock().pop();       // exit if None + active_workers==0
         active_workers.fetch_add(1);
         let new_subdirs = walk_one_directory(job.canonical, job.scope);
         queue.lock().extend(new_subdirs);
         active_workers.fetch_sub(1);
       }
```

**Coordination invariants**:
- Every popped `SubtreeJob`'s `canonical` was NOT previously in `visited` — the worker checks-and-inserts under the visited mutex before pushing subdirs. Prevents duplicate descent.
- Every worker's `read_dir` output produces per-reader dispatch calls. Reader mutexes serialize per-reader-output appends but not each other's — the pip reader's mutex doesn't contend with the npm reader's mutex.
- Drain condition: BOTH `queue.is_empty()` AND `active_workers == 0` must hold simultaneously. A worker holding `active_workers > 0` may still push new jobs; drain waits for all in-flight jobs to complete.

---

## Transition table

| Pre-milestone state | Post-milestone state | Trigger |
|---|---|---|
| `SharedWalker::walk_inner` recursive DFS on the main thread | `walk_one_directory` per-directory step, driven by bounded thread-pool workers OR by the serial fallback (identical logic in both cases) | R1 + R2 |
| `visited: HashSet<PathBuf>` owned by `SharedWalker` | `visited: Arc<Mutex<HashSet<PathBuf>>>` shared across workers | R3 |
| `output: HashMap<ReaderId, Mutex<Vec<...>>>` (already Mutex-per-reader) | Same shape wrapped in `Arc` so workers can clone the handle | R4 (no logical change) |
| `WalkerMetrics` with `&mut self` counter methods | `WalkerMetrics` with `&self` methods over `AtomicU64` fields | R7 |
| Reader outputs emitted in walker-DFS traversal order | Reader outputs sorted per-reader after workers join | R5 + FR-007 + SC-004 |
| Walker panics propagate via the main thread's stack unwind | Worker panics captured in `JoinHandle::join()`, re-propagated via `tracing::error!` + `anyhow::bail!` | R6 + FR-008 |

Every column-1 → column-2 transition preserves the m664 reader-registration API (`ReaderRegistration`, `on_file`, `on_dir`, `descend_into`) unchanged (FR-002).
