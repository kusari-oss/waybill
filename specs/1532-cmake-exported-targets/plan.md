# Implementation Plan: Attribute installed files to their package through CMake exported targets

**Branch**: `1532-cmake-exported-targets` (base `main` at `cba55f7b`) | **Date**: 2026-11-15 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1532-cmake-exported-targets/spec.md`

## Summary

Issue #1563 extends the #1531 installed-library reader with a third file kind, CMake exported-targets files.

**Phase 1** parses each such file textually into a `TargetsFile`. For a main file that is its `_IMPORT_PREFIX` raise count; for a configuration file it is the raw `IMPORTED_LOCATION*` strings.

**Phase 2** works per directory. It:
- resolves each location under the scan root, root-relatively;
- finds the package's #1531 component: the same directory's version record, or else a same-name record in the same prefix;
- appends each shipped file to that entry's evidence, with the hash reused from the binary scan where one exists;
- counts every outcome in a new `exported-targets` object inside C225.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only. The reused pieces:
- #1531's `installed_lib` module (`classify_name`, `location`, `resolve`, `venv_candidates`);
- its CMake command tokenizer (`cmake_version::commands`);
- `binary::is_path_claimed`.

**Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**:
- **Unit tests** for the targets parser (raise count, locations, configurations, comments, foreign variables), for location resolution, and for package association.
- **Integration tests** on synthetic prefixes, covering both naming conventions, `Exports`, unversioned packages, escapes, OS ownership and the hash-reuse check.
- **Corpus goldens**, regenerated in CI.
- **The ROCm measurement**, CI only: a new `measure-full` job in the fixture repository.

**Target Platform**: as today.
**Project Type**: CLI.
**Performance Goals**: SC-006, at most 1.25× the baseline median scan time on the full ROCm tree. Hash reuse is the main mitigation (research R5).
**Constraints**: Offline, no subprocess, no evaluation. Root-relative resolution and the OS-owned skip apply as in #1531.
**Scale/Scope**: on the ROCm 7.14 SDK, 150 targets files name 914 locations; 903 attach (R1).

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. Nothing is attributed without a versioned package and an exact location, and every non-attachment is counted by reason. Foreign variables are never resolved. |
| IV. Type-Driven Correctness | Pass. File role and counts are typed. Reasons serialise in one place. |
| V. Specification Compliance | Pass. Evidence uses the native CycloneDX `evidence.occurrences`, and C18 in every format. No new catalog row; C225's text is extended. |
| VII. Test Isolation | Pass. Synthetic fixtures; ROCm only in CI through the public fixture repository. |
| VIII. Completeness | Pass. 64.6% of product bytes attributed, against 15.8% (#1563 measurement). |
| IX. Accuracy | Pass. A file is named by its own package's metadata, with no name matching. Test packages keep their own identity (#1564 scopes them). |
| X. Transparency | Pass. The census script and its output are committed; C225 records every outcome. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1532-cmake-exported-targets/
├── spec.md, plan.md, research.md (R1–R8), data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/
    ├── targets-census.py      # R1 (reads the fixture clone; no tarball)
    └── targets-census.json
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/installed_lib/
├── mod.rs            # classify_name → ExportedTargets; TargetsFile records in finalize
├── targets.rs        # NEW: parse a targets file (raise count, locations), using cmake_version::commands
├── cmake_version.rs  # commands() made pub(crate)
└── resolve.rs        # per-directory resolution, package association, evidence, ExportedTargetsCoverage
waybill-cli/src/scan_fs/mod.rs   # seed manifest_sha_cache from binary file components' file-sha256 (R5)
docs/reference/sbom-format-mapping.md   # C225 text
waybill-cli/tests/installed_lib_targets.rs   # integration tests
CHANGELOG.md
(fixture repo) .github/workflows/measure.yml  # measure-full job (R8)
```

**Structure Decision**: an extension of the #1531 module. The only change outside the module is seeding the hash cache.

## Approach

1. **Re-verify R3 to R5 at implementation.**
   - Check that the CMake reader emits nothing from a targets file, extending the #1531 inert test.
   - Check that the binary component's `file-sha256` equals `manifest_occurrence`'s hash of the same file.
2. **The parser, written test-first** (`targets.rs`): raise count, locations across configurations, comments, foreign variables and malformed input.
3. **Phase 1**: classification, records, and the walker and venv-probe patterns.
4. **Phase 2**:
   - per-directory resolution;
   - package association (version record, then same-name);
   - evidence attachment;
   - coverage;
   - hash-cache seeding.
5. **Integration tests** from every acceptance scenario, including the controls: targets files removed (SC-001), and a `.pc`-only package for the same-name rule.
6. **Mutations**, each reverted with `cp`, each required to fail a test:
   - ignore the raise count;
   - resolve foreign variables;
   - attach to the first component of the same name in any prefix;
   - skip the existence check;
   - drop the OS-owned skip;
   - no hash reuse (by a test counting hash computations, or by comparing digests).
7. **Gate**, then corpus goldens in CI. The prediction is no change (R7).
8. **ROCm measurement** in CI: the fixture repository's `measure-full` job gives SC-003's attached count (903) and SC-006's time ratio. If the ratio exceeds 1.25×, the path-only fallback per FR-005a, recorded with its measurement.
9. **Docs** (the C225 row and the CHANGELOG), then the PR.

## Complexity Tracking

None.
