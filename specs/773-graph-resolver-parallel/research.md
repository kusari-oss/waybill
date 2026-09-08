# Phase 0 Research — m773 parallel graph_resolver

**Feature**: 773-graph-resolver-parallel
**Status**: Complete
**Date**: 2026-09-05

The spec's design decisions are largely pre-baked by the m771 US2 precedent (`waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass`). Research consolidates the technical choices to lock in before implementation. Each entry follows the Decision / Rationale / Alternatives format.

---

## R1 — Concurrency primitive

**Decision**: Bounded thread pool using `std::thread` + `Arc<Mutex<Vec<WorkspaceJob>>>` work queue + `std::sync::mpsc::channel` for result collection. Worker count = `std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)`.

**Rationale**:
- Zero new Cargo dependencies (FR-011). Rayon / tokio / crossbeam not needed.
- Verbatim reuse of the m771 US2 pattern proven at `apply_go_mod_why_pass` — reviewer familiarity is maximal.
- `std::thread::scope` from Rust 1.63+ lets workers borrow references bound to stack lifetimes (no `'static` requirement on workers). Same pattern m772's attempted parallel walker used; even though m772 was rolled back, the concurrency primitive itself was sound.
- `available_parallelism()` returns `NonZeroUsize` on all four supported host classes.

**Alternatives considered**:
- **`rayon::par_iter` on the workspace slice**: cleaner code but adds a workspace-level Cargo dep. Rejected per FR-011.
- **`tokio` async runtime**: overkill for CPU-bound work with synchronous subprocess calls inside; adds massive dep surface. Rejected.
- **Fixed pool size**: doesn't adapt to CI runners vs dev machines.

---

## R2 — Worker-count helper reuse

