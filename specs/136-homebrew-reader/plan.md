# Implementation Plan: Homebrew (brew + Linuxbrew) package detection

**Branch**: `136-homebrew-reader` | **Date**: 2026-06-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/136-homebrew-reader/spec.md`

## Summary

Add a sixth OS package-DB reader to mikebom alongside dpkg, apk, rpm, opkg, and alpm (milestone 135). Parses Homebrew's `<prefix>/Cellar/<formula>/<version>/INSTALL_RECEIPT.json` files for formulae and (Homebrew 4.0+) `<prefix>/Caskroom/<cask>/<version>/.metadata/<version>/<timestamp>/Casks/<token>.json` files for casks. Emits `pkg:brew/<name>@<version>[?tap=<owner>/<tap>][&type=cask]` PURLs per industry convention (the purl-spec doesn't yet define a `brew` type — informal extension, parallels milestone-128's `pkg:yocto`). Three install-prefix locations detected independently: `/opt/homebrew` (Apple Silicon), `/usr/local` (Intel macOS), `/home/linuxbrew/.linuxbrew` (Linux). Integrates into the existing `read_all` dispatcher; no new Cargo dependencies; no `mikebom:*` annotations introduced.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–135; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `serde`/`serde_json` (workspace; receipt + cask JSON parsing), `tracing` (warn-and-skip per FR-007), `anyhow`/`thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation). **No new Cargo dependencies.** Ruby-DSL `.rb` casks (pre-Homebrew-4.0) are intentionally NOT parsed per Constitution Principle I (Pure Rust, Zero C — extends to "no embedded scripting parsers" by spirit); they warn-and-skip with operator-visible diagnostic.

