# Implementation Plan: SPDX 2.3 §10.1 conformance — milestone 153 (closes #485)

**Branch**: `153-spdx-license-refs-conformance` | **Date**: 2026-07-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/153-spdx-license-refs-conformance/spec.md`

## Summary

Close GitHub issue [#485](https://github.com/kusari-oss/mikebom/issues/485) by extending the SPDX 2.3 document-assembly layer with a post-`build_packages` sweep that extracts every `LicenseRef-<idstring>` substring from the assembled packages' license fields (`licenseDeclared` / `licenseConcluded` / `licenseInfoFromFiles`) and emits matching `hasExtractedLicensingInfos[]` entries with a locked placeholder text (per Clarifications Q1). The sweep dedups with the pre-existing milestone-012 hash-fallback entries produced by `spdx/packages.rs:build_packages` — existing entries with real `extractedText` win over placeholder entries emitted by the new sweep.

The SPDX 3.0.1 emitter needs investigation: its license model (`simplelicensing_LicenseExpression` graph elements + `hasDeclaredLicense`/`hasConcludedLicense` relationships at `spdx/v3_licenses.rs`) doesn't use `hasExtractedLicensingInfos`. If SPDX 3 requires a `licensing_CustomLicense` element per LicenseRef, the fix applies there too; otherwise the SPDX 3 investigation closes with no code change. `spdx3-validate==0.0.5` (per milestone 078) is the empirical adjudicator.

Net new code surface: ~100 LOC in `mikebom-cli/src/generate/spdx/document.rs` (1 new helper + wiring at line ~352, plus a placeholder-text `const`) + possibly ~50 LOC in `mikebom-cli/src/generate/spdx/v3_licenses.rs` (if SPDX 3 needs equivalent) + ~10 new unit tests + 1 CHANGELOG entry.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–152; no nightly required).
**Primary Dependencies**: Existing only — `regex = "1"` (workspace; already a direct dep for the milestone-013 catalog parser; here used to extract LicenseRef- substrings), `serde`/`serde_json` (JSON emission), `mikebom_common::types::license::SpdxExpression` (unchanged), `spdx = "0.10"` (workspace; already a direct dep since milestone 152). **No new Cargo dependencies.**
**Storage**: N/A — pure-function sweep over the assembled `Vec<SpdxPackage>` at document-serialization time. No caches, no persistence.
**Testing**: New unit tests inline in `mikebom-cli/src/generate/spdx/document.rs`'s existing `#[cfg(test)] mod tests` block (or an adjacent module for the new helper). Plus the existing SPDX 2.3 golden test infrastructure at `mikebom-cli/tests/fixtures/golden/*/`.
**Target Platform**: Cross-platform Rust binary. Same surface as the rest of the SPDX 2.3 emitter.
**Project Type**: Single-file-ish Rust change (`spdx/document.rs` extension; possibly `spdx/v3_licenses.rs` extension pending SPDX 3 investigation).
**Performance Goals**: No measurable perf impact. The sweep runs once per SPDX 2.3 document emission (O(packages × avg-license-field-length) with a compiled regex); for a typical scan that's ~30 packages × ~50-char fields = ~1500 char-positions scanned. Regex compile is amortized via `std::sync::OnceLock` (same pattern used by every other regex in the workspace).
**Constraints**: SC-002 byte-identity for happy-path scans (verified via existing golden tests). SC-007 no wire-format changes beyond the intended `hasExtractedLicensingInfos[]` array.
**Scale/Scope**: ~100–150 LOC in `spdx/document.rs`, ~50 LOC in `spdx/v3_licenses.rs` (conditional on SPDX 3 investigation), ~10 unit tests, 1 CHANGELOG entry.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.5.0 evaluation:

