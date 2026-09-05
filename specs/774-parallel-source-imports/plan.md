# Implementation Plan: Parallel Go Source-Import Collection (m774)

**Branch**: `774-parallel-source-imports` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/774-parallel-source-imports/spec.md`

## Summary

Extract the two `collect_production_imports` + `collect_test_imports` calls from the serial per-workspace loop at `waybill-cli/src/scan_fs/package_db/golang/legacy.rs:2224–2240` into a bounded parallel phase that runs AFTER the main loop completes. Per-worker local `HashSet<String>` accumulators are merged by a Phase 2 serial reduce (set-union) into `signals.production_imports` + the outer `test_imports` local at `legacy.rs:1697`. Reuses the m771 US2 pattern verbatim: `std::thread::scope` + `Arc<Mutex<Vec<Job>>>` work queue + `mpsc::channel` reducer + `mod_why::worker_count()` helper.

Empirical target (m774 profiling on test-kubernetes, 8-core macOS aarch64, warm cache): `collect_*_imports` phase 16.68s → ~2-3s. Total scan wall (walker-isolated `--offline --no-go-mod-why`): 22.5s → ≤ 10s. Default scan: 34s → ≤ 18s.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–773; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `std::thread`, `std::sync::{Arc, Mutex, mpsc}`, `std::collections::HashSet`, `tracing`, `anyhow`, `thiserror`. Reuses `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs::worker_count(workspace_count)` (m771 helper, extracted at `mod_why.rs:204`). **Zero new Cargo dependencies at any workspace level** (FR-010 + SC-003).
**Storage**: N/A — per-worker `HashSet<String>` locals live for the parallel phase; merged then dropped. No caches, no persistence.
**Testing**: `cargo +stable test --workspace` — existing `golang_transitive_*`, `scan_go_*`, `cdx_regression`, `spdx_regression`, `spdx3_regression` suites + one new integration test at `waybill-cli/tests/collect_imports_parallel_774.rs`.
**Target Platform**: macOS aarch64 + Linux x86_64 (m669 reference class); Windows x86_64 (m100/m101 experimental support inherited unchanged).
**Project Type**: Single Cargo workspace (`waybill-cli` + `waybill-common` + `waybill-ebpf`); this milestone touches only `waybill-cli`.
**Performance Goals**: SC-001 — default `--offline sbom scan` on test-kubernetes ≤ 18s (from 34s); `--offline --no-go-mod-why sbom scan` ≤ 10s (from 22.5s). Both on macOS aarch64 8-core warm cache.
**Constraints**: Byte-identity across every existing fixture (SC-002); zero new Cargo deps (SC-003); deterministic output across independent runs (SC-004); single-workspace degenerate path ≤ ±3% of pre-milestone p50 (SC-005); `./scripts/pre-pr.sh` clean (SC-006); `--no-go-mod-why` orthogonality preserved (SC-007).
**Scale/Scope**: Reference fixture `test-kubernetes` — 39 go.mod files under go.work, ~25k+ `.go` files per workspace, 30k total files scanned. Parallel phase spawns `min(N, available_parallelism())` workers where N = `parsed_roots.len()`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)** — ✅ Pass. Zero new deps at any layer; stdlib `std::thread` + `std::sync` only. No C toolchain touched.
- **Principle II (eBPF-Only Observation)** — ✅ N/A. This milestone is a source-tree scan-mode parallelization; the trace mode's eBPF discovery surface is not touched.
- **Principle III (Fail Closed)** — ✅ Pass. Worker panic propagates via `std::thread::scope` + `ScopedJoinHandle::join()` `Err(_)` → `resume_unwind(payload)` (FR-007). No silent fallback, no partial-result emission if a worker fails.
- **Principle IV (Type-Driven Correctness)** — ✅ Pass. No new `.unwrap()` in production code paths. Error propagation via `anyhow` for the outer scan; `thiserror` for any new error enum (none currently anticipated — worker panic uses `resume_unwind` propagation, not `Result`). Test code retains the existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard.
- **Principle V (Specification Compliance)** — ✅ Pass. Zero wire changes. No new `waybill:*` annotation, no new SBOM field, no new PURL construction. FR-011 + SC-002 pin byte-identity of every existing fixture output (CDX, SPDX 2.3, SPDX 3). The G4 filter's `LifecycleScope::Test` tagging + CDX/SPDX 2.3/SPDX 3 wire mappings are unchanged (documented in `docs/reference/sbom-format-mapping.md` since m052/m179 — no catalog additions).
- **Principle VI (Three-Crate Architecture)** — ✅ Pass. `waybill-cli` only; no new crates, no changes to `waybill-common` or `waybill-ebpf`.
- **Principle VII (Test Isolation)** — ✅ Pass. All new tests run in standard CI without elevated privileges. The one new integration test uses `tempfile::tempdir` + synthetic go.mod fixtures (same pattern as `waybill-cli/tests/golang_transitive_*`).

**Gate result**: PASS. No violations, no Complexity Tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/774-parallel-source-imports/
├── plan.md                                     # This file
├── spec.md                                     # Feature specification
├── research.md                                 # Phase 0 output (this run)
├── data-model.md                               # Phase 1 output (this run)
├── quickstart.md                               # Phase 1 output (this run)
├── contracts/
│   └── collect-imports-parallelism.md          # Phase 1 output — call-site contract
├── checklists/
│   └── requirements.md                         # From /speckit.specify
└── tasks.md                                    # Phase 2 output (/speckit.tasks — NOT this run)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── golang/
│               ├── legacy.rs                   # PRIMARY EDIT — the parallelized site
│               │                               # Extract collect_*_imports pair from
│               │                               # the loop body at :2224–2240 into a
│               │                               # new post-main-loop parallel phase.
│               ├── mod_why.rs                  # READ-ONLY — reuse worker_count()
│               │                               # helper at :204 (m771 US2).
│               └── mod.rs                      # UNTOUCHED — public surface stable.
└── tests/
    ├── golang_transitive_*                     # Existing regression coverage.
    ├── scan_go_*                               # Existing integration coverage.
    ├── cdx_regression*.rs                      # Byte-identity guard (SC-002).
    ├── spdx_regression*.rs                     # Byte-identity guard.
    ├── spdx3_regression*.rs                    # Byte-identity guard.
    └── collect_imports_parallel_774.rs         # NEW — determinism + panic + degenerate
                                                # single-workspace + multi-workspace
                                                # merge correctness.
```

**Structure Decision**: Single primary edit in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`. Zero other production files touched. One new integration test file at `waybill-cli/tests/collect_imports_parallel_774.rs`. No new modules, no new files under `src/` beyond the edit-in-place. This mirrors m771 US2's shape at `waybill-cli/src/scan_fs/package_db/mod.rs::apply_go_mod_why_pass` — the parallelization is a call-site refactor, not a module extraction.

## Complexity Tracking

*No Constitution Check violations. Section intentionally empty.*
