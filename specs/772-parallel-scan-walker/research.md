# Phase 0 Research — m772 parallel scan_fs walker

**Feature**: 772-parallel-scan-walker
**Status**: Complete
**Date**: 2026-09-04

Clarify step resolved the highest-impact ambiguity (Q1: work-stealing vs top-level fan-out vs depth-N pre-fan-out). Remaining research items are technical-decision confirmations to lock in before implementation. Each entry follows the Decision / Rationale / Alternatives format.

---

## R1 — Worker count formula

**Decision**: Use `std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)`. When the tree has ≥ 2 discoverable subdirectories AND the returned count ≥ 2, spawn that many worker threads. Otherwise fall back to the pre-milestone serial `walk_inner` recursion.

**Rationale**:
- Mirrors m771 US2's `worker_count` helper at `mod_why.rs::worker_count` (research R3 in the m771 plan) — reviewer familiarity is high.
- `available_parallelism()` was stabilized in Rust 1.59, well below waybill's MSRV. Returns `NonZeroUsize`.
- The `unwrap_or(1)` guards unusual embedded targets where the primitive can fail.
- Fallback on tiny trees avoids thread-spawn overhead exceeding walk cost. The k8s fixture (55K files, 39 go.mod files) has ≥ 10 top-level directories; the fallback only activates for genuinely single-child trees.

**Alternatives considered**:
- **Fixed pool size (e.g., 8)**: rejected — doesn't adapt to 2-core CI runners or 16-core dev machines.
- **`num_cpus` crate**: rejected — adds a Cargo dep for zero functional gain over `available_parallelism()`.

---

## R2 — Work-queue shape

**Decision**: `Arc<Mutex<Vec<SubtreeJob>>>` where `SubtreeJob = (PathBuf /* canonical */, Option<HashSet<ReaderId>> /* descend_into scope */)`. Workers acquire the mutex, `pop()` the last element (LIFO for cache locality — recently-pushed subdirs are close to previously-visited ones), release the mutex, do work, re-acquire to push new subdirs.

**Rationale**:
- Per clarify Q1, work-stealing shape with dynamic queue growth.
- LIFO (stack) over FIFO (queue): LIFO gives depth-first-like locality per worker, which matches operating-system prefetch behavior for sibling directories. FIFO would spread each worker across the tree horizontally — worse for OS-level dir-cache warmth.
- `Vec` + `Mutex` chosen over `crossbeam::deque` because: no new Cargo dep (FR-010). Contention is bounded — each mutex hold is O(1) push/pop; `read_dir` (the actual work) is O(N children) OUTSIDE the lock.
- `Option<HashSet<ReaderId>>` payload preserves m664 Contract C10 (descend_into scope restriction) — when a normally-skipped dir is descended, the scope MUST propagate to its children.

**Alternatives considered**:
- **`crossbeam::deque::Injector` + per-worker `Worker<T>`**: canonical work-stealing shape from the rayon ecosystem. Rejected because crossbeam is a new Cargo dep.
- **Per-worker queues with random steal**: complex, requires atomic empty-flag coordination. Overkill for the bounded contention expected.
- **`Arc<Mutex<VecDeque>>` (FIFO)**: rejected per LIFO locality argument above.

---

## R3 — Visited-set sharing

**Decision**: `Arc<Mutex<HashSet<PathBuf>>>`. Worker canonicalizes a directory, then does a lock-check-insert-release cycle. If the insert reports "already present", the worker skips that subtree entirely.

**Rationale**:
- Preserves the m054/m114 symlink-loop safety invariant across worker boundaries — critical for FR-003 + SC-005. Per-worker visited-sets would miss cross-subtree loops (worker A visits `dir1/` which symlinks to `dir2/`; worker B visits `dir2/` — both would descend into the loop from opposite ends without shared state to catch it).
- Contention is bounded: 55K files × ~O(1) mutex hold = tens of milliseconds total contention over an 18s walker. Worth the safety win.
- The insert-and-check pattern (`HashSet::insert` returns `bool`) is one atomic operation under the mutex; no separate contains-then-insert race.

**Alternatives considered**:
- **`Arc<DashMap<PathBuf, ()>>` or `flurry::HashSet`**: adds Cargo dep. Rejected.
- **Sharded shared set (16-way lock stripe on `path.hash() % 16`)**: pure stdlib, but 3× more code for the same practical throughput at bounded scan sizes. Deferred — can revisit if benchmarks show visited-set contention as a bottleneck.
- **Per-worker sets + merge-at-end**: rejected because it doesn't catch cross-subtree symlink loops during discovery.

---

## R4 — Reader dispatch under concurrency

**Decision**: Reuse the existing `output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>` collectors as-is. Workers acquire the per-reader mutex when appending. No new collector abstraction (FR-006).

**Rationale**:
- The m664 walker ALREADY uses `Mutex<Vec<PackageDbEntry>>` per-reader for output. Contention is per-reader — the npm reader's mutex only conflicts with itself, not with the pip reader's mutex.
- Per-reader lock granularity is the sweet spot: fewer reader lock-contentions than a global output lock; less overhead than per-worker output buffers + merge.
- Sort-at-end (FR-007 + SC-004) happens in `SharedWalker::run` after all workers join, converting the per-reader `Vec` into a deterministic order.

**Alternatives considered**:
- **Per-worker output buffers with merge-at-end**: better lock-free throughput; more code + higher peak memory. Rejected as premature optimization.
- **`SegQueue<PackageDbEntry>` per reader**: adds crossbeam dep.

