# Implementation Plan: Single-Pass Walker with Reader-Registry Dispatch

**Branch**: `664-single-pass-walker` | **Date**: 2026-08-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/664-single-pass-walker/spec.md`

## Summary

Introduce a shared, single-pass filesystem walker and a reader-registry that dispatches per-file callbacks to matching package-db readers by filename pattern. Today, `package_db::read_all` invokes ~28 ecosystem readers sequentially and each calls `safe_walk` independently — on the m664 ansible baseline, ~1.1s (28% of the 4.10s wall time) is spent by 20+ readers walking the same 500 directories looking for manifest files that never exist. The refactor collapses that O(dirs × readers) syscall pattern into O(dirs + readers) by having readers register interest in filename patterns at scan init; a single walker traverses the tree once, builds an in-memory (directory → filenames) index, and dispatches per-file callbacks to interested readers. Two-phase readers (pip, cargo, gem, ...) use a sibling-lookup helper that queries the in-memory index rather than triggering fresh `read_dir()` syscalls. Migration is per-reader with a coexistence window (FR-004) — during migration, the shared walker runs additively alongside non-migrated legacy walkers, so US1 pilot sizing must move enough hot readers to net the SC target after paying the shared-walker walker-floor tax (~120 ms on ansible per the m664 `--no-package-db` measurement).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–663; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `globset = "0.4"` (already a direct workspace dep since milestones 113 + 118; used here for filename-pattern matching), `std::path::{Path, PathBuf}`, `std::fs::{read_dir, canonicalize}`, `std::collections::{HashMap, HashSet}`, `tracing` (INFO-level FR-009 diagnostic log), `anyhow`/`thiserror` (error surface), `serde`/`serde_json` (existing — no new schema). **Zero new Cargo dependencies.**
**Storage**: N/A — the (directory → filenames) index is in-process per scan; dropped at `read_all` return. Mirrors every scan_fs milestone since 002.
**Testing**: `cargo +stable test --workspace` + `cargo +stable clippy --workspace --all-targets`. Unit tests for registry dispatch + sibling lookup + panic isolation live in `waybill-cli/src/scan_fs/walk_registry/`. Regression-guard microbenchmark for SC-005 lives in `waybill-cli/tests/perf_walk_dispatch.rs` (unprivileged; runs on every CI lane).
**Target Platform**: macOS + Linux + Windows host (per m100). Reference-environment perf targets in SC-001/002/003 are macOS APFS release-mode with warm caches; CI-linux perf assertion is SC-005's per-file dispatch overhead microbenchmark (not wall-clock).
**Project Type**: single-project (Rust workspace at repository root; three crates per Principle VI: `waybill-cli`, `waybill-common`, `waybill-ebpf`). This milestone touches `waybill-cli` only.
**Performance Goals**: SC-001 ansible offline ≤ 1.2s (baseline 4.10s, ≥3.4×). SC-002 pytorch offline ≤ 1.5s (baseline 4.30s). SC-003 mongodb offline ≤ 3.0s (baseline 15.68s, ≥5×). SC-005 p95 per-file dispatch overhead ≤ 100 µs on synthetic 10k-file tree.
**Constraints**: FR-006 byte-identity of every existing golden SBOM (blocking gate per-migration PR). FR-010 no new Cargo dependencies. FR-011 no CLI-flag or SBOM-field semantic change. FR-012 no reader parallelism this milestone.
**Scale/Scope**: ~28 walker-using readers per the m664 diagnostic sample. Migration proceeds one reader at a time (FR-004 coexistence). US1 pilot: 6-8 top-hotness readers covering ≥1.0s of legacy walker cost on ansible. US2: remaining ~20 walker-using readers. US3: legacy call-site removal + FR-008 regression guard.

## Constitution Check

*GATE: All 12 principles evaluated pre-Phase 0. Re-check after Phase 1 design.*

| Principle | Assessment | Notes |
|---|---|---|
| I. Pure Rust, Zero C | ✅ PASS | FR-010 forbids new deps; all code in `std::` + existing workspace crates. |
| II. eBPF-Only Observation | N/A | This milestone is scan-side (source-tree walk), not trace-side. |
| III. Fail Closed | ✅ PASS | FR-002 preserves m114 permissive-on-error walker semantics; no fail-closed-vs-static tradeoff. |
| IV. Type-Driven Correctness | ✅ PASS with note | Registry uses newtype `ReaderId(&'static str)` — see research §R3. No production `.unwrap()`. |
| V. Specification Compliance | ✅ PASS | FR-006 + FR-011 preserve every emitted SBOM field byte-identically. Zero new `waybill:*` annotations. FR-009 diagnostic log is `tracing::info!`, not an SBOM property. |
| VI. Three-Crate Architecture | ✅ PASS | All new code in `waybill-cli/src/scan_fs/`. No new crates. |
| VII. Test Isolation | ✅ PASS | SC-005 perf regression test is unprivileged (synthetic tree in `tempfile::tempdir()`). No eBPF involvement. |
| VIII. Completeness | ✅ PASS | FR-006 golden byte-identity blocks any completeness regression per-migration. |
| IX. Accuracy | ✅ PASS | Same — no output changes. |
| X. Transparency | ✅ PASS | FR-009 log line is a diagnostic transparency signal; consumers gain scan-perf visibility. |
| XI. Enrichment | N/A | No enrichment change. |
| XII. External Data Source Enrichment | N/A | No external data source touched. |

**Gate: PASSED.** No violations, no justifications required, no entries in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/664-single-pass-walker/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output — reader-migration recipe
├── contracts/
│   └── registry-api.md  # Rust API contract for the walker registry
├── checklists/
│   └── requirements.md  # From /speckit.specify
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── scan_fs/
│       ├── walk.rs                # existing m054/m114 safe_walk (kept as escape hatch — see FR-005)
│       ├── walk_registry/         # NEW — the single-pass walker + reader-registry
│       │   ├── mod.rs             # public API: SharedWalker, ReaderRegistry, ReaderId, register(...)
│       │   ├── registry.rs        # ReaderRegistry impl — filename-pattern index → Vec<ReaderId>
│       │   ├── walker.rs          # SharedWalker::run() — one-pass traversal + dispatch
│       │   ├── dir_index.rs       # in-memory (dir → filenames) index for sibling lookup
│       │   ├── dispatch.rs        # per-file dispatch loop + catch_unwind isolation
│       │   ├── walk_context.rs    # SharedWalkerContext — the reader-facing callback arg
│       │   └── perf_metrics.rs    # dispatch-count aggregator for FR-009 diagnostic log
│       └── package_db/
│           ├── mod.rs             # read_all — orchestrator; adds shared-walker invocation step
│           ├── haskell.rs         # US1 pilot: migrate discover_by_filenames + discover_cabal_files to registry
│           ├── ipk_file.rs        # US1 pilot: migrate discover_ipk_files to registry
│           ├── pants_common/      # US1 pilot: migrate discover_build_files to registry
│           ├── scala.rs           # US1 pilot: migrate 4 discover_* walker sites to registry
│           ├── erlang.rs          # US1 pilot: migrate 3 discover_* walker sites to registry
│           ├── rpm_file.rs        # US1 pilot: migrate discover_rpm_files to registry
│           └── (~22 other readers)  # US2: migrate one at a time; each PR flips one reader's flag
└── tests/
    ├── perf_walk_dispatch.rs      # SC-005 microbenchmark — p95 per-file dispatch overhead
    └── walk_registry_integration.rs  # US1/US2 acceptance-scenario coverage; golden-identity assertions
```

**Structure Decision**: Single-project workspace (Principle VI). All new files land under `waybill-cli/src/scan_fs/walk_registry/`. The existing `waybill-cli/src/scan_fs/walk.rs::safe_walk` stays in place as the escape hatch for FR-005 (npm inner walk) and for readers that have not yet migrated during the coexistence window.

## Complexity Tracking

No constitution violations. This section is empty by construction.
