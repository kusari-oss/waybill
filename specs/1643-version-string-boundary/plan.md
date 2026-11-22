# Implementation Plan: An embedded version string is found when a byte other than NUL precedes it

**Branch**: `1643-version-string-boundary` | **Date**: 2026-11-22 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1643-version-string-boundary/spec.md`. Closes #1568; Refs #1641.

## Summary

The curated embedded-version scanner accepts a match only after a NUL byte, so a vendor-built zlib comes out unversioned and a vendor-built ncurses is missed whenever another byte precedes their banners. Both are reproduced at the current `main`: 3 of 3 runs per case (research R1).

The plan has three steps:
1. **Classify.** Each `CuratedLibrary` gets a `Boundary`: `Anchored` for zlib, expat and ncurses, whose matches are fixed at both ends, and `NulOnly` for the other twelve (R2). The classification is an exhaustive `match` (R3).
2. **Relax the anchored patterns.** They accept any preceding byte except an identifier character. zlib applies the test to the byte before its leading space (R4).
3. **Count first.** A log-only commit emits `waybill::version_census` events for every pattern occurrence with its preceding byte class. TheRock's full SDK, the quality corpus and the public corpus are counted before the rule changes (R5, R6).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only: `tracing` for the census events, and test-only `strum::EnumIter`, already derived on `CuratedLibrary`. **Zero new Cargo dependencies.**
**Storage**: N/A. Per-binary, in process.
**Testing**: `cargo test --workspace --no-fail-fast` through `./scripts/pre-pr.sh`; unit tests in `version_strings.rs`; integration `waybill-cli/tests/version_string_boundary.rs` on the synthetic ELF helper; mutations M1–M7 (R8).
**Target Platform**: every host waybill builds on. The scanner reads ELF, Mach-O and PE.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-006, TheRock SDK scan within 1.05× `main`, median of ≥3 interleaved runs (R9).
**Constraints**: byte-identical output for binaries whose matches are NUL-preceded (FR-006); no new annotation or row (FR-007).
**Scale/Scope**: one module (`scan_fs/binary/version_strings.rs`), one call site for the census (`scan_fs/binary/mod.rs::prepare_held`), one new integration test file, and a census workflow in the fixture repository.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| IV. Type-driven correctness | Pass. The class is an enum with an exhaustive match, so an unclassified library cannot compile. No `.unwrap()` in production. |
| V. Specification compliance | Pass. No new field. A gained version is the same `pkg:generic/<lib>@<version>` with the existing `embedded-version-string` evidence. |
| VIII. Completeness | This is the point of the change. Bundled libraries gain the version that vulnerability matching needs. |
| IX. Accuracy | Guarded three ways. Only patterns fixed at both ends are relaxed. An identifier character still rejects a match. The census judges every new match before wiring, and FR-005 moves a pattern back to NUL-only on any false positive. |
| X. Transparency | Pass. These components already carry `waybill:confidence = heuristic` and the evidence kind; nothing about that changes. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1643-version-string-boundary/
├── plan.md
├── research.md          # R1–R9
├── data-model.md
├── quickstart.md
├── contracts/
│   └── wire.md          # fixture cases and expected components
├── measurements/
│   ├── probe.sh         # R1 byte-mutation probe (real Nix-built libraries, local only)
│   └── probe.md         # R1 results at 83657af2
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/binary/
├── version_strings.rs   # Boundary enum, CuratedLibrary::boundary, relaxed zlib/expat/ncurses, census()
└── mod.rs               # prepare_held: emits waybill::version_census events when enabled
waybill-cli/tests/
├── version_string_boundary.rs   # new: byte-class fixtures, parity, census positive control
└── common/installed_lib.rs      # existing elf_shared_lib helper (unchanged)
docs/reference/
└── (no catalog row changes; FR-007)
CHANGELOG.md             # Fixed entry for #1568 / #1641

kusari-sandbox/waybill-fixture-therock-sdk (separate repository):
├── .github/workflows/census-versions.yml   # full SDK tarball + 18 quality-corpus targets
└── census/versions.py                       # summarises waybill::version_census events
```

**Structure Decision**: The single existing module is changed in place. The census lives in the scanner it measures, so it cannot drift from the patterns.

## Complexity Tracking

No violations.
