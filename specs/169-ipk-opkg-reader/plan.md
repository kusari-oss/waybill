# Implementation Plan: ipk/opkg package-database reader

**Branch**: `169-ipk-opkg-reader` | **Date**: 2026-07-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/169-ipk-opkg-reader/spec.md` (closes #500)

## Summary

Close the ipk/opkg ecosystem coverage gap surfaced by issue #500 (Yocto build outputs → 0 components on `.ipk` files). Two code paths, both landing in this milestone:

1. **US1 archive-file reader** (`.ipk` build outputs at `tmp/deploy/ipk/*.ipk`) — NEW module `mikebom-cli/src/scan_fs/package_db/ipk_file.rs`. Analogous to milestone-004 `rpm_file.rs`. Parses ar-archive → `control.tar.gz` → control file → emits `pkg:opkg/<name>@<version>?arch=<arch>` components. First ar-archive parser in the codebase.
2. **US2 installed-DB reader** (`/var/lib/opkg/status`) — **ALREADY EXISTS** at `mikebom-cli/src/scan_fs/package_db/opkg.rs:52` (landed in milestone 107). Small hardening delta only: (a) FR-015 evidence-kind emission (`opkg-status-db` currently emits `None` at opkg.rs:203); (b) FR-014 `/var/lib/opkg/info/*.control` fallback when `status` is absent (m107 currently early-returns empty); (c) FR-016 dedup precedence when archive-file + installed-DB both fire in the same scan.

**Scope revision**: the Q1 clarification (add installed-DB coverage) is largely a no-op because m107 landed most of it. This milestone's real weight is on US1 (archive-file reader). Total scope: ~20-25 tasks (down from the ~25-30 the Q1-expanded spec anticipated).

Plus supporting changes: file-tier walker allowlist (`.ipk`) per FR-001; shared helpers (control-file parser already at `control_file.rs:108` per m107 — no new sharing work); binary-walker skip-set integration (opkg already at `opkg::collect_claimed_paths`; archive-file skip-set is new via `ipk_file::collect_claimed_paths`).

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–168; no nightly required for this user-space-only work).
**Primary Dependencies**: Existing only, with one contested choice:
  - `flate2` + `tar` (workspace; already used pervasively for `.tar.gz` extraction across rpm/deb-like paths). Reused for `control.tar.gz` + `data.tar.gz` extraction from ipk archives.
  - `mikebom_common::types::purl::{Purl, encode_purl_segment}` for `pkg:opkg/` PURL construction (m107 pattern at `opkg.rs:34` — reused).
  - `parse_stanzas` at `mikebom-cli/src/scan_fs/package_db/control_file.rs:108` — shared RFC-822 parser (m107 refactor). Reused verbatim by ipk_file.rs.
  - `mikebom_common::types::license::SpdxExpression::try_canonical` for License field routing (m152/153/154 pipeline).
  - **NEW decision point**: ar-archive parsing. mikebom has no existing ar reader. Two options: (a) add `ar = "0.9"` as a workspace dep; (b) hand-roll (ar format is trivial — 8-byte magic `!<arch>\n` + repeating 60-byte per-entry headers with fixed offsets). Research §R2 resolves this — decision is **hand-roll**, keeping the zero-new-deps Constitution posture.
**Storage**: N/A — all state in-process per scan; matches every milestone since 002.
**Testing**: 12+ unit tests per SC-009 (7 archive-file cases + 4 installed-DB deltas + 1 shared distro-qualifier + 1 alternative-list Q2 case), 1 integration test at `mikebom-cli/tests/ipk_reader.rs` per SC-010, plus vendored real-world `.ipk` fixtures at `mikebom-cli/tests/fixtures/ipk-files/` (3-5 packages, ~10-100 KB each) + synthetic installed-DB tree at `mikebom-cli/tests/fixtures/opkg-installed-db/`.
**Target Platform**: All mikebom-supported hosts.
**Project Type**: Rust CLI (mikebom-cli) — single-crate scope. 1 new module (`ipk_file.rs`) + 1 edited module (`opkg.rs` gap fixes) + 1 file-tier walker allowlist edit + 1 dispatcher wire-up in `package_db/mod.rs::read_all`.
**Performance Goals**: Sub-second overhead on scans that don't touch ipk/opkg (no-op path). On the 4587-file Yocto reproduction (SC-001), scan wall-clock SHOULD complete under 30s (matches m069 rpm_file precedent for a similar scale).
**Constraints**: Constitution Principle I (Pure Rust, Zero C) — reader uses existing `flate2` + `tar` workspace deps only (research §R2 revised: OpenWrt `.ipk` is gzipped-tarball, not ar; no hand-roll + no new crates needed). Principle IV (no `.unwrap()` in production). Principle V (standards-native PURL — `pkg:opkg/` is purl-spec native). Principle X (transparency — every skipped file fires WARN per FR-006/FR-007).
**Scale/Scope**: Issue #500 reproduces at 4587 `.ipk` files per Yocto build. Real OpenWrt release feeds carry ~2000 `.ipk` per architecture × ~30 architectures. Scan wall-clock at that scale = key concern; ar-header parsing is O(N) linear in entry count with trivial per-header cost, so N=4587 completes in milliseconds regardless of hand-roll vs crate choice.

## Constitution Check

**GATE**: Pass before Phase 0 research. Re-check after Phase 1 design.

Constitution v1.5.0 principles evaluated against milestone 169's scope:

- **I. Pure Rust, Zero C**: PASS — reader uses existing `flate2` + `tar` workspace deps for the gzipped-tarball outer envelope (research §R2 revised 2026-07-06 based on Phase 1 empirical evidence that OpenWrt `.ipk` is `gzip( tar { ... } )`, NOT `ar { ... }`). Zero new crates. `flate2` transitively links `miniz_oxide` (pure Rust) so `.tar.gz` extraction stays C-free. Original spec's hand-rolled ar parser plan is DROPPED — the ar format isn't what modern `.ipk` uses.
- **II. Deterministic Scan Output**: PASS — same input produces same output. Tarball entry iteration is deterministic; control-file field ordering is preserved by `parse_stanzas`.
- **III. Attestation-First**: N/A — no attestation code touched.
- **IV. No `.unwrap()` in Production**: PASS — the reader uses `Option::or` / `Result::or_else` patterns per data-model.md; test code with `.unwrap()` guarded by `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **V. Specification Compliance (standards-native precedence)**: PASS — `pkg:opkg/` PURL is standards-native per purl-spec; no `mikebom:*` invention for the identity. The `mikebom:evidence-kind` + `mikebom:dep-alternative-alternates` (Q2) + `mikebom:archive-size-skipped` annotations are parity-bridges per Principle V's documented-exception path, mirroring m004 rpm's `mikebom:evidence-kind` = `"rpm-file"` / `"rpmdb-sqlite"` precedent.
- **VI. Three-Crate Architecture**: PASS — only `mikebom-cli` touched.
- **VII. eBPF-Only Observation**: N/A — user-space code path.
- **VIII. Completeness — Never Silently Drop**: PASS — every skipped `.ipk` file fires WARN per FR-006/FR-007. Every malformed ipk falls back to filename-only parsing per US2 — no silent drops.
- **IX. Accuracy — No Fake Versions**: PASS — versions come from control file `Version:` field; filename-fallback derivation matches Yocto/OpenWrt's mandatory `<name>_<version>_<arch>.ipk` convention.
- **X. Transparency — Explicit Signals**: PASS — WARN on every skip; INFO on installed-DB fallback (FR-014); `mikebom:evidence-kind` per FR-009/FR-015; `mikebom:dep-alternative-alternates` per Q2.
- **XI. Every Scan Produces an SBOM**: PASS — no scan-termination path added.
- **XII. Ecosystem Coverage**: PASS — extends per-ecosystem coverage from 5 to 6 OS package formats (dpkg + apk + rpm + alpm + homebrew + opkg-installed-DB) plus new opkg archive-file reader.

**Strict Boundaries** (v1.5.0):

- §1 (deterministic PURL): PASS.
- §2 (workspace layout): PASS.
- §3 (constitution amendment process): N/A.
- §4 (single source of truth): PASS — `parse_stanzas` is the shared RFC-822 parser; both dpkg + opkg + ipk_file consume it.
- §5 (no duplicate file-tier components): PASS — FR-011 + FR-017 ensure ipk-claimed files are skipped by the binary walker; FR-016 dedups archive+installed-DB emissions by PURL.

**Verdict**: 12 principles + 5 boundaries clear. No Complexity Tracking entries needed. The hand-rolled ar parser is the load-bearing Constitution-I decision — documented in research.md §R2 with alternatives considered.

## Project Structure

### Documentation (this feature)

```text
specs/169-ipk-opkg-reader/
├── plan.md              # This file
├── research.md          # Phase 0 — ar parser choice + m107 gap analysis + fixture strategy
├── data-model.md        # Phase 1 — ipk_file module + opkg.rs deltas + annotation values
├── quickstart.md        # Phase 1 — how to reproduce SC-001 + SC-011 end-to-end
├── contracts/
│   └── README.md        # Contract surface: `pkg:opkg/` PURL + evidence-kind values + wire annotations
├── checklists/
│   └── requirements.md  # /speckit.specify output (16/16 pass post-clarify)
└── tasks.md             # /speckit.tasks output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/
├── src/
│   ├── scan_fs/
│   │   ├── file_tier/
│   │   │   └── content_shape.rs                    # ← EDITED: add `.ipk` to recognized-suffix allowlist (FR-001)
│   │   └── package_db/
│   │       ├── mod.rs                               # ← EDITED: register `pub mod ipk_file;` + wire read_all
│   │       ├── ipk_file.rs                          # ← NEW: archive-file reader for `.ipk` (US1)
│   │       ├── opkg.rs                              # ← EDITED: FR-014 fallback + FR-015 evidence-kind + FR-016 dedup precedence
│   │       └── control_file.rs                      # ← UNCHANGED: `parse_stanzas` shared parser (m107) — reused
└── tests/
    ├── fixtures/
    │   ├── ipk-files/                               # ← NEW: 3-5 vendored real-world `.ipk` files
    │   └── opkg-installed-db/                       # ← NEW: synthetic runtime rootfs + `/var/lib/opkg/{status,info/}`
    └── ipk_reader.rs                                # ← NEW: SC-010 integration test
```

**Structure Decision**: Two-module split. `ipk_file.rs` is the flagship new module (US1 archive-file reader — the primary m169 delta). `opkg.rs` gets a minor 3-fix hardening pass (m107 gaps per FR-014/FR-015/FR-016). The file-tier walker allowlist gets one line added for `.ipk`. `read_all` in `package_db/mod.rs` gets one new call to `ipk_file::read(...)` following the pattern established at `mod.rs:1505` where `rpm_file::read(...)` was added by m004.

Zero new external contracts (only per-format annotation values on the parity-catalog C50 `mikebom:evidence-kind` row).

## Complexity Tracking

No entries required. All Constitution gates pass without justification. This is a scoped ecosystem-coverage addition with a clear precedent (m004 rpm dual-reader pattern + m107 opkg installed-DB foundation) and a single load-bearing decision (hand-roll vs crate for ar parsing — research §R2 pins to hand-roll).
