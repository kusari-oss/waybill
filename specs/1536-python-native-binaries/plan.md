# Implementation Plan: Native binaries inside a Python package get binary analysis

**Branch**: `1536-python-native-binaries` (base `main` at `1ae157d5`) | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1536-python-native-binaries/spec.md`

## Summary

Issue #1567. A `RECORD` claim stops suppressing binary analysis. The pip claims move out of the shared claim set into a `RecordOwners` map (research R1). A binary that only a `RECORD` owns is analysed as an unclaimed binary (R3). Then a pass after the binary scan attaches the results (R4):

- **Containment:** owner package → binary → what the binary identifies.
- **Edges:** package → binary, and binary → {linked libraries, identified components, other binaries of the same package}.

Three more pieces complete it:

- **Formats:** the emitters learn three-level nesting. CycloneDX folds recursively, with ancestor-chain `bom-ref`s and dependency refs mapped to them; SPDX 2.3 emits `CONTAINS` from nested parents (R5). No corpus golden contains a nested component (measured), so this changes nothing outside the feature.
- **Hash suffix:** auditwheel's hash suffix is stripped from names under `<pkg>.libs/` (R6).
- **Counts:** C230 `waybill:python-owned-binaries` (R7).

OS-package claims and `.ipk` claims are untouched.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: Existing only.
- `object`, through the binary scan, unchanged;
- `regex`, for the suffix (already a direct dependency);
- `serde_json`, `tracing`;
- `waybill_common::types::purl`.

**Zero new Cargo dependencies** (FR-011).
**Storage**: N/A. Everything is in-process for one scan.
**Testing**:
- **Unit tests:** `RecordOwners` (sorted first-wins, three-layer lookup); the attach pass; the suffix strip; CycloneDX recursive folding and ref mapping; SPDX 2.3 nested `CONTAINS`.
- **Integration tests:** synthetic `waybill-fixture-*` wheels in both layouts, plus auditwheel and delocate shapes, built from a library the tests compile or copy with an embedded version string. Each is checked in all three formats.
- **Conformance:** the CycloneDX schema and `spdx3-validate`.
- **CI:** corpus goldens (SC-002), the ROCm SDK fixture (SC-003), and the real wheels (SC-004).

**Target Platform**: as today. The fixtures must build on Linux, macOS and Windows CI, or be gated to `#[cfg(unix)]` at file scope (memory note: unix-only tests need file-scope cfg).
**Project Type**: CLI.
**Performance Goals**: no new cost beyond what the same tree costs without its `.dist-info` (R10). Wall time is recorded on the real wheels.
**Constraints**: no network at scan time; OS claims unchanged; deterministic.
**Scale/Scope**: the #1567 wheel has about 300 native libraries (quoted, not measured). NumPy and the ROCm wheel are measured in CI.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. An owner whose PURL can't be resolved is not invented: the binary is analysed without nesting (R2). A binary in both sets keeps the OS behaviour. |
| IV. Type-Driven Correctness | Pass. `RecordOwners` and `OwnedBinary` are typed. The ownership decision is one function, with three outcomes: OS or `.ipk` claimed, owned, or unclaimed. |
| V. Specification Compliance | Pass. Containment uses CycloneDX nested components and SPDX `CONTAINS`/`contains`; edges use native `dependsOn`/`DEPENDS_ON`. Only the counts need a new row (C230), because nothing existing counts this population. |
| VII. Test Isolation | Pass. Synthetic fixtures. TheRock and the PyPI wheels only in CI. |
| VIII. Completeness | Pass. It restores identification lost since #1529, and adds a `site-packages` inventory that never existed. |
| IX. Accuracy | Pass. No identity is invented. The owner PURL comes from the emitted pip component, and the suffix strip is confined to auditwheel's directory. |
| X. Transparency | Pass. C230 separates "analysed, nothing found" from "not analysed". The research and measurements are committed. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1536-python-native-binaries/
├── spec.md, plan.md, research.md (R1–R10), data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/   wheel census + scans (CI), golden review, mutations, gate log
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/pip/dist_info.rs   # collect_claimed_paths → RecordOwners
waybill-cli/src/scan_fs/package_db/pip/mod.rs         # re-export
waybill-cli/src/scan_fs/package_db/mod.rs             # DbScanResult.record_owners; pip no longer in `claimed`; test at :3398
waybill-cli/src/scan_fs/binary/mod.rs                 # owned → unclaimed path, no collapse; returns Vec<OwnedBinary>
waybill-cli/src/scan_fs/binary/owned.rs               # NEW: OwnedBinary, attach pass, internal links, coverage
waybill-cli/src/scan_fs/binary/entry.rs               # file-level name: auditwheel suffix strip under *.libs/
waybill-cli/src/scan_fs/binary/linkage.rs             # internal sonames not aggregated
waybill-cli/src/scan_fs/mod.rs                        # wire RecordOwners → binary::read → attach; C230 into ScanResult
waybill-cli/src/generate/cyclonedx/builder.rs         # recursive fold; ancestor-chain bom-refs
waybill-cli/src/generate/cyclonedx/dependencies.rs    # refs mapped to bom-refs
waybill-cli/src/generate/spdx/relationships.rs        # CONTAINS from nested parents
waybill-cli/src/generate/spdx/{packages,v3_packages}.rs + ids.rs  # nested identifiers from purl + parent chain
cli/scan_cmd.rs, generate/{mod.rs, cyclonedx/metadata.rs, spdx/{annotations,v3_annotations}.rs}  # C230 (the C228/C229 pattern)
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs     # C230
docs/reference/sbom-format-mapping.md                 # C230 row
docs/ecosystems.md, CHANGELOG.md
waybill-cli/tests/python_owned_binaries.rs            # integration tests
(fixture repo) measure-wheels.yml                     # SC-004
```

**Structure Decision**: the attach pass and its types live in a new `binary/owned.rs`. `binary/mod.rs` is already 1,227 lines, and the pass also reaches entries from `go_binary`. The emitter changes stay in their existing files.

## Approach

1. **Measure first (CI).** `measure-wheels.yml` in the fixture repo takes a layout census of the three pinned wheels, before any code depends on it (R6, R8). It records `.libs`/`.dylibs` counts, suffix shapes, `RECORD` entries outside the package, and sizes against `--max-file-size`. It also produces the baseline scan of each at `main`.
2. **Tests first:**
   - unit tests for `RecordOwners` and the suffix strip;
   - the integration fixtures, with US1 scenarios 1–6, US2 (dpkg plus `RECORD`, byte-identical) and US3 (both layouts).
3. **The claim split** (R1) and owned analysis (R3).
4. **The attach pass** (R4), giving nesting, edges, internal links and coverage.
5. **The emitters** (R5): recursive folding, ref mapping, nested `CONTAINS`, nested identifiers. The two-packages-one-library test is the V4 check.
6. **C230** (R7), with its extractors and mapping row.
7. **Mutations**, each reverted with `cp` and each required to fail a test:
   - the OS claim no longer winning;
   - no nesting;
   - edges from the root;
   - internal links reported as external;
   - no suffix strip;
   - the strip applied outside `.libs/`;
   - the two layouts diverging;
   - the binary also emitted top-level;
   - linkage counted as an identification.
8. **The gate**, then corpus goldens and the ROCm SDK fixture in CI. Corpus: SC-002, byte-identity predicted (R9). ROCm fixture: SC-003, byte-identical.
9. **Real wheels in CI** (SC-004), with the branch build.
10. **Docs, CHANGELOG and PR.**

## Complexity Tracking

None.
