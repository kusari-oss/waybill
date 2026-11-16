# Tasks: Attribute installed binaries to their source through embedded build and source paths

**Input**: `specs/1534-embedded-path-attribution/` (spec, plan, research R1–R7, data-model, contracts/wire.md, quickstart).
**Tests**: required. SC-005 asks for a mutation per rule, and every acceptance scenario becomes an integration test.

## Phase 1: Setup (measurement first)

- [x] T001 Add the census workflow and scripts to the fixture repository (`kusari-sandbox/waybill-fixture-therock-sdk` `census-binaries.yml`, `census/*`). Runs 43784750746, 43786377987 and 43786608413.
- [x] T002 Commit the census inputs and outputs in `specs/1534-embedded-path-attribution/measurements/`: the scripts, `unattributed-binaries.txt`, `versioned-names.txt`, `census.json` and `attribution.json` (research R1–R4).

## Phase 2: Foundational (blocks both stories)

- [x] T003 Write the unit tests first, in a new `waybill-cli/src/scan_fs/binary/embedded_paths.rs`. Each test feeds a NUL-separated byte region into `extract(region, &names) -> EmbeddedPathFinding`. Cover:
  - compiled vs header extensions;
  - the toolchain prefixes `/usr/`, `/builddir/`, `/opt/`;
  - each vendoring marker (`third_party`, `third-party`, `thirdparty`, `3rdparty`, `external`, `extern`, `vendor`) and `_deps/<name>-src`;
  - the outermost match (`/src/rocthrust-x/testing/hip/t.cu` with names {`hip`, `rocthrust-x`} gives `rocthrust-x`);
  - the stoplist (`amd`) and the exclusion list (`sysdeps`);
  - a `:123` line suffix;
  - non-UTF-8 bytes;
  - an empty region;
  - `has_paths` and `toolchain_only`.

  Use only synthetic `waybill-fixture-*` names.
- [x] T004 Implement `embedded_paths.rs`:
  - `EmbeddedPathFinding { packages: BTreeSet<String>, header_only: bool, bundled: BTreeSet<String>, has_paths: bool, toolchain_only: bool }`;
  - `STOPLIST = ["amd"]` and `BUNDLED_EXCLUSIONS = ["sysdeps"]`, each with a comment citing research R2/R4;
  - a byte-level scanner (split on NUL, then walk path components), with no regex.

  Make T003 pass.
- [x] T005 Add `pub(crate) fn versioned_names(phase1: &Phase1, scan_root: &Path, claims: &OsClaims<'_>) -> BTreeSet<String>` to `waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs`. It returns the lowercased names of records with a recognised prefix, an `Ok` version and no OS owner, minus the stoplist. Unit-test that it equals the set of names phase 2 emits for a mixed fixture.
- [x] T006 Run the extraction in `waybill-cli/src/scan_fs/binary/scan.rs::scan_binary`:
  - call `embedded_paths::extract` on the existing `string_region`, always, with the (possibly empty) names set;
  - carry the finding on `BinaryScan`.

  Then, in `waybill-cli/src/scan_fs/binary/mod.rs`, thread an optional `(&BTreeSet<String>, &mut BTreeMap<String, EmbeddedPathFinding>)` through `read()`, keyed by the binary file component's `source_path`. Follow the #1524 `test_inputs` out-parameter. Update every caller of `read()`.
