# Implementation Plan: Identify-unknown-binaries enrichment

**Branch**: `096-binary-id-enrich` | **Date**: 2026-05-12 | **Spec**: [spec.md](spec.md)

## Summary

Three new signal-extraction passes added to the existing `mikebom-cli/src/scan_fs/binary/` module to better identify components inside binaries with no provenance:

1. **Embedded version-string scan** of read-only data sections (ELF `.rodata` / Mach-O `__cstring` / PE `.rdata`) for known-pattern strings (OpenSSL/zlib/libcurl/sqlite/libxml2 in v1). Emits `pkg:generic/<name>@<version>` components with `evidence.identity[].technique = embedded-version-string`, confidence 0.6.
2. **Packer signature scan** for UPX (v1; mpress/ASPack/PECompact are stretch). Emits `mikebom:binary-packer = <name>` property on file-level binary components, ALWAYS — value `none` for unpacked binaries per Clarification Q2.
3. **Symbol-table fingerprinting** of ELF `.dynsym` exports against published library symbol sets (OpenSSL/zlib/libcurl in v1). Emits `pkg:generic/<name>` (no version) with `evidence.identity[].technique = symbol-fingerprint`, confidence 0.4.

When both techniques match the same library on the same binary, the result is **one component carrying both `evidence.identity[]` entries** (composite-evidence pattern, Clarification Q1). Cross-binary dedup is **strict-PURL-equality** (Clarification Q3) — `pkg:generic/openssl@3.0.13`, `pkg:generic/openssl@3.0.12`, and `pkg:generic/openssl` are three separate components, each merging the N binaries that produced that exact PURL into a single `evidence.occurrences[]`.

**Constitution-friendly**: existing `mikebom:binary-class` + `mikebom:binary-stripped` parity-catalog precedent (C10 + C11 rows at `parity/extractors/mod.rs:140-141`) means the new `mikebom:binary-packer` slots in as C12 with the same per-format extractor pattern; no new SBOM-spec primitives. Zero new Cargo deps (the `object` crate already handles all parsing). Zero production code outside `mikebom-cli/src/scan_fs/binary/` + the parity-catalog row registration.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited; no nightly required).
**Primary Dependencies**: existing only — `object` crate (already in workspace, handles ELF/Mach-O/PE section iteration), `serde`/`serde_json`, `tracing`, `anyhow`. NO new crates.
**Storage**: N/A — pure read-only inference from the binary file itself.
**Testing**: `./scripts/pre-pr.sh` (pre-PR gate); 3 new synthetic test fixtures (statically-linked OpenSSL binary, UPX-packed ELF, symbol-only-fingerprint binary) built reproducibly via a small `build.rs`-style helper in `mikebom-cli/tests/`.
**Target Platform**: same as workspace — Linux x86_64, macOS aarch64, Linux aarch64. Symbol-fingerprinting is ELF-only in v1 (per spec FR-004 + research §3 below); embedded-version-string + packer detection cover all three platforms.
**Project Type**: Rust workspace, single binary (mikebom-cli) — adding three new analysis passes to the existing `binary/` module.
**Performance Goals**: per spec assumptions, ≤10 MB binary scan is bounded by linear `.rodata`/section walk; specific budget pinned at research §4 below.
**Constraints**: FR-007 (no new Cargo deps), FR-008 (production code only in `binary/` + parity-catalog row addition), FR-009 (zero golden regen on the 9 existing ecosystems; ≤1 new component spurious-match across them per SC-007), FR-010 (PURL conformance), FR-011 (strict-PURL-equality global dedup per Q3).
**Scale/Scope**: ~3 new files in `binary/` (`version_strings.rs` already exists as a stub; extend it. New `packer.rs` already exists as a stub; extend it. New `symbol_fingerprint.rs` — new file), 3 new test fixtures, 1 new parity-catalog row (C12). Total diff target: <800 lines.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