**Decision**: Reuse the existing `mod_why::worker_count(workspace_count: usize) -> usize` helper introduced in m771 US2 at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`. It already implements the exact formula (`min(workspace_count, available_parallelism())` clamped to `[1, workspace_count]`; zero-workspace case returns 0). Public via `pub fn`.

**Rationale**:
- Zero new code for identical semantics.
- Single source of truth for the concurrency-cap formula across m771 US2 (classifier) and m773 (resolver).
- Already unit-tested at `mod_why.rs::tests::m771_worker_count_bounded_by_available_parallelism`.

**Alternatives considered**:
- **Duplicate the helper in `legacy.rs`**: rejected. Same formula, same tests would apply — no reason to fork.

---

## R3 — Determinism strategy for Phase 2 reduce (FR-004)

**Decision**: Assign each workspace a stable `workspace_index: usize` before spawning workers (0-indexed sort of the input `parsed_roots` slice). Workers include their index in the mpsc payload. Main thread collects all results into a `Vec<Option<ResolveResult>>` sized to `parsed_roots.len()`, indexed by `workspace_index`. After the mpsc drains, iterate the Vec in index order for Phase 2 reduce.

**Rationale**:
- Deterministic reduce order regardless of worker completion timing.
- No sort needed at reduce time — Vec-by-index is O(1) placement + O(N) drain.
- The `parsed_roots` slice is already the pre-milestone iteration order; using its indices means pre-milestone output byte-identity is preserved trivially (SC-002 + SC-004).
- Alternative "sort by workspace path at reduce time" is O(N log N) and re-sorts something the caller already had in a deterministic order.

**Alternatives considered**:
- **Sort by workspace path at reduce**: adds O(N log N) work; if the input `parsed_roots` order is already deterministic (it is — comes from a sorted walk), this re-sort is redundant.
- **Use `BTreeMap<usize, ResolveResult>` for the collector**: works but O(log N) per insert vs O(1) for Vec-by-index. Marginal.

---

## R4 — Shared state across workers

**Decision**:
- `resolver: Arc<GraphResolver>` — shared read-only handle. `GraphResolver` contains only a `GraphResolverConfig` (immutable); `resolve()` is `&self`.
- `cache: Arc<GoModCache>` — shared read-only handle. `GoModCache` methods are all `&self`.
- `job_queue: Arc<Mutex<Vec<WorkspaceJob>>>` — the pool of workspaces awaiting assignment.
- `tx: mpsc::Sender<ResolveResult>` — result collector; each worker clones its own `Sender`.

No `Arc<Mutex<>>` needed for any of `signals` / `entries` / `out` / `seen_purls` / `backfilled_paths` — those are Phase 2 reduce state and remain single-threaded on the main thread.

**Rationale**:
- The resolver and cache are already thread-safe by virtue of `&self`-only APIs (compile-time proof; spec FR-009 + FR-010 pin these API surfaces unchanged).
- Post-loop state confinement to Phase 2 avoids the entire class of Arc-Mutex ceremony m772's rolled-back walker parallelism attempted.
- Send+Sync-ness of `GraphResolver` and `GoModCache` will be asserted at compile time via a `assert_send_sync::<T>()` helper in the new integration test.

**Alternatives considered**:
- **Per-worker cache clones**: rejected. `GoModCache` may contain paths to real cache files; re-discovering them per worker is wasteful. Arc-sharing is trivially correct.
- **`Arc<RwLock<>>` for the cache**: rejected. Read-only means no writer contention exists; a plain `Arc<T>` is simpler and doesn't pay atomics overhead on reads.

---

## R5 — Panic propagation (FR-006)

**Decision**: Workers are spawned via `std::thread::scope`, returning per-worker `ScopedJoinHandle`. Main thread `join()`s each handle inside the scope block. If any `.join()` returns `Err(payload)`, the resolver loop propagates via `std::panic::resume_unwind(payload)` — surfacing the panic to the caller (`scan_fs::scan_path`) with the offending worker's identifier logged via `tracing::error!`.

**Rationale**:
- Fail-fast per Constitution Principle III + FR-006. A panic in one workspace analysis MUST NOT be silently absorbed — the resulting SBOM would silently miss that workspace's contribution.
- `std::thread::scope` from Rust 1.63+ gives borrow-checked scoped threads; no `'static` bounds needed on worker bodies.
- Verbatim m771 US2 pattern (see the classifier's worker `join` loop at `apply_go_mod_why_pass`).
- The workspace's absolute path is available at panic time via the `workspace_index → parsed_roots[i].0` lookup on the main thread.

**Alternatives considered**:
- **`std::panic::catch_unwind` inside each worker**: rejected. Isolates the panic but converts it to a silent-return path unless the caller re-checks; more complex without safety gain.
- **`thread::spawn` (non-scoped)**: rejected. Would require `'static` bounds; forcing `Arc<...>` clones of the resolver + cache, but every worker would need its own clone rather than a shared borrow. Scoped threads are strictly simpler.

---

## R6 — Integration test fixture

**Decision**: Reuse the m771 mod_why_scaling fixture at `waybill-cli/tests/fixtures/golang/mod_why_scaling/` (4 workspaces: 3 members under `go.work` + 1 loose). Its 4-workspace shape triggers the parallelism gate (worker_count ≥ 2 AND workspace_count ≥ 2). Add a new integration test binary `waybill-cli/tests/graph_resolver_parallel_773.rs` that:

1. Runs waybill against the fixture with `WAYBILL_GO_MOD_WHY_BUDGET_MS=1` (short-circuits the classifier per m771's convention so the test focuses on the resolver path).
2. Asserts the scan exits 0 + emits at least 4 workspace-summary log lines (one per workspace).
3. Runs twice back-to-back with output masking and asserts byte-identity (SC-004 direct verification).

**Rationale**:
- Reuses the m771 fixture — no new synthetic fixture needed. Zero churn to `tests/fixtures/`.
- The 4-workspace shape is enough to trigger the parallel path on any 2+ CPU machine.
- Byte-identity verification via double-run is the simplest SC-004 test.

**Alternatives considered**:
- **Larger new synthetic fixture (20+ workspaces)**: rejected. Doesn't add safety over the 4-workspace fixture; wastes CI seconds per run.
- **Use the full Kubernetes clone as an integration-test fixture**: rejected — 380 MB per CI run is unacceptable. Empirical validation happens via the m669 benchmark harness locally.

---

## R7 — `Send + Sync` compile-time assertion

**Decision**: Add a small `assert_send_sync<T: Send + Sync>()` helper in the new integration test file and call it with `GraphResolver` and `GoModCache` at test-startup time. Compile-time proof that these types are safe to share across threads. Test fails at compile if a future patch adds a `!Send` or `!Sync` field.

**Rationale**:
- Catches regression at compile time, not at debug-hunt time.
- Zero runtime cost — the assertion generates no code beyond the type-check.
- Complements FR-009 / FR-010 (the API surface is preserved, but the underlying type's Send/Sync-ness could still regress).

**Alternatives considered**:
- **Runtime assertion via `std::any::TypeId`**: rejected. Doesn't fire at compile time; regression would only surface when the test runs (and even then, only if actually spawning threads).

---

## R8 — Empirical validation methodology (SC-001)

**Decision**: Same protocol as m771 (research R8 there). Fresh clone of `kusari-sandbox/test-kubernetes`, warm cache, macOS aarch64 8-core reference host, `time` command, release build. Two measurements:
1. Walker-isolated: `time waybill --offline --no-go-mod-why sbom scan --path /tmp/k8s --no-deep-hash --format cyclonedx-json --output /tmp/out.cdx.json`
2. Default: `time waybill --offline sbom scan ...`

Update `docs/perf/baseline.json` (m669) post-merge in a separate polish PR.

**Rationale**:
- Reproducibility — the same commands that populated issue #793 validate the fix.
- Per-phase log decomposition (via the methodology at `docs/development/perf-methodology.md`) also runs to confirm the resolver's contribution dropped from ~15s to ~2s.

---

## Findings summary

Every technical unknown that the spec surfaced is resolved:
- Concurrency primitive (R1) — `std::thread::scope` + mpsc, m771 US2 pattern
- Worker-count helper reuse (R2) — reuse `mod_why::worker_count`
- Determinism strategy (R3) — workspace_index-ordered Vec collector
- Shared state model (R4) — Arc-shared read-only, no locks needed
- Panic propagation (R5) — scoped join + `resume_unwind`
- Test fixture (R6) — reuse m771 mod_why_scaling
- Send+Sync guarantee (R7) — compile-time assertion helper
- Empirical validation (R8) — mirrors m771 protocol

Zero `NEEDS CLARIFICATION` markers remain. Ready for Phase 1.
