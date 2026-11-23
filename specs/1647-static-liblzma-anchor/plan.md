# Implementation Plan: A statically linked liblzma is versioned from the version it embeds

**Branch**: `1647-static-liblzma-anchor` | **Date**: 2026-11-23 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1647-static-liblzma-anchor/spec.md`. Refs #1664, #1641, #1663; tracked in #1543.

## Summary

Spec 1644 anchors liblzma on a defined `lzma_version_string` or `lzma_version_number`. On ELF it reads only `.dynsym`, so a statically linked liblzma, whose symbols are in `.symtab`, is never versioned. The plan:

1. **Census first (R4, R5).** A log-only commit computes, with waybill's own `anchored_verdicts`, what each rule would give, and reports it as a `liblzma_static` event:
   - rule A: defined `.symtab` anchors;
   - rule B: the xz header magic.

   Counted on TheRock, the quality corpus, the public corpus (by golden comparison) and the 13 magic-carrying Ubuntu files. A real static build in the control job is the positive control.
2. **FR-004 decided by the census.** Research R2 measured `7zr` (7-Zip, no liblzma) holding the magic and one bare `13.2.0`. If the census confirms that rule B would emit `liblzma@13.2.0`, FR-004 is amended to *rejected* and #1664 is updated. Rule B is then removed.
3. **Wire rule A (FR-002).** Defined `.symtab` anchors join `.dynsym`'s. `.symtab` is iterated only when one `memmem` over `.strtab` finds `lzma_version_` (R3). Undefined entries never anchor.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `object` 0.36 (`symbols()` reads ELF `.symtab`, R1), `memchr` for the `.strtab` gate, `serde_json` and `tracing` for the census. **Zero new Cargo dependencies.**
**Storage**: N/A. Per binary, in process.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`.
- Unit tests `s1647_*` in `version_strings.rs` and `scan.rs`.
- A new integration file `waybill-cli/tests/version_string_static.rs` (contracts/wire.md S1–S8), on a new test-only `.symtab` ELF helper (R6).
- Mutations M1–M4 (R7).

**Target Platform**: every host. Rule A is ELF-only. Mach-O already reads its full symbol table (R1).
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-007, TheRock SDK scan within 1.05× `main` (median of ≥3 interleaved runs). Each ELF pays one `memmem` over `.strtab` (R3).
**Constraints**:
- byte-identical output where no verdict changes (FR-007);
- no new document field (FR-006);
- census before output (FR-001).

**Scale/Scope**:
- `scan.rs` (the read), `entry.rs` (the field's type), `version_strings.rs` (`Anchor::Symbol` gains table and binding; the census event), `mod.rs` (emit the event);
- one test helper and one test file;
- the fixture repository's census script and control job;
- docs and CHANGELOG.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No crate, no subprocess. |
| III. Fail closed | Pass. A table that cannot be read yields no anchor, so no version. Nothing is guessed. |
| IV. Type-driven correctness | Pass. `SymbolTable` and `Binding` are enums matched exhaustively; no `.unwrap()` in production. |
| V. Specification compliance | Pass. Output is the existing `pkg:generic/liblzma` match with the CycloneDX `binary-analysis` technique (spec 1646). |
| VIII. Completeness | The purpose: statically linked liblzma becomes matchable when unstripped. |
| IX. Accuracy | Guarded by definedness, by spec 1644's one-candidate and shared-candidate rules, and by rejecting the magic once a measured false version is confirmed (R2). The census judges every verdict before wiring. |
| X. Transparency | Pass. The census reports which table anchored each binary. A stripped static liblzma is documented as unversioned, with the reason. |
| Strict boundaries | Unaffected (scan mode). |

**Gate: pass.** Re-checked after Phase 1: unchanged. `AnchorSymbol` widens an existing never-emitted field; no new mechanism is added.

## Project Structure

### Documentation (this feature)

```text
specs/1647-static-liblzma-anchor/
├── plan.md
├── research.md                       # R1–R7
├── data-model.md
├── quickstart.md
├── contracts/wire.md                 # S1–S8, census event
├── measurements/
│   ├── probe/                        # a.c, b.c, probe.sh, magic-*.sh, sect.sh, README.md
│   └── magic-population-2026-11-23.md
├── checklists/requirements.md
└── tasks.md                          # /speckit-tasks
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/binary/
├── scan.rs              # .strtab gate; defined .symtab anchors; AnchorSymbol with table and binding
├── entry.rs             # BinaryScan.anchor_symbols: Vec<AnchorSymbol>
├── version_strings.rs   # AnchorSymbol, SymbolTable, Binding; Anchor::Symbol{table,binding}; liblzma_static census
└── mod.rs               # prepare_held: rule A into anchored_verdicts (wired); liblzma_static event
waybill-cli/tests/
├── version_string_static.rs          # new: S1–S8, parity
└── common/installed_lib.rs           # new test-only elf_with_symtab(e_type, rodata, defined, undefined)
docs/reference/binary-identity.md     # liblzma anchor row; static and stripped behaviour
CHANGELOG.md

kusari-sandbox/waybill-fixture-therock-sdk (separate repository):
├── census/versions.py                # summarise liblzma_static events
└── .github/workflows/census-versions.yml   # control job builds probe/a.c statically, unstripped and stripped
```

**Structure Decision**: Extend spec 1644's anchor in place. `anchored_verdicts` keeps keying on anchor names, so the verdict rules are reused unchanged, and the census's rule evaluations call the same function (R4).

## Complexity Tracking

No violations.
