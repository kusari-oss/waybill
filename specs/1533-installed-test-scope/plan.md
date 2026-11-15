# Implementation Plan: Report an installed tree's test and benchmark payload with a test scope

**Branch**: `1533-installed-test-scope` (base `main` at `1cb56c56`) | **Date**: 2026-11-15 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1533-installed-test-scope/spec.md`

## Summary

Issue #1564. In an installed tree, files a test declaration names, and files under a conventional test directory, are reported with the test scope.

**Phase 1** collects declarations alongside the #1531 and #1563 records:
- exported-targets files of a package named as a test package;
- installed `CTestTestfile.cmake` files.

**Phase 2** resolves the locations they declare, inside the scan root.

**One post-pass in `scan_cmd`** runs after file-tier emission and before `--exclude-scope`. It sets `lifecycle_scope = Test` and C62 on:
- binary file components and file-tier components whose every path matches a rule;
- the installed-library component of a declared test package.

Because it runs after scope propagation, it cannot seed propagation (FR-007 by construction, research R2).

**What research measured on ROCm:**
- CTest declarations name 432 files (2.63 GB);
- the `rocprofiler-sdk-tests` export set names 22 (0.188 GB);
- test directories add 14,465 (0.236 GB).

That totals 3.06 GB, half of #1564's census. No name pattern is adopted (R4): every candidate either adds nothing beyond the declarations or adds only files with no independent check. HIP's `catch_tests` stays unscoped, because its tests are discovered at run time (R1).

**Emission** is the existing m052 path (CycloneDX `excluded`, C42 in all formats). The one new piece is SPDX 3's `software_primaryPurpose: "test"`, with the binary role moved to `software_additionalPurpose`.

**A new document-scope row**, C226 `waybill:test-scope-coverage`, counts the declarations, the outcome of every declared location, and the components each rule scoped.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only. The reused pieces:
- #1531's `installed_lib` module (`location`, `cmake_version::commands`, the shared-walker registration);
- #1563's `targets` resolution;
- `binary::discover::TEST_INPUT_DIRS`, the #1524 directory names.

**Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**:
- **Unit tests:** the CTest parser, interpreter handling, rule matching (exact names, all-paths), strength order, and installed-tree recognition.
- **Integration tests:** one per acceptance scenario, plus no-propagation, OS ownership, the subject, byte-identity of an unrecognised source tree, and `--include-test-binaries`.
- **Corpus goldens:** regenerated in CI. The prediction is in research R5.
- **The ROCm measurement:** CI only, through `measure-full`.

**Target Platform**: as today.
**Project Type**: CLI.
**Performance Goals**: SC-006, at most 1.10× the main-branch median on the full ROCm tree. The pass is path matching over components the scan already holds, plus parsing 28 small CTest files.
**Constraints**: Offline, no subprocess, no evaluation. Root-relative resolution. Never re-scope a component that already carries a scope.
**Scale/Scope**: the ROCm 7.14 SDK, with 28 CTest files, 2 exported-targets test-package candidates (one qualifies), 41 conventional test directories (0.4 GB), and 809 regular files directly in `bin/`.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. A file is scoped only by a rule matching its own path, and every path of a deduplicated component must match. Unresolvable declarations are counted, never guessed. Name patterns are adopted only with a measurement (FR-009). |
| IV. Type-Driven Correctness | Pass. The rule, its strength and its C62 value are one enum. Recognition is an enum. Coverage serialises in one place. |
| V. Specification Compliance | Pass. Native carriers first: CycloneDX `scope`, and SPDX 3 `software_primaryPurpose` / `software_additionalPurpose`. C42 and C62 are reused. C226 is new only because no existing row can carry it (contracts/wire.md). |
| VII. Test Isolation | Pass. Synthetic `waybill-fixture-*` prefixes. ROCm only in CI. |
| VIII. Completeness | Pass. Nothing is dropped; test payload is reported with a scope. |
| IX. Accuracy | Pass. Declarations before conventions before names. Scope never flows to linked libraries. A product library is never scoped (SC-003). |
| X. Transparency | Pass. C62 on every scoped component, C226 for the document. The census script and its output are committed. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1533-installed-test-scope/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/
    ├── test-census.py        # R1/R3/R4: reads the fixture clone + listing; no tarball
    └── test-census.json
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/installed_lib/
├── mod.rs        # registration: + **/CTestTestfile.cmake; Phase1 gains ctest records
├── ctest.rs      # NEW: parse add_test / subdirs; interpreter handling
└── resolve.rs    # declared-location resolution; test-package exported targets
waybill-cli/src/scan_fs/test_scope.rs   # NEW: rules, recognition, the post-pass, C226 coverage
waybill-cli/src/scan_fs/mod.rs          # carry declarations + recognition out of scan_path
waybill-cli/src/cli/scan_cmd.rs         # call the post-pass after file tier, before --exclude-scope
waybill-cli/src/generate/spdx/v3_packages.rs     # primaryPurpose test + additionalPurpose role
waybill-cli/src/generate/{cyclonedx,spdx}/…      # C226 document annotation (C225 pattern)
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs  # C226; A13 SPDX 3 reads additionalPurpose
docs/reference/sbom-format-mapping.md   # C226 row; C62 values; A13 note
waybill-cli/tests/installed_test_scope.rs   # integration tests
CHANGELOG.md
(fixture repo) capture.yml (done: CTest files), measure-full.yml (report scoped bytes)
```

**Structure Decision**: Collection belongs with the #1531 and #1563 module, which already walks for CMake metadata and resolves inside the root. The post-pass is its own module, because it acts on every component tier, not only installed libraries.

## Approach

1. **Measure first.**
   - Census script R1, R3 and R4 over the new capture: CTest contents and forms, resolved locations, and bytes per rule.
   - Name-pattern review table: FR-009's adoption decision, committed.
2. **CTest parser**, test-first (`ctest.rs`). Forms are fixed by R1, along with interpreters, variables, absolute paths, comments and malformed input.
3. **Phase 1**:
   - registration;
   - the test-package selector on #1563 `TargetsFile`s;
   - recognition evidence.
4. **Phase 2**:
   - location resolution and outcomes;
   - `declared_test_paths`;
   - which installed-library components are declared test packages.
5. **The post-pass** (`test_scope.rs`):
   - rule matching, all paths, strength, exclusions;
   - C62;
   - the coverage record.
6. **Emission**:
   - SPDX 3 purpose;
   - C226 in all three formats;
   - extractors, plus the A13 fix.
7. **Integration tests** from every acceptance scenario and edge case, including the controls:
   - a declared test package with its declaration removed;
   - an unrecognised source tree with `tests/`.
8. **Mutations**, each reverted with `cp`, each required to fail a test:
   - drop the all-paths rule;
   - allow propagation (run the pass before scope propagation);
   - substring directory matching;
   - drop the recognition gate;
   - scope OS-owned files;
   - drop the interpreter rule;
   - re-scope an already scoped component;
   - drop the A13 extractor fix.
9. **Gate**, then corpus goldens in CI. Prediction in R5.
10. **ROCm measurement** in CI, which gives SC-001 to SC-004 and SC-006.
11. **Docs:**
    - the C226 row, the C62 values and the A13 note;
    - `docs/ecosystems.md`;
    - the CHANGELOG.

    Then the PR.

## Complexity Tracking

None.
