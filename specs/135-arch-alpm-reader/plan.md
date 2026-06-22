# Implementation Plan: Arch Linux pacman/alpm package database reader

**Branch**: `135-arch-alpm-reader` | **Date**: 2026-06-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/135-arch-alpm-reader/spec.md`

## Summary

Add a fifth OS package-DB reader to mikebom alongside the existing dpkg, apk, rpm, and opkg readers. Parses pacman's `/var/lib/pacman/local/<pkg>-<ver>/{desc,files}` per-package directories into `PackageDbEntry` instances, emits `pkg:alpm/<distro>/<name>@<version>?arch=<arch>` PURLs per the purl-spec `alpm` type, threads the package's owned-file paths into the cross-reader file-claim tracker (milestone 004) so the binary walker de-duplicates pacman-owned binaries, and integrates into the existing `read_all` dispatcher. Distro identity comes from `/etc/os-release` via the existing `os_release` reader; rolling-release Arch (no `VERSION_ID`) gets an unqualified PURL, derivatives get the `distro=<namespace>-<version>` qualifier mirroring the dpkg/apk/rpm convention.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–134; no nightly required for this user-space-only work).

**Primary Dependencies**: Existing only — `std::fs` (directory walk over `local/`), `tracing` (warn-and-skip on malformed per-package directories), `anyhow`/`thiserror` (error propagation), `mikebom_common::types::purl::Purl` (PURL construction + validation), the existing `mikebom-cli/src/scan_fs/os_release.rs` reader (distro detection). **No new Cargo dependencies.** The pacman `desc` and `files` formats are plain text — stdlib line iteration plus a small stanza-style state machine covers parsing.

**Storage**: N/A — all state is in-process for the duration of a single scan; mirrors every OS-package reader since milestone 002.

**Testing**: `cargo +stable test --workspace`. Synthetic-fixture pattern via `tempfile::tempdir()` constructing minimal `/var/lib/pacman/local/<pkg>-<ver>/` directory trees with hand-crafted `desc` + `files` content. New integration test files at `mikebom-cli/tests/alpm_*.rs` mirroring the existing `dpkg_*.rs` / `apk_*.rs` test families. SC-003 byte-identity preservation guarded by the existing 11-ecosystem golden suite (no pacman DB present → those goldens stay unchanged).

**Target Platform**: Cross-platform read-only filesystem access — the pacman DB is plain text and trivially parseable on Linux / macOS / Windows host build environments (the scan target itself is typically a Linux rootfs or container image extraction, but the reader's host portability matches the existing dpkg/apk/rpm reader posture).

**Project Type**: CLI tool — extends the `mikebom sbom scan` pipeline via the `read_all` dispatcher.

**Performance Goals**: ≤5 ms overhead per package on the read path; ≤100 ms for a stock Arch image with ~250 installed packages. The no-pacman-DB fast path (early-return when `/var/lib/pacman/local/` doesn't exist) MUST add ≤1 ms to every non-Arch scan.

**Constraints**:
- Byte-identical SBOM goldens when no pacman DB present (SC-003).
- Zero new Cargo deps (matches the milestone-002/004/107 OS-reader posture).
- Per-package parse failures MUST be warn-and-skip, not fail-the-scan (FR-009).
- File-claim tracker integration MUST NOT regress existing dpkg/apk/rpm claim handling.

**Scale/Scope**: Stock Arch image: ~250 packages. Heavy desktop install: ~3000 packages. Reader walks `local/` exactly once per scan; per-package directory open is O(1).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Verdict | Justification |
|---|---|---|
| I. Pure Rust, Zero C | ✓ | All new code is user-space Rust; no FFI, no C. |
| II. eBPF-Only Observation | N/A | This reader processes installed-package metadata for components ALREADY present on the scanned rootfs; no new dependency-discovery surface. Matches every other OS-package reader (dpkg/apk/rpm/opkg). |
| III. Fail Closed | ✓ | A missing or empty pacman DB is a clean no-op (FR-008), NOT a fail-closed condition — pacman absence simply means "no Arch packages here", same posture as dpkg-absence on an Alpine scan. Per-package parse failures warn-and-skip (FR-009) preserving partial output. |
| IV. Type-Driven Correctness | ✓ | Uses the existing `Purl` newtype for PURL construction; no stringly-typed identifiers. Reader-local helper structs (e.g., `PacmanDescStanza`) carry typed fields. Production code MUST NOT call `.unwrap()` — error propagation via `anyhow::Result`. |
| V. Specification Compliance | ✓ | **Audit completed in Phase 0 research R1**: the purl-spec defines a native `alpm` type for Arch Linux Pacman packages with explicit handling of distro namespace + `arch` qualifier. NO `mikebom:*` annotation is introduced — the PURL itself is the native identity carrier. The `distro=` qualifier reuses the exact convention dpkg/apk/rpm already established. |
| VI. Three-Crate Architecture | ✓ | All new code lives in `mikebom-cli`. No new workspace crate. Reader is a peer of `dpkg.rs` / `apk.rs` / `rpm.rs` / `opkg.rs` under `mikebom-cli/src/scan_fs/package_db/`. |
| VII. Test Isolation | ✓ | Synthetic tempfile fixtures only; no host-state dependency, no Linux-only requirements (parser is byte-level over plain-text formats). |
| VIII. Completeness | ✓ | This feature IS a completeness improvement — eliminates the false-negative gap where every pacman-installed package was invisible to the scan. The orphan-fallback file-tier emission (milestone 133) continues to fire for anything pacman doesn't claim. |
| IX. Accuracy | ✓ | PURL identity comes directly from the on-disk pacman DB; no heuristic guesses. File-claim tracker integration (US3) prevents the false-positive `pkg:generic/bash` duplicate that would otherwise appear alongside `pkg:alpm/arch/bash`. |
| X. Transparency | ✓ | Per-package parse failures (FR-009) emit `tracing::warn!` with the affected package name; downstream consumers can correlate the missing component against the warn log. No silent drops. |
| XII. External Data Source Enrichment | ✓ | The pacman DB IS the discovery source — same posture as dpkg/apk/rpm. No external enrichment in this feature. |

**Verdict: PASS.** No violations, no justifications required.

## Project Structure

### Documentation (this feature)

```text
specs/135-arch-alpm-reader/
├── plan.md              # This file
├── spec.md              # Feature spec (already written)
├── research.md          # Phase 0 output — Principle V audit + design decisions
├── data-model.md        # Phase 1 output — PacmanInstalledPackage + DistroIdentity types
├── quickstart.md        # Phase 1 output — operator-facing walkthrough
├── contracts/           # Phase 1 output — wire-format contracts
│   └── alpm-component-purl.md
├── checklists/
│   └── requirements.md  # Spec-quality checklist (already written)
└── tasks.md             # Phase 2 output (via /speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/
├── scan_fs/
│   ├── os_release.rs                  # USE: distro detection (existing — milestone 002)
│   ├── package_db/
│   │   ├── mod.rs                     # MODIFY: register alpm in read_all dispatcher
│   │   ├── alpm.rs                    # NEW: pacman desc/files reader + claim tracker
│   │   ├── dpkg.rs                    # REFERENCE: closest-shape sibling reader
│   │   ├── apk.rs                     # REFERENCE: lighter-weight sibling reader
│   │   └── rpm.rs                     # REFERENCE: file-claim integration model
│   └── binary/                        # USE: file-claim tracker consumers (unchanged)
└── (no changes to scan_fs/mod.rs, generate/, parity/)