- **I. Pure Rust, Zero C**: ✅ no FFI, no C toolchains; `object` crate is pure Rust.
- **II. eBPF-Only Observation**: N/A — filesystem/binary inspection, not eBPF-trace.
- **III. Fail Closed**: ✅ no scan-discovery behavior changes. New techniques add components when signals match; absence of a signal is an honest "we don't know", not a heuristic gap-fill.
- **IV. Type-Driven Correctness / no `.unwrap()` in production**: ✅ all parsing through the existing `object`-crate `Result` chains and `?`-propagation.
- **V. Specification Compliance / standards-native precedence**: ✅ THIS IS THE LOAD-BEARING AUDIT.
  - **`mikebom:binary-packer`** (new property): is there a standards-native equivalent? **CDX**: no native packer-status field; the closest is `bom.metadata.properties[]` or `component.properties[]` — exactly where we put it. **SPDX 2.3**: no native packer field; `Package.annotations[]` with `comment` carries the prose. **SPDX 3**: no native packer field; `Annotation.statement` carries the prose. **Audit result**: no native construct exists across all 3 formats; the `mikebom:*` property is justified. Parity-catalog row C12 documents this per the milestone-049/052 precedent.
  - **`evidence.identity[].technique = embedded-version-string` and `symbol-fingerprint`** (new technique values): CDX `evidence.identity[].methods[].technique` is a string enum with values like `manifest-analysis`, `binary-analysis`, `source-code-analysis`, `filename`, `ast-fingerprint`, `hash-comparison`, `instrumentation`, `attestation`, `other`. The CDX spec permits `other` for tool-specific techniques. **Decision**: use `binary-analysis` as the spec-native technique value with a `mikebom:identification-method` companion property capturing the precise sub-technique (`embedded-version-string` vs `symbol-fingerprint`). This way the spec-native field stays compliant and the sub-method is captured per Constitution V's bridging convention. SPDX 2.3 + SPDX 3 use the same approach via existing `evidence` annotations (consistent with milestone-091's `mikebom:resolver-step` carrier pattern).
- **VI. Three-Crate Architecture**: ✅ unchanged; new code in `mikebom-cli` only.
- **VII. Test Isolation**: ✅ new tests are unit tests on the binary scanner + small synthetic fixtures (no privilege required).
- **VIII. Completeness, IX. Accuracy, X. Transparency**: ✅ confidence values (0.6 / 0.4) explicitly capture per-technique uncertainty; absence of identification is honest "we don't know" not silently-empty.
- **XI. Enrichment**: ✅ this milestone enriches binary-derived components with version + technique evidence.
- **XII. External Data Source Enrichment**: N/A — no external lookups in v1; FR-007 forbids new deps + Out-of-Scope items list external-DB lookups as deferred.

**No violations.** No Complexity Tracking entry needed.

## Project Structure

### Documentation (this feature)

```text
specs/096-binary-id-enrich/
├── plan.md                              # This file
├── research.md                          # Phase 0: pattern set, symbol set, UPX signatures, perf budget, parity catalog
├── data-model.md                        # Phase 1: per-deliverable shape (pattern table, signature table, fingerprint table, parity row)
├── contracts/
│   └── binary-id-contracts.md           # Phase 1: assertion shapes per technique + per-edge case + per-property
├── quickstart.md                        # Phase 1: maintainer recipes (apply, build fixtures, verify, ship)
├── checklists/
│   └── requirements.md                  # 16/16 pass — already complete
└── spec.md                              # Feature spec (with Q1/Q2/Q3 clarifications recorded)
```

### Source Code (repository root)

