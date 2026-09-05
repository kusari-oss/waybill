# Implementation Plan: Single-Flight Go Preflight + go.work Directive Tolerance (m775)

**Branch**: `775-preflight-single-flight` | **Date**: 2026-09-05 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/775-preflight-single-flight/spec.md`

## Summary

Two independent defect fixes surfaced by one profiling pass.

**US1 (P1)** replaces the cache-stampede pattern at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:687` — check-cache → release-lock → run 11s subprocess → re-acquire → insert — with a per-scope single-flight cell. The first worker to claim a scope holds *that cell* across the subprocess; concurrent workers for the same scope block on the cell and reuse the memoized outcome. The scope-keyed cache mutex is never held across a spawn, so distinct scopes stay independent. Adds a preflight-invocation counter to the m112 FR-013 summary line (FR-015) so the invariant is observable in CI.

**US2 (P2)** extracts the valid `go.work` directive keyword set into one shared definition consulted by both parsers (the strict validator at `gowork.rs:150` and the lenient member-extractor at `mod_why.rs:69`), then teaches the strict parser to accept `toolchain` and `godebug`. Fixes `waybill:go-workspace-mode = malformed: unknown-directive` on every modern Go monorepo.

Verified on scratch branch `scratch-m775-preflight-stampede` (commit `d572e95`): k8s default scan 26.29s → 18.73s (−29%), `go list all` invocations 22 → 5, byte-identical CycloneDX output (817/817 components).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–774; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `std::sync::{Arc, Mutex}`, `std::collections::HashMap`, `std::process::Command` (unchanged invocation shape), `tracing`, `anyhow`, `thiserror`. **Zero new Cargo dependencies at any workspace level** (FR-009 + SC-006).
**Storage**: N/A — the single-flight cells and the preflight cache are per-scan in-process state, dropped when the classifier returns. No caches persist across scans (spec edge case: "repeated scans in one process").
**Testing**: `cargo +stable test --workspace`. Existing `golang_*`, `scan_go_*`, `cdx_regression`, `spdx_regression`, `spdx3_regression`, m669 corpus goldens, plus new unit tests in `mod_why.rs` (single-flight semantics, counter accuracy) and `gowork.rs` (directive tolerance, parser agreement).
**Target Platform**: macOS aarch64 + Linux x86_64 (m669 reference class); Windows x86_64 inherited unchanged.
**Performance Goals**: SC-001 — default offline scan on `test-kubernetes` ≤ 21s (from 26.3s). SC-003 — preflight invocations ≤ 6 (from 22). SC-004 — preflight subprocess wall down ≥ 90% (from 198.1s). SC-009 — single-workspace within ±3%.
**Constraints**: Byte-identity across every fixture except the US2 annotation value (FR-007 + SC-002); zero new deps (FR-009); no new operator surface (FR-008); existing FR-013 summary fields preserved, new field additive only (FR-010); no deadlock or spin (NFR-001); panic-safe cells (NFR-002).
**Scale/Scope**: Reference fixture `test-kubernetes` — 39 `go.mod` files, one 34-member `go.work` scope, 5 loose workspaces, 18-worker pool on the reference host. Stampede magnitude scales with pool size.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Pure Rust, Zero C)** — ✅ Pass. Stdlib synchronization only; zero new deps; no C toolchain touched.
- **Principle II (eBPF-Only Observation)** — ✅ N/A. Source-tree scan-mode work; the trace-mode eBPF discovery surface is untouched.
- **Principle III (Fail Closed)** — ✅ Pass. A failed preflight still skips every member of its scope per m771 FR-007 (FR-005 requires identical propagation to all waiters). NFR-001/NFR-002 require that a panicking or budget-exhausted preflight terminates waiters rather than silently degrading them into an unclassified pass.
- **Principle IV (Type-Driven Correctness)** — ✅ Pass. No new `.unwrap()` in production paths; mutex acquisition uses `.expect()` with a descriptive message matching the existing `mod_why.rs` convention (the crate root denies `clippy::unwrap_used`). Test modules retain the `#[cfg_attr(test, allow(clippy::unwrap_used))]` guard.
- **Principle V (Specification Compliance)** — ✅ Pass. No new `waybill:*` annotation is introduced; US2 *corrects the value* of the existing C112 `waybill:go-workspace-mode` annotation, whose catalog row and cross-format parity treatment are unchanged. FR-015's counter is a log field, not an SBOM field — no emitted-document surface change. The standards-native audit required by Principle V bullet 5 is vacuous here: no new property is added.
- **Principle VI (Three-Crate Architecture)** — ✅ Pass. `waybill-cli` only; no new crates.
- **Principle VII (Test Isolation)** — ✅ Pass. All new tests run without elevated privileges. Single-flight concurrency tests use in-process threads with an injectable work function — no `go` toolchain required, so they run on hosts without Go installed (see research R4).

**Gate result**: PASS. No violations; Complexity Tracking omitted.

## Project Structure

### Documentation (this feature)

```text
specs/775-preflight-single-flight/
├── plan.md                                  # This file
├── spec.md                                  # Feature specification (post-clarify)
├── research.md                              # Phase 0 output (this run)
├── data-model.md                            # Phase 1 output (this run)
├── quickstart.md                            # Phase 1 output (this run)
├── contracts/
│   ├── preflight-single-flight.md           # US1 coordination contract
│   └── gowork-directive-vocabulary.md       # US2 parser-agreement contract
├── checklists/
│   └── requirements.md                      # From /speckit.specify
└── tasks.md                                 # Phase 2 output (/speckit.tasks — NOT this run)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── mod.rs                       # EDIT (US1): GoModWhyOutcome gains the
│           │                                # preflight-invocation counter; FR-013
│           │                                # summary line gains one additive field.
│           └── golang/
│               ├── mod_why.rs               # PRIMARY EDIT (US1): SharedPreflightCache
│               │                            # gains per-scope single-flight cells;
│               │                            # analyze_main_module's preflight branch
│               │                            # rewritten; MainModuleAnalysis reports
│               │                            # whether THIS call spawned a preflight.
│               │                            # (US2): lenient parser consults the
│               │                            # shared directive vocabulary.
│               └── gowork.rs                # EDIT (US2): shared directive vocabulary
│                                            # defined here; strict parser accepts
│                                            # `toolchain` + `godebug`; malformed-reason
│                                            # vocabulary preserved unchanged.
└── tests/                                   # Existing suites unchanged; new coverage
                                             # lands as in-file #[cfg(test)] modules
                                             # (see research R4 for why).
```

**Structure Decision**: Three files edited, zero files created under `src/`. `mod_why.rs` is the primary surface for US1; `gowork.rs` is the primary surface for US2; `mod.rs` carries only the counter plumbing and the additive log field. New tests live in in-file `#[cfg(test)]` modules rather than `tests/` — the single-flight tests need library-scoped access to `SharedPreflightCache` and an injectable work function (research R4), and the parser-agreement test needs both parsers, one of which is `pub` only within the crate.

## Complexity Tracking

*No Constitution Check violations. Section intentionally empty.*