mikebom-cli/tests/
├── alpm_arch_baseline.rs              # NEW: US1 — stock Arch fixture
├── alpm_derivative_distros.rs         # NEW: US2 — SteamOS, Manjaro, EndeavourOS, CachyOS
├── alpm_file_claim_dedupe.rs          # NEW: US3 — binary-walker dedup invariant
└── alpm_edge_cases.rs                 # NEW: malformed desc, empty DB, noarch packages,
                                       # group filtering, multi-version coexistence

docs/reference/
└── (no changes — alpm rides the native `pkg:alpm/*` purl identity;
   no new mikebom:* annotation = no new C-row in the parity catalog)
```

**Structure Decision**: Extends the existing `mikebom-cli/src/scan_fs/package_db/` reader family. New file `alpm.rs` is a peer of `dpkg.rs`/`apk.rs`/`rpm.rs`/`opkg.rs`. Integration site is the existing `read_all` dispatcher in `mikebom-cli/src/scan_fs/package_db/mod.rs:1169` (alongside the existing `dpkg::read(...)` / `apk::read(...)` / `rpm::read(...)` invocations). Test files follow the existing `<reader>_<scenario>.rs` integration-test naming convention. **No new workspace crate per Principle VI; no new Cargo deps; no new annotation in the parity catalog (because the PURL itself is the native identity per Principle V).**

## Complexity Tracking

> No Constitution Check violations — no justifications required.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none)    | n/a        | n/a                                  |