**Storage**: N/A — all state is in-process for the duration of a single scan. Mirrors every OS-package reader since milestone 002.

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()` constructing minimal `Cellar/<formula>/<version>/INSTALL_RECEIPT.json` trees. Four new integration test files at `mikebom-cli/tests/brew_*.rs` mirroring the milestone-135 alpm test family. SC-004 byte-identity preservation guarded by the existing 11-ecosystem golden suite (no Homebrew install present → those goldens stay unchanged).

**Target Platform**: Cross-platform reader. The pacman/dpkg/alpm precedent applies — mikebom's host portability is independent of the scanned target's OS. A macOS host CAN scan a Linux rootfs; a Linux host CAN scan a macOS Homebrew install via `--path /Volumes/macOS-volume/` etc. The reader is pure-Rust JSON parsing.

**Project Type**: CLI tool — extends the `mikebom sbom scan` pipeline via the `read_all` dispatcher.

**Performance Goals**: ≤2 ms overhead per formula on the read path; ≤500 ms for a heavy developer install (300 formulae + 50 casks). The no-Homebrew-detected fast path (early-return when none of the three prefix `Cellar/` dirs exist) MUST add ≤3 µs per non-macOS scan (three `Path::exists()` calls).

**Constraints**:
- Byte-identical SBOM goldens when no Homebrew install present (SC-004).
- Zero new Cargo deps (matches the milestone-002/004/107/135 OS-reader posture).
- Per-formula JSON parse failures MUST warn-and-skip, not fail the scan (FR-007).
- The `pkg:brew/` PURL type is informal; downstream consumers should treat it as a non-spec-blessed extension (documented in spec Assumptions).
- File-claim tracker integration is OUT OF SCOPE per the spec's explicit deferral (Homebrew's symlink-heavy bottling warrants a separate spec).
- **License emission is OUT OF SCOPE** per the spec's explicit deferral (research §R2 — license is NOT in `INSTALL_RECEIPT.json`; surfacing would require reading the formula's `.rb` source via Ruby parser (Principle I conflict) or hitting the JSON API at `formulae.brew.sh` (FR-010 conflict)).

**Scale/Scope**: Stock developer install: ~50 formulae. Heavy install: ~300 formulae + ~50 casks. Per-formula JSON read + parse: ~200 µs warm-cache. Cask walk includes the `<version>/<timestamp>/Casks/<token>.json` nested-directory traversal (~500 µs per cask).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C, no embedded interpreter for `.rb` casks (we explicitly warn-and-skip Ruby-DSL casks rather than parse them — see research §R5). |
| II. eBPF-Only Observation | N/A | This reader processes installed-package metadata for components ALREADY present on the scanned rootfs; no new dependency-discovery surface. Matches every OS-package reader (dpkg/apk/rpm/opkg/alpm). |
| III. Fail Closed | ✓ | A missing or empty Homebrew install at any of the three prefixes is a clean no-op (FR-006), NOT a fail-closed condition. Per-formula parse failures warn-and-skip (FR-007). |
| IV. Type-Driven Correctness | ✓ | Uses the existing `Purl` newtype for PURL construction; no stringly-typed identifiers. Receipt-deserializing structs are typed via `serde` derives. Production code MUST NOT call `.unwrap()` — error propagation via `Result`. |
| V. Specification Compliance | ✓ | **Audit completed in Phase 0 research R1**: the purl-spec does NOT yet define a `brew` type. mikebom emits `pkg:brew/...` per industry convention (CycloneDX-bom-gen + syft both use this shape) pending a purl-spec extension proposal. Per Principle V's bullet on parity-bridging carriers: when no native field exists, `mikebom:*` properties MAY exist; here the question is the PURL TYPE itself, which IS the standards-native carrier — just with a not-yet-blessed type token. The carrier is correct; only the type-name registration is pending. A follow-up issue will propose the spec extension. Documented as a deferred concern in research §R1. |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli`. No new workspace crate. Reader is a peer of `dpkg.rs` / `apk.rs` / `rpm.rs` / `opkg.rs` / `alpm.rs` under `mikebom-cli/src/scan_fs/package_db/`. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency. The reader uses pure-Rust JSON parsing — runs on any host. |
| VIII. Completeness | ✓ | This feature IS a completeness improvement — eliminates the false-negative gap where every Homebrew-installed component was invisible to the scan. The orphan-fallback file-tier emission (milestone 133) continues to fire for anything brew doesn't claim (especially relevant since brew file-claim integration is deferred). |
| IX. Accuracy | ✓ | PURL identity comes directly from the on-disk INSTALL_RECEIPT.json (or Cask's `.json` metadata); no heuristic guesses. The receipt's `runtime_dependencies` array is the authoritative dep-graph source. **Dep-name extraction normalizes tap-qualified `full_name` to bare name** (see data-model §"dep-name extraction") so cross-component lookups in `scan_fs/mod.rs::name_to_purl` succeed for third-party-tap dependencies — closes the analysis-finding I1 risk class. |
| X. Transparency | ✓ | Per-formula parse failures (FR-007) emit `tracing::warn!` with the affected formula path. Ruby-DSL casks (pre-4.0) warn-and-skip with a diagnostic naming the cask. No silent drops. |
| XII. External Data Source Enrichment | ✓ | The receipt + cask metadata ARE the discovery sources — same posture as dpkg/apk/rpm/alpm. No external enrichment in this feature. |

**Verdict: PASS.** No violations, no justifications required.

## Project Structure

### Documentation (this feature)

```text
specs/136-homebrew-reader/
├── plan.md              # This file
├── spec.md              # Feature spec (already written; FR-011 deferred post-analysis)
├── research.md          # Phase 0 output — Principle V audit + INSTALL_RECEIPT.json schema + cask metadata format + design decisions
├── data-model.md        # Phase 1 output — InstallReceipt + CaskMetadata types + PackageDbEntry field mapping
├── quickstart.md        # Phase 1 output — operator-facing walkthrough
├── contracts/           # Phase 1 output — wire-format contracts
│   └── brew-component-purl.md
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already written)
└── tasks.md             # Phase 2 output (via /speckit.tasks)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── package_db/
│   │   ├── mod.rs                     # MODIFY: register brew in read_all dispatcher
│   │   │                              # (no claim_paths integration — out of scope per spec)
│   │   ├── brew.rs                    # NEW: prefix detection + receipt parsing +
│   │   │                              # cask JSON parsing + PURL construction +
│   │   │                              # tap-qualified-name normalization
│   │   ├── alpm.rs                    # REFERENCE: milestone-135 closest sibling
│   │   ├── dpkg.rs                    # REFERENCE: original OS-reader shape
│   │   └── rpm.rs                     # REFERENCE: JSON-DB reader pattern (rpmdb sqlite)
│   └── (no other scan_fs changes — brew is purely additive)
├── generate/cyclonedx/builder.rs       # MODIFY: extend mikebom:evidence-kind enum
│                                       # to include "brew-install-receipt" + "brew-cask-metadata"
│                                       # (mirrors the milestone-135 T002b analysis remediation)
└── (no changes to other generate/, parity/, common/)

mikebom-cli/tests/
├── brew_apple_silicon_baseline.rs     # NEW: US1 — /opt/homebrew/Cellar/ fixture
├── brew_alternate_prefixes.rs         # NEW: US2 — /usr/local/Cellar/ + linuxbrew prefix +
│                                       # cross-reader (Linuxbrew + dpkg) coexistence
├── brew_casks.rs                      # NEW: US3 — Caskroom/<cask>/<version>/.metadata/
└── brew_edge_cases.rs                 # NEW: malformed receipt, missing fields, multi-version
                                       # coexistence, third-party tap, .rb-only cask
                                       # warn-and-skip, /usr/local-without-Cellar non-match,
                                       # formula+cask same-name collision (analysis U4)

docs/reference/
└── (no changes — pkg:brew is the native-ish PURL carrier; no new mikebom:* annotation,
   no new C-row in the parity catalog. Spec Assumptions document the purl-spec extension
   follow-up.)
```

**Structure Decision**: Extends the existing `mikebom-cli/src/scan_fs/package_db/` reader family. New file `brew.rs` is a peer of the five existing OS-DB readers. Integration site is the existing `read_all` dispatcher in `mikebom-cli/src/scan_fs/package_db/mod.rs` — same pattern as the milestone-135 alpm dispatcher block. Test files follow the existing `<reader>_<scenario>.rs` integration-test naming convention established by milestone 135. **No new workspace crate per Principle VI; no new Cargo deps; no new annotation in the parity catalog.**

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
