> # ⚠️ ABANDONED — NOT IMPLEMENTED
>
> This milestone was specced, planned, tasked, and implemented, then
> **abandoned before merge**. No part of it shipped.
>
> **Why**: the target was wrong. Per-phase tracing showed
> `walk_registry::SharedWalker::run` costs **63ms of a 20-second scan**
> — 0.3% of wall time. The "~18s dominated by the walker" figure that
> motivated it came from toggling `--no-go-mod-why` and attributing the
> entire remainder to the walker, but that flag skips only the
> classifier; the remainder was the sum of five phases.
>
> Kept as the worked example behind
> `docs/development/perf-methodology.md`, which cites this milestone by
> name. Issue #791 is closed with the corrected measurements.
>
> The Go-path work that *did* move the needle is m771, m774, and m775.

# Feature Specification: Parallelize the scan_fs shared-walker

**Feature Branch**: `772-parallel-scan-walker`
**Created**: 2026-09-04
**Status**: Draft
**Input**: Issue #791 empirical decomposition — see Motivation

## Motivation

Post-milestone-771 (PRs #788/#789/#790, issue #745 closed), the Go-mod-why classifier bottleneck is resolved (60s → 13.6s on Kubernetes). The remaining wall time is now dominated by a **different** subsystem: the `scan_fs` shared walker itself.

Empirical decomposition on `kusari-sandbox/test-kubernetes` (39 `go.mod` files, ~55K files across 380 MB; macOS aarch64 8-core, warm cache, post-m771 release build):

| Configuration | Wall time | CPU util | Attribution |
|---|---:|---:|---|
| A — default (all m771 tiers active) | 33.4s | 738% | walker + classifier (sequential in wall time) |
| B — `+ --no-go-mod-why` | **18.7s** | **99%** | walker + everything else, **single-threaded** |
| C — `+ --no-binary-scan=all` | 17.8s | 99% | binary tier costs only ~0.9s |
| D — `+ --exclude-path vendor,third_party` | 17.8s | 99% | k8s doesn't use `vendor/` |

**Key findings**:

- The walker is **~18 seconds single-threaded** (99% CPU on 1 core out of 8 available).
- Walker and classifier run **sequentially** in wall time: 33.4s ≈ 18.7s (walker) + 13.6s (classifier). The classifier only starts after `scan_fs::scan_path` returns.
- Binary tier + vendor exclusion together account for < 1 second — not the bottleneck.

The `m664` SharedWalker (`waybill-cli/src/scan_fs/walk_registry/walker.rs::SharedWalker::run`) is a synchronous recursive DFS. Per-file work: canonicalize + `read_dir` + `should_skip_by_basename` + reader dispatch. Reader output already uses per-reader `Mutex<Vec<PackageDbEntry>>` collectors, so partial thread-safety is already in place.

## Clarifications

### Session 2026-09-04

- Q: How should worker threads discover work — top-level fan-out, work-stealing, or depth-N pre-fan-out? → A: **Work-stealing**. Start with rootfs as the single seed job; each worker reads one directory, pushes discovered subdirs back onto the shared queue for other workers to steal. Balanced across workers regardless of tree shape (k8s's `staging/` dominating total file count is a real fixture where top-level fan-out would leave 6-7 cores idle after the tiny top-level dirs drain).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Parallel walker over sub-trees (Priority: P1)

An operator scans a large repository (representative fixtures: Kubernetes, MongoDB, Cassandra) with default flags. The scan_fs walker fans out across the operator's available CPU cores, walking distinct sub-trees concurrently. Reader output lands in the same per-reader `Mutex<Vec<PackageDbEntry>>` collectors as today (no reader-registration API change). Wall-time drops from ~18s single-threaded to ~3-5s on 8 cores.

**Why this priority**: This is the entire milestone — one user story, one shippable win. The parallelization is bounded (worker count = `std::thread::available_parallelism()`) and default-on. Operators observe strictly faster scans with byte-identical output.

**Independent Test**: Run the SC-001 command against the Kubernetes fixture. Wall-time must fall from ~18.7s (walker-isolated, `--no-go-mod-why`) to ≤ 5s. Full-default scan (SC-001 default target ≤ 22s) is the operator-facing outcome. Component count + emit order preserved.

**Acceptance Scenarios**:

1. **Given** a repository with ≥ 4 top-level directories on a machine with ≥ 4 logical CPUs, **When** an operator runs `waybill sbom scan --path <repo>` with any reader mix, **Then** the walker uses at least 2 concurrent worker threads as observed by overlapping child-process or thread-level wall time.
2. **Given** the Kubernetes fixture, **When** the operator runs `waybill --offline --no-go-mod-why sbom scan --path <k8s>`, **Then** wall-time is ≤ 5 seconds (from 18.7s pre-milestone) and the emitted CDX has the same 817 components.
3. **Given** any existing Go / Cargo / npm / pip fixture in the byte-identity regression suite (`waybill-cli/tests/fixtures/`), **When** waybill runs against it, **Then** the emitted CDX / SPDX 2.3 / SPDX 3 output is byte-identical to the pre-milestone baseline (modulo version-string cascades).
4. **Given** a repository containing symlink loops (e.g., a directory that symlinks to its own parent), **When** waybill runs against it, **Then** no worker gets stuck in an infinite descent; the m054/m114 canonicalize + visited-set safety invariant holds under parallel workers.
5. **Given** two independent runs of the same scan on the same tree, **When** the emitted CDX component arrays are compared, **Then** the arrays are byte-identical (deterministic emit order via sort-at-end).

### Edge Cases

- **Single top-level directory**: rootfs has only one child dir. Parallelism has nothing to fan out on at the top level; must still deliver correct output without deadlock or degradation. Reasonable design: fall back to serial walk OR fan out at a deeper level.
- **Very small trees (< 100 files)**: parallelism setup overhead (thread spawn + queue init) can exceed the walk cost. Fall back to serial path — byte-identical to pre-milestone behavior.
- **Available parallelism = 1**: single-CPU host or `std::thread::available_parallelism()` returns unusual value. Fall back to serial path.
- **Rootfs is a single file (not a directory)**: pre-existing edge case; behavior unchanged (walker enumerates zero subdirs, returns).
- **Symlink loop crossing worker boundaries**: worker A visits `dir1/`, worker B visits `dir2/`; each contains a symlink pointing at the other. Per-worker visited-set alone MAY not detect this. Design must either share the visited-set (with mutex) OR provide guarantees that per-worker sets suffice within the m664 canonicalize-based semantics.
- **Worker panics mid-walk**: propagates via `thread::join`; scan MUST fail fast rather than silently drop partial output.
- **`m113 ExclusionSet` matches under a worker**: still applies — the exclusion gate lives inside `walk_inner` and is unchanged.
- **`descend_into` scope-restriction from m664 Contract C10**: preserved. When a worker descends into a normally-skipped dir via a reader's `descend_into` override, the restricted-scope semantics are maintained per-worker.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `SharedWalker::run` entry point MUST distribute work across ≤ `std::thread::available_parallelism()` worker threads using a **work-stealing** shape (per Clarification 2026-09-04 Q1): the queue is seeded with the rootfs as a single job; each worker pops a directory, reads its immediate children, dispatches per-file reader callbacks, and pushes any newly-discovered subdirectories back onto the shared queue for peer workers to steal. Hosts reporting `available_parallelism() == 1` OR trees where the rootfs contains zero subdirectories MUST fall back to the pre-milestone serial code path.
- **FR-002**: Reader dispatch (`ReaderRegistry`-driven `on_file` / `on_dir` callbacks) MUST NOT change its calling contract, invocation shape, or emitted `PackageDbEntry` structure. Concurrent workers MAY invoke reader callbacks in parallel; readers that require serial invocation MUST already document this via existing shared-state discipline.
- **FR-003**: The m054/m114 canonicalize + visited-set symlink-loop safety invariant MUST continue to hold. A visited-set entry added by one worker MUST prevent duplicate descent by any other worker into the same canonical path.
- **FR-004**: The m113 `ExclusionSet` gates MUST apply identically under parallel workers as under serial walk. An excluded directory MUST NOT be descended by ANY worker regardless of which worker discovered it.
- **FR-005**: The m664 `descend_into` scope-restriction contract (Contract C10) MUST be preserved. When a normally-skipped directory is descended via a reader's `descend_into` override, dispatch under that subtree remains restricted to the requesting reader set.
- **FR-006**: The `output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>` collector MUST remain the sole sink for reader-emitted entries. Concurrent workers append via the existing per-reader `Mutex` — no new collector abstraction is introduced.
- **FR-007**: Emitted `PackageDbEntry` collections MUST be sorted deterministically before being returned to `scan_fs::scan_path` so that the emit-order observable in the final SBOM does not depend on which worker discovered which entry. Sort key: existing type-natural key (typically PURL string, or the per-reader canonical ordering that already exists).
- **FR-008**: When a walker worker panics, the panic MUST propagate to the main thread (via `thread::join` result inspection) and the scan MUST fail fast with a diagnostic naming the worker + subtree — never silently drop partial output.
- **FR-009**: The default scan (no operator flags) MUST NOT introduce any new operator-facing CLI flags. Parallelism is default-on, tuned automatically to the host.
- **FR-010**: No new Cargo dependencies MUST be added at the workspace `Cargo.lock` level. Implementation MUST use `std::thread`, `std::sync::{Arc, Mutex, mpsc}`, and existing waybill types.
- **FR-011**: Any newly introduced `fn walk_*` function (per the m115/m117 walker-audit-allowlist mechanism) MUST be added to `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` in the same PR that introduces it, per the existing walker-audit contract.
- **FR-012**: The `scan_fs::scan_path` → `apply_go_mod_why_pass` ordering MUST NOT change. This milestone parallelizes the walker only; the classifier still runs sequentially after the walker returns.

### Key Entities

- **`SharedWalker`**: The m664 walker instance. Its `run()` method is the entry point being parallelized. Fields (`visited`, `dir_index`, `output`) are the shared state that must be made thread-safe.
- **Worker thread**: A `std::thread`-spawned executor that pops sub-trees from a shared work queue and walks them via the existing recursion, emitting to the shared reader collectors.
- **Work queue**: `Arc<Mutex<Vec<SubtreeJob>>>` — the shared work-stealing pool of directories awaiting walk. Seeded with the rootfs as the single initial job; each worker pushes newly-discovered subdirectories back onto the queue so peers can steal them. `SubtreeJob` carries `(canonical_path, current_scope: Option<HashSet<ReaderId>>)` to preserve C10 scope-restriction semantics.
- **Shared visited-set**: `Arc<Mutex<HashSet<PathBuf>>>` — the m054/m114 symlink-loop guard. Shared across workers so cross-subtree loops are still caught.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Kubernetes fixture (`kusari-sandbox/test-kubernetes`, ~55K files, 380 MB), a **default scan** (`waybill --offline sbom scan --path <k8s> --no-deep-hash --format cyclonedx-json --output …`) completes in ≤ 22 seconds wall-time on a reference class of machine (macOS aarch64 8-core, warm cache), from a pre-milestone baseline of 33.4 s. Interim indicator: `--no-go-mod-why` (walker-isolated) MUST complete in ≤ 5 seconds, from a pre-milestone baseline of 18.7 s.
- **SC-002**: Every existing byte-identity regression suite continues to pass unchanged: `waybill-cli/tests/scan_go*.rs`, `scan_cargo*.rs`, `scan_python*.rs`, `scan_npm*.rs`, `cdx_regression`, `spdx_regression`, `spdx3_regression`, and every `walk_registry_*.rs` integration test. Golden CDX / SPDX 2.3 / SPDX 3 files MUST match byte-identical modulo version-string cascades per the release-bump normalized-diff protocol.
- **SC-003**: The milestone introduces zero new Cargo dependencies at the workspace `Cargo.lock` level. Verified via `git diff --stat Cargo.lock waybill-cli/Cargo.toml waybill-common/Cargo.toml xtask/Cargo.toml` showing no lines added to any `[dependencies]` block.
- **SC-004**: Emit order across two independent runs of the same default scan on the same tree produces byte-identical CDX component arrays. Verified by running the SC-001 command twice back-to-back and `diff`-ing the outputs (zero diff expected, modulo timestamp / UUID fields that are already runtime-random).
- **SC-005**: A symlink-loop fixture (e.g., the existing `walks_symlink_loop_without_hanging` regression at `waybill-cli/src/scan_fs/package_db/golang/go_binary.rs::tests`) continues to terminate promptly under the parallel walker. Wall-time MUST be within an order of magnitude of the pre-milestone measurement on the same fixture.
- **SC-006**: On a small-tree fixture (e.g., `waybill-cli/tests/fixtures/golang/mod_why_scaling/` — 4 workspaces, ~10 files), the walker MUST use the serial fallback path (single-threaded) — verified by observing CPU utilization ≤ 100% on the scan or by an INFO-level log line naming the fallback trigger.

## Assumptions

- Reference-class benchmark host is macOS aarch64 with ≥ 8 logical CPUs and a warm module cache — same as m771's SC-001 reference. Linux CI runners (2-4 vCPUs) benefit proportionally but their exact wall-times are not pinned by SC-001.
- The `kusari-sandbox/test-kubernetes` fixture (~55K files, 380 MB, m771-shape) is the canonical performance-regression corpus for this milestone. Public reproducibility via `git clone --depth 1`.
- Existing readers do NOT rely on serial-walk ordering for correctness. Reader implementations that mutate their own state during `on_file` / `on_dir` callbacks are expected to be internally thread-safe (spot-checked at code-review time via the m664 registration contract).
- Sort-at-end for FR-007 determinism is a per-reader concern. Readers whose outputs already have a natural sort key (PURL string, file path) use it; readers without one must define one (the m664 registration contract already implies this for stable output).
- Symlink-loop coverage under parallel workers is provided by sharing the visited-set (Arc<Mutex<HashSet<PathBuf>>>). Alternative designs (per-worker visited-sets) would need to prove they still catch cross-subtree loops; the shared-set approach preserves the m054/m114 invariant most directly.
- The 60-second wall-clock budget from m112 (the `WAYBILL_GO_MOD_WHY_BUDGET_MS` construct) is not touched by this milestone; the classifier's budget starts when `apply_go_mod_why_pass` is invoked, which happens after the walker returns.

## Non-Goals

- No overlap of walker with classifier (`scan_fs::scan_path` → `apply_go_mod_why_pass` sequential ordering preserved). Interleaving those two phases is a bigger architectural refactor; deferred to a future milestone if the walker-parallelism win doesn't meet operator expectations.
- No new reader-registration API surface. Readers keep their existing `on_file` / `on_dir` / `descend_into` contract from m664.
- No new operator-facing CLI flags. Parallelism is default-on and tuned by `available_parallelism()`.
- No changes to the m664 walker-audit allowlist mechanism (m115/m117). Any new `fn walk_*` function this milestone introduces is added to the existing allowlist file in the same PR.
- No new Cargo dependencies. Implementation uses stdlib primitives (`std::thread`, `std::sync::{Arc, Mutex, mpsc}`).
- No changes to the `visited` semantics for symlink safety (m054/m114). The invariant is preserved; only the concurrency envelope changes.
- No cross-scan caching. All walker state is per-scan (matches every reader-tier milestone since m002).

## Dependencies

- Builds on m664 shared-walker registry (`waybill-cli/src/scan_fs/walk_registry/`). Extends its `SharedWalker` type; does not modify the registration API.
- Compatible with the m669 benchmark harness. New wall-time measurements will surface in `xtask bench` output; the m669 `baseline.json` should be refreshed post-merge (deferred to a follow-up polish task).
- Compatible with the m115/m117 walker-audit allowlist. Any new `fn walk_*` function is added to `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` in the same PR.
- Compatible with m771's `apply_go_mod_why_pass` concurrent classifier — the two systems remain sequential in wall time (walker completes, then classifier runs).
