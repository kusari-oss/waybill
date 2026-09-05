# Phase 0 Research — m774 parallel source-import collection

**Feature**: 774-parallel-source-imports
**Status**: Complete
**Date**: 2026-09-04

Zero `NEEDS CLARIFICATION` remain from `spec.md` after `/speckit.clarify` Q1 → Option A. Research below records the design decisions the plan phase locks in for `/speckit.tasks` to enumerate.

---

## R1 — Reuse the m771 US2 pattern verbatim

**Decision**: Adopt the m771 US2 shape exactly: bounded `std::thread::scope` pool + `Arc<Mutex<Vec<Job>>>` work queue + `mpsc::channel` reducer + `mod_why::worker_count()` sizing helper. No custom variations.

**Rationale**: m771 US2 landed in `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass` (lines ~1218–1280) and shipped stable across 3 PRs (#788/#789/#790). It's the established shape for waybill's synchronous per-workspace parallelization. Reusing it verbatim means:
- Zero risk of introducing a new pattern that requires separate review of concurrency correctness.
- Team familiarity — anyone who reviewed m771 will read m774 as "the same shape applied to a different call site."
- The `worker_count()` helper at `mod_why.rs:204` is already unit-tested (`m771_worker_count_bounded_by_available_parallelism` at `mod_why.rs:1089`); reuse means no new sizing-logic tests needed.

**Alternatives considered**:
- **Custom `rayon`-style parallel iterator**: would require adding the `rayon` crate. Rejected — violates FR-010 (zero-new-deps).
- **`tokio` async pool**: would require adding `tokio` runtime to this call path. Rejected — violates FR-013 (no `tokio` in resolver path) and clashes with the sync m771 pattern.
- **Manual `std::thread::spawn` without `scope`**: would require `'static` bounds on borrowed data, forcing `Arc` clones of `known_modules` and `project_root`. Rejected — `std::thread::scope` (stable since Rust 1.63) is strictly superior for this borrow shape.

---

## R2 — Panic propagation via `catch_unwind` per job + `std::thread::scope` auto-join

**Decision** (REVISED per remediation F1): Each worker wraps its per-job body in `std::panic::catch_unwind(AssertUnwindSafe(...))`. On `Err(payload)`, the worker logs the workspace's absolute path AND `workspace_index` (both preserved from the popped `WorkspaceImportJob`) via `tracing::error!`, then calls `std::panic::resume_unwind(payload)`. The unwind propagates through `std::thread::scope`'s automatic join at scope-close, terminating the scan with a non-zero exit — identical to pre-milestone failure semantics. Fail-fast per FR-007.

**Rationale**: The prior draft of R2 relied on explicit `ScopedJoinHandle::join()` on each pool worker, then logged from the main thread. That approach breaks FR-007's "log the workspace's absolute path" requirement because pool workers process MULTIPLE `WorkspaceImportJob`s over their lifetime — by the time `.join()` sees an `Err`, the workspace_index of the offending job is gone (consumed from the queue at pop time). The remediation moves the log line INTO the worker, where the current job's `project_root` is still in scope. `resume_unwind` after logging preserves the pre-milestone diagnostic shape verbatim.

Pre-milestone, a panic inside `collect_*_imports` unwinds the caller's stack, terminating the scan with a non-zero exit. Post-milestone semantic MATCHES because:
- `catch_unwind` momentarily catches so the log can fire, but `resume_unwind` immediately re-raises with the original payload.
- `std::thread::scope` guarantees all spawned workers `join()` before the scope block returns (RAII); the re-raised unwind propagates out of the scope block into the enclosing `pub fn read` call, matching the pre-milestone unwind path.

Consequence for the m669 corpus + CI: identical exit behavior on the pathological "one workspace has malformed UTF-8 in a `.go` file" case, PLUS the operator now gets a workspace-scoped diagnostic that names the failing project_root — strictly more useful than pre-milestone stderr.

**Alternatives considered**:
- **`ScopedJoinHandle::join()` on main thread + log after scope**: rejected — loses workspace_index granularity (see F1 remediation above).
- **`Result`-returning wrapper for `collect_*_imports`**: rejected — violates FR-005 (signature preservation) and requires a shim layer that hides the failure origin.
- **Swallow panic + continue**: rejected — violates Principle III (Fail Closed) and FR-007.

**Testability note**: The `catch_unwind` boundary is compatible with `AssertUnwindSafe` because per-job state (`prod`, `test`, `job`) is either freshly-constructed inside the closure or a plain-owned handle. `HashSet`, `PathBuf`, `usize`, `&[String]` all satisfy the (implicit) unwind-safety contract for this usage — nothing shared-mutable escapes the closure.

---

## R3 — `worker_count()` helper reuse

**Decision**: Call `mod_why::worker_count(parsed_roots.len())` directly. No wrapper, no rename, no per-milestone copy. Semantics: `min(N, available_parallelism()).max(1)` for N ≥ 1; returns 0 for N == 0.

**Rationale**: The helper is already `pub` in `mod_why.rs:204` and its semantics match this call site's needs exactly. Reuse:
- Avoids duplicating the `available_parallelism()` fallback logic (returns 1 on unusual embedded targets per the doc-comment at `mod_why.rs:198–199`).
- Makes it trivial to future-tune the sizing policy in one place if a subsequent milestone wants an env-var override.
- Sidesteps the temptation to invent a m774-specific naming (`import_worker_count`, `parallel_import_count`, etc.) that would fragment the codebase.

**Alternatives considered**:
- **Inline `min(N, available_parallelism())`**: adds copy of `available_parallelism()` fallback logic; rejected as duplication.
- **New helper `imports_worker_count`**: rejected — same math, different name, no benefit.

---

## R4 — `known_modules` sharing via `std::thread::scope` lifetime

**Decision**: Pass `known_modules: &'a [String]` (or `&'a Arc<[String]>` if the borrow can't be threaded — verified below) to each worker via the closure capture. No clone, no `Arc` wrap unless the pre-loop construction site already `Arc`s it.

**Rationale**: `known_modules` is read-only across the parallel phase. `std::thread::scope` closure captures borrow the outer scope's data with lifetime `'scope`; workers see `&'scope [String]` for free. No sync primitive needed. This matches m771 US2's pattern where `Arc<GoModCache>` is threaded through closures (though m774's `known_modules` is even simpler — a plain slice, not an owned wrapper).

Empirical check (grep result at plan time): `known_modules` is a local `Vec<String>` in `pub fn read` at `legacy.rs:1697` scope (built before the loop from `parsed_roots`). It's declared `let known_modules = ...` — no `Arc`. Perfectly borrow-shareable via scope lifetime.

**Alternatives considered**:
- **`Arc<Vec<String>>`**: would work but adds a heap allocation the borrow-shared path avoids. Rejected — no benefit over borrow.
- **`Arc<[String]>`** (immutable slice): same as above; adds allocation for zero win. Rejected.
- **Per-worker clone**: would allocate N × `known_modules` copies (N = workers). Rejected — wasted memory.

---

## R5 — HashSet merge is commutative + deterministic

**Decision**: Phase 2 reduce merges per-worker `HashSet<String>` accumulators into `signals.production_imports` + `test_imports` via successive `HashSet::extend()` calls. Iteration order over the merged sets is `HashSet`'s default (random-seeded), but every DOWNSTREAM consumer that iterates them either:
1. Sorts via `BTreeSet` conversion or explicit `.sort()` before emission (e.g., `apply_go_production_set_filter` at `mod.rs:702–734` iterates the entries slice, checking `test_only_imports.contains(&e.name)` — read-only lookup, order-independent), OR
2. Iterates in element-independent semantics (`test_imports.difference(&production_imports)` at `legacy.rs:2281` — set difference is order-independent).

**Rationale**: Set-union is mathematically commutative and associative. The final merged set's ELEMENT CONTENT is identical regardless of merge order; only the internal iteration order (opaque, seed-dependent) differs. Every downstream consumer verified as order-independent → determinism preserved by construction.

**Validation plan**: SC-004 (double-run byte-identity) confirms empirically. If a downstream consumer turns out to be order-sensitive (unlikely — we grepped every consumer during Phase 0), the reduce can trivially switch to `BTreeSet` merge without algorithmic change.

**Alternatives considered**:
- **`BTreeSet` merge**: guarantees deterministic iteration by construction. Rejected as unnecessary complexity — the two consumers are proven order-independent. Retained as a defense-in-depth fallback if SC-004 catches an unexpected regression.
- **Sort at reduce time into `Vec<String>`**: would require changing `signals.production_imports` from `HashSet` to `Vec` — signature change on downstream code. Rejected — violates FR-005 spirit.

---

## R6 — Interaction with the m773 rollback: resolver stays serial

**Decision**: `resolver.resolve()` remains inline in the main serial loop at `legacy.rs:1802`. NOT parallelized. Per Clarifications Q1 → Option A, only `collect_*_imports` moves to the post-loop parallel phase.

**Rationale**: m773's rollback established that:
- `resolver.resolve()` is 118ms cumulative across 38 workspaces (0.67% of loop). Parallelizing it saves nothing user-visible.
- Broader refactor of the loop body has hidden costs (m773's byte-identity work took a full spec cycle).
- The m774 profiling table shows exactly one dominant phase (`collect_*_imports` at 95.4%); attacking it in isolation gives the full Amdahl win with the smallest surface change.

Consequence for m774: the main serial loop is UNCHANGED except for extracting the two `collect_*_imports` calls (which move to the new post-loop phase). Resolver, entries build, `+incompatible` filter, `stamp_go_transitive_annotations`, `build_main_module_entry`, orphan backfill, seen_purls dedup, `out.push`, and Issue #250/#251/#255 log lines all stay on their current serial code path. This is a strict subset-extraction, not a whole-body refactor.

**Alternatives considered**:
- **Hybrid parallel (Option C from clarify)**: also parallelize `resolver.resolve()`. Rejected per m773's empirical outcome — the 118ms gain doesn't justify the second parallelization site.
- **Whole-body parallel (Option B from clarify)**: matches m773's shape. Rejected per Clarifications Q1.

---

## R7 — Determinism verification protocol (SC-004)

**Decision**: SC-004 verifies determinism via two independent `waybill sbom scan` invocations against the same fixture on the same host, comparing outputs after masking `serialNumber` + `created` per the m669 protocol. New test at `waybill-cli/tests/collect_imports_parallel_774.rs::m774_determinism_across_runs` codifies this at CI-time using a `tempfile::tempdir` + synthetic multi-workspace go.mod fixture (small enough to run in CI budget — 3 workspaces, 20 `.go` files each).

**Rationale**: The `HashSet` iteration order is randomly seeded but the merged content is content-identical (per R5). The consumers are order-independent (per R5). Determinism therefore holds by construction — the test empirically verifies this claim.

**Alternatives considered**:
- **Sort-at-reduce-time (BTreeSet)**: also delivers determinism, at the cost of the reduce complexity noted in R5. Kept as fallback; won't be adopted unless R5's empirical check fails.

---

## R8 — Test harness for panic-fail-fast (FR-007)

**Decision**: Test at `waybill-cli/tests/collect_imports_parallel_774.rs::m774_worker_panic_fails_fast` constructs a synthetic multi-workspace fixture where one workspace's directory tree contains a symlink loop or a `.go` file with malformed UTF-8 crafted to trigger a panic (if such a case exists in `collect_*_imports` today) OR wraps the collect call in a `#[cfg(test)] panic_hook` shim if no natural panic path exists.

**Empirical check pending**: `collect_*_imports` at `legacy.rs:2503–2544` may or may not panic on malformed input; today it likely uses `String::from_utf8_lossy` and silently continues. If NO natural panic path exists, the test uses a `#[cfg(test)]`-gated panic-injection helper inserted at the top of `collect_production_imports` guarded by a `TEST_INJECT_PANIC` thread-local. This is standard practice for testing panic-propagation logic (see `waybill-cli/src/trace/tests.rs` for a similar pattern).

**Rationale**: FR-007 requires fail-fast propagation of worker panics. The contract MUST be tested; if the current code has no natural panic path, we inject one under `#[cfg(test)]`. The test verifies: (a) scan exits non-zero, (b) `tracing::error!` log with the workspace's absolute path is emitted BEFORE unwinding.

**Alternatives considered**:
- **Skip the test, rely on FR-007 code review**: rejected — panic propagation is a load-bearing invariant per Principle III (Fail Closed); it needs empirical coverage.

---

## R9 — Degenerate single-workspace path (NFR-002)

**Decision**: When `parsed_roots.len() <= 1`, the parallel phase code path SHORT-CIRCUITS to the pre-milestone serial call shape: `let mut prod = HashSet::new(); let mut test = HashSet::new(); collect_production_imports(root, 0, &known_modules, &mut prod); collect_test_imports(root, 0, &known_modules, &mut test); merge into signals.production_imports + test_imports;`. No `std::thread::scope`, no `mpsc::channel` construction, no `Mutex` allocation. Same as pre-milestone latency (verified by SC-005).

**Rationale**: Thread-spawn overhead (`~50-100μs`) is significant relative to a small single-workspace scan (`go-module-medium` fixture is ~200ms total). Short-circuiting eliminates the overhead entirely for the common single-workspace case.

**Alternatives considered**:
- **Always spawn `worker_count() == 1` thread**: adds ~100μs unconditionally. Rejected — measurable regression on single-workspace scans.
- **Inline call in worker_count==1 case only**: same as decision, expressed differently.

---

## R10 — Test-fixture strategy for CI parallelism verification

**Decision**: New integration test at `waybill-cli/tests/collect_imports_parallel_774.rs` uses `tempfile::tempdir` + 3 synthetic go.mod workspaces (small enough for CI budget). Each workspace has:
- 1 `go.mod` declaring `module github.com/kusari-oss/waybill-fixture-m774-ws-N`
- 10-20 `.go` production files importing 2-3 modules from `known_modules`
- 2-3 `_test.go` files importing 1-2 additional test-only modules

Assertions:
- `signals.production_imports` union across all workspaces equals the union of per-workspace prod imports.
- `signals.test_only_imports = (test_imports_union - production_imports_union)`.
- Two independent scans produce byte-identical CDX + SPDX 2.3 + SPDX 3 outputs (SC-004).
- Under worker-panic injection, scan exits non-zero (FR-007).
- Single-workspace fixture path takes ≤ pre-milestone p50 + 3% (NFR-002; approximated by comparing degenerate-path wall time against a `parsed_roots.len() == 2` path scaled inversely).

**Rationale**: Fixture-based tests are the shipped pattern for every prior perf-milestone (m669 corpus, m090 waybill-test-fixtures). The synthetic-name convention (`waybill-fixture-*`) matches the feedback memory `feedback_fixture_synthetic_package_names` — no real coordinates.

**Alternatives considered**:
- **Reuse `test-kubernetes` in CI**: 39-workspace, 30k-file fixture is far too big for CI budget. Rejected.
- **Property-based fuzzing with `proptest`**: overkill for a set-union merge; rejected as violating FR-010.

---

## Non-decisions / explicit deferrals

- **Env-var override for worker count** (`WAYBILL_MAX_IMPORT_WORKERS`): deferred. m771 shipped without one; no operator has requested one. Trivial to add in a follow-up if needed.
- **Size-aware worker scheduling** (assign biggest workspaces first): deferred. Worker imbalance is acceptable at 3s vs 17.5s (per Assumptions). Attack only if SC-001 misses.
- **Walker-side single-pass source-file inventory** (Option B from pre-spec analysis): deferred, explicitly out of scope per FR-012. Would require m664 SharedWalker extension; separate milestone if the m774 residual (~3s parallel walkers) becomes worth attacking.
- **Panic-payload string capture in FR-007 log line**: deferred to implementation choice. `resume_unwind` preserves the panic string in stderr via the default panic hook; explicit capture in the `tracing::error!` line is a nice-to-have. Task file will include as an implementation note.
- **Cross-`pub fn read` invocation log aggregation**: deferred. FR-014 requires one log per invocation; multi-invocation scans (rare) emit N lines. Downstream tooling shape can be adjusted separately.
