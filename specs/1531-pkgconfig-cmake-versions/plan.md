# Implementation Plan: Installed C/C++ library versions from pkg-config and CMake version files

**Branch**: `1531-pkgconfig-cmake-versions` (base `main` at `96ad6804`) | **Date**: 2026-11-15 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1531-pkgconfig-cmake-versions/spec.md`

## Summary

Issue #1531 adds a new `installed_lib` reader, which works in two phases (research R2).

**Phase 1, in the shared walker.** The reader collects `*.pc`, `*ConfigVersion.cmake` and `*-config-version.cmake` paths. Its finalize parses each one into a record:
- the module or package name;
- the version resolved by pkg-config's own variable rules, or CMake's literal `PACKAGE_VERSION`;
- `Requires` and `Requires.private`;
- the install prefix, derived from the location rules.

**Phase 2, a post-pass in `scan_fs::scan_path`.** It runs right after the binary scan and:
- drops records that an OS package database owns (an OS-only claim set, plus a Homebrew keg check);
- resolves `Requires` to exact PURLs within each prefix;
- emits `pkg:generic/<lowercased>@<version>` entries at tier `deployed`, ahead of the binary entries, so that an identical embedded-version match merges into them;
- lists matching shared-library files in their evidence.

Two new catalog rows carry the private-only requirements (component scope) and the coverage counts (document scope).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only: `serde`/`serde_json`, `globset` (walker patterns), `tracing`; `waybill_common::types::purl::{Purl, encode_purl_segment}`; `binary::is_path_claimed`. **Zero new Cargo dependencies.** No subprocess.
**Storage**: N/A. Everything is in process for one scan.
**Testing**: `cargo test`.
- **Unit tests:** the `.pc` parser (variables, continuation, comments, `Requires` grammar), the CMake version parser, the location and prefix rules, and `Requires` resolution.
- **Integration tests:** synthetic install prefixes in a tempdir. They include a fake dpkg database for FR-015 and a fake ELF shared library for US3.
- **Goldens:** the corpus goldens regenerated in CI (R10).

**Target Platform**: Linux, macOS and Windows hosts, as today.
**Project Type**: CLI (single binary).
**Performance Goals**: Zero cost when no matching file exists. Otherwise a textual parse of files a few hundred bytes long. One risk is recorded: re-hashing large matched shared libraries for evidence occurrences (R9). It is measured in T030 and in SC-006's CI run.
**Constraints**: Offline. Nothing is evaluated: no CMake and no pkg-config code. Nothing outside the scan root is read. The output with no matching file is byte-identical.
**Scale/Scope**: 44 `.pc` and 78 CMake version files in the ROCm payload #1531 measured. 0 in the 20 git corpus targets, and 2 (both expected to be dpkg-owned) in `image-postgres16` (R10).

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, Statically Linked | Pass. No new crates, no subprocess. |
| III. Fail Closed | Pass. No version is guessed. An unresolved template, a computed CMake version, or an unresolved `Requires` produces no component and no edge, and it is counted in the coverage record. Ownership is never inferred in the reading direction: a file with no ownership data is read, never assumed owned. |
| IV. Type-Driven Correctness | Pass. The skip reason is a closed enum serialised in one place. The record kinds are an enum, and the prefix is computed once per record. |
| V. Specification Compliance | Pass. `pkg:generic` is used because purl-spec defines no pkg-config or CMake type. Names are percent-encoded with the shared encoder. Two new catalog rows have extractors in all three formats. The C55 catalog text gains two values. |
| VII. Test Isolation | Pass. Synthetic `waybill-fixture-*` prefixes are built in tempdirs. No ROCm payload is fetched locally; SC-006 is measured in CI. |
| VIII. Completeness | Pass. This closes #1531's gap of exact versions shipped next to binaries and never read. |
| IX. Accuracy | Pass. OS-owned files are skipped so that distro images gain no duplicate identities (FR-015). Merges happen only on identical identity, with no name mapping. Binaries are matched only within their own prefix, and an ambiguous match lists a binary under no library. |
| X. Transparency | Pass. The coverage record states every skip by reason in the document itself. The corpus measurement and its script are committed under `measurements/`. |

No violations, so Complexity Tracking is empty. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1531-pkgconfig-cmake-versions/
├── spec.md                       # amended during planning: R4, R9, R10 (marked inline)
├── plan.md
├── research.md                   # R1–R11
├── data-model.md
├── quickstart.md
├── contracts/wire.md             # identity, C55 values, the two new rows, edge rules
├── checklists/requirements.md
└── measurements/
    ├── corpus-pc-cmake.py        # R10 (GitHub tree API; no clone)
    ├── corpus-pc-cmake.tsv
    └── corpus-pc-cmake-control.tsv
```