- [x] T007 In `waybill-cli/src/scan_fs/mod.rs`:
  - compute `versioned_names` before `binary::read` (beside the #1564 recognition);
  - pass names and a findings map into `binary::read`;
  - pass the findings into `installed_lib::resolve::resolve`.

  Extraction always runs: bundled detection needs no names (analysis I1). With no versioned names, pass an empty set, so attribution matches nothing.

**Checkpoint:** the gate is green and the output byte-identical, because nothing consumes the findings yet.

## Phase 3: User Story 1, attribution to a known package (P1, the MVP)

**Goal:** a binary whose compiled-source paths name versioned package(s) joins their evidence.
**Independent test:** spec US1. Synthetic `waybill-fixture-p` and `waybill-fixture-q`, with five binaries.

- [x] T008 [US1] Write the integration tests first, in a new `waybill-cli/tests/installed_lib_embedded_paths.rs`. Use `common::installed_lib` helpers (`write`, `elf_shared_lib`, `scan`, `component`, `source_files`, `row_all_formats`, `coverage`). Test US1 scenarios 1–6, each in CycloneDX, SPDX 2.3 and SPDX 3:
  - one package;
  - shared, with `shared-with` in C227;
  - header-only, not attributed;
  - toolchain-only, not attributed;
  - already attributed through exported targets, not re-attributed;
  - test-scoped (#1564), attributed with the scope unchanged.

  Plus:
  - the control: the same binaries without paths, nothing attributed and C225 byte-identical;
  - an OS-owned binary, not read;
  - the exact C225 `embedded-paths` object.
- [x] T009 [US1] Implement attribution in `waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs`:
  - skip binaries whose path is already in any live record's evidence (#1531 name match, #1563 exported targets);
  - for each name in `packages`, find the live record (CMake version record first, then a same-name `.pc`) and push the binary's absolute path to its evidence;
  - build C227 per record (sorted `{"path"}` / `{"path","shared-with"}`) into `extra_annotations` under `waybill:embedded-path-binaries`;
  - count `ExportedTargetsCoverage`-style into a new `EmbeddedPathsCoverage`, serialised as C225's `embedded-paths` object only when `read` > 0.

  Make T008 pass.
- [x] T010 [US1] Add C227's parity extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` (`component` scope, `SymmetricEqual`). In the same change, add the C227 row to `docs/reference/sbom-format-mapping.md` and extend C225's row text with `embedded-paths`. The lib-target gate requires the row and its extractor together.

**Checkpoint:** US1 is complete and shippable.

## Phase 4: User Story 2, bundled third-party code (P2)

**Goal:** each bundled tree a binary names becomes a versionless component nested under that binary.
**Independent test:** spec US2. A synthetic binary naming `third_party/waybill-fixture-fmt/` and `third_party/sysdeps/`.

- [x] T011 [US2] Write the integration tests first, in `waybill-cli/tests/installed_lib_embedded_paths.rs`, for US2 scenarios 1–3:
  - `waybill-fixture-fmt` is nested, with no version, `waybill:evidence-kind: embedded-source-path`, CycloneDX nesting, and SPDX 2.3/3 `CONTAINS`/`contains`;
  - `sysdeps` gives nothing;
  - two binaries naming the same tree give two components, one under each;
  - C228 `waybill:bundled-source-trees` counts them, including in a tree with no installed-library components (no C225).
- [x] T012 [US2] Implement bundled components:
  - in `waybill-cli/src/scan_fs/binary/entry.rs`, add a constructor in the symbol-fingerprint entry's shape (tier `analyzed`, confidence `heuristic`, version absent, `parent_purl` = the binary file component's PURL, `source_path` = the binary);
  - emit them from `binary::read` for each finding's `bundled`;
  - register `embedded-source-path` in `BINARY_EVIDENCE_KINDS` in `waybill-cli/src/scan_fs/binary/mod.rs`, and add the value to the evidence-kind row in `docs/reference/sbom-format-mapping.md`;
  - emit C228 at document scope in all three formats (the C226 pattern), with its parity extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` and its catalog row in `docs/reference/sbom-format-mapping.md`, in the same change.

  Make T011 pass.

## Phase 5: Polish and cross-cutting

- [ ] T013 [P] Docs:
  - `docs/ecosystems.md`, installed C/C++ libraries section: an "embedded source paths" bullet;
  - `CHANGELOG.md` `[Unreleased]` Added.
- [ ] T014 Mutations, each reverted with `cp` and each required to fail a test. Record them in `specs/1534-embedded-path-attribution/measurements/mutations.md`:
  - **M1:** any directory, not the outermost;
  - **M2:** an empty stoplist;
  - **M3:** headers count;
  - **M4:** no toolchain exclusion;
  - **M5:** no already-attributed exclusion;
  - **M6:** no shared marking (first name only);
  - **M7:** an empty bundled exclusion list;
  - **M8:** a version taken from the path, by setting the bundled component's version from the directory name.
- [ ] T015 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record it in `measurements/gate-log.md`, with the positive signals counted.
- [ ] T016 Regenerate the corpus goldens in CI (`public-corpus.yml`, `regen_goldens=true`). Byte-compare every file, and record the result in `measurements/golden-review.md` against plan step 9's prediction. Explain every bundled component in a golden by the path its binary names.
- [ ] T017 Run the ROCm measurement in CI. In the fixture repo's `measure-full.yml`, report:
  - C225 `embedded-paths`;
  - the attributed binaries per package, with bytes;
  - the bundled components;
  - the SC-002 check: an independent script compares each attribution in C227 with `measurements/attribution.json`'s per-file `outermost` names, and lists any disagreement (analysis U1);
  - the timing.

  Run it against `main`. Record SC-001, SC-002, SC-004 and SC-006 in `measurements/therock-full.md`.
- [ ] T018 Open the PR. The body carries the measurements, the mutations, the gate result and the attribution line. `Closes #1532`.

## Dependencies

- **Setup** is done.
- **Foundational:** T003 → T004 (`embedded_paths.rs`) → T006. T005 runs alongside T003 and T004 (different file), and T007 needs T005 and T006.
- **US1:** T008 → T009 → T010. It needs Phase 2.
- **US2:** T011 → T012. It needs Phase 2. It is independent of US1, except that the two share the test file, so run it after T008.
- **Polish:** T013 at any time after T009. Then T014 → T015 → T016 and T017 → T018.

## Parallel examples

- T005 (`resolve.rs`) alongside T003 and T004 (`embedded_paths.rs`).
- T013 (docs) alongside T014 (mutations).

## Implementation strategy

1. **The MVP is US1.** It is 258 binaries, 1.308 GB on ROCm, and invents nothing.
2. **US2** adds bundled-library inventory, about 13 distinct libraries across 52 binaries on ROCm.
3. **Measure in CI before the PR** (T016, T017). A count that disagrees with `measurements/attribution.json` is investigated, not explained away.
