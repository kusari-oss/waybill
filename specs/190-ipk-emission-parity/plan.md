# Implementation Plan: ipk Emission Parity with RPM Reader

**Branch**: `190-ipk-emission-parity` | **Date**: 2026-07-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/190-ipk-emission-parity/spec.md`

## Summary

Close three ipk-reader emission gaps discovered in a yocto-test rerun against alpha.59:

- **#550 (US1)**: CDX 1.6 emits raw BitBake `&`/`|` (and `&&`/`||`) license operators, breaking SPDX conformance. Fix: route the raw license through the same normalization helper the SPDX 2.3 side already uses — string-level operator substitution (long-form first) + `SpdxExpression::try_canonical` validation — before it reaches the CDX `components[].licenses[].expression` field.
- **#551 (US2)**: SPDX 3 `software_Package` elements omit *all* license fields — no `simplelicensing_LicenseExpression`, no `simplelicensing_CustomLicense`. Fix: wire the same canonicalized license through the SPDX 3 emission path, mirroring the m154 sweep already applied to rpm packages.
- **#552 (US3)**: ipk PURLs carry epoch inline (`pkg:opkg/netbase@1:6.4-r0`) instead of as a `?epoch=N` qualifier — violates the purl-spec's opkg/deb/rpm convention. Fix: mirror `rpm_file.rs::assemble_entry`'s epoch-extraction pattern (regex `^<digits>:<rest>$` → strip prefix, add `&epoch=N` qualifier when non-zero).

Technical approach: single ipk-reader milestone, three small parallel workstreams that share test fixtures. No new Cargo dependencies. No new `mikebom:*` annotations (Principle V). Verify at planning time that the rpm reader emits format-idiomatic empty-license output per Q3; if it diverges, align rpm alongside ipk in the same milestone.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–189; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only — `spdx = "0.10"` (already at `mikebom-common/src/types/license.rs`; used for `SpdxExpression::try_canonical`), `regex = "1"` (workspace; already used by cmake/vcpkg/alpm/brew/yocto/cocoapods/elixir/erlang/scala/haskell/ipk readers for line-format and DSL extraction), `serde`/`serde_json` (JSON emission), `tracing` (info/warn/debug logs on classifier decisions), `anyhow`/`thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation; the `opkg` type is purl-spec-blessed). **Zero new Cargo dependencies.** No subprocess calls. No network access.
**Storage**: N/A — all state is in-process for the duration of a single scan. Matches every ipk-reader milestone since 002.
**Testing**: `cargo +stable test --workspace` (workspace-wide unit + integration). New assertions:
- Unit tests co-located with new helpers (`normalize_bitbake_license`, `parse_opkg_version_with_epoch`) in `ipk_file.rs`.
- Integration tests scanning synthetic ipk fixtures (built via the existing m187 ar-format fixture-builder helper) with compound licenses, empty licenses, epoch-prefixed versions, and no-epoch controls.
- `spdx3-validate==0.0.5` gate — SPDX 3 output from a compound-license fixture MUST pass with zero conformance errors (matches memory `reference_spdx3_validator`).
**Target Platform**: Linux + macOS host builds; behavior is host-agnostic (parses `.ipk` bytes; emits JSON).
**Project Type**: CLI (existing `mikebom sbom scan` subcommand path).
**Performance Goals**: No new perf regression concerns. Fixture cost ≈ ~50 KB per new synthetic ipk (matches m187 fixture footprint).
**Constraints**: Byte-identity of every golden without a non-zero epoch AND without a compound license (SC-006). Byte-identity relaxation permitted for goldens that DO carry epoch or compound license (FR-014) — those must be regenerated once and re-committed as documented milestone drift.
**Scale/Scope**: Three co-located fixes in `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`; possible small deltas in the CDX and SPDX 3 emitters if license routing needs a shim (verify at Phase 0). Estimated 15–20 tasks across US1/US2/US3.

## Constitution Check

Post-Phase-0 recheck below. Initial pass:

