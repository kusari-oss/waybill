> # ⚠️ ROLLED BACK — IMPLEMENTED, MEASURED, REVERTED
>
> This milestone was fully implemented and then **rolled back**. No part
> of it shipped.
>
> **Why**: it worked and did not help. `resolver.resolve()` went from
> 15s to 20ms — 400× on the targeted slice — and total wall time did not
> move. The 400ms-per-workspace figure came from the inter-line delta on
> a log line that fires at the *end* of `GraphResolver::resolve()`, so it
> captured the whole loop iteration rather than the resolver's own cost.
> Decomposed: `resolver.resolve()` was 1ms, and
> `build_entries + main_module_entry + annotations` was the other 399ms.
>
> Kept as the worked example behind the log-line-as-cost-proxy pitfall in
> `docs/development/perf-methodology.md`. Issue #793 is closed with the
> corrected measurements.

# Feature Specification: Parallelize the golang::graph_resolver per-workspace loop

**Feature Branch**: `773-graph-resolver-parallel`
**Created**: 2026-09-05
**Status**: Draft
**Input**: Issue #793 empirical decomposition — see Motivation

## Motivation

Post-milestone-771 (PRs #788/#789/#790, issue #745 closed) resolved the `go mod why` classifier bottleneck (60s → 13.6s on Kubernetes). Post-milestone-772-rollback (issue #791, PR-less rollback documented at `specs/772-parallel-scan-walker/`) established the perf-methodology rule at `docs/development/perf-methodology.md`: **per-phase tracing decomposition is mandatory before spec'ing a perf milestone**.

Applying that methodology to the post-m771 Kubernetes scan produces the empirical breakdown captured in issue #793:

| Phase | Wall time | Attribution |
|---|---:|---|
| Startup + walker + reader-init | ~700ms | Pre-loop overhead |
| **`golang::graph_resolver` loop** | **~15 seconds** | **38 workspaces × ~400ms average, serial** |
| `scan_fs` finalization | ~2s | Post-loop overhead |
| Emission | ~1s | Format-specific writers |
| **Total wall** | **~19s** | (`--offline --no-go-mod-why` on kusari-sandbox/test-kubernetes) |

Root cause is a serial `for (project_root, doc, sums) in &parsed_roots` loop at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1780` that calls `resolver.resolve(&ctx, &cache)` synchronously per iteration. Each `resolve()` is **stateless per workspace** — `GraphResolver` methods are all `&self`; `GoModCache` methods are all `&self` (read-only); `WorkspaceContext` is constructed fresh per iteration; the shared mutable state (`signals`, `entries`, `out.push`, `seen_purls`, `backfilled_paths`) all mutates in the loop body AROUND the `resolve()` call, not inside it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Parallel per-workspace graph resolution (Priority: P1)

An operator scans a Go monorepo (representative fixture: Kubernetes, 39 `go.mod` files under `go.work`) with default flags (`waybill --offline sbom scan --path <k8s> --no-deep-hash --format cyclonedx-json --output out.cdx.json`). The graph_resolver's per-workspace 4-step ladder runs concurrently across the operator's available CPU cores. Wall-time drops from ~19 seconds walker-isolated / ~34 seconds default to ~6-8 seconds / ~15-17 seconds respectively. Byte-identical output is preserved via deterministic sort-by-workspace-path ordering in the Phase 2 reduce.

**Why this priority**: This is the entire milestone — one user story, one shippable slice (mirrors m771 US2 shape verbatim). Same parallelism pattern already validated in m771, applied to a new but structurally-identical serial loop. Bounded thread pool + mpsc reducer + sort-at-reduce for determinism. Zero new operator flags, zero new Cargo dependencies, byte-identical output.

**Independent Test**: Run the SC-001 command against the Kubernetes fixture. Wall-time must fall from 34s default / 19s walker-isolated to ≤ 20s default / ≤ 8s walker-isolated. Component count + emit order preserved.

**Acceptance Scenarios**:

1. **Given** a Go monorepo with ≥ 2 workspaces (multiple `go.mod` files, `go.work` present) on a machine with ≥ 2 logical CPUs, **When** the operator runs `waybill sbom scan` with default flags, **Then** the graph_resolver invocations use at least 2 concurrent worker threads as measured by overlapping `resolver.resolve()` wall-time.
2. **Given** the Kubernetes fixture, **When** the operator runs `waybill --offline --no-go-mod-why sbom scan --path <k8s>`, **Then** wall-time is ≤ 8 seconds (from 19s pre-milestone) and the emitted CDX has the same component count.
3. **Given** any existing Go / Cargo / npm / pip fixture in the byte-identity regression suite (`waybill-cli/tests/fixtures/`), **When** waybill runs against it, **Then** the emitted CDX / SPDX 2.3 / SPDX 3 output is byte-identical to the pre-milestone baseline (modulo version-string cascades).
4. **Given** two independent runs of the same default scan on the same tree, **When** the emitted CDX component arrays are compared (with `serialNumber` + `created` masked), **Then** the arrays are byte-identical (deterministic emit order via sort-by-workspace-path in Phase 2 reduce).
5. **Given** a scan with `--no-go-mod-why` set, **When** the classifier short-circuit path activates, **Then** the graph_resolver pass still runs (unchanged behavior — the classifier skip is orthogonal to the resolver).

### Edge Cases

- **Single-workspace repo** (1 `go.mod`, no `go.work`): parallelism has nothing to fan out on. Must fall back to serial invocation — byte-identical to pre-milestone behavior and single-threaded observable via CPU utilization ≤ 100%.
- **Zero-workspace scan** (no Go source): the resolver loop is empty; no worker threads spawned; behavior unchanged.
- **`available_parallelism()` returns 1** (single-CPU host or unusual embedded target): fall back to serial invocation.
- **Worker panics during `resolver.resolve()`**: propagates via `thread::join` result inspection; scan MUST fail fast rather than silently drop workspace results. Same contract as m771 US2 FR-008.
- **Workspace with a corrupt / unreadable `go.mod`**: existing per-workspace error handling preserved. The failed workspace produces an empty `graph_map` (pre-milestone behavior); Phase 2 reduce handles it identically.
- **`GoModCache` state race**: `GoModCache` methods are `&self` (read-only); no race exists. If a future patch adds `&mut self` methods, that PR MUST re-audit the parallelization safety.
- **Post-loop shared state mutation** (`signals`, `entries`, `out.push`, `seen_purls`): these mutate ONLY in Phase 2 reduce on the main thread. Workers return `(workspace_index, ResolveResult)` tuples; the reduce iterates in workspace_index order and does all the mutation single-threaded. No `Arc<Mutex<>>` needed for post-loop state.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The graph_resolver loop at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1780` MUST distribute work across ≤ `std::thread::available_parallelism()` worker threads when the workspace count is ≥ 2. Single-workspace scans OR hosts reporting `available_parallelism() == 1` MUST fall back to the pre-milestone serial code path.
- **FR-002**: Each worker MUST call `resolver.resolve(&ctx, &cache)` synchronously per workspace and send the resulting `(workspace_index, ctx, graph_map)` tuple back via `std::sync::mpsc` for the main thread to reduce. No shared-state mutation happens inside the worker body beyond the resolver's own reads.
- **FR-003**: The `GoModCache` MUST be shared across workers via `Arc<GoModCache>`. Its methods are already `&self` (read-only), so no per-worker locking is required.
- **FR-004**: The main thread's Phase 2 reduce MUST iterate worker results in **workspace_index order** (i.e., the same order the pre-milestone serial loop processed them). Determinism guaranteed by the reduce order, not by worker completion order.
- **FR-005**: All existing post-`resolve()` state mutation (`signals.go_transitive_coverage` merge via `merge_coverage`, `signals.gosum_fallback_count` accumulator, `entries` build + `+incompatible` filter, `out.push`, `seen_purls` dedup, `build_main_module_entry` augmentation, `backfilled_paths` set) MUST run on the main thread in Phase 2. No changes to this mutation logic — it moves from inside the serial loop body to inside the reduce loop body.
- **FR-006**: Worker panics MUST propagate via `JoinHandle::join()` result inspection. The scan MUST fail fast with a diagnostic naming the failing workspace's absolute path — never silently drop that workspace's contribution. Same contract as m771 US2 FR-008.
- **FR-007**: The FR-009 summary log emitted per workspace at `graph_resolver.rs:716` (`"go transitive edges resolution summary"` line with fields `total_modules`, `go_mod_graph_count`, `cache_count`, `proxy_count`, `gosum_count`, `unresolved_count`, `coverage`) MUST retain its existing wire-shape and MUST still fire once per workspace. Log-line ordering under concurrent workers MAY interleave; each line still carries the workspace's identifying fields.
- **FR-008**: The `--no-go-mod-why` operator flag MUST continue to short-circuit exactly as today. This milestone parallelizes the graph_resolver loop; the classifier's skip semantics are unchanged.
- **FR-009**: The `GraphResolver::resolve()` API surface MUST NOT change. Same signature (`resolve(&self, ctx: &WorkspaceContext, cache: &GoModCache) -> Result<ModuleGraphMap, GraphResolverError>`), same 4-step ladder body (step 1 `go mod graph` → step 2 cache walk → step 3 proxy fetch → step 5 go.sum fallback → step 6 empty fallthrough), same per-workspace summary log emission.
- **FR-010**: The `GoModCache` API surface MUST NOT change. Same `pub fn` signatures — all `&self`.
- **FR-011**: Zero new Cargo dependencies at the workspace `Cargo.lock` level. The parallelization MUST use `std::thread`, `std::sync::{Arc, mpsc}` primitives, and existing waybill types — mirror the m771 US2 shape at `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass`.
- **FR-012**: No new operator-facing CLI flags. Parallelism is default-on and tuned automatically by `available_parallelism()`. The `WAYBILL_*` env-var namespace also gains no new entries.

### Key Entities

- **`GraphResolver`**: the per-scan resolver instance. Its `resolve()` method is the per-workspace work unit being parallelized. Shared across workers via `Arc<GraphResolver>` — the type is already `Send + Sync`-safe (contains only a `GraphResolverConfig`).
- **`GoModCache`**: the read-only module-cache lookup handle. Shared across workers via `Arc<GoModCache>`. All methods are `&self`.
- **`WorkspaceJob`**: a work-queue payload for the parallel loop. Carries `(workspace_index: usize, project_root: PathBuf, doc_snapshot, sums_snapshot)`. Workers pop `WorkspaceJob`s; the workspace_index is preserved through the pipeline to enable deterministic ordering in Phase 2 reduce.
- **`ResolveResult`**: per-workspace worker output. Carries `(workspace_index: usize, ctx: WorkspaceContext, graph_map: ModuleGraphMap)`. Sent from worker to main thread via `mpsc::Sender<ResolveResult>`.
- **`WorkspaceContext`**: existing type — the per-workspace resolver input. Constructed fresh per worker inside the worker body (identical to pre-milestone loop-body behavior).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Kubernetes fixture (`kusari-sandbox/test-kubernetes`, 39 `go.mod` files), a **default scan** (`waybill --offline sbom scan --path <k8s> --no-deep-hash --format cyclonedx-json --output …`) completes in ≤ **20 seconds** wall-time on a reference class of machine (macOS aarch64 8-core, warm cache), from a pre-milestone baseline of 34 seconds. Interim indicator: `--no-go-mod-why` walker-isolated scan MUST complete in ≤ **8 seconds** (from 19s pre-milestone).
- **SC-002**: Every existing byte-identity regression suite continues to pass unchanged: `waybill-cli/tests/scan_go*.rs`, `scan_cargo*.rs`, `scan_python*.rs`, `scan_npm*.rs`, `cdx_regression`, `spdx_regression`, `spdx3_regression`, plus every m771 golden fixture. Golden CDX / SPDX 2.3 / SPDX 3 files MUST match byte-identical modulo version-string cascades.
- **SC-003**: The milestone introduces zero new Cargo dependencies at the workspace `Cargo.lock` level. Verified via `git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml` showing no lines added to any `[dependencies]` block.
- **SC-004**: Emit order across two independent runs of the same default scan on the same tree produces byte-identical CDX component arrays (after masking `serialNumber` + `created` runtime-random fields). Verified by running the SC-001 command twice back-to-back and `diff`-ing the outputs.
- **SC-005**: The FR-013 summary log emitted per workspace (`"go transitive edges resolution summary"` at `graph_resolver.rs:716`) retains its exact wire-shape and continues to fire exactly once per workspace analyzed. Field names + ordering unchanged.
- **SC-006**: `--no-go-mod-why` continues to short-circuit correctly. Scan of any fixture with `WAYBILL_NO_GO_MOD_WHY=1` (or the equivalent flag) produces byte-identical CDX / SPDX 2.3 / SPDX 3 output vs. the pre-milestone binary. Regression pin.

## Assumptions

- Reference-class benchmark host is macOS aarch64 with ≥ 8 logical CPUs and a warm module cache. Same reference class as m771 SC-001 + m669 baseline. Other host classes (Linux CI runners, Windows) benefit proportionally but their exact wall-times are not pinned by SC-001.
- The `kusari-sandbox/test-kubernetes` fixture (v0.6.1 shape: 39 `go.mod` files, `go.work` present, ~246 modules in root `go.sum`) is the canonical performance-regression corpus. Public reproducibility via `git clone --depth 1`.
- `GraphResolver` and `GoModCache` are stateless per-workspace: their `&self` method signatures are the guarantee. If a future patch adds `&mut self` methods to either type, this milestone's parallelization safety re-audit is required.
- Post-loop shared state (`signals`, `entries`, `out`, `seen_purls`, `backfilled_paths`) mutation is confined to Phase 2 reduce on the main thread. No `Arc<Mutex<>>` needed for these — the reduce is single-threaded by construction.
- Deterministic emit order via workspace_index-ordered reduce is sufficient for SC-004 byte-identity. Reader outputs are already re-sorted by downstream emitters; the workspace-ordering is the only per-scan variable this milestone touches.
- Worker panic propagation via `JoinHandle::join()` matches m771 US2 shape. No `catch_unwind` needed.
- `--offline` mode (which is the SC-001 test configuration) skips step 1 (`go mod graph` subprocess) and step 3 (proxy fetch) inside `resolve()`. The remaining step 2 (cache walk) + step 5 (go.sum fallback) are the dominant per-workspace work.

## Non-Goals

- No change to `GraphResolver::resolve()` semantics or the 4-step ladder body. The resolver stays serial per workspace.
- No change to the per-workspace deterministic post-processing (`signals` merge, `entries` filter, `out.push`, `seen_purls` dedup, main-module edge augmentation). Determinism is preserved via workspace_index-ordered Phase 2 reduce.
- No new operator-facing CLI flags. Parallelism is default-on.
- No new `WAYBILL_*` env vars.
- No new Cargo dependencies. Implementation uses stdlib primitives only (m771 US2 precedent).
- No cross-scan caching. All state is per-scan (matches every reader-tier milestone since m002).
- No changes to the `--no-go-mod-why` classifier flag or its skip semantics.
- No overlap of graph_resolver with the m664 walker OR with `apply_go_mod_why_pass`. These phases remain sequential in wall time (walker → resolver → classifier → emission). Interleaving them is a bigger architectural refactor deferred to a future milestone if needed.

## Dependencies

- Builds on the m771 US2 concurrency pattern at `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass`. That code is the reference implementation for the bounded thread-pool + mpsc-reducer shape; the m773 implementation mirrors it applied to a different serial loop.
- Compatible with the m669 benchmark harness. New wall-time measurements will surface in `xtask bench` output; the m669 `baseline.json` refresh is a Phase 6 polish task deferred to after merge.
- Compatible with the m115/m117 walker-audit allowlist. This milestone doesn't add any `fn walk_*` functions — it modifies an existing per-workspace loop inside `legacy.rs`.
- Compatible with the m771 classifier parallelization. Both systems remain sequential in wall time (resolver completes for all workspaces → classifier runs).
- Follows the perf-methodology at `docs/development/perf-methodology.md`. The Motivation section's empirical decomposition IS the required Step-1/Step-2/Step-3 evidence.
