# Implementation Plan: Reduce `go mod why` subprocess-spawn amplification on Go monorepos

**Branch**: `771-gomodwhy-subprocess-scale` | **Date**: 2026-09-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/771-gomodwhy-subprocess-scale/spec.md`

## Summary

Milestone 112 (`go mod why -m -vendor` build-inclusion classification) currently amplifies subprocess spawns on Go monorepos: 39-workspace fixture × (1 preflight + 13 chunks) = **546 `go` invocations** on the Kubernetes source tree, which exhausts the 60-second shared budget and skips 22/39 workspaces entirely. This milestone reduces the amplification through three independently-shippable tiers (US1 bump `CHUNK_SIZE`; US2 parallel workspace analysis; US3 shared `go list all` preflight per `go.work` scope), targeting default-flags scan wall-time ≤ 10s on Kubernetes with zero new Cargo dependencies and byte-identity for every existing Go fixture.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–770; no nightly required).
**Primary Dependencies**: Existing only — `std::thread`, `std::sync::{mpsc, atomic, Arc}`, `std::process::Command`, `std::time::{Duration, Instant}`, `tracing`, `anyhow`, `thiserror`. Reuses the m055 / m173 spawn-thread + `mpsc::recv_timeout` subprocess pattern already at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:240` and `graph_resolver.rs:1001`. **Zero new Cargo dependencies** (FR-013).
**Storage**: N/A — all state in-process per scan; matches every reader milestone since m002. The `BudgetTracker` (`Instant + Duration`, naturally `Send + Sync` via `Copy`) shared budget is passed as `Arc<BudgetTracker>` to concurrent workers per FR-004.
**Testing**: `cargo +stable test --workspace --no-fail-fast` (existing pipeline). New tests: (a) unit tests in `mod_why.rs::tests` for argv-length guard + go.work member enumeration; (b) integration test at `waybill-cli/tests/mod_why_scaling.rs` exercising the three US independently; (c) empirical wall-time regression measurement against a small synthetic multi-workspace Go fixture (NOT the full Kubernetes clone — that stays as a benchmark-only reference per spec Assumption 1).
**Target Platform**: macOS aarch64 / linux-x86_64 / linux-aarch64 / windows-x86_64 (waybill's four supported host classes per milestone 100). The `available_parallelism()` primitive is stable on all four.
**Project Type**: Rust CLI (waybill-cli crate) — single project, no cross-tree changes.
**Performance Goals**: SC-001 wall-time thresholds on Kubernetes fixture — US1 ≤ 30s, US1+US2 ≤ 15s, all three ≤ 10s (reference class: macOS aarch64 ≥ 8 logical CPUs, warm cache). SC-002 zero workspaces skipped due to `budget-exhausted`.
**Constraints**: (a) Byte-identity for every fixture in `waybill-cli/tests/fixtures/golang/` (SC-003); (b) `--no-go-mod-why` flag byte-identical to pre-milestone (SC-006 regression pin); (c) FR-013 summary-log wire shape preserved (FR-009); (d) `WorkspaceMode` enum unchanged (FR-014); (e) shared 60s budget default unchanged.
**Scale/Scope**: 39-workspace / 246-module Kubernetes fixture is the canonical stress case. Non-goal: cross-scan cache (all state per-scan). New/modified source files ≤ 3: `mod_why.rs` (US1 + US2 + US3 core logic), `scan_fs/package_db/mod.rs` (caller-site invocation change), plus one new integration-test file. LOC delta: ≤ ~300 lines net add (US1 ~10 lines, US2 ~120 lines, US3 ~180 lines including go.work parsing).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Verified against Waybill Constitution v2.1.0 (`.specify/memory/constitution.md`):

- **I. Pure Rust, Zero C** — ✅ No C sources introduced. Pure-Rust stdlib primitives only.
- **II. eBPF-Only Observation** — ✅ N/A. This milestone does not touch the eBPF trace path; it is entirely in the user-space `scan_fs` classifier tier which per Principle II operates as _enrichment_ (m112 classifies edges from lockfile-observed modules; no new components are introduced).
- **III. Fail Closed** — ✅ Preserved. All existing degrade paths (`SkipReason::UnresolvablePackages`, `SkipReason::BudgetExhausted`, `Invocation::TimedOut`, `Invocation::SpawnFailed`) remain in place. FR-007 explicitly propagates preflight failure to every member of the shared scope.
- **IV. Type-Driven Correctness** — ✅ Existing newtype vocabulary (`WorkspaceMode`, `GoModWhyVerdict`, `SkipReason`, `BudgetTracker`, `Invocation`) is reused verbatim. New types this milestone introduces (`GoWorkScope`, `SharedPreflightCache`) follow the same newtype-struct pattern. Zero `.unwrap()` in production paths.
- **V. Specification Compliance** — ✅ No new `waybill:*` properties, annotations, or relationship types. This milestone is pure execution-model — SBOM output is byte-identical (SC-003). No format-mapping catalog rows added.
- **VI. Three-Crate Architecture** — ✅ Changes are confined to `waybill-cli/src/scan_fs/package_db/`. No new crates, no cross-crate boundary changes.
- **VII. Test Isolation** — ✅ All new tests are pure user-space (no eBPF privilege dependency). Runs under standard `cargo test` without root or `CAP_BPF`.
- **VIII. Completeness** — ✅ US2 + US3 IMPROVE completeness: 22 currently-skipped workspaces on the Kubernetes fixture will now be classified. SC-002 codifies this.
- **IX. Accuracy** — ✅ Preserved. FR-015 pins verdict rules unchanged. Byte-identity fixture regression (SC-003, SC-006) is the guardrail.
- **X. Transparency** — ✅ Preserved. FR-005 requires per-workspace log-correlation under concurrent execution. FR-009 preserves the FR-013 summary-log wire shape; downstream tooling that parses it continues to work.
- **XI. Enrichment** — ✅ N/A. Not an enrichment-source change.
- **XII. External Data Source Enrichment** — ✅ N/A. `go mod why` is the same external data source as pre-milestone; the milestone changes how it's invoked, not what it does.

**Result**: All 12 principles pass. Zero justifications required for the Complexity Tracking table.

## Project Structure

### Documentation (this feature)

```text
specs/771-gomodwhy-subprocess-scale/
├── plan.md              # This file (/speckit.plan output)
├── spec.md              # Feature spec (/speckit.specify output; /speckit.clarify amended)
├── research.md          # Phase 0 output (/speckit.plan)
├── data-model.md        # Phase 1 output (/speckit.plan)
├── contracts/           # Phase 1 output (/speckit.plan)
│   └── go-toolchain-invocation.md
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
│           ├── mod.rs                       # MODIFIED — caller-site changes (US2 concurrent invocation, US3 scope grouping)
│           └── golang/
│               ├── mod_why.rs               # MODIFIED — US1 CHUNK_SIZE bump + argv guard; US2 concurrent analyze; US3 shared-preflight cache
│               └── mod_why/                 # NEW (optional) — if mod_why.rs grows past a manageable size, extract:
│                   ├── argv_guard.rs        #   (US1) chunk-size selection + ARG_MAX defense
│                   ├── concurrent.rs        #   (US2) parallel workspace analyzer
│                   └── shared_preflight.rs  #   (US3) go.work scope enumeration + preflight cache
└── tests/
    └── mod_why_scaling.rs                   # NEW — integration tests for US1/US2/US3 acceptance scenarios
```

**Structure Decision**: Single-crate change to `waybill-cli`. The pilot implementation should keep everything in `mod_why.rs` (~300 line addition) to preserve context locality; if code review flags the file as unwieldy, split into the `mod_why/` submodule at that time (deferred decision documented in `research.md` R6). No new tests directory; the new integration test lives alongside the ~200 other `waybill-cli/tests/*.rs` binaries.

## Complexity Tracking

> **No Constitution violations to justify.** Table intentionally empty.