```text
mikebom/
├── mikebom-cli/src/scan_fs/binary/
│   ├── version_strings.rs               # EXTEND: was stub per milestone-004; add v1 pattern table + matcher
│   ├── packer.rs                        # EXTEND: was stub per milestone-004; add UPX signature scan
│   ├── symbol_fingerprint.rs            # NEW: ELF .dynsym match against library fingerprint set
│   ├── mod.rs                           # MODIFY: wire the three new passes into the read() dispatcher
│   └── (existing scan.rs, elf.rs, etc. untouched)
├── mikebom-cli/src/parity/extractors/
│   ├── mod.rs                           # MODIFY: register C12 row for mikebom:binary-packer
│   ├── cdx.rs                           # MODIFY: c12_cdx extractor stub
│   ├── spdx2.rs                         # MODIFY: c12_spdx23 extractor stub
│   └── spdx3.rs                         # MODIFY: c12_spdx3 extractor stub
├── mikebom-cli/src/generate/{cyclonedx,spdx}/...  # MODIFY: emit mikebom:binary-packer property
├── mikebom-cli/tests/
│   ├── binary_embedded_version_strings.rs  # NEW: integration tests for version-string extraction
│   ├── binary_packer_detection.rs          # NEW: integration tests for packer detection
│   └── binary_symbol_fingerprint.rs        # NEW: integration tests for symbol fingerprinting
└── docs/reference/sbom-format-mapping.md   # MODIFY: add C12 row for mikebom:binary-packer
```

**Structure Decision**: extension of existing `binary/` module (the stubs at `version_strings.rs` + `packer.rs` from milestone-004's deferred items are exactly the slots we fill). One new file (`symbol_fingerprint.rs`). Parity-catalog C12 row added across the 4-file extractor matrix. Three new integration test files. Zero changes to any other ecosystem reader or to crates outside `mikebom-cli/src/scan_fs/binary/` + parity infra.

## Complexity Tracking

> Not applicable — no Constitution gate violations.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| (none)    | (none)     | (none)                               |

## Phase Plan

### Phase 0 — Research (`research.md`)

Five decision points resolved:

1. **v1 version-string pattern set** — exact `(library, anchor, version-capture-regex)` tuples for the 5 starter libraries. Validated against published version-string formats.
2. **UPX detection signatures** — exact magic bytes / section names that uniquely identify UPX across ELF/Mach-O/PE. Well-documented in the UPX source.
3. **Symbol-fingerprint set + match threshold** — exact symbol lists per library (OpenSSL/zlib/libcurl) + the 80%-match-required threshold validated against false-positive risk.
4. **Performance budget per binary** — pinned at concrete bytes-scanned bound; per-binary scan time bounded by file size.
5. **Parity-catalog C12 row** — exact extractor shapes per format (CDX `component.properties[]`, SPDX 2.3 / SPDX 3 `Package.annotations[]` / `Annotation.statement` — matches C10/C11's pattern).

Plus deferred-from-clarify items resolved:
- **Symbol-fingerprint platform scope**: ELF-only in v1; PE/Mach-O symbol fingerprinting deferred (Out-of-Scope per spec).
- **Confidence-value tuning**: 0.6 / 0.4 inherited from milestone-004 binary-scanner convention (manifest-analysis = 0.85 → version-string = 0.6 → symbol-only = 0.4); no per-library variation in v1.

### Phase 1 — Design (`data-model.md`, `contracts/`, `quickstart.md`)

- **data-model.md** — per-table shape: 5-row version-string pattern table, 1-row packer signature table (with mpress/ASPack/PECompact stretch slots commented), 3-row symbol fingerprint table, the parity-catalog C12 row spec.
- **contracts/binary-id-contracts.md** — concrete assertion shapes per technique: which fields appear on emitted components, dedup invariants under composite-evidence (Q1) and strict-PURL-equality (Q3), packer property always-emitted convention (Q2).
- **quickstart.md** — maintainer recipes: build OpenSSL-statically-linked fixture, UPX-pack a fixture, build symbol-only-fingerprint fixture, run the three new test files, regen any goldens.

Re-evaluate Constitution Check post-design: still no violations expected (the C12 parity-catalog row is the only standards-native bridging addition and it follows the C10/C11 precedent exactly).

### Phase 2 — Tasks

Out-of-scope for `/speckit.plan`; will be generated by `/speckit.tasks`.

## Agent Context Update

The agent-context update script will be re-run after Phase 1; this milestone adds no new technology surface (`object` crate already in use, no new Cargo deps), so the agent context delta should be empty or trivial.
