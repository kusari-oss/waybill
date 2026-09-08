# Implementation Plan: Parallelize the scan_fs shared-walker

**Branch**: `772-parallel-scan-walker` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/772-parallel-scan-walker/spec.md`

## Summary

The m664 SharedWalker is a synchronous recursive DFS — a single hot path (`walk_inner` at `walker.rs:101`) that reads directories, dispatches per-file reader callbacks, and mutates internal state (`visited`, `dir_index`, per-reader output). Post-m771, this walker is the dominant bottleneck on Kubernetes-scale monorepos: **18.7s single-threaded, 99% CPU on 1 of 8 cores**. This milestone parallelizes it via a **work-stealing** shape (per Clarification 2026-09-04 Q1): rootfs seeds a shared `Arc<Mutex<Vec<SubtreeJob>>>` queue; N workers (N = `available_parallelism()`) pop directories, run `read_dir` + reader dispatch, push discovered subdirectories back onto the queue. Shared `Arc<Mutex<HashSet<PathBuf>>>` visited-set preserves the m054/m114 symlink-loop invariant across worker boundaries. Zero new Cargo deps; zero new operator flags; byte-identical output preserved via sort-at-end (FR-007). Target: ≤ 5 s walker-isolated wall time on k8s (from 18.7s); ≤ 22 s default scan wall (from 33.4s).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–771; no nightly required).
**Primary Dependencies**: Existing only — `std::thread`, `std::sync::{Arc, Mutex, mpsc}`, `std::process::Command`, `std::fs::{canonicalize, read_dir}`, `std::path::{Path, PathBuf}`, `std::collections::{HashMap, HashSet}`, `tracing`, `anyhow`. Reuses the m771 US2 spawn-thread + `Arc<Mutex<work-queue>>` + mpsc-reducer pattern verbatim (see `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass` for the reference shape). **Zero new Cargo dependencies** (FR-010 + SC-003).
**Storage**: N/A — all state in-process per scan; matches every walker milestone since m664. The shared `Arc<Mutex<HashSet<PathBuf>>>` visited-set and `Arc<Mutex<Vec<SubtreeJob>>>` work queue live for the duration of a single `SharedWalker::run` invocation and drop when the last worker joins.
**Testing**: `cargo +stable test --workspace --no-fail-fast` (existing pipeline). New tests: (a) unit tests in `walk_registry/walker.rs::tests` for the work-queue seeding + subtree-push shape (mock-directory driven, no real filesystem walk needed for hot-path structural checks); (b) integration test at `waybill-cli/tests/walker_parallelism_772.rs` exercising the work-stealing path against the milestone-771 synthetic fixture + a new symlink-loop fixture; (c) empirical wall-time measurement on Kubernetes via the m669 benchmark harness (`xtask bench --update-baseline`).
**Target Platform**: macOS aarch64 / linux-x86_64 / linux-aarch64 / windows-x86_64 (waybill's four supported host classes per milestone 100). `std::thread::available_parallelism()` returns `NonZeroUsize` on all four.
**Project Type**: Rust CLI (`waybill-cli` crate) — single project, no cross-tree changes.
**Performance Goals**: SC-001 wall-time thresholds on Kubernetes fixture — default scan ≤ 22 s (from 33.4s); walker-isolated (`--no-go-mod-why`) ≤ 5 s (from 18.7s). Reference class: macOS aarch64 ≥ 8 logical CPUs, warm cache. SC-005: symlink-loop test terminates within an order of magnitude of pre-milestone.
**Constraints**: (a) Byte-identity for every existing fixture in `waybill-cli/tests/fixtures/` (SC-002); (b) deterministic emit order across runs (SC-004 — sort at end); (c) zero new operator flags (FR-009); (d) m664 registration API surface unchanged (FR-002); (e) m054/m114 canonicalize + visited-set invariant preserved across workers (FR-003); (f) m113 ExclusionSet + m664 descend_into semantics preserved (FR-004 + FR-005); (g) worker panics fail fast, don't silently drop output (FR-008); (h) m115/m117 walker-audit allowlist updated for any new `fn walk_*` function (FR-011).
**Scale/Scope**: 55K-file Kubernetes fixture is the stress case. Non-goal: cross-scan cache (all state per-scan). LOC delta expected ≤ ~250 lines net add — the parallel scaffolding + `walk_inner` split into "read-dir + dispatch" (worker body) and "recurse into subdirs" (push to queue). New test file adds ~200 lines.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Verified against Waybill Constitution v2.1.0 (`.specify/memory/constitution.md`):

- **I. Pure Rust, Zero C** — ✅ No C sources introduced. Pure-Rust stdlib primitives only (`std::thread`, `std::sync::{Arc, Mutex}`).
- **II. eBPF-Only Observation** — ✅ N/A. Scan_fs walker is a user-space enrichment path per Principle II's scan-mode exception (established at m002+); this milestone parallelizes an existing walker, not adding a new discovery mechanism.
- **III. Fail Closed** — ✅ Preserved. FR-008 explicitly requires worker panics to propagate to the main thread via `thread::join`; no silent partial-output drops. All existing degrade paths (unreadable dir, canonicalize failure) remain permissive-tolerance per m054/m114.
- **IV. Type-Driven Correctness** — ✅ New types (`SubtreeJob`) follow the newtype-struct pattern. Existing vocabulary (`ReaderId`, `PackageDbEntry`, `WalkerMetrics`, `DirIndex`) reused verbatim. Zero `.unwrap()` in production paths; `.expect()` reserved for mutex-acquisition sites where poisoning would already indicate an unrecoverable panic per Rust convention.
- **V. Specification Compliance** — ✅ No new `waybill:*` properties, annotations, or relationship types. This milestone is pure execution-model — SBOM output is byte-identical (SC-002 + SC-004). No format-mapping catalog rows added.
- **VI. Three-Crate Architecture** — ✅ Changes confined to `waybill-cli/src/scan_fs/walk_registry/`. No new crates, no cross-crate boundary changes.
- **VII. Test Isolation** — ✅ All new tests are pure user-space (no eBPF privilege dependency). Runs under standard `cargo test` without root or CAP_BPF.
- **VIII. Completeness** — ✅ Preserved via SC-002 byte-identity across every existing fixture. Deterministic sort-at-end (FR-007 + SC-004) prevents parallel-discovery ordering from leaking into the SBOM.
- **IX. Accuracy** — ✅ Preserved. FR-002 pins reader dispatch contract unchanged; readers see identical `on_file` / `on_dir` calls in identical shape.
- **X. Transparency** — ✅ Preserved. FR-008 requires panic diagnostics naming the worker + subtree. SC-006 requires an INFO-level fallback log when the walker degrades to serial mode.
- **XI. Enrichment** — ✅ N/A. Not an enrichment-source change.
- **XII. External Data Source Enrichment** — ✅ N/A. `scan_fs` walker doesn't touch external data sources.

**Result**: All 12 principles pass. Zero justifications required for the Complexity Tracking table.

## Project Structure

### Documentation (this feature)

```text
specs/772-parallel-scan-walker/
├── plan.md              # This file (/speckit.plan output)
├── spec.md              # Feature spec (/speckit.specify + /speckit.clarify output)
├── research.md          # Phase 0 output (/speckit.plan)
├── data-model.md        # Phase 1 output (/speckit.plan)
├── contracts/           # Phase 1 output (/speckit.plan)
│   └── walker-parallelism.md
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
│       ├── walk_registry/
│       │   ├── walker.rs                # MODIFIED — split walk_inner into per-dir body + queue-push; add ParallelWalker::run
│       │   ├── mod.rs                   # MODIFIED — public re-exports if new types need cross-module visibility
│       │   └── perf_metrics.rs          # MODIFIED — thread-safe metric accumulation (Arc<Mutex<>> or per-worker sum-at-end)
│       └── walk.audit-allowlist.txt     # MODIFIED — add any new `fn walk_*` function name per FR-011
└── tests/
    └── walker_parallelism_772.rs        # NEW — integration tests for FR-001, FR-003, FR-007, FR-008 acceptance scenarios
```

**Structure Decision**: Single-crate change to `waybill-cli`. The parallelization keeps everything in `walker.rs` — no new files, extending the existing ~882-line module by ~250 lines. If code review flags the file as unwieldy post-implementation, extract to a `walker/serial.rs` + `walker/parallel.rs` split at that time (deferred decision).

## Complexity Tracking

> **No Constitution violations to justify.** Table intentionally empty.
