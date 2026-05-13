# Implementation Plan: Compiler/linker version extraction for build provenance

**Branch**: `098-compiler-version-extract` | **Date**: 2026-05-12 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/Users/mlieberman/Projects/mikebom/specs/098-compiler-version-extract/spec.md`

## Summary

Extend the existing `mikebom-cli/src/scan_fs/binary/{elf,macho,pe}.rs` per-format readers with three new extraction passes — ELF `.comment` section, Mach-O `LC_BUILD_VERSION` SDK + tools fields, PE `IMAGE_OPTIONAL_HEADER.MajorLinkerVersion`/`MinorLinkerVersion` — and emit four new `mikebom:*` annotation properties on the file-level binary component via the existing `extra_annotations` bag. Single-file delta per format; no changes to emission code; no new Cargo dependencies. Mirrors milestone-023/024/028 ELF/Mach-O/PE identity-helper patterns exactly.

The existing `parse_min_os_version` at `macho.rs:209` already iterates `LC_BUILD_VERSION` for the `min_os` field; extending it to also extract `sdk` + the trailing tools-array is a small follow-on (~30 lines). The existing `parse_pe_identity` at `pe.rs:54` already touches the optional header for Machine + Subsystem; reading the two-byte linker version is two more accessor calls (verified at audit time — `object::pe::ImageOptionalHeader32`/`64` expose `major_linker_version: u8` and `minor_linker_version: u8` directly). The new `.comment` parser is the only genuinely-new code (~50 lines) but uses the same `section_by_name_bytes` accessor `elf::parse_note_package_public` already uses.

The file-level component builder at `entry.rs::make_file_level_component` plumbs the new fields through the existing `build_elf_identity_annotations` / `build_macho_identity_annotations` / `build_pe_identity_annotations` helpers — the same pattern the milestone-023/024/028 identity properties already follow.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–097; no nightly required).
**Primary Dependencies**: Existing only — `object` crate's `section_by_name_bytes` (ELF), the existing `for_each_load_command` helper at `macho.rs:178` (Mach-O), the existing `PeFile32`/`PeFile64::optional_header()` accessor exposed by `object` 0.36 (PE). `serde`/`serde_json` for `Value` construction in the `extra_annotations` bag. **No new Cargo deps.**
**Storage**: N/A — pure read-only inference per scan; no caches, no persistence.
**Testing**: `cargo +stable test` workspace. New unit tests in each of `elf.rs`, `macho.rs`, `pe.rs` modules (8 total per SC-006). One optional integration test in `mikebom-cli/tests/binary_build_provenance.rs` exercising any compiled ELF on the host (gracefully skips if no `/bin/ls` available).
**Target Platform**: Host-agnostic (cross-platform mikebom build). ELF parsing works on any host; Mach-O parsing works on any host; PE parsing works on any host — all via the `object` crate which is platform-independent.
**Project Type**: Rust CLI workspace (`mikebom-cli` binary + `mikebom-common` lib + `xtask`).
**Performance Goals**: ≤1ms additional per-binary scan time. `.comment` is typically <1 KiB; `LC_BUILD_VERSION` is a fixed-layout load command (~20 bytes + tools-array of ~8 bytes/tool); PE linker-version is two bytes. Sub-millisecond per binary.
**Constraints**: Zero new Cargo deps (FR-005). Production code confined to `mikebom-cli/src/scan_fs/binary/` (FR-006). Per-entry 4 KiB / total 64 KiB cap on `.comment` property values (FR-001). Always-emit `mikebom:pe-linker-version` per FR-003 (matches milestone-096 Q2 convention).
**Scale/Scope**: Three per-format extractors + four new properties + parity-catalog rows. Codebase delta: ~150 lines new code + ~80 lines new tests + 4 new parity-catalog rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Rationale |
|-----------|--------|-----------|
| **I. Pure Rust, Zero C** | ✅ PASS | All new code is Rust. No new Cargo deps. |
| **II. eBPF-Only Observation** | ✅ N/A | Enrichment milestone, not discovery; no observation path touched. |
| **IV. Test Discipline** | ✅ PASS | Per-format unit tests for happy path + edge cases + defensive parsing. Pre-PR gate per SC-005. |
| **V. Specification Compliance** | ⚠️ AUDIT | Four new `mikebom:*` annotation properties. Audit at planning time (research §1): CDX 1.6 / SPDX 2.3 / SPDX 3 have NO native compiler-stamp, build-version, or linker-version fields. The `mikebom:*` annotation namespace is justified — same Constitution V audit conclusion that justified milestone-023/024/028 identity properties. New parity-catalog rows needed (FR-010). |
| **X. Transparency** | ✅ PASS | Property absence is informative (FR-007); always-emit pe-linker-version exposes packer/obfuscator zeroing as a transparency signal. |
| **XII. External Data Source Enrichment** | ✅ N/A | No external API calls; all parsing is in-source against the binary bytes. |

**No CRITICAL violations.** Constitution V audit recorded in research §1.

## Project Structure

### Documentation (this feature)

```text
specs/098-compiler-version-extract/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
├── checklists/
│   └── requirements.md  # Already exists
├── spec.md              # Already exists (+ Clarifications)
└── tasks.md             # Phase 2 output (NOT created here)
```

### Source Code (repository root)

```text
mikebom-cli/src/scan_fs/binary/
├── elf.rs               # EXTEND — add `parse_comment_section` + tests
├── macho.rs             # EXTEND — new `parse_build_version_full` helper
                         #          (or extend parse_min_os_version)
├── pe.rs                # EXTEND — extend parse_pe_identity to also read
                         #          MajorLinkerVersion / MinorLinkerVersion
├── entry.rs             # MODIFY — add 4 fields to BinaryScan; emit via
                         #          existing build_elf/macho/pe_identity_annotations
└── scan.rs              # MODIFY — invoke the new parsers; populate BinaryScan

mikebom-cli/src/parity/extractors/
├── mod.rs               # MODIFY — add 4 new catalog rows (C16-C19 or next-available)
├── cdx.rs               # MODIFY — 4 new cdx_anno! macro invocations
├── spdx2.rs             # MODIFY — 4 new spdx23_anno! macro invocations
└── spdx3.rs             # MODIFY — 4 new spdx3_anno! macro invocations

docs/reference/
└── sbom-format-mapping.md  # MODIFY — add 4 new rows to the C-row table

mikebom-cli/tests/
└── binary_build_provenance.rs  # NEW (optional) — system-binary integration test
```

**Structure Decision**: per-format readers extend their existing modules. New `BinaryScan` fields plumb through `entry.rs::build_*_identity_annotations` helpers (the same pattern milestones 023/024/028 already established). Four new properties → four new parity-catalog rows mirroring C10/C11/C15.

## Complexity Tracking

No constitution violations. Table empty.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| — | — | — |
