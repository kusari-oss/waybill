# Implementation Plan: Fix walk_registry test flake

**Branch**: `666-walker-test-flake-fix` | **Date**: 2026-08-26 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/666-walker-test-flake-fix/spec.md`

## Summary

Replace the file-scoped `static SEMANTICS_LOG: Mutex<Vec<String>>` in `waybill-cli/src/scan_fs/walk_registry/walker.rs`'s test module with per-test-owned `Arc<Mutex<Vec<String>>>` sinks threaded through the walker's existing m664 contract C4 state slot (`ReaderRegistration.state: Option<Arc<dyn Any + Send + Sync>>`). Each test creates its own sink on its stack frame, passes an `Arc` clone as the registration's state, and asserts against its own copy of the `Arc` after the walker returns. Result: three tests can run in genuine parallel (cargo default `--test-threads = nproc`) without racing on shared mutable state.

Zero new deps, zero new API surface, single-file change, no golden churn. The fix reuses the same `state` slot every migrated production reader (dart, cocoapods, go_binary, cmake, cargo, ...) already threads its per-scan config through — the pattern is already established; tests just adopt it.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–665; no nightly required for this user-space test-only fix).
**Primary Dependencies**: Existing only — `std::sync::{Arc, Mutex}` (stdlib), `ReaderRegistration.state: Option<Arc<dyn Any + Send + Sync>>` (m664 contract C4 slot at `waybill-cli/src/scan_fs/walk_registry/mod.rs:378-385`), `SharedWalkerContext::state::<T>(reader_id) -> Option<&T>` (accessor at `walk_context.rs:53`). **Zero new Cargo dependencies at any layer (production or dev).**
**Storage**: N/A — per-test in-memory sink dropped at test-scope exit.
**Testing**: `cargo +stable test -p waybill --test-threads=<N>` for parallelism verification. 100-iteration harness for SC-001 verification (run at implementation time; not shipped as a persistent CI test).
**Target Platform**: macOS-arm64, Linux-x86_64, Linux-aarch64, Windows-x86_64 (all four release-lane targets). `walker_survives_symlink_loop` remains `#[cfg(unix)]`-gated per pre-fix state.
**Project Type**: Unit-test-only fix inside the `waybill` binary crate's `#[cfg(test)]` module. Not a feature; not a refactor.
**Performance Goals**: No runtime perf impact — this is test-only. Test wall-time change should be within statistical noise (each test's setup is unchanged; the sink lookup adds one `HashMap`-scan-then-downcast per file visit, which the m664 SC-005 microbenchmark already covers under production loads).
**Constraints**: Zero net golden churn (SC-003). Zero pre-existing test regressions (SC-002). Fix survives both `--test-threads=1` (deterministic serial) AND `--test-threads=8` (aggressive parallel), per SC-004.
**Scale/Scope**: 3 tests refactored. 1 file touched (`walker.rs`). ~30 lines net delta (remove ~5 lines `SEMANTICS_LOG` + `record_visit`; add ~25 lines of per-test sink wiring + 3 per-test callbacks).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

All 12 principles evaluated. Every principle either does not apply to test-only fixes or is satisfied by this fix.

| # | Principle | Applies? | Verdict |
|---|-----------|----------|---------|
| I | Pure Rust, Zero C | N/A — test-only, no language-stack changes | PASS |
| II | eBPF-Only Observation | N/A — no discovery mechanism touched | PASS |
| III | Fail Closed | N/A — no runtime behavior change | PASS |
| IV | Type-Driven Correctness | Marginal — the sink type `Arc<Mutex<Vec<String>>>` is not a "domain value" (not a PURL, hash, or license), so Principle IV's newtype-wrapping rule does not apply. Production code's `.unwrap()` ban does not apply to `#[cfg(test)]` blocks (existing precedent: `waybill-cli` crate already `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md; this fix follows the same convention). | PASS |
| V | Specification Compliance | N/A — no SBOM emission changes | PASS |
| VI | Three-Crate Architecture | N/A — no crate structure change | PASS |
| VII | **Test Isolation** | **HIGH RELEVANCE**. This principle mandates that unit tests "MUST run without elevated privileges in standard CI environments" — implicitly, MUST run reliably (a flaky test that intermittently fails in CI is a violation of "runs" for the purposes of the fast-feedback-loop guarantee). The current `SEMANTICS_LOG` shared static VIOLATES this because the tests intermittently fail under cargo's parallel scheduler. **This fix RESTORES conformance** by making each test's observation state per-test-owned. | PASS (fix restores compliance) |
| VIII | Completeness | N/A — no SBOM emission | PASS |
| IX | Accuracy | N/A — no SBOM emission | PASS |
| X | Transparency | N/A — no SBOM emission | PASS |
| XI | Enrichment | N/A — no SBOM emission | PASS |
| XII | External Data Source Enrichment | N/A — no external data sources touched | PASS |

**No constitution violations to justify.** Complexity tracking section stays empty.

## Project Structure

### Documentation (this feature)

```text
specs/666-walker-test-flake-fix/
├── plan.md                        # This file
├── research.md                    # Phase 0 output — technical decisions
├── data-model.md                  # Phase 1 output — entity + relationships
├── contracts/
│   └── test-visit-sink.md         # Phase 1 output — test-pattern contract
├── quickstart.md                  # Phase 1 output — "how to add a 4th test"
├── checklists/
│   └── requirements.md            # Spec-quality checklist (pre-existing)
└── tasks.md                       # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/walk_registry/
├── mod.rs                         # Unchanged. Hosts ReaderRegistration + FileCallback typedef.
├── walker.rs                      # ← ONLY file touched by this fix.
│                                  #   Removes: `static SEMANTICS_LOG` (line 477) +
│                                  #            `fn record_visit` (line 479).
│                                  #   Adds:    3 per-test sink helpers + 3 per-test callbacks.
│                                  #   Modifies: 3 `#[test]` fn bodies (loop / exclusion / noise-dirs).
├── walk_context.rs                # Unchanged. Existing `SharedWalkerContext::state::<T>()` reused verbatim.
├── registry.rs                    # Unchanged.
├── dispatch.rs                    # Unchanged.
├── dir_index.rs                   # Unchanged.
└── perf_metrics.rs                # Unchanged.
```

**Structure Decision**: Single-file change in `waybill-cli/src/scan_fs/walk_registry/walker.rs`. No new module. No new file. No `waybill-cli/src/testing/` helper crate. FR-006's constraint on API surface + SC-005's discoverability-in-one-file-read gate both point at same-file changes.

## Complexity Tracking

> No constitution violations to justify. This section is intentionally empty.
