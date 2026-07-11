# Implementation Plan: ipk reader bug fixes (filename fallback + license extraction)

**Branch**: `185-ipk-reader-fixes` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/185-ipk-reader-fixes/spec.md`

## Summary

Two surgical correctness-bug fixes closing issues #538 and #539, both surfaced by the yocto-test testbed's `003-ipk-package-format` rerun against alpha.58:

- **US1 (#538) — `parse_ipk_filename` at `ipk_file.rs:609`**: switch from left-to-right `split('_')` with a strict `len() != 3` guard to right-to-left `rsplitn(3, '_')`. Accepts version fields containing embedded `_` (BitBake `SRCPV` shape for git-sourced upstream builds). Zero behavior change on canonical 2-underscore filenames.
- **US2 (#539) — opkg reader at `opkg.rs:289`**: wire `stanza.license()` (already exposed at `control_file.rs:71`) through a normalization pipeline that reuses two rpm reader helpers (`normalize_bitbake_license_operators`, `preserve_known_operands_with_license_ref` — currently private in `rpm_file.rs`, promoted to `pub(crate)` in m185). Adds an m185-specific 4th-pass wholesale-wrap fallback (per the Q1 clarification: when both first-pass strict SPDX parse AND second-pass LicenseRef-wrap-of-operands fail, wrap the WHOLE original string as a single `LicenseRef-<sanitized>` operand).

**Rpm reader stays byte-identical** — rpm_file.rs's 3-pass pipeline is unchanged; only visibility of two helper functions changes (`fn` → `pub(crate) fn`). The 4th-pass wholesale-wrap is opkg-only, not applied to rpm's call site. Preserves the rpm-side non-modification invariant (research.md Decision 3; verified by SC-005/SC-006 which gate rpm-labeled goldens as non-Yocto-ecosystem).

Zero new production Cargo dependencies. Both fixes are on the pure-Rust user-space path.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–184; no nightly required for this user-space-only bug-fix work).

**Primary Dependencies**: Existing only — `spdx` crate (already used by `rpm_file.rs` for `SpdxExpression::try_canonical`), `serde`/`serde_json` (annotation values), `tracing` (info/warn logs), `anyhow`. Reuses the existing rpm-side license normalization helpers verbatim. **Zero new Cargo dependencies.**

**Storage**: N/A — all state in-process per scan; persisted only inside the emitted SBOM via the existing `licenses[]` array and `extra_annotations` channel. Matches every milestone since 002.

**Testing**: `cargo +stable test --workspace` (unit + integration tests), `cargo +stable clippy --workspace --all-targets -- -D warnings` (lint). Golden regen via `MIKEBOM_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`.

**Target Platform**: Linux + macOS user-space (unchanged from prior milestones).

**Project Type**: CLI (Rust binary + shared common crate). Existing three-crate architecture: `mikebom-cli`, `mikebom-common`, `xtask`.

**Performance Goals**: `rsplitn(3, '_')` is O(N) in filename length; the normalization pipeline is O(N) in license-string length. Negligible per-scan cost — an opkg installed-database scan touches thousands of stanzas but each License extraction is microsecond-range.

**Constraints**: SC-005/SC-006 byte-identity guards for non-Yocto goldens (rpm-labeled goldens included in this scope). Research Decision 3 rpm-side non-modification invariant — the rpm_file.rs helpers change visibility only; behavior on the rpm call site is untouched. FR-013 preserves the `mikebom:source-mechanism = "ipk-file-filename-fallback"` annotation byte-identically. (Note: FR-011 is about m107 opkg regression tests continuing to pass, NOT about rpm-side byte-identity — corrected per /speckit-analyze I1.)

**Scale/Scope**: 2 user stories, 2 code sites, 2 rpm-helper visibility changes, 1 new m185 wholesale-wrap fallback. Estimated ~22-24 tasks across 5 phases.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Principle I (Pure Rust, Zero C)** — PASS. Zero new Cargo dependencies. `rsplitn` is stdlib; the normalization pipeline is existing pure-Rust code from rpm_file.rs.

**Principle II (eBPF-Only Observation)** — N/A. m185 is user-space bug-fix work; `mikebom-ebpf` untouched.

**Principle III (Fail Closed)** — PASS. FR-003 preserves the fail-safe None-return for malformed filenames. FR-007 preserves the licenses-empty fallback for absent-License stanzas. The new m185 wholesale-wrap is a MORE-preserving fallback — it emits data rather than dropping it — but does not weaken any pre-existing validation (unparseable inputs still fail the strict-SPDX first pass; they now recover via the 4th-pass wholesale-wrap instead of dropping to NOASSERTION).

**Principle IV (Type-Driven Correctness)** — PASS. The `parse_ipk_filename` fix keeps the same `Option<(String, String, String)>` return type. The opkg licenses wiring keeps the same `Vec<SpdxExpression>` field type.

**Principle V (Specification Compliance + Native-first)** — PASS. `licenses[]` and `licenseDeclared` are both native SPDX / CDX constructs — no `mikebom:*` annotation invention. The `LicenseRef-<sanitized>` shape is spec-blessed per SPDX 2.3 §10.

**Principle VI (Three-Crate Architecture)** — PASS. Changes confined to `mikebom-cli/src/scan_fs/package_db/`. Zero changes to `mikebom-common`, `mikebom-ebpf`, or `xtask`.

**Principle VII (Test Isolation)** — PASS. New unit tests colocated with the code under test in `ipk_file.rs` (parser) and `opkg.rs` (license extraction). Cross-reader helper tests (rpm's `preserve_known_operands_with_license_ref` etc.) already exist in `rpm_file.rs::tests`; m185 does not modify them.

**Principle VIII (Completeness)** — PASS. m185 closes both #538 and #539 correctness gaps entirely. The 5 non-kernel "other affected packages" from #538 are explicitly deferred (distinct root cause; deferred until reproducible standalone repro is available).

**Principle IX (Accuracy)** — PASS. US1 corrects wholesale-misclassification (null-PURL) for 4 kernel modules per stock Yocto image. US2 corrects wholesale-absence (`licenses: []`) for every opkg-installed component (4586 in `core-image-minimal`).

**Principle X (Transparency)** — PASS. FR-013 preserves the `mikebom:source-mechanism = "ipk-file-filename-fallback"` provenance annotation. The m185 wholesale-wrap fallback preserves the ORIGINAL raw string (via `LicenseRef-<sanitized>`) so operators can recover the pre-normalization data.

**Principle XI (Enrichment)** — N/A. No external-data enrichment for m185.

**Principle XII (External Data Source Enrichment)** — N/A. Same as XI.

**Result**: All 12 principles PASS. No violations to justify. No Complexity Tracking table needed.

## Project Structure

### Documentation (this feature)

```text
specs/185-ipk-reader-fixes/
├── plan.md                    # This file
├── research.md                # Phase 0 output (5 decisions)
├── data-model.md              # Phase 1 output (parser semantic + opkg license wiring)
├── quickstart.md              # Phase 1 output (operator + developer worked examples)
├── contracts/
│   ├── parser-decision-matrix.md   # US1 canonical input × output table
│   └── license-pipeline.md          # US2 4-pass normalization contract
├── checklists/
│   └── requirements.md        # 16/16 PASS from /speckit-specify + Q1 clarification
├── spec.md                    # Feature specification (with ## Clarifications section)
└── tasks.md                   # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   └── scan_fs/
│       └── package_db/
│           ├── ipk_file.rs         # US1 — parse_ipk_filename rsplitn(3, '_') swap + 3-5 new unit tests
│           ├── opkg.rs             # US2 — stanza.license() wire-up at build_entry + normalization + wholesale-wrap + 4-6 new unit tests
│           ├── rpm_file.rs         # US2 support — 2 helper visibilities: fn → pub(crate) fn (zero behavior change)
│           └── control_file.rs     # UNCHANGED — stanza.license() accessor already exists at line 71
└── tests/
    └── fixtures/
        └── golden/
            ├── cyclonedx/apk.cdx.json       # UNCHANGED (non-Yocto)
            ├── cyclonedx/deb.cdx.json       # UNCHANGED (non-Yocto)
            ├── cyclonedx/rpm.cdx.json       # UNCHANGED per FR-011 (rpm behavior invariant)
            ├── spdx-2.3/rpm.spdx.json       # UNCHANGED per FR-011
            └── spdx-3/rpm.spdx3.json        # UNCHANGED per FR-011
```

**Structure Decision**: Three-file scope inside `mikebom-cli/src/scan_fs/package_db/`. No new files needed. `rpm_file.rs` gets 2 visibility bumps but ZERO behavior change (its call site continues to use the 3-pass pipeline). `opkg.rs` gets the license-wiring at one call site (line 289) plus the 4th-pass wholesale-wrap logic. `ipk_file.rs` gets the parser swap at one function (line 609). Follows the m179+ minimal-touch precedent — no shared-helper file introduced, no cross-crate coordination.

## Complexity Tracking

*No violations to justify — all 12 constitution principles PASS.*