### Source Code (repository root)

```text
waybill-cli/src/scan_fs/package_db/
├── installed_lib/                 # NEW reader
│   ├── mod.rs                     # registration (3 patterns), candidate collection, finalize → Vec<Record>
│   ├── pkgconfig.rs               # .pc parser: variables, continuation, fields, Requires grammar (R7)
│   ├── cmake_version.rs           # literal PACKAGE_VERSION reader (R7)
│   ├── location.rs                # recognised locations, multiarch triplets, prefix (R7)
│   └── resolve.rs                 # phase 2: ownership filter, Requires → exact PURL, entries, binary match, coverage (R2–R6, R9)
├── walk_registry/mod.rs           # ReaderId::INSTALLED_LIB + ALL_READER_IDS
├── mod.rs                         # register in run_shared_walker_pilot; SharedPilotOutput field; DbScanResult.os_claimed + OS-reader call sites (R3)
waybill-cli/src/scan_fs/mod.rs     # call the phase-2 post-pass after binary::read; splice ahead of binary entries (R4)
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs   # the two new rows
waybill-cli/src/generate/…        # coverage record (document scope) + private-requires (component) in each emitter
docs/reference/sbom-format-mapping.md   # C55 values; two new rows
waybill-cli/tests/installed_lib_*.rs    # integration tests (tempdir prefixes)
waybill-cli/tests/fixtures/public_corpus/image-postgres16/   # regenerated in CI (R10)
CHANGELOG.md
```

**Structure Decision**: A single project. There is one new reader module and one post-pass. There is one narrow change to shared plumbing: an OS-only claim set filled at the existing OS-reader call sites. It is byte-identical when no matching file is present.

## Approach

1. **Re-verify the research.** This follows the memory rule to re-verify research claims at implementation.
   - Check that the CMake reader emits nothing for a version file (R1).
   - Check that `deduplicate` still groups by name and version and keeps the first entry (R4).
   - Check that the last catalog row is still what R8 assumed. PR #1561 claims C223, so this feature expects C224 and C225.
2. **Shared plumbing, with a no-op control.**
   - Add the `os_claimed` set and its inserts at the dpkg, apk, alpm, opkg and rpm call sites.
   - Control: the full gate passes with zero golden churn before any reader code exists.
3. **The parsers, written test-first.** Start with the `.pc` parser (variables, cycles, continuation, `Requires` grammar), then the CMake version parser, then the location and prefix rules. Each is a pure function with unit tests taken from the spec's acceptance scenarios and edge cases.
4. **Phase 1**: registration and finalize.
5. **Phase 2**:
   - the ownership filter, with an OS-only set test that includes a pip `RECORD` control;
   - `Requires` resolution;
   - the entries, spliced ahead of the binary entries;
   - the binary match;
   - coverage.
6. **Rows.** Add the two catalog rows, the emitters and the extractors. The parity tests and `every_catalog_row_has_an_extractor` must pass.
7. **Mutations.** Each is reverted with `cp`, and each must fail a test:
   - read the shared claim set instead of the OS-only one (the wheel test must catch it);
   - skip variable expansion;
   - take a `Requires` comparison version as a module name;
   - splice after the binary entries (the zlib merge test must catch the tier and C55);
   - drop the prefix-first resolution;
   - accept a computed CMake version.
8. **Goldens.**
   - Locally, nothing should change.
   - In CI, dispatch the corpus regeneration and review a normalised diff. Only `image-postgres16`'s three files should move, each gaining a coverage record with `owned-by-os-package: 2` (R10).
9. **ROCm (SC-006).** A branch-only CI step, never local (R11).
10. **Docs and gate.** Update the catalog rows, the C55 values and the CHANGELOG, keeping the CHANGELOG entry away from any other unreleased entry. Then run `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`.

## Cataloger-interface notes (#1545)

This reader is generic: it covers two formats every installed C/C++ SDK shares. ROCm-specific installed-artifact knowledge belongs to the ROCm cataloger (#1541), such as its shipped manifest of source pins and its prefix-renamed bundled libraries.

Observed here for `CatalogOutput` (#1483): a cataloger needs to know which paths an OS package owns. Today that knowledge exists only inside `read_all`. This feature's OS-only claim set is the first form of it that could be handed to a cataloger.

## Complexity Tracking

None.