| Principle | Status | Notes |
|---|---|---|
| I. Pure Rust, Zero C | PASS | No new deps; no C code introduced. |
| II. eBPF-Only Observation | N/A | User-space `sbom scan` path — orthogonal to eBPF trace. |
| III. Fail Closed | PASS | Malformed ipk fixtures continue to warn-and-skip per FR-007 in m187; this milestone does not add a silent-fallback path. |
| IV. Type-Driven Correctness | PASS | New `epoch: Option<NonZeroU32>` or `Option<u32>` field on the parsed control record; new `CanonicalLicenseExpression` reuses existing `SpdxExpression` newtype from `mikebom-common`; no `.unwrap()` in production code. |
| V. Specification Compliance | PASS + explicitly enforced by FR-015. Standards-native fields (CDX `licenses.expression`, SPDX 2.3 `licenseDeclared`, SPDX 3 `simplelicensing_LicenseExpression`, purl-spec `?epoch=N` qualifier) carry all needed data. Zero new `mikebom:*` annotations. Audit result cited: opkg PURL epoch qualifier is documented in the purl-spec ecosystem list. |
| VI. Three-Crate Architecture | PASS | Changes limited to `mikebom-cli` + shared type reuse from `mikebom-common`. No new crate. |
| VII. Test Isolation | PASS | All new tests are pure-Rust unit + integration; no eBPF privilege required. |
| VIII. Completeness | PASS | No components dropped by any of the three fixes. Empty-license handling emits the component with format-idiomatic absent-license marker (Q3 answer B). |
| IX. Accuracy | PASS | Every PURL emitted with `?epoch=N` is canonical per purl-spec; every license expression validated via `SpdxExpression::try_canonical`. No heuristic guessing. |
| X. Transparency | PASS | Non-standard version-shape observations emit a debug log per edge-case bullet; no new `mikebom:*` transparency annotations required (native fields already carry all information the operator needs). |
| XI. Enrichment | N/A | No external data source touched. |
| XII. External Data Source Enrichment | N/A | No external data source touched. |
| Strict Boundary 1 (No lockfile discovery) | N/A | Not a discovery change. |
| Strict Boundary 2 (No MITM proxy) | N/A | Not a network change. |
| Strict Boundary 3 (No C code) | PASS | No C code. |
| Strict Boundary 4 (No .unwrap() in production) | PASS | New helpers use `Result` throughout; tests are guarded per the existing convention. |
| Strict Boundary 5 (No file-tier duplicates in default mode) | N/A | Not a file-tier-emission change. |

**No violations. Proceed to Phase 0.**

## Project Structure

### Documentation (this feature)

```text
specs/190-ipk-emission-parity/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output — CDX/SPDX 2.3/SPDX 3 emission-shape contracts
├── checklists/
│   └── requirements.md  # Created by /speckit-specify
└── tasks.md             # Created by /speckit-tasks (NOT this command)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           └── ipk_file.rs         # PRIMARY: license normalization routing +
│                                   # epoch extraction. New helpers:
│                                   # `normalize_bitbake_license` (extends the
│                                   # m152 helper), `parse_opkg_version_with_epoch`.
│                                   # Extended parsed-control record: `epoch:
│                                   # Option<u32>` field.
├── src/
│   └── generate/
│       ├── cyclonedx/
│       │   └── builder.rs          # SANITY-CHECK: verify the emit path uses
│                                   # component.licenses directly; may need a
│                                   # shim if the license is stored raw and CDX
│                                   # bypasses the m152 normalization.
│       ├── spdx/
│       │   └── licenses.rs         # NON-INVASIVE: verify SPDX 2.3 emission
│                                   # unchanged; adjust only if Q3's NOASSERTION
│                                   # for empty is not already the current
│                                   # behavior.
│       └── spdx3/
│           └── v3_licenses.rs      # PRIMARY: wire ipk license into the
│                                   # existing SPDX 3 license-expression +
│                                   # custom-license emission path (mirror the
│                                   # rpm/m154 sweep).
└── tests/
    ├── ipk_file_reader.rs          # EXTEND: add compound-license + empty-license +
                                    # epoch-prefixed fixtures; unit-test-scoped.
    ├── ipk_license_parity.rs       # NEW: integration test — scan a compound-license
                                    # ipk fixture with all 3 formats, assert
                                    # normalized-license equality across CDX/SPDX 2.3/SPDX 3.
    └── ipk_epoch_purl.rs           # NEW: integration test — scan an epoch-prefixed
                                    # ipk fixture, assert `?epoch=N` qualifier + naked
                                    # version + PURL equality across all 3 formats.
```

**Structure Decision**: Existing `mikebom-cli` crate; no new modules. The three fixes co-locate in `ipk_file.rs` (parser side) with small emission-side deltas in `generate/cyclonedx/builder.rs` and `generate/spdx3/v3_licenses.rs`. Test files follow the m185/m187 pattern (per-behavior integration test alongside the existing `ipk_file_reader.rs`).

## Complexity Tracking

*No constitution violations — table intentionally empty.*
