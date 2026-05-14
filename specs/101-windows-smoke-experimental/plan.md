# Implementation Plan: Windows smoke test + experimental docs callout

**Branch**: `101-windows-smoke-experimental` | **Date**: 2026-05-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/101-windows-smoke-experimental/spec.md`

## Summary

Add a Windows-host integration smoke test (`mikebom-cli/tests/scan_windows_smoke.rs`, `#[cfg(windows)]`-gated) that runs `mikebom sbom scan` against two existing fixtures — the cargo `lockfile-v3` fixture and the `polyglot-monorepo` (pypi + npm) fixture — and asserts exit code 0, well-formed CycloneDX 1.6 JSON, ≥1 component PURL per ecosystem, no backslashes in path-shaped fields, and a 60-second per-scan timeout. On failure, write the emitted SBOM to a tempdir and print inline diagnostics. Also: split the Windows CI lane's test step into a blocking smoke step + a non-blocking workspace step (the latter retains milestone-100's `continue-on-error: true` for the #210 backlog), update README.md's Windows row from `✅ supported` to `🧪 experimental`, and add experimental callouts to README.md + `docs/user-guide/installation.md` linking to issue #210.

Test-and-docs-only PR: zero production-code changes, zero new Cargo dependencies. Diff scope ≤4 files modified + 1 new test file.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–100; no nightly required for this user-space test-and-docs work).
**Primary Dependencies**: Existing only — `std::process::Command` (binary invocation), `std::time::Instant` + `std::thread` (60-second timeout via spawn-and-kill), `tempfile` (already in dev-deps), `serde_json::Value` (JSON parsing), `env!("CARGO_BIN_EXE_mikebom")` (cargo's integration-test binary-path mechanism), `env!("MIKEBOM_FIXTURES_DIR")` (milestone-090's fixture cache). **No new crates.**
**Storage**: N/A — per-test in-memory; `actual.cdx.json` written to a per-test tempdir on failure for diagnostic purposes only.
**Testing**: `cargo test` integration test pattern. The smoke test is `#[cfg(windows)]`-gated so it's a no-op on Linux/macOS; per-host coverage uses existing goldens regression on Unix.
**Target Platform**: Windows x86_64 (`x86_64-pc-windows-msvc`). The test compiles on all platforms (cargo always compiles integration tests) but only the `#[cfg(windows)]` test fn runs on Windows hosts; on Unix/macOS the integration-test crate compiles empty.
**Project Type**: Single-crate workspace (`mikebom-cli`), test-tree + docs-tree edits + CI YAML edit.
**Performance Goals**: <30 seconds total CI runtime for the smoke step (two scans + parse + assert; typical scan is ~1-2 sec on Linux/macOS, expect 2-5× on the colder Windows runner).
**Constraints**: Diff scope ≤4 modified files (`README.md`, `docs/user-guide/installation.md`, `.github/workflows/ci.yml`, plus the NEW `tests/scan_windows_smoke.rs`); zero production-code changes; zero new Cargo deps; zero changes to Linux/macOS CI behavior.
**Scale/Scope**: One test file (~150 lines), two fixture reuses, two docs touch-points, one CI YAML split — small, contained PR.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution version: **1.4.0** (`.specify/memory/constitution.md`).

| Principle | Compliance | Notes |
|-----------|------------|-------|
| **I. Pure Rust, Zero C** | ✅ PASS | No new deps; test uses `std::process::Command` (pure Rust). |
| **II. eBPF-Only Observation** | N/A | Test exercises the user-space SBOM-emitter; no eBPF surface touched. |
| **III. Fail Closed** | ✅ PASS | Smoke test fails the CI lane (after FR-008 split) when the scan errors, hangs, or emits malformed JSON — exactly the "fail closed" contract for the Windows binary at the integration boundary. |
| **IV. Type-Driven Correctness** | ✅ PASS | Test code uses `.unwrap()` under the existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` test convention; no production-code changes. |
| **V. Specification Compliance** | ✅ PASS (with audit) | The smoke test asserts CycloneDX 1.6 envelope conformance (`bomFormat == "CycloneDX"`, `specVersion == "1.6"`) plus PURL prefix validity. No new `mikebom:*` properties introduced — the test READS path-shaped values from `mikebom:source-files`/`evidence.occurrences[].location` which are pre-existing properties from milestones 100/004/etc. **Standards-native-precedence audit**: this milestone does NOT introduce any new mikebom-prefixed property; it consumes existing ones for assertion purposes. No reviewer-action required. |
| **VI. Three-Crate Architecture** | ✅ PASS | Touches only `mikebom-cli/tests/` (integration test) + `.github/` (CI) + root-level docs. No new crates. |
| **VII. Test Isolation** | ✅ PASS | The smoke test runs without eBPF privileges (eBPF is `#[cfg(target_os = "linux")]`-gated anyway, and the smoke test is `#[cfg(windows)]`). Standard `cargo test --workspace` invocation. |
| **VIII. Completeness** | N/A (test-only) | The smoke test does not change the SBOM emission semantics. |
| **IX. Accuracy** | N/A (test-only) | Ditto. |
| **X. Transparency** | ✅ PASS | Failure-diagnostic policy (FR-012) prints inline diagnostics + `actual.cdx.json` — a transparency-friendly debugging contract. |
| **XI. Enrichment** | N/A | No new enrichment. |
| **XII. External Data Source Enrichment** | N/A | No external data source consumed. |

**Strict Boundaries**:
1. No lockfile-based dependency discovery → N/A (test reads its own SBOM output).
2. No MITM proxy → N/A.
3. No C code → ✅ no new deps.
4. No `.unwrap()` in production → ✅ test code only, gated under existing `cfg_attr(test, allow(clippy::unwrap_used))` convention.

**Result**: All gates PASS. No complexity-tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/101-windows-smoke-experimental/
├── plan.md                                  # This file
├── spec.md                                  # Feature spec (with Clarifications)
├── research.md                              # Phase 0 (Q&A on tech choices)
├── data-model.md                            # Phase 1 (file-by-file shape)
├── contracts/
│   └── smoke-test-contracts.md              # Phase 1 (behavioral contracts)
├── quickstart.md                            # Phase 1 (maintainer recipes)
├── checklists/
│   └── requirements.md                      # Spec quality checklist (12/12 PASS)
└── tasks.md                                 # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
mikebom-cli/
├── Cargo.toml                               # unchanged
├── src/                                     # unchanged — zero production-code changes
└── tests/
    ├── scan_windows_smoke.rs                # NEW: the Windows smoke test (#[cfg(windows)])
    └── (existing test files unchanged)

.github/workflows/
├── ci.yml                                   # MODIFY: split Windows test step into smoke + workspace
└── (other workflows unchanged)

README.md                                    # MODIFY: experimental callout + table cell update
docs/
└── user-guide/
    └── installation.md                      # MODIFY: experimental callout consistent with README
```

**Structure Decision**: Test code lives at `mikebom-cli/tests/scan_windows_smoke.rs` — cargo's standard integration-test location, matches the milestone-100 PR's `scan_walker_loops.rs` and the pre-existing `scan_polyglot_monorepo.rs` (which the new smoke test borrows fixture-loading pattern from). The `#[cfg(windows)]` gate is at the `#[test]` fn level so the file compiles on every host but the test only RUNS on Windows. Docs touch-points are README.md + `docs/user-guide/installation.md`, the same two files milestone 100 updated.

## Complexity Tracking

No constitution violations to justify. Feature is small, scoped, additive, and test-and-docs-only. Trivial PR by all metrics — zero production-code risk, zero new dependencies, zero changes to non-Windows CI behavior.
