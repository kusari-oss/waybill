# Implementation Plan: Attribute installed binaries to their source through embedded build and source paths

**Branch**: `1534-embedded-path-attribution` (base `main` at `96a7ef32`) | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1534-embedded-path-attribution/spec.md`

## Summary

Issue #1532.

**The binary scan** already collects each binary's read-only string region for its version-string patterns. It now also extracts absolute source paths from that region (research R1: the 16 MiB cap and the absence of DWARF parsing were both measured to lose nothing). Each compiled-source path is reduced to:
- its outermost directory naming a versioned installed-library package (R2), or
- a bundled third-party tree (R4).

**Phase 2** (#1531/#1563's `installed_lib::resolve`) attaches each binary to the package(s) it names, the way #1563 attaches exported-targets files. C227 lists them per component, and C225 gains an `embedded-paths` count.

**Bundled trees** become versionless components nested under the binary.

**Measured target (SC-001):** 258 binaries, 1.308 GB, 0 shared, on the ROCm SDK.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only.
- `object`, already used by the binary scan;
- `serde_json`, `tracing`;
- the #1531/#1563 `installed_lib` module.

**Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**:
- **Unit tests:** path extraction and classification (compiled vs header, toolchain, vendoring markers, outermost match, the stoplist and exclusion list, truncated strings).
- **Integration tests:** synthetic ELF fixtures with crafted `.rodata` (the `elf_shared_lib` helper from `tests/common/installed_lib.rs`), one per acceptance scenario.
- **Corpus goldens** in CI.
- **The ROCm measurement** in CI, through the fixture repository's `measure-full`.

**Target Platform**: as today. The string region exists for ELF, Mach-O and PE; the paths found are platform-independent strings.
**Project Type**: CLI.
**Performance Goals**: SC-004, at most 1.25× the main-branch median on the full ROCm tree. The extraction is a linear pass over a buffer already in memory (≤ 16 MiB per binary).
**Constraints**: Offline, no subprocess. No version from a path. No project-specific manifest.
**Scale/Scope**: 708 candidate binaries, 3 GB, on ROCm; 312 versioned names.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates. |
| III. Fail Closed | Pass. These are counted, never guessed: header-only, toolchain-only, unmatched, a name on the stoplist. The outermost rule and the stoplist were added because measurement found false positives (R2). |
| IV. Type-Driven Correctness | Pass. Path classes and findings are types, and the counts serialise in one place. |
| V. Specification Compliance | Pass. Evidence uses the native `evidence.occurrences` and C18; nesting uses native component nesting and `CONTAINS`. C227 is new because no format carries "attributed through embedded paths". C225 is extended, not duplicated. |
| VII. Test Isolation | Pass. Synthetic `waybill-fixture-*` ELF; ROCm only in CI. |
| VIII. Completeness | Pass. 1.308 GB of previously unattributed binaries gain a package (R3). |
| IX. Accuracy | Pass. Compiled sources only; the outermost directory; a stoplist; exact names; no version from a path. |
| X. Transparency | Pass. C227 per component, C225 counts, and the census scripts and outputs committed. |

No violations. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation

```text
specs/1534-embedded-path-attribution/
├── spec.md, plan.md, research.md (R1–R7), data-model.md, quickstart.md
├── contracts/wire.md
├── checklists/requirements.md
└── measurements/  embedded-paths.py, attribution.py, summarize.py, unattributed-binaries.txt,
                   versioned-names.txt, census.json (v1), attribution.json (v3)
```

### Source Code

```text
waybill-cli/src/scan_fs/binary/embedded_paths.rs   # NEW: extract + classify paths; EmbeddedPathFinding
waybill-cli/src/scan_fs/binary/scan.rs             # run extraction on the existing string region
waybill-cli/src/scan_fs/binary/mod.rs              # read(): names in, findings out; bundled entries; evidence kind
waybill-cli/src/scan_fs/binary/entry.rs            # bundled-component entry (symbol-fingerprint shape)
waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs  # versioned_names(); attribution; C225 embedded-paths; C227
waybill-cli/src/scan_fs/mod.rs                     # thread names/findings between phase 1, binary::read, phase 2
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs  # C227
docs/reference/sbom-format-mapping.md              # C225 text, C227 row, evidence-kind value
waybill-cli/tests/installed_lib_embedded_paths.rs  # integration tests
CHANGELOG.md
```

**Structure Decision**: path extraction lives with the binary scan, because it reads the binary's buffer. Attribution lives with #1563's, because both attach files to the same installed-library components.

## Approach

1. **Unit tests first** for `embedded_paths.rs`, covering:
   - compiled vs header extensions;
   - toolchain prefixes;
   - each vendoring marker and `_deps/<name>-src`;
   - outermost match (a `hip/` directory inside `rocthrust/`);
   - the stoplist (`amd`);
   - the exclusion list (`sysdeps`);
   - a path with `:line` suffix;
   - non-UTF-8 bytes;
   - an empty region.
2. **Extraction** in `scan_binary`, on `string_region`, given `VersionedNames`.
3. **Plumbing**, in this order:
   - `versioned_names(&Phase1, root, claims)` in `resolve.rs`, computed before `binary::read`;
   - `binary::read` takes the names and returns findings through an out-parameter (the #1524 pattern);
   - phase 2 takes the findings.
4. **Attribution** in `resolve.rs`:
   - exclude binaries already in any evidence list;
   - attach through `extra_source_paths` (hash reuse as in #1563);
   - build C227 and the `embedded-paths` coverage.
5. **Bundled components** in `binary::read`. Register `embedded-source-path` in `BINARY_EVIDENCE_KINDS`.
6. **Emission and extractors:** C227 in all three formats, and its parity extractor and catalog row together (the lib-target gate).
7. **Integration tests** from every acceptance scenario. Controls:
   - the same binaries with their paths stripped;
   - a binary naming two packages;
   - a header-only binary;
   - a toolchain-only binary;
   - an already-attributed binary.
8. **Mutations**, each reverted with `cp`:
   - any directory instead of the outermost;
   - drop the stoplist;
   - headers count;
   - drop the toolchain exclusion;
   - drop the already-attributed exclusion;
   - drop the shared marking;
   - drop the exclusion list.
9. **Gate**, then corpus goldens in CI. The prediction is no change: no corpus target has versioned installed-library components beyond `image-postgres16`, whose binaries are dpkg-owned.
10. **ROCm** `measure-full` in CI: SC-001, SC-002 (list every attribution with its justifying directory), SC-004 and SC-006.
11. **Docs and CHANGELOG**, then the PR.

## Complexity Tracking

None.
