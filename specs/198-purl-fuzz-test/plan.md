# Implementation Plan: Versionless PURL Round-Trip Fuzz Test

**Branch**: `198-purl-fuzz-test` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/198-purl-fuzz-test/spec.md`

## Summary

Land one new integration-test file `mikebom-common/tests/versionless_purl_fuzz.rs`
that exercises the `Purl` newtype's parse → re-serialize round-trip
across 1100+ synthetic versionless PURL inputs (100+ per ecosystem × 11
ecosystems). Zero new Cargo dependencies; hand-rolled catalog-driven
generator. Zero changes to `Purl` newtype source. Purely additive test
infrastructure. Closes #566.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–197; no nightly).
**Primary Dependencies**: Existing only — `mikebom_common::types::purl::Purl` (the type under test). No `proptest`, no `quickcheck`, no `libfuzzer-sys` per spec FR-005 zero-new-Cargo-deps constraint.
**Storage**: N/A.
**Testing**: New integration-test binary `mikebom-common/tests/versionless_purl_fuzz.rs`. Runs as part of the default `cargo test --workspace` invocation (no opt-in gate per FR-006). Note: mikebom-common has no `tests/` directory today — the plan creates it. Existing tests in that crate live inline as `#[cfg(test)] mod tests` blocks; adding an integration-test binary is idiomatic + doesn't disturb the inline tests.
**Target Platform**: Same as mikebom-common itself. Pure-computation test; no host-specific behavior.
**Project Type**: Test augmentation. Zero library / CLI / SBOM-shape changes.
**Performance Goals**: FR-006 caps wall-clock contribution at ≤ 5 seconds. `Purl::new` is O(input-length) parse + O(1) canonicalize; 1100 invocations at ~microseconds each = ~1ms total execution. Test overhead is dominated by cargo-test binary startup, well under budget.
**Constraints**: (a) zero new Cargo deps; (b) `Purl` newtype source at `mikebom-common/src/types/purl.rs` MUST NOT change (per spec FR-007 — if the fuzz surfaces a real `Purl` bug, the milestone bounds that finding as scope-out); (c) `./scripts/pre-pr.sh` wall-clock delta ≤ 5s vs pre-m198 baseline per SC-004.
**Scale/Scope**: 1 new test file (~200 LOC total: ~100 LOC catalog table + ~50 LOC test body + ~50 LOC diagnostic-emission helpers).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Pure Rust, Zero C** — ✅ PASS. Pure Rust test file.
- **II. eBPF-Only Observation** — ✅ N/A.
- **III. Fail Closed** — ✅ PASS. Fuzz-test failures panic per cargo-test convention; no silent skips of failures.
- **IV. Type-Driven Correctness** — ✅ PASS. Catalog is a typed `const &[(&str, &[&str])]`.
- **V. Specification Compliance** — ✅ PASS. No new `mikebom:*` annotations, no format changes. Fuzz is a check of purl-spec conformance, not an emitter.
- **VI. Three-Crate Architecture** — ✅ PASS. New file in `mikebom-common/tests/` — the crate under test.
- **VII. Test Isolation** — ✅ PASS. Every invocation constructs a fresh `Purl` from a static string; no cross-test state.
- **VIII. Completeness** — ✅ PASS. This test IS a completeness-of-emission check for the `Purl` newtype's contract.
- **IX. Accuracy** — ✅ PASS. Round-trip byte-identity is the strictest accuracy check.
- **X. Transparency** — ✅ PASS. On failure, diagnostic per FR-004 names ecosystem + shape + observed + expected.
- **XI. / XII. Enrichment** — ✅ N/A.
- **Strict Boundary §5 (file-tier)** — ✅ PASS.

**Result**: All principles PASS. No violations.

**Post-Phase-1 re-check**: N/A here — Phase 1 introduces one entity (the catalog) already documented in the spec's Key Entities. No shape changes surface post-design.

## Project Structure

### Documentation (this feature)

```text
specs/198-purl-fuzz-test/
├── plan.md              # This file
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0 output — 3 investigation decisions
├── data-model.md        # Phase 1 output — catalog shape + diagnostic shape
├── quickstart.md        # Phase 1 output — 3 reproducers
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (created by /speckit-tasks)
```

No `contracts/` sub-directory. The fuzz test has no external contract surface — it consumes the existing `Purl` newtype public API and produces stdout diagnostics per cargo-test convention.

### Source Code (repository root)

```text
mikebom-common/tests/
└── versionless_purl_fuzz.rs   # NEW — 1 test file, ~200 LOC total.
                               # First tests/*.rs file in mikebom-common; creating the directory
                               # is a natural consequence of adding it.
```

**Structure Decision**: Single new file under `mikebom-common/tests/`. Adds an integration-test binary target (cargo discovers `tests/*.rs` files automatically); no `Cargo.toml` edit required. No changes to `src/`, no changes to any existing file.

## Complexity Tracking

No constitution violations. No complexity beyond what m197 already carries.
