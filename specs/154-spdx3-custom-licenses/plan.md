# Implementation Plan: SPDX 3 `simplelicensing_CustomLicense` for LicenseRef-* — milestone 154 (closes #487)

**Branch**: `154-spdx3-custom-licenses` | **Date**: 2026-07-02 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/154-spdx3-custom-licenses/spec.md`

## Summary

Close GitHub issue [#487](https://github.com/kusari-oss/mikebom/issues/487) — the paired SPDX 3 follow-up to milestone 153 — by extending the SPDX 3 emitter with a post-`build_license_elements_and_relationships` sweep that emits one `simplelicensing_CustomLicense` graph element per unique `LicenseRef-<idstring>` referenced across all emitted `simplelicensing_LicenseExpression` elements. Placeholder text reused byte-identically from milestone 153's `PLACEHOLDER_EXTRACTED_TEXT` const (promoted from module-private to `pub(crate)` visibility so the SPDX 3 emitter can import it — single source of truth for the wire contract).

The 3 issue-#487 reference LicenseRefs on the Yocto testbed (same set as issue #485 by coincidence — same underlying Yocto build): `LicenseRef-bzip2-1.0.4`, `LicenseRef-PD`, `LicenseRef-GPL-2.0-with-OpenSSL-exception`. Post-154 the emitted SPDX 3 graph carries a matching `simplelicensing_CustomLicense` element for each.

**IRI construction**: `{doc_iri}/licenseref/{idstring}` per Clarifications Q1 — readable path segment, no percent-encoding needed (idstring alphabet is a subset of RFC 3986 unreserved chars).

**Cross-format symmetry contract** (FR-009 / FR-010 / FR-011): the set of `LicenseRef-<idstring>` full-tokens defined by SPDX 2.3's `hasExtractedLicensingInfos[].licenseId` equals the set derivable from SPDX 3's `simplelicensing_CustomLicense.name` fields (with LicenseRef- prefix reapplied); placeholder text and name fields are byte-identical across formats.

Net new code surface: ~90 LOC in `mikebom-cli/src/generate/spdx/v3_licenses.rs` (1 new helper + regex helper + ~5 unit tests) + 1-line visibility change in `mikebom-cli/src/generate/spdx/document.rs` (`const` → `pub(crate) const`) + ~3 lines wiring in `mikebom-cli/src/generate/spdx/v3_document.rs` + 1 CHANGELOG entry.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–153; no nightly required).
**Primary Dependencies**: Existing only — `regex = "1"` (workspace; already direct dep since milestone 013; here used identically to milestone 153's sweep), `serde_json` (used by v3_licenses.rs for `json!` macro), `std::sync::OnceLock` (regex compile), `std::collections::BTreeMap` (dedup by idstring). **No new Cargo dependencies.**
**Storage**: N/A — pure-function sweep over `&[Value]` (the already-emitted `simplelicensing_LicenseExpression` elements). No caches, no persistence.
**Testing**: New unit tests inline in `mikebom-cli/src/generate/spdx/v3_licenses.rs`'s existing test module (if present) or an adjacent module. Plus reuse of the existing milestone-078 `spdx3-validate` harness at `mikebom-cli/tests/spdx3_conformance.rs` to verify SC-003 (no validator regression).
**Target Platform**: Cross-platform Rust binary. Same surface as the rest of the SPDX 3 emitter.
**Project Type**: Single-file-primary Rust change (`v3_licenses.rs` extension) + 1-line visibility change in `document.rs` + ~3-line wiring in `v3_document.rs` + CHANGELOG.
**Performance Goals**: No measurable perf impact. The sweep runs once per SPDX 3 document emission (O(license_expression_elements × avg-expression-length) with a compiled regex); for a typical scan that's ≤10 elements × ~50-char expressions = ~500 char-positions scanned.
**Constraints**: SC-002 byte-identity for happy-path scans (verified via existing SPDX 3 golden tests). SC-004 cross-format placeholder identity (byte-identical to milestone 153).
**Scale/Scope**: ~90 LOC in `v3_licenses.rs`, 1-line const-visibility change in `document.rs`, ~3-line wiring in `v3_document.rs`, ~5 unit tests, 1 CHANGELOG entry.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.5.0 evaluation:

| Principle | Applicability | Status | Notes |
|-----------|---------------|--------|-------|
| I. Pure Rust, Zero C | APPLIES | PASS | All new code is Rust. |
| II. eBPF-Only Observation | N/A | PASS | Not a discovery path. |
| III. Fail Closed | N/A | PASS | Additive: no LicenseRef- references → no `simplelicensing_CustomLicense` elements emitted. |
| IV. Type-Driven Correctness | APPLIES | PASS | All new helpers use `&[Value]` / `Vec<Value>` / `&str` / no `.unwrap()` in production. |
| **V. Specification Compliance** | **APPLIES** | **PASS + REINFORCED** | Closes an SPDX-3-side symmetry gap using the standards-native `simplelicensing_CustomLicense` graph element. **No new `mikebom:*` annotation key** (per FR-016). No catalog changes per SC-007. |
| VI. Three-Crate Architecture | APPLIES | PASS | All edits land in `mikebom-cli`; no new crates. |
| VII. Test Isolation | APPLIES | PASS | New unit tests are unprivileged; run via `cargo test --workspace`. |
| VIII. Completeness | APPLIES | PASS | Improves completeness by defining previously-undefined identifiers. |
| **IX. Accuracy** | **APPLIES** | **PASS** | The placeholder text explicitly discloses that mikebom did not extract the real text — same guarantee as milestone 153. |
| **X. Transparency** | **APPLIES** | **PASS** | Cross-format placeholder identity (byte-locked from milestone 153) is a machine-parseable signal consumers can pattern-match on across both SPDX formats. |
| XI. Enrichment | N/A | PASS | Not an enrichment path. |
| XII. External Data Source Enrichment | N/A | PASS | No external sources. |
| Strict Boundary 5 | N/A | PASS | File-tier emission untouched. |

**Gate Outcome**: PASS. No violations. No complexity-tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/154-spdx3-custom-licenses/
├── plan.md                       # This file
├── research.md                   # Phase 0 — SPDX 3 emitter survey + IRI scheme + placeholder const sharing + wiring site
├── data-model.md                 # Phase 1 — sweep function signature + element shape + IRI construction + dedup rule
├── quickstart.md                 # Phase 1 — issue-#487 testbed cross-format symmetry + happy-path regression
├── contracts/
│   └── sweep-api.md              # Phase 1 — new helper's contract + integration site + cross-format invariants
├── checklists/
│   └── requirements.md           # Spec validation checklist (from /speckit-specify)
└── tasks.md                      # Phase 2 — generated by /speckit-tasks (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/generate/spdx/
├── document.rs                   # 1-line change: `const PLACEHOLDER_EXTRACTED_TEXT` →
                                  # `pub(crate) const PLACEHOLDER_EXTRACTED_TEXT`. Const
                                  # value BYTE-IDENTICAL — do not change. Single source of
                                  # truth for the cross-format wire contract.
├── v3_licenses.rs                # +~90 LOC: new `sweep_custom_licenses` helper +
                                  # `license_ref_regex()` (either duplicate milestone-153's
                                  # pattern or promote to shared per implementation
                                  # judgment) + ~5 new unit tests. Existing
                                  # `build_license_elements_and_relationships` UNCHANGED.
└── v3_document.rs                # +~3 LOC: invoke `sweep_custom_licenses` after step 6
                                  # (line ~587) and push returned elements to `graph`
                                  # before step 7.

CHANGELOG.md                      # +~30 LOC: new entry under [Unreleased] documenting
                                  # the SPDX 3 symmetry fix + issue #487 reference + the
                                  # byte-identical cross-format placeholder guarantee.
```

