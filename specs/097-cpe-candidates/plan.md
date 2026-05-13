# Implementation Plan: CPE candidate emission for binary-identified components

**Branch**: `097-cpe-candidates` | **Date**: 2026-05-12 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/Users/mlieberman/Projects/mikebom/specs/097-cpe-candidates/spec.md`

## Summary

Add a `"generic"` ecosystem arm to the existing `mikebom-cli/src/generate/cpe.rs::synthesize_cpes()` synthesizer, backed by a small in-source `(library_slug, vendor, product)` mapping table for the 10 v1-supported libraries (the 11 milestone-096 / earlier version-string libraries minus BoringSSL — which has no NVD-tracked CPE namespace). For each binary-extracted `pkg:generic/<lib>@<version>` component whose `<lib>` matches a table row, emit ONE CPE 2.3 string at the component's CDX `component.cpe` field, SPDX 2.3 `externalRefs[].cpe23Type` ref, and SPDX 3 `Software:cpe` array. Symbol-fingerprint-only components (empty version) inherit the existing `synthesize_cpes` empty-version fast-return path (FR-004 satisfied for free). Composite-evidence merges inherit the version-string PURL's identity so the version-pinned CPE wins (FR-005 satisfied for free).

The existing module already handles CPE 2.3 format-string emission, escape rules per spec §6.2, candidate-list ordering, and downstream wiring into CDX/SPDX 2.3 emission paths (CDX: `entry["cpe"] = json!(component.cpes[0])` at `cyclonedx/builder.rs:710`; SPDX 2.3: `cpe23Type` external ref at `spdx/packages.rs:406`). SPDX 3 audit at planning time (Phase 0) confirms the corresponding `Software:cpe` emission path. Net diff: ~50 lines of new mapping table + ecosystem-arm code + ~6 new unit tests + 1 integration test asserting CPE-present-on-binary-extracted-OpenSSL.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–096; no nightly required).
**Primary Dependencies**: Existing only — `serde`/`serde_json` (CDX/SPDX JSON output), `tracing`, `anyhow`. No new Cargo deps.
**Storage**: N/A — pure metadata transform on SBOM emission code path; no caches, no persistence.
**Testing**: `cargo +stable test` workspace. Three new unit tests in `cpe.rs::tests`; one new integration test in `mikebom-cli/tests/cpe_binary_id.rs` (or extension of existing `binary_id_enrich.rs`).
**Target Platform**: Same as host mikebom (linux/macos/win — the CPE synthesizer is host-platform-agnostic).
**Project Type**: Rust CLI workspace (`mikebom-cli` binary + `mikebom-common` lib + `xtask`).
**Performance Goals**: ≤1µs per component for CPE generation (a HashMap lookup + 12 `format!` calls). Sub-millisecond per full-SBOM emission delta.
**Constraints**: Zero new Cargo deps (FR-007). Production code changes confined to `mikebom-cli/src/generate/cpe.rs` (FR-008). Goldens may regenerate for ≤1 fixture component per milestone-096 SC-007.
**Scale/Scope**: 10 mapping-table rows in v1; pattern-set extension is one line per added library. SBOM emission already at workspace test-suite scale.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Rationale |
|-----------|--------|-----------|
| **I. Pure Rust, Zero C** | ✅ PASS | All new code is Rust. No new Cargo deps. |
| **II. eBPF-Only Observation** | ✅ N/A | This milestone is enrichment, not discovery; no observation path touched. |
| **IV. Test Discipline** | ✅ PASS | TDD + `cargo +stable clippy --workspace --all-targets -- -D warnings` clean. |
| **V. Specification Compliance** | ✅ PASS | CDX 1.6 `component.cpe`, SPDX 2.3 `cpe23Type` external ref, SPDX 3 `Software:cpe` — all standards-native fields. NO `mikebom:*` annotation added. Existing `mikebom:cpe-candidates` property (carrying the 2nd-onward synthesized candidates) is unchanged. |
| **X. Transparency** | ✅ PASS | The mapping table lives in-source with per-row NVD-citation rationale comments; no hidden lookups, no network calls. |
| **XII. External Data Source Enrichment** | ✅ N/A | No external API calls; mapping table is in-source static data. |

**No violations to track in Complexity Tracking.**

## Project Structure

### Documentation (this feature)

```text
specs/097-cpe-candidates/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
├── checklists/
│   └── requirements.md  # Already exists (from /speckit-specify)
├── spec.md              # Already exists
└── tasks.md             # Phase 2 output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/generate/
├── cpe.rs               # EXTEND — add "generic" ecosystem arm + LIBRARY_CPE_MAPPINGS table
└── (untouched)          # cyclonedx/builder.rs:710 + spdx/packages.rs:406 already
                         # consume cpes[0]; no emission-side changes needed

mikebom-cli/tests/
└── cpe_binary_id.rs     # NEW — integration test asserting CPE on milestone-096
                         # binary-extracted PackageDbEntry pipeline (or extend
                         # binary_id_enrich.rs — implementer's call at task time)
```

**Structure Decision**: Single-file delta to `mikebom-cli/src/generate/cpe.rs`. The existing synthesizer is already wired into the post-dedup component-aggregation step at `scan_fs/mod.rs:641`, into CDX emission at `cyclonedx/builder.rs:710`, and into SPDX 2.3 emission at `spdx/packages.rs:406`. SPDX 3 emission path needs audit during Phase 0 — if it doesn't already plumb `cpes` through, add the same wire-up.

## Complexity Tracking

No constitution violations. Table empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
