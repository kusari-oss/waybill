# Implementation Plan: Report declared third-party datasets as data components

**Branch**: `1535-declared-datasets` (base `main` at `1e5c53d1`) | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1535-declared-datasets/spec.md`

## Summary

Issue #1565. The CMake reader learns `file(DOWNLOAD)`, for:
- literal URLs;
- URLs whose only variable is an element of a same-file literal list, iterated in a loop.

A resolved download in such a loop, with a paired digest list checked by `file(<ALG>)`, is a **dataset** (research R2: on TheRock's pinned sources this classifies exactly the 8 matrix downloads and none of the 3 tool installers). Each distinct list element becomes a data component:
- identity `pkg:generic/<ns>/<name>` with `download_url` and `checksum` qualifiers;
- no version;
- carried natively in all three formats, with SPDX 2.3 bridged through C160 `component-kind: dataset`.

Other downloads are counted in a new document-scope row, C229, and emit nothing.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only.
- `regex`, already used by `cmake.rs`;
- `serde_json`, `tracing`;
- `waybill_common::types::{purl, hash}`.

**Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**:
- **Unit tests:** the download parser (literal; list loop in both shapes; digest pairing; fallback de-duplication; `$ENV`, cache and function-argument variables; unequal list lengths).
- **Integration tests:** synthetic CMake trees, checked in all three formats.
- **Conformance:** `spdx3-validate` and the CycloneDX schema.
- **Corpus goldens** in CI.
- **The ROCm-source measurement** in CI.

**Target Platform**: as today.
**Project Type**: CLI.
**Performance Goals**: negligible. The reader already reads these files.
**Constraints**: No network at scan time, no evaluation beyond FR-001, and no version invented.
**Scale/Scope**: on TheRock's pins, 26 downloads in 17 files; 24 datasets in rocSPARSE.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. Unresolvable URLs and unclassified downloads are counted, never emitted. A digest disagreement records no digest. |
| IV. Type-Driven Correctness | Pass. Resolution and classification are enums; the counts serialise in one place. |
| V. Specification Compliance | Pass. Native data kinds in CycloneDX and SPDX 3. SPDX 2.3 reuses C160 (an extensible enum) rather than adding a row. C229 is new because nothing counts declared downloads. |
| VII. Test Isolation | Pass. Synthetic fixtures; ROCm only in CI. |
| VIII. Completeness | Pass. 24 datasets previously invisible. |
| IX. Accuracy | Pass. A structural rule measured at 8 of 8 dataset calls classified and 0 of 3 tools (R2). No guessed type, no guessed version. |
| X. Transparency | Pass. C229, and the census committed. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1535-declared-datasets/
├── spec.md, plan.md, research.md (R1–R7), data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/  downloads.py, downloads.json (census run 43800233978),
                   the three ROCm *.cmake files at 9811f2ea5235, README.md
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/cmake_download.rs  # NEW: parse file(DOWNLOAD); resolve; classify; DeclaredDownload
waybill-cli/src/scan_fs/package_db/cmake.rs           # call it per discovered file; emit dataset entries; coverage
waybill-cli/src/scan_fs/mod.rs + cli/scan_cmd.rs + generate/**  # C229 plumbing (the C226/C228 pattern)
waybill-cli/src/generate/cyclonedx/builder.rs         # type "data" for component-kind dataset
waybill-cli/src/generate/spdx/packages.rs             # primaryPackagePurpose OTHER
waybill-cli/src/generate/spdx/v3_packages.rs          # dataset_DatasetPackage + datasetType + primaryPurpose data
waybill-cli/src/parity/extractors/*                   # C229; C160 unchanged
docs/reference/sbom-format-mapping.md                 # C160 value, C229 row
waybill-cli/tests/cmake_declared_datasets.rs          # integration tests
CHANGELOG.md
(fixture repo) census-downloads.yml                   # + scan rocsparse/hipsparse roots for SC-001
```

**Structure Decision**: a new module beside `cmake.rs`, which is already over 2,000 lines. It owns download parsing; `cmake.rs` calls it.

## Approach

1. **Parser unit tests first** (`cmake_download.rs`), on synthetic shapes copied from the measured forms in R1, with `waybill-fixture-*` names.
2. **Resolution and classification** (data-model).
3. **Emission:**
   - dataset entries;
   - C160 `dataset`;
   - CycloneDX `type: data`, SPDX 2.3 `OTHER`, SPDX 3 `dataset_DatasetPackage`;
   - C229.
4. **Integration tests** in all three formats, plus `spdx3-validate` and the CycloneDX schema (SC-002).
5. **Mutations**, each reverted with `cp`:
   - drop the digest-pairing requirement;
   - accept `$ENV`;
   - no fallback de-duplication;
   - identity by URL rather than element;
   - type `library` instead of `data`;
   - invent a version.
6. **Gate.** Then corpus goldens in CI. Byte-identity is predicted: there is no CMake target in the corpus, and no `file(DOWNLOAD)` in the repository's own CMake fixtures or the fixtures repository (research R2).
7. **ROCm source** in CI: extend `census-downloads` to scan the `projects/rocsparse` and `projects/hipsparse` roots with the branch build (SC-001).
8. **Docs, CHANGELOG, PR.**

## Complexity Tracking

None.
