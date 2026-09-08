# Implementation Plan: Parallelize the golang::graph_resolver per-workspace loop

**Branch**: `773-graph-resolver-parallel` | **Date**: 2026-09-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/773-graph-resolver-parallel/spec.md`

## Summary

Post-m771 + post-m772-rollback, the Kubernetes-scale scan bottleneck is a serial `for (project_root, doc, sums) in &parsed_roots` loop at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:1780` calling `resolver.resolve(&ctx, &cache)` synchronously per iteration — 38 workspaces × ~400ms average = ~15 seconds. This milestone parallelizes that loop using the m771 US2 concurrency shape (bounded thread pool over a workspace queue + mpsc reducer + workspace_index-ordered deterministic reduce). `GraphResolver` and `GoModCache` are already stateless per-workspace (all-`&self` methods), so parallelization requires only `Arc<GraphResolver>` + `Arc<GoModCache>` shared across workers. Post-loop shared state (`signals`, `entries`, `out`, `seen_purls`, `backfilled_paths`) remains single-threaded in the Phase 2 reduce — no `Arc<Mutex<>>` needed for those. Zero new operator flags; zero new Cargo dependencies; byte-identical output preserved. Target: k8s scan wall-time 34s → ≤ 20s default; 19s → ≤ 8s walker-isolated.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–772; no nightly required).
**Primary Dependencies**: Existing only — `std::thread`, `std::sync::{Arc, mpsc}`, `std::path::{Path, PathBuf}`, `std::collections::{HashMap, HashSet}`, `tracing`, `anyhow`. Reuses the m771 US2 spawn-thread + `Arc<Mutex<work-queue>>` + `mpsc::channel` reducer pattern verbatim (see `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass` lines ~1218-1280 for the reference shape). **Zero new Cargo dependencies** (FR-011 + SC-003).
**Storage**: N/A — all state in-process per scan. The shared `Arc<GraphResolver>` and `Arc<GoModCache>` live for the duration of the loop; per-workspace `WorkspaceContext` and `ModuleGraphMap` are moved through the mpsc channel and dropped after Phase 2 reduce consumes them.
**Testing**: `cargo +stable test --workspace --no-fail-fast` (existing pipeline). New tests: (a) unit test at `graph_resolver.rs::tests` verifying `GraphResolver` + `GoModCache` are `Send + Sync` (compile-time proof via `assert_send_sync` helper); (b) integration test at `waybill-cli/tests/graph_resolver_parallel_773.rs` exercising the concurrent path against the m771 mod_why_scaling fixture + a synthetic small multi-workspace Go fixture; (c) empirical wall-time measurement on Kubernetes via the m669 benchmark harness after merge.
**Target Platform**: macOS aarch64 / linux-x86_64 / linux-aarch64 / windows-x86_64 (waybill's four supported host classes per milestone 100). `std::thread::available_parallelism()` returns `NonZeroUsize` on all four.
**Project Type**: Rust CLI (`waybill-cli` crate) — single project, no cross-tree changes.
**Performance Goals**: SC-001 wall-time thresholds on Kubernetes fixture — default scan ≤ 20 s (from 34s post-m771); walker-isolated (`--no-go-mod-why`) ≤ 8 s (from 19s). Reference class: macOS aarch64 ≥ 8 logical CPUs, warm cache.
**Constraints**: (a) Byte-identity for every fixture in `waybill-cli/tests/fixtures/` (SC-002); (b) deterministic emit order across runs via workspace_index-ordered reduce (SC-004); (c) zero new operator flags (FR-012); (d) `GraphResolver::resolve()` API surface unchanged (FR-009); (e) `GoModCache` API surface unchanged (FR-010); (f) FR-013 summary log wire-shape preserved and continues to fire per-workspace (FR-007 + SC-005); (g) `--no-go-mod-why` orthogonality (FR-008 + SC-006); (h) worker panics fail-fast (FR-006).
**Scale/Scope**: 38-workspace / ~55K-file Kubernetes fixture is the stress case. Non-goal: cross-scan cache. LOC delta expected ≤ ~150 lines net add — a single-function refactor of the loop body at `legacy.rs:1780` into `(spawn workers, mpsc-collect, index-ordered reduce)` shape. Integration test adds ~150 lines.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Verified against Waybill Constitution v2.1.0 (`.specify/memory/constitution.md`):

- **I. Pure Rust, Zero C** — ✅ No C sources. Pure-Rust stdlib primitives only.
- **II. eBPF-Only Observation** — ✅ N/A. Scan_fs is user-space enrichment per Principle II's scan-mode exception. This milestone parallelizes an existing enrichment loop; no new discovery mechanism.
- **III. Fail Closed** — ✅ Preserved. FR-006 explicitly requires worker panics to propagate via `thread::join`; no silent workspace drops. Existing `resolver.resolve()` error handling (per-workspace `Ok`/`Err` match at `legacy.rs:1794-1804`) preserved verbatim.
- **IV. Type-Driven Correctness** — ✅ New types (`WorkspaceJob`, `ResolveResult`) follow the newtype-struct pattern. Existing vocabulary (`WorkspaceContext`, `GraphResolver`, `GoModCache`, `ModuleGraphMap`) reused unchanged. Zero `.unwrap()` in production paths; `.expect()` reserved for mutex-acquisition sites where poisoning already indicates an unrecoverable panic.
- **V. Specification Compliance** — ✅ No new `waybill:*` properties, annotations, or relationship types. SBOM output is byte-identical (SC-002 + SC-004). No format-mapping catalog rows added.
- **VI. Three-Crate Architecture** — ✅ Changes confined to `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`. No new crates.
- **VII. Test Isolation** — ✅ All new tests are pure user-space (no eBPF privilege). Runs under standard `cargo test`.
- **VIII. Completeness** — ✅ Preserved via SC-002 byte-identity. The workspace_index-ordered reduce (FR-004) prevents parallel-completion ordering from leaking into the SBOM.
- **IX. Accuracy** — ✅ Preserved. FR-005 confines all post-loop state mutation to Phase 2 reduce on the main thread; per-workspace verdicts, edge attribution, and dedup logic all execute identically to pre-milestone.
- **X. Transparency** — ✅ Preserved. FR-006 requires panic diagnostics naming the failing workspace's absolute path. FR-007 preserves the per-workspace FR-013 summary log wire-shape.
- **XI. Enrichment** — ✅ N/A. Not an enrichment-source change.
- **XII. External Data Source Enrichment** — ✅ N/A. `graph_resolver` is the same downstream data source as pre-milestone; parallelization changes how workspaces are iterated, not what's queried.

**Result**: All 12 principles pass. Zero justifications required for the Complexity Tracking table.

## Project Structure

### Documentation (this feature)

```text
specs/773-graph-resolver-parallel/
├── plan.md              # This file (/speckit.plan output)
├── spec.md              # Feature spec (/speckit.specify output)
├── research.md          # Phase 0 output (/speckit.plan)
├── data-model.md        # Phase 1 output (/speckit.plan)
├── contracts/           # Phase 1 output (/speckit.plan)
│   └── graph-resolver-parallelism.md
├── quickstart.md        # Phase 1 output (/speckit.plan)
├── checklists/
│   └── requirements.md  # From /speckit.specify
└── tasks.md             # Phase 2 output (/speckit.tasks; not created by /speckit.plan)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── golang/
│               └── legacy.rs                # MODIFIED — parallelize the loop at line 1780
└── tests/
    └── graph_resolver_parallel_773.rs       # NEW — integration tests for FR-001, FR-004, FR-006
```

**Structure Decision**: Single-file change to `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`. The parallelization is a ~100-line refactor of the loop body plus supporting Send+Sync assertions. If the refactor grows past the point where the enclosing function becomes unwieldy, extract to a `golang/resolve_workspaces_parallel.rs` submodule at code-review time (deferred decision).

## Complexity Tracking

> **No Constitution violations to justify.** Table intentionally empty.
