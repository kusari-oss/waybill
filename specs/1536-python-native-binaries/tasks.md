# Tasks: Native binaries inside a Python package get binary analysis

**Input**: `specs/1536-python-native-binaries/` (spec, plan, research R1–R10, data-model, contracts/wire.md, quickstart).
**Tests**: required. SC-005 asks for a mutation per rule, and every acceptance scenario becomes a test.
**Fixtures**: synthetic `waybill-fixture-*` names only. Real wheels and TheRock run only in CI (FR-011, SC-004).

## Phase 1: Setup (measurement first)

- [X] T001 Add `measure-wheels.yml` to `kusari-sandbox/waybill-fixture-therock-sdk`, with inputs `waybill_ref` and `base_ref` (research R8).
  - **Wheels**, each pinned by version and SHA-256:
    - `rocm-sdk-libraries` from `https://stable.repo.amd.com/rocm/whl-next/`;
    - one NumPy `manylinux_x86_64` wheel and one NumPy `macosx_arm64` wheel from PyPI.
  - **Census of each wheel:**
    - native libraries under `<pkg>.libs/` and `<pkg>/.dylibs/`;
    - file names matching `-[0-9a-f]{8}\.so`;
    - `RECORD` entries outside the package directory;
    - sizes over the default `--max-file-size`.
  - **Baseline:** scan each wheel at `base_ref` with its `.dist-info`, and without it.

  Commit the script, the run ID and the census to `specs/1536-python-native-binaries/measurements/wheels.md`. Then confirm or correct research R6's suffix pattern and R8's NumPy layout claim before T015. Nothing is downloaded locally.
- [X] T002 [P] Extend `waybill-cli/tests/common/installed_lib.rs` with `elf_shared_lib_with_needed(rodata, needed: &[&str]) -> Vec<u8>`: a `.dynamic` and `.dynstr` that carry `DT_NEEDED` entries. Add a positive-control test in a new `waybill-cli/tests/python_owned_binaries.rs`. It scans an unclaimed tree holding the library (no `.dist-info`) and asserts:
  - the embedded zlib version identifies `pkg:generic/zlib@<v>`;
  - a linkage component exists for a `needed` entry.

  This is the control that the fixture can make each signal fire (memory: a zero result can mean broken).

  **Done as a Mach-O, not an ELF:** `macho_dylib(loads, cstring)`. The `object` crate reports an ELF import's library only through symbol versioning (`.gnu.version_r`), not `DT_NEEDED`, so a minimal ELF cannot carry linkage. A Mach-O names its libraries in `LC_LOAD_DYLIB`. Tests that need a Linux rootfs, which drops Mach-O, use `elf_shared_lib` for the version string alone.

## Phase 2: Foundational (blocks every story)

- [X] T003 Write unit tests in `waybill-cli/src/scan_fs/package_db/pip/dist_info.rs` for a new `RecordOwners` (data-model). Cover:
  - one `RECORD`, giving path → `.dist-info`;
  - two `RECORD`s listing one path, where the sorted-first `.dist-info` wins (V1, FR-010);
  - `../../bin/<tool>` entries;
  - lookup by raw path, canonical path, and (Unix) inode;
  - `files_by_owner`;
  - a `RECORD` line with an empty path is skipped;
  - a `RECORD` entry naming a missing file is skipped (spec edge case).
- [X] T004 Implement `RecordOwners` in `waybill-cli/src/scan_fs/package_db/pip/dist_info.rs`. Replace `collect_claimed_paths`'s insert into the shared set with a fill of `RecordOwners`, using `candidate_site_packages_roots` and sorted `.dist-info` iteration.
  - Add `record_owners: RecordOwners` to `DbScanResult` in `waybill-cli/src/scan_fs/package_db/mod.rs`, and remove the pip call's writes into `claimed`.
  - Update the test at `package_db/mod.rs:3398`: the pip claim is in `record_owners`, and in neither `claimed_paths` nor `os_claimed_paths`. Keep the subset assertion.

  Make T003 pass (research R1).
