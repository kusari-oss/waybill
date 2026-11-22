# Implementation Plan: bzip2, GMP and xz are versioned from the bare version they embed, when a second anchor names the library

**Branch**: `1644-bare-version-anchors` | **Date**: 2026-11-22 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1644-bare-version-anchors/spec.md`. Refs #1641, #1526, #1555, #1556, #1662.

## Summary

bzip2, GMP and xz each carry a bare version string with no name beside it, so the curated scanner, whose every pattern is a single self-naming literal, cannot read them. The plan adds a second kind of curated evidence: a library is versioned when the binary has the library's **anchor**, and exactly one whole NUL-bounded string of the library's **version shape** that no other anchored library in the same binary also accepts.

The anchors, from upstream source (research R2, R3):
- bzip2: the text `bzip2/libbzip2`;
- GMP: the text `GNU MP:`;
- xz: a **defined** `lzma_version_string` or `lzma_version_number` symbol.

The work goes in this order:
1. **Census first (R7).** A log-only commit reports every anchored binary in TheRock and the quality corpus, with its candidates and verdict, before anything is emitted.
2. **Definedness (R3).** A new `BinaryScan.anchor_symbols` holds only defined anchor symbols, because the existing `symbol_names` includes imports.
3. **Wire.** The three new `CuratedLibrary` variants (`bzip2`, `gmp`, `liblzma`), the `WholeString` boundary class and the verdict rules (FR-005, FR-005a).
4. **CPE rows (R4).** `gmp` → `gmplib:gmp` (35 dictionary entries); `liblzma` → `tukaani:xz` and `xz_project:xz`.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `memchr` (already a direct dependency, `waybill-cli/Cargo.toml:135`) for the anchor search, `object` (`read` feature) for symbol definedness, `tracing` for the census, and test-only `strum::EnumIter`. **Zero new Cargo dependencies.**
**Storage**: N/A. Per-binary, in process.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in `version_strings.rs`;
- a new integration file, `waybill-cli/tests/version_string_anchored.rs`, on a new test-only ELF helper with `.dynsym` (R8);
- mutations M1–M7 (R9).

**Target Platform**: every host. The anchor texts work on ELF, Mach-O and PE. The xz symbol anchor reads defined ELF dynamic symbols, defined Mach-O globals and PE exports.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-007, TheRock SDK scan within 1.05× `main` (median of ≥3 interleaved runs). Unanchored binaries pay two `memmem` searches and nothing else (R6).
**Constraints**: byte-identical output for binaries with no anchor (FR-012); no new annotation, row or field in the document (FR-011).
**Scale/Scope**:
- one module (`version_strings.rs`);
- one scan field (`scan.rs`, `entry.rs`);
- one call site (`mod.rs::prepare_held`);
- two CPE rows;
- one new test file and one test helper;
- the fixture repository's census script.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| IV. Type-driven correctness | Pass. The new variants join the exhaustive `CuratedLibrary` matches, and the verdict is an enum. No `.unwrap()` in production. |
| V. Specification compliance | Pass. The output is the existing `pkg:generic/<slug>@<version>` with `embedded-version-string` evidence, and the CPE pairs are measured against NVD's dictionary (R4). |
| VIII. Completeness | The point of the change: three common bundled libraries become matchable. |
| IX. Accuracy | Guarded five ways: a second, independent anchor; definedness for symbols; whole-string candidates; one candidate or none; and a candidate shared between libraries versions neither. The census judges every verdict before wiring. |
| X. Transparency | Pass. The components carry the existing `waybill:confidence = heuristic` and evidence kind. Missing versions are explained in the census, not invented. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after Phase 1: unchanged. The `WholeString` class is a third value of an existing enum, not a new mechanism outside the curated scanner.

## Project Structure

### Documentation (this feature)

```text
specs/1644-bare-version-anchors/
├── plan.md
├── research.md          # R1–R10
├── data-model.md
├── quickstart.md
├── contracts/
│   └── wire.md          # fixture cases, expected components, census event shape
├── measurements/
│   ├── cpe-probe-2026-11-22.txt     # R4, probe-cpe-dictionary.sh
│   └── nvd-titles-2026-11-22.txt    # R4, cpe-titles.py
├── checklists/
│   └── requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/binary/
├── version_strings.rs   # Bzip2/Gmp/Liblzma variants, Boundary::WholeString, ANCHOR_SYMBOLS, scan_anchored(), anchored census
├── scan.rs              # collect anchor_symbols (defined only) for ELF, Mach-O, PE
├── entry.rs             # BinaryScan.anchor_symbols
└── mod.rs               # prepare_held: append scan_anchored matches; census events when enabled
waybill-cli/src/generate/
└── cpe_table.toml       # rows: pkg:generic/gmp, pkg:generic/liblzma
waybill-cli/tests/
├── version_string_anchored.rs    # new: US1–US4 fixtures, parity, positive control
└── common/installed_lib.rs       # new test-only elf_shared_lib_with_dynsym
docs/reference/binary-identity.md # three libraries, the anchored rule
CHANGELOG.md

kusari-sandbox/waybill-fixture-therock-sdk (separate repository):
├── census/versions.py                       # summarise `anchored` events; look up existing bzip2/gmp/liblzma/xz components
└── .github/workflows/census-versions.yml    # adds the synthetic positive-control tree
```

**Structure Decision**: The existing module is extended in place, following spec 1643's precedent. The anchored libraries share `CuratedLibrary` so that emission, CPE lookup and the slug test cover them without new plumbing (R6).

## Complexity Tracking

No violations.