**Structure Decision**: Primary Rust edit in `v3_licenses.rs` + 1-line visibility change in `document.rs` + ~3-line wiring in `v3_document.rs`. Mirrors milestone 153's shape exactly: sweep helper co-located with the format-specific emitter, invoked post-assembly, dedup-by-key, deterministic output ordering. Placeholder const shared via `pub(crate)` visibility promotion (single source of truth).

## Constitution Check — POST-DESIGN re-evaluation

Phase 0 (research.md) + Phase 1 (data-model.md, contracts/sweep-api.md, quickstart.md) produced no surprises:

- The new sweep is a pure-function walk over the already-emitted `simplelicensing_LicenseExpression` graph elements. No I/O, no side effects, no async. Type-safe per Principle IV.
- The regex `(?:^|[^:])(LicenseRef-[a-zA-Z0-9.-]+)` is compiled once via `OnceLock` — same pattern + implementation strategy as milestone 153 (research.md §R3 documents the decision on whether to duplicate inline vs share).
- The dedup rule (research.md §R2) uses a `BTreeMap<String, Value>` keyed by the LicenseRef idstring (dedup by idstring produces the natural set). Sorted-by-spdxId output preserves determinism.
- The IRI construction `{doc_iri}/licenseref/{idstring}` (per Clarifications Q1) is unambiguous: idstring alphabet subset of RFC 3986 unreserved characters. No percent-encoding.
- Placeholder const sharing (research.md §R4): `pub(crate)` visibility promotion in `document.rs`. v3_licenses.rs imports as `super::document::PLACEHOLDER_EXTRACTED_TEXT`. Single source of truth; no risk of byte-drift.
- No new Cargo dependencies. No subprocess calls. No new I/O.
- Per Principle V: `simplelicensing_CustomLicense` is the SPDX 3.0.1-spec-blessed element; the catalog (`docs/reference/sbom-format-mapping.md`) is untouched.

**Post-design gate outcome**: PASS. No new violations.

## Complexity Tracking

No constitution-gate violations to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _(none)_  | _(none)_   | _(none)_                            |
