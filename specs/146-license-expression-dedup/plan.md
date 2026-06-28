# Implementation Plan: SPDX license expression operand dedup

**Branch**: `146-license-expression-dedup` | **Date**: 2026-06-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/146-license-expression-dedup/spec.md`

## Summary

Fix [issue #470](https://github.com/kusari-oss/mikebom/issues/470) — Yocto-built RPMs ship `License: GPL-2.0-only AND GPL-2.0-only` in their headers, and mikebom passes the duplication through verbatim. The fix is a small dedup pass inside `mikebom_common::types::license::SpdxExpression::try_canonical` that collapses byte-identical top-level operands in homogeneous AND-chains and OR-chains.

Phase 0 research §A confirmed the `spdx = "0.10"` crate exposes a clean tree-walking API: `Expression::iter()` returns `ExprNode::Req(ExpressionReq) | ExprNode::Op(Operator)` in postfix order; `LicenseReq` implements `Display` that includes `WITH <exception>` suffix when present, so the byte-comparison naturally treats `GPL-2.0-or-later WITH Classpath-exception-2.0` as one atomic operand. The implementation uses this API directly — no string-walking fallback needed.

Total code surface estimate: ~30-50 LOC in `mikebom-common::types::license` + ~150 LOC of tests + golden refresh (likely small — most existing goldens carry single-id licenses). No new Cargo dependencies. One coordinated change in one crate.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–145; no nightly required).
**Primary Dependencies**: Existing only — `spdx = "0.10"` (already a workspace dep at `mikebom-common`; reused for parse + tree-walking + display). **No new Cargo dependencies.**
**Storage**: N/A — pure-function dedup; no state, no caches, no persistence.
**Testing**: `cargo +stable test --workspace`. New tests are in-file `#[cfg(test)] mod tests` in `mikebom-common/src/types/license.rs`. One synthetic integration test scans a runtime-built RPM (via `rpm::PackageBuilder`) whose `License:` header carries `MIT AND MIT` and asserts the emitted CDX + SPDX 2.3 + SPDX 3 outputs all show `MIT`.
**Target Platform**: Linux x86_64 + macOS arm64 + Windows (CI lanes). Pure-Rust pure-function; no platform-specific behavior.
**Project Type**: Library — touches `mikebom-common`; downstream consumers in `mikebom-cli` (CDX builder, SPDX 2.3 emitter, SPDX 3 emitter) benefit transparently with zero call-site changes.
**Performance Goals**: Microsecond-cost dedup per `SpdxExpression::try_canonical` call. Called at scan-time per component license; total cost across a 5000-component scan is well under 1 ms (cheap string comparisons + a small `BTreeSet`).
**Constraints**:
- **Constitution V (standards-native > `mikebom:*`)**: No new `mikebom:*` annotations introduced; only normalizes the canonical form of an EXISTING type. Compliance audit complete in spec FR-009.
- **Constitution IV (Type-Driven Correctness)**: The dedup pass is implemented on the typed `SpdxExpression` newtype, not on raw `String`. No new `.unwrap()` in production paths.
- **No subprocess calls**: pure Rust string + collection operations.
- **Pre-PR gate**: `./scripts/pre-pr.sh` (clippy `-D warnings` + `cargo test --workspace`) MUST exit 0 before PR open. The pre-existing local `sbomqs_parity` env-only failure documented in milestone 144 T001 still applies; CI on a clean runner validates.
**Scale/Scope**: 7 distinct duplicated expressions × ≥30 components on the Yocto audit baseline. The fix scales O(N) in the number of `SpdxExpression::try_canonical` calls per scan (one per component-license). For the largest Yocto baseline (~4500 components after milestone 144's RPM size cap raise), this is bounded by component count, microseconds total.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle / Boundary | Status | Note |
|---|---|---|---|
| I | Pure Rust, Zero C | ✅ PASS | No new Cargo deps; existing `spdx = "0.10"` is pure Rust (verified per milestone-011 audit). |
| II | eBPF-Only Observation | ✅ N/A | Pure value-normalization in user-space; eBPF discovery untouched. |
| III | Fail Closed | ✅ PASS | No change to scan-failure semantics. The dedup pass is a no-op for malformed inputs (which would have failed `try_canonical` anyway). |
| IV | Type-Driven Correctness | ✅ IMPROVES | The fix lives on the typed `SpdxExpression` newtype; no new `String`-typed boundaries crossed. |
| V | Specification Compliance | ✅ PASS | **No new `mikebom:*` annotations introduced** (spec FR-009 audit). The fix normalizes an EXISTING type's canonical form. `X AND X ≡ X` per SPDX 2.x grammar — the simplification is semantics-preserving. |
| VI | Three-Crate Architecture | ✅ PASS | All code change in `mikebom-common`. Downstream consumers (`mikebom-cli` emitters) benefit transparently without modification. |
| VII | Test Isolation | ✅ PASS | All new tests are pure-logic unit tests; no eBPF privilege requirements. |
| VIII | Completeness | ✅ NEUTRAL | No change to component-discovery or component-inclusion. |
| IX | Accuracy | ✅ IMPROVES | Removes noise from emitted SBOMs (duplicated license operands), improving downstream-tool license-parsing accuracy. |
| X | Transparency | ✅ NEUTRAL | The `MIT AND MIT` → `MIT` simplification is observable but SPDX-equivalent — no information loss to transparency-report. |
| XI | Enrichment | ✅ N/A | No enrichment-source changes. |
| XII | External Data Source Enrichment | ✅ N/A | No external source involvement. |
| SB-1 | No lockfile-based discovery | ✅ N/A | |
| SB-2 | No MITM proxy | ✅ N/A | |
| SB-3 | No C code | ✅ N/A | |
| SB-4 | No `.unwrap()` in production | ✅ PASS | Existing `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention preserved on new tests. |
| SB-5 | No file-tier duplicates in default mode | ✅ N/A | |

**All gates pass. No Complexity Tracking entries required.**

## Project Structure

### Documentation (this feature)

```text
specs/146-license-expression-dedup/
├── plan.md              # This file
├── research.md          # Phase 0 output (spdx-crate tree-walking API verification)
├── data-model.md        # Phase 1 output (SpdxExpression dedup contract)
├── quickstart.md        # Phase 1 output (operator-facing verification)
├── contracts/
│   └── spdx-expression-dedup.md   # Pure-function contract for the new dedup pass
└── checklists/
    └── requirements.md  # Already exists from /speckit-specify
```

### Source Code (repository root)

Touched files (one crate, narrow scope):

```text
mikebom-common/
├── Cargo.toml                                  # No change (existing `spdx = "0.10"` dep is sufficient)
└── src/
    └── types/
        └── license.rs                          # PRIMARY change — dedup pass inside try_canonical + 7-8 new tests
mikebom-cli/
├── (no source changes — emitters consume the deduped SpdxExpression transparently)
└── tests/
    ├── (existing parity-catalog + byte-identity goldens may need refresh if any fixture contains pre-fix `X AND X`)
    └── license_dedup_integration_md146.rs       # NEW one-file integration test — synthetic RPM with License: MIT AND MIT, scan, assert single-id in all 3 formats
```

**Structure Decision**: Single change in `mikebom-common::types::license::SpdxExpression::try_canonical`. Zero new modules, zero signature changes to public APIs, zero call-site changes in `mikebom-cli`. The dedup happens transparently at the type-construction boundary.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

Not applicable — all gates pass.