| Principle | Applicability | Status | Notes |
|-----------|---------------|--------|-------|
| I. Pure Rust, Zero C | APPLIES | PASS | All new code is Rust. |
| II. eBPF-Only Observation | N/A | PASS | Not a discovery path. |
| III. Fail Closed | N/A | PASS | The sweep is additive: if it finds no LicenseRef- values, the emitted array is absent (per FR-006); if it finds any, the corresponding entries are always added. |
| IV. Type-Driven Correctness | APPLIES | PASS | All new helpers use `&str` / `Vec<SpdxExtractedLicensingInfo>` / no `.unwrap()` in production. |
| **V. Specification Compliance** | **APPLIES** | **PASS + REINFORCED** | This milestone EXISTS to close a §10.1 conformance gap. SPDX 2.3 `hasExtractedLicensingInfos` is the standards-native carrier for defining `LicenseRef-*` identifiers. **No new `mikebom:*` annotation key** (per FR-013). No catalog changes per SC-007. |
| VI. Three-Crate Architecture | APPLIES | PASS | All edits land in `mikebom-cli`; no new crates. |
| VII. Test Isolation | APPLIES | PASS | New unit tests are unprivileged; run via `cargo test --workspace`. |
| VIII. Completeness | APPLIES | PASS | The sweep improves completeness by defining previously-undefined identifiers. |
| **IX. Accuracy** | **APPLIES** | **PASS** | The placeholder text explicitly discloses that mikebom did not extract the real text, letting consumers act appropriately rather than assume authoritative content. Matches Principle IX's "flag ambiguous or low-confidence [signal] rather than silently include as definitive." |
| **X. Transparency** | **APPLIES** | **PASS** | The uniform placeholder text is a machine-parseable signal (per Clarifications Q1 wire contract) that consumers can pattern-match on to distinguish mikebom-placeholder entries from entries with real extracted text. |
| XI. Enrichment | N/A | PASS | Not an enrichment path. |
| XII. External Data Source Enrichment | N/A | PASS | No external sources. |
| Strict Boundary 5 | N/A | PASS | File-tier emission untouched. |

**Gate Outcome**: PASS. No violations. No complexity-tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/153-spdx-license-refs-conformance/
├── plan.md                       # This file
├── research.md                   # Phase 0 — infra survey + regex grammar + dedup rule + SPDX 3 investigation
├── data-model.md                 # Phase 1 — sweep function signature + placeholder const + dedup logic
├── quickstart.md                 # Phase 1 — issue-#485 testbed verification + happy-path regression check
├── contracts/
│   └── sweep-api.md              # Phase 1 — the new helper's contract + integration site diff
├── checklists/
│   └── requirements.md           # Spec validation checklist (from /speckit-specify)
└── tasks.md                      # Phase 2 — generated by /speckit-tasks (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/generate/spdx/
├── document.rs                   # +~100 LOC: new `sweep_extracted_license_refs` helper
                                  # + wiring at line ~352 (after build_packages) + the
                                  # PLACEHOLDER_EXTRACTED_TEXT const + ~6 new unit tests.
                                  # SpdxExtractedLicensingInfo struct + serde UNCHANGED.
└── v3_licenses.rs                # +~50 LOC IFF SPDX 3 investigation concludes it needs an
                                  # equivalent `licensing_CustomLicense` emission path. Same
                                  # sweep pattern, different graph-element shape.

CHANGELOG.md                      # +~30 LOC: new entry under [Unreleased] describing the
                                  # §10.1 conformance fix + the locked placeholder text +
                                  # SPDX 3 investigation outcome (either "applied" or "no-op").
```

**Structure Decision**: Single-file-primary Rust edit (`document.rs`) + conditional `v3_licenses.rs` extension + CHANGELOG. The sweep helper is co-located with the existing `SpdxExtractedLicensingInfo` struct definition — no new module. Test coverage lives in the same file (matching the milestone-152 + milestone-478 convention of inline test modules).

## Constitution Check — POST-DESIGN re-evaluation

Phase 0 (research.md) + Phase 1 (data-model.md, contracts/sweep-api.md, quickstart.md) produced no surprises:

- The new sweep is a pure-function walk over the already-assembled `Vec<SpdxPackage>`. No I/O, no side effects, no async. Type-safe per Principle IV.
- The regex `LicenseRef-[a-zA-Z0-9.-]+` is compiled once via `OnceLock` (standard workspace pattern; matches existing usages in `scan_fs/package_db/` regexes). Pure ASCII grammar per SPDX 2.3 §10.1.
- The dedup rule (research.md §R4) uses a `BTreeMap<String, SpdxExtractedLicensingInfo>` keyed by `license_id`, seeded with the milestone-012 entries FIRST — the sweep only inserts entries for LicenseRef-ids the pre-existing path didn't cover. This guarantees milestone-012 entries with real text win over placeholder entries (FR-005).
- SPDX 3 investigation (research.md §R5 / §R6): the empirical `spdx3-validate` run determines whether `licensing_CustomLicense` elements are required. Preliminary reading of the SPDX 3.0.1 spec + the JPEWdev validator behavior suggests they ARE required for standard-conformance; the plan reserves ~50 LOC in `v3_licenses.rs` for this case AND documents the fallback if the validator disagrees.
- No new Cargo dependencies. No subprocess calls. No new I/O. No new emission shapes beyond the intended array.
- Per Principle V: the `hasExtractedLicensingInfos[]` construct is SPDX 2.3-spec-blessed; the milestone reuses existing struct + serde config verbatim. The catalog (`docs/reference/sbom-format-mapping.md`) is untouched.

**Post-design gate outcome**: PASS. No new violations.

## Complexity Tracking

No constitution-gate violations to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _(none)_  | _(none)_   | _(none)_                            |