- [X] T005 In `waybill-cli/src/scan_fs/binary/mod.rs`, add a `&RecordOwners` parameter to `binary::read` and return `(Vec<PackageDbEntry>, Vec<OwnedBinary>)`.
  - **Owned:** a binary is owned when the shared set does not claim it and `RecordOwners` resolves it. For such a binary, set `path_claimed = false` and skip both collapsers. `rpm_dir_heuristic` and `go_in_linux` stay as they are (R3).
  - **Record:** collect, per owned binary, its file-level PURL, the linkage PURLs of its imports, its internal-link file names (soname base name ∈ the owner's `files_by_owner`), and whether a content-derived identification fired.
  - **Internal links:** sonames that are internal links are not passed to `LinkageAggregator`.

  Pass `&RecordOwners` to `go_binary`'s finalize (`waybill-cli/src/scan_fs/package_db/go_binary.rs:662`, `:739`). There an owned path counts as claimed for diagnostic suppression only (research R1).

  Define `OwnedBinary` in a new `waybill-cli/src/scan_fs/binary/owned.rs`. Update the call site in `waybill-cli/src/scan_fs/mod.rs:598` and every other caller (find them with `grep -rn "binary::read(" waybill-cli/`; count the whole output, no `head`).
- [X] T006 [P] Write emitter unit tests:
  - **`waybill-cli/src/generate/cyclonedx/builder.rs`:** a three-level `parent_purl` chain folds into nested `components[]`, and the nested `bom-ref`s are `<purl>#<parent bom-ref>`. The same PURL under two parents gives two distinct `bom-ref`s (V4).
  - **`waybill-cli/src/generate/cyclonedx/dependencies.rs`:** every `ref` and `dependsOn` value is a `bom-ref`. A target with two copies resolves to the copy under the source (V5).
  - **`waybill-cli/src/generate/spdx/relationships.rs`:** `CONTAINS` is emitted from a nested parent.
  - **`waybill-cli/src/generate/spdx/ids.rs`:** a nested component's identifier differs from a top-level component with the same PURL, and differs between two parents.
- [X] T007 Implement R5 to make T006 pass:
  - **`builder.rs`:** fold recursively, deepest first, with ancestor-chain `bom-ref`s.
  - **`dependencies.rs`:** map PURLs to `bom-ref`s.
  - **`spdx/relationships.rs`:** `CONTAINS` from any emitted parent.
  - **Identifiers:** nested SPDX 2.3 `SPDXID`s and SPDX 3 IRIs derive from the PURL plus the parent chain (`spdx/ids.rs`, `spdx/packages.rs`, `spdx/v3_packages.rs`, `spdx/v3_relationships.rs`).

  Then run the whole gate once, since every nested-component test in the repository exercises this.

**Checkpoint:** owned binaries are analysed, but still flat. The emitters can carry three levels.

## Phase 3: User Story 1, a vendored library identified under its package (P1, the MVP)

**Goal:** package → binary → identifications, with the edge chain, in both layouts.
**Independent test:** spec US1. The T002 library is listed in a `RECORD`, under an unpacked-wheel root and under `usr/lib/python3.12/site-packages/`.

- [X] T008 [US1] Write the integration tests first in `waybill-cli/tests/python_owned_binaries.rs`, for both layouts. Cover US1 scenarios 1–6:
  - **Nesting:** `pkg:pypi/waybill-fixture-pkg@1.2.3` → library → `pkg:generic/zlib@<v>`, and the root is unchanged.
  - **The same tree in `site-packages`.**
  - **Edges:** library → linkage component and library → zlib; package → library.
  - **Internal link:** two owned libraries, one `DT_NEEDED` on the other, give an edge to the other's component and no linkage component for it.
  - **A Python-only `RECORD`:** the output equals today's.
  - **An owned library with no identification:** it appears once, nested, and there is no `file`-tier component for its path.
  - **FR-001's other signals:** an owned library carrying an ELF `.note.package` gives a note component nested under it. An owned library whose strings hold a source path that spec 1534 attributes gives the attributed component nested under it. Reuse the builders in `waybill-cli/tests/common/installed_lib.rs` and `installed_lib_embedded_paths.rs`.

  Also cover:
  - the wire contract C-1 to C-4 in `contracts/wire.md`;
  - C230 `{"analysed":N,"identified":M}`, agreeing across the three formats, with a linkage-only library counted in `analysed` and not in `identified` (clarification 4);
  - FR-006: the two layouts' nested trees and edges compared, root normalised;
  - the CycloneDX output passing `tests/common/cdx_schema.rs`, and the SPDX 3 output passing `spdx3-validate` under `WAYBILL_REQUIRE_SPDX3_VALIDATOR`.
- [X] T009 [US1] Implement the attach pass in `waybill-cli/src/scan_fs/binary/owned.rs` (data-model "The attach pass", research R4):
  - resolve the owner PURL from the pip entry whose `source_path` is `<dist-info>/METADATA`;
  - set `parent_purl` and `depends` per the data-model table, covering `go_binary` entries whose `source_path` is an owned path;
  - return `OwnedBinaryCoverage`;
  - V2: an unresolved owner is analysed, not nested and not counted.

  Call it in `waybill-cli/src/scan_fs/mod.rs` after `binary::read` and the installed-library splice, and before the name index is built.

  Unit-test the pass in `owned.rs` with synthetic entries:
  - a `go_binary` module whose `source_path` is an owned path nests under the binary (spec edge case, Go build info);
  - a symbol-fingerprint entry nests the same way;
  - an entry from another path is untouched.

  **As built:** version-string, symbol-fingerprint, v2-matcher and ELF-note entries are nested by `binary::read` itself, when it emits them for an owned binary. Bundled trees and cargo-auditable crates already were. The attach pass nests only what another reader emits, which is `go_binary` modules. So fingerprint nesting is covered where it happens, by the same code path that T008's version-string case exercises, not by a unit test of the pass.
- [X] T010 [US1] Plumb C230 `waybill:python-owned-binaries` (the C228/C229 pattern):
  - `scan_fs::ScanResult` → `cli/scan_cmd.rs` → `generate::ScanArtifacts` → `generate/cyclonedx/{metadata,builder,mod}.rs` and `generate/spdx/{annotations,v3_annotations}.rs`, plus every test constructor of `ScanArtifacts`;
  - its parity extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`;
  - its row in `docs/reference/sbom-format-mapping.md`, in the same change (memory: the gate is in the lib target).

  Make T008 pass.
- [X] T011 [US1] Add edge-case tests to `waybill-cli/tests/python_owned_binaries.rs`:
  - **One library, two packages:** two packages each vendor a byte-identical library. There are two nested copies with distinct identifiers in all three formats, and the documents still pass the schema and `spdx3-validate` (V4).
  - **`../../bin/<tool>`:** a `RECORD` entry holding the cargo-auditable fixture from `closure_binary_contents.rs`. It is owned and nested, with its crates under it.
  - **One file in two `RECORD`s:** the owner is the sorted-first `.dist-info` (FR-010).
  - **No owner:** a `.dist-info` with a `RECORD` and unreadable `METADATA`. The library is analysed top-level, and C230 is absent (V2).
  - **A library that identifies its own package:** a version string naming `waybill-fixture-pkg`. It is nested under the binary, not merged into the `pkg:pypi` component (spec edge case).

**Checkpoint:** US1 is complete and independently demonstrable with `quickstart.md`.

## Phase 4: User Story 2, OS-owned binaries unchanged (P1)

**Goal:** dpkg, rpm, apk, alpm, opkg and `.ipk` claims keep today's output.
**Independent test:** spec US2, compared with `main` output.

- [X] T012 [P] [US2] Add tests to `waybill-cli/tests/python_owned_binaries.rs`:
  - **dpkg plus `RECORD`:** a rootfs whose dpkg `.list` (`common::installed_lib::dpkg_owning`) and a `site-packages` `RECORD` both list one library. Assert no file-level, linkage or identification components for it, no nesting, and no C230, exactly as for a dpkg-only claim (FR-005).
  - **`.ipk` plus `RECORD`:** the same, for an `.ipk` claim (FR-007).
  - **CPython `lib-dynload`:** the CPython collapse is unchanged.
  - **An owned Go binary whose build info doesn't parse:** no diagnostic `pkg:generic` component, only its nested file-level component (research R1).
  - **A binary the package doesn't own links a pip-owned library on a standard search path:** a linkage component is emitted (research R1, accepted).
- [X] T013 [US2] If T012 fails, fix it in the ownership decision in `waybill-cli/src/scan_fs/binary/mod.rs`, never in the tests. Record the outcome either way in `specs/1536-python-native-binaries/measurements/us2.md`.

## Phase 5: User Story 3, repaired-wheel layouts (P2)

**Goal:** auditwheel `<pkg>.libs/` and delocate `<pkg>/.dylibs/` are owned and analysed, and auditwheel's hash is stripped from names.
**Independent test:** spec US3, with synthetic wheels in each layout.

- [X] T014 [US3] Write tests first: unit tests in `waybill-cli/src/scan_fs/binary/entry.rs`, integration tests in `waybill-cli/tests/python_owned_binaries.rs`. Cover:
  - **The suffix pattern:** `libfoo-0123abcd.so.1` → `libfoo.so.1` under `waybill_fixture_pkg.libs/`.
  - **No strip outside `*.libs/`:** the same name elsewhere is not stripped.
  - **No strip on a non-hex or non-8-character suffix.**
  - **An auditwheel wheel:** `waybill_fixture_pkg.libs/libwaybill-fixture-z-0123abcd.so.1`, `DT_NEEDED` from an extension module in `waybill_fixture_pkg/`. It is owned, nested, internally linked and named without the hash.
  - **A delocate wheel:** `waybill_fixture_pkg/.dylibs/`. It is owned and nested.

  Use T001's census for the pattern, if it corrected R6.
- [X] T015 [US3] Implement the strip in the file-level name built in `waybill-cli/src/scan_fs/binary/entry.rs`. It applies only to owned binaries directly under a directory whose name ends in `.libs` (R6). The `file-sha256` qualifier is unchanged. Make T014 pass.

## Phase 6: Polish and cross-cutting

- [X] T016 [P] Docs:
  - in `docs/ecosystems.md`, a Python section entry on owned native binaries: nesting, edges, C230, the OS-claim rule, and the bare `lib/python3.X/site-packages` limit;
  - in `CHANGELOG.md`, `[Unreleased]` entries: a Fixed entry for the #1529 regression; an Added entry for the `site-packages` inventory and C230; and a Changed entry for nesting depth. That entry says that all nested components, cargo-auditable and shade jars included, now have CycloneDX `dependencies` refs that match their `bom-ref`, and that SPDX emits one element per PURL.
- [X] T017 Mutations, each reverted with `cp` and each required to fail a named test. Record them in `specs/1536-python-native-binaries/measurements/mutations.md`:
  - **M1:** the OS claim no longer wins;
  - **M2:** no `parent_purl` on the binary;
  - **M3:** no `parent_purl` on identifications;
  - **M4:** edges left to the root fallback;
  - **M5:** internal links fed to the aggregator;
  - **M6:** no suffix strip;
  - **M7:** the strip applied outside `.libs/`;
  - **M8:** `site-packages` owners dropped (layouts diverge);
  - **M9:** the binary also emitted top-level;
  - **M10:** linkage counted as an identification;
  - **M11:** the owner chosen by last insert.
  - **M12, M13** (added at implement time, for R5's revised SPDX and CycloneDX behaviour): one SPDX package per copy, and dependency refs not mapped to `bom-ref`s.
- [X] T018 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record it in `measurements/gate-log.md` with the positive signals counted (`test result: ok.` lines and `python_owned_binaries` N of N), with no `head`.
- [ ] T019 Regenerate the corpus goldens in CI (`public-corpus.yml`, `regen_goldens=true`). Diff them normalised and sorted, and explain every changed file by a `RECORD`-only claimed binary (SC-002, research R9). Record the result in `measurements/golden-review.md`.
- [X] T020 [P] Run the ROCm SDK fixture's `measure` workflow at `main` and at the branch. Assert byte-identical output (SC-003), and record it in `measurements/rocm-sdk.md`.
- [X] T021 [P] Run `measure-wheels.yml` (T001) with `waybill_ref` set to the branch. For each wheel, report:
  - owned binaries analysed and identified (C230);
  - the identifications and linkage that the no-`.dist-info` scan yields, all present and nested;
  - no binary reported twice;
  - wall time for both scans.

  Record SC-004 in `measurements/wheels.md`.
- [ ] T022 Open the PR. The body carries the measurements, the mutations, the gate result and the attribution line, ending `Closes #1567`.

## Dependencies

- **Setup:** T001 runs in CI, in parallel with everything until T015. T002 comes before T008.
- **Foundational:** T003 → T004 → T005; T006 → T007. The two chains are independent.
- **US1:** T008 → T009 → T010 → T011. It needs T005 and T007.
- **US2:** T012 → T013. It needs T005 only, and can run alongside US1.
- **US3:** T014 → T015. It needs T009 (nesting) and T001 (the pattern).
- **Polish:** T016 at any time after T010. T017 → T018 → T019. T020 and T021 run after T018, in parallel with T019. T022 comes last.

## Parallel examples

- T001 (CI census) alongside T002–T007.
- T003/T004 (claim split) alongside T006/T007 (emitters): different files.
- T012 (US2 tests) alongside T008–T011.
- T019, T020 and T021 run in CI together.

## Implementation strategy

1. **The MVP is US1 plus US2.** US1 fixes the regression; US2 proves it does not reopen OS double-counting. Both are P1 and ship together.
2. **US3** lands in the same PR. The two layouts are how most vendored native code reaches a wheel, and T001's census tells us whether R6's pattern holds on real data.
3. **Acceptance:**
   - T021 on the real wheels;
   - T019's corpus review;
   - T020's ROCm byte-identity, which shows nothing regressed where no `.dist-info` exists.