---

## R5 — Sort-key strategy for deterministic emit order (FR-007)

**Decision**: Each reader is responsible for sorting its own `Vec<PackageDbEntry>` before returning. Where a PURL string is present, sort by `entry.purl.as_str()` alphabetically. Where PURL is absent (source-tier entries), sort by the reader's natural key (e.g., `source_path` field). If a reader has neither, use the `name` field as tiebreak — this is required to exist per the `PackageDbEntry` shape.

**Rationale**:
- Simplest realization of SC-004 (byte-identity across runs).
- Sort work is O(N log N) per reader; on k8s at ~500 entries per reader, sort adds ~microseconds. Negligible vs the ~18s walker cost being reduced.
- Sorting inside each reader (rather than a caller-side sort) preserves reader-specific tiebreak semantics — a reader that ships multiple `PackageDbEntry`s for the same PURL (rare but happens) can retain its intended internal ordering.

**Alternatives considered**:
- **Sort caller-side in `SharedWalker::run`**: violates the per-reader natural-key principle. Some readers already sort in-flight; forcing a caller-side re-sort would need a stable tiebreak.
- **BTreeMap<Key, Vec<...>> collectors**: adds continuous sort overhead during dispatch; rejected as premature optimization.

---

## R6 — Panic propagation strategy (FR-008)

**Decision**: Workers are spawned via `thread::spawn`, returning `JoinHandle<()>`. Main thread `join()`s each handle after the queue drains. If any `JoinHandle::join()` returns `Err(_)`, the walker propagates the panic via `std::panic::panic_any` OR returns an `anyhow::Error` up the call stack — implementation chooses whichever fits the caller-site error handling (SharedWalker's caller is `scan_fs::scan_path` which today has permissive-tolerance for walker errors).

**Rationale**:
- Fail-fast per Principle III + FR-008. A panic in one worker MUST NOT be silently absorbed — the resulting SBOM would silently miss subtrees.
- `thread::join` gives us the panic message; log via `tracing::error!` + propagate.
- No `catch_unwind` needed; panic in a spawned thread doesn't tear down the process, and the `join` result tells us what happened.

**Alternatives considered**:
- **`std::panic::catch_unwind` per worker**: rejected because it complicates control flow without adding safety — a bare `thread::spawn` already isolates the panic, and `join` surfaces it.
- **Silent tolerance (log-and-continue)**: rejected per FR-008.

---

## R7 — Metrics aggregation under concurrency

**Decision**: Migrate `WalkerMetrics` (currently `tick_dir()` / `tick_file()` / etc. on a `&mut self` field) to `Arc<WalkerMetrics>` where each counter is an `AtomicU64` (`std::sync::atomic::AtomicU64::fetch_add(1, Ordering::Relaxed)`). Workers share the `Arc`.

**Rationale**:
- Preserves the FR-013 metric-emission contract (m664 walker emits per-reader tick counts).
- `Relaxed` ordering is correct for monotone counters; no total-order guarantee needed.
- Zero new Cargo dependencies.
- Atomics vs `Mutex<u64>` per counter: atomics are lock-free, ~10× faster on the hot path (`tick_file()` is called per file).

**Alternatives considered**:
- **Per-worker sum-at-end**: complicates the metrics API (`tick_file` needs a worker-id argument); rejected as premature complexity.
- **Global `Mutex<WalkerMetrics>`**: rejected — every `tick_file()` would contend the same lock. On a 55K-file scan that's a lot of lock traffic.

---

## R8 — Test fixture strategy for FR-003 symlink-loop coverage

**Decision**: Reuse the existing `walks_symlink_loop_without_hanging` regression fixture at `waybill-cli/src/scan_fs/package_db/golang/go_binary.rs::tests::walks_symlink_loop_without_hanging` (m054 fixture). Add a new integration-test-side symlink-loop fixture at `waybill-cli/tests/fixtures/walk_registry/symlink_loop_cross_subtree/` that creates two sibling directories mutually referring to each other — the case that ONLY the shared visited-set catches.

**Rationale**:
- Existing m054 test covers single-subtree loops; the new fixture covers cross-subtree loops (the specific case R3's shared visited-set was chosen for).
- Two workers walking the two sibling directories concurrently is the scenario that unit-test-scale race conditions can exercise.

**Alternatives considered**:
- **Only reuse the m054 fixture**: insufficient — that fixture is a single-subtree loop, doesn't exercise cross-worker interaction.
- **Property-based test with random symlink graphs**: over-engineering for FR-003; the cross-subtree fixture + m054 fixture together cover the invariant.

---

## R9 — Empirical validation methodology (SC-001)

**Decision**: Same protocol as m771 (research R8 there). Fresh clone of `kusari-sandbox/test-kubernetes`, warm cache, macOS aarch64 8-core reference host, `time` command, release build. Update `docs/perf/baseline.json` (m669) post-merge.

**Rationale**: Reproducibility — the same command that populated issue #791 validates the fix.

---

## Findings summary

Every technical unknown that the spec surfaced is resolved:
- Worker count formula (R1)
- Work-queue shape + LIFO ordering (R2)
- Visited-set sharing strategy (R3)
- Reader dispatch under concurrency (R4)
- Sort-key strategy for determinism (R5)
- Panic propagation (R6)
- Metrics under concurrency (R7)
- Symlink-loop test fixture (R8)
- Empirical validation (R9)

Zero `NEEDS CLARIFICATION` markers remain. Ready for Phase 1.
