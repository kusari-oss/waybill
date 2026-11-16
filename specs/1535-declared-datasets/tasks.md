# Tasks: Report declared third-party datasets as data components

**Input**: `specs/1535-declared-datasets/` (spec, plan, research R1–R7, data-model, contracts/wire.md, quickstart).
**Tests**: required. SC-004 asks for a mutation per rule, and every acceptance scenario becomes a test.

## Phase 1: Setup (measurement first)

- [x] T001 Run the CMake download census over TheRock's pinned submodules in the fixture repo (`census-downloads.yml`, `census/downloads.py`, run 43800233978). Commit its script and output in `specs/1535-declared-datasets/measurements/` (research R1, R2).
- [x] T002 Commit the three ROCm declaration files at `rocm-libraries@9811f2ea5235`, and the stem comparison with the installed matrices, in `specs/1535-declared-datasets/measurements/` (spec Background).

## Phase 2: Foundational (blocks both stories)

- [x] T003 Write the unit tests first, in a new `waybill-cli/src/scan_fs/package_db/cmake_download.rs`, against `parse_downloads(content: &str, file: &str) -> Vec<DeclaredDownload>`. Use synthetic `waybill-fixture-*` names in the measured shapes (research R1). Cover:
  - a literal URL;
  - `foreach(m IN LISTS L)` with `file(DOWNLOAD https://…/${m}.tar.gz …)`;
  - `foreach(i RANGE 0 ${n})` with `list(GET L ${i} m)` and `list(GET H ${i} md5)` plus `file(MD5 …)`, the rocSPARSE shape;
  - a fallback second `file(DOWNLOAD)` of the same element;
  - `$ENV{…}` and a cache or function-argument variable, which are `Unresolved`;
  - lists of unequal length, which give no digest;
  - comments;
  - a truncated file.
- [x] T004 Implement `cmake_download.rs`:
  - `DeclaredDownload { file, url_template, resolution: Resolution, digests: Option<Vec<(HashAlgorithm, String)>> }`, with `Resolution::{Literal(String), ListLoop { list: String, elements: Vec<String> }, Unresolved}`;
  - `classify(&DeclaredDownload) -> Classification::{Dataset, NotClassified, Unresolved}` (research R2);
  - `datasets(downloads) -> (Vec<DatasetDecl>, DownloadCoverage)`. It de-duplicates by element (R3), keeps the first URL and digest, and on a digest disagreement drops the digest and counts it.

  Tokenize with `installed_lib::cmake_version::{commands, strip_comments}`, which handle quoting, bracket arguments (spec 1533) and comments. Make T003 pass.

## Phase 3: User Story 1, declared download lists become data components (P1, the MVP)

**Goal:** a rocSPARSE-shaped declaration gives one dataset component per element, with URL and digest.
**Independent test:** spec US1. A synthetic tree with one `cmake/` file holding three elements, a primary URL and a fallback URL.

- [x] T005 [US1] Write the integration tests first, in a new `waybill-cli/tests/cmake_declared_datasets.rs`. Write the CMake file under `cmake/` (the reader's discovery, research R7). Test US1 scenarios 1–5:
  - one component per element, PURL `pkg:generic/<ns>/<name>?download_url=…&checksum=md5:…`, no version;
  - the fallback de-duplicated;
  - `$ENV` unresolved and counted;
  - two declaring files of one element giving one component with both as evidence;
  - a literal tool download, `NotClassified`, counted and not emitted (US1 scenario 1);
  - lists of unequal length, counted in `digest-mismatches`;
  - the exact C229 object.
- [x] T006 [US1] In `waybill-cli/src/scan_fs/package_db/cmake.rs::read`, call `cmake_download::parse_downloads` on each discovered file. Emit one `PackageDbEntry` per `DatasetDecl` (data-model "Dataset"):
  - tier `source`;
  - evidence: the declaring files;
  - `waybill:component-kind: dataset`;
  - hash from the digest.

  Accumulate `DownloadCoverage`.
- [x] T007 [US1] Plumb C229 `waybill:declared-downloads` (the C228 pattern):
  - `scan_fs::ScanResult` → `cli/scan_cmd.rs` → `generate::ScanArtifacts` → `generate/cyclonedx/{metadata,builder,mod}.rs` and `generate/spdx/{annotations,v3_annotations}.rs`, and the five test constructors;
  - add its parity extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` and its row in `docs/reference/sbom-format-mapping.md` in the same change.

  Make T005 pass.

**Checkpoint:** datasets are emitted and counted, so far as ordinary components.

## Phase 4: User Story 2, native data kinds (P1)

**Goal:** each format carries the dataset as its native kind.
**Independent test:** spec US2. The US1 fixture, checked field by field and against the SPDX 3 validator and the CycloneDX schema.

- [x] T008 [US2] Add tests to `waybill-cli/tests/cmake_declared_datasets.rs`:
  - CycloneDX `type: "data"`;
  - SPDX 3 element `type: dataset_DatasetPackage`, `software_primaryPurpose: "data"`, `dataset_datasetType: ["noAssertion"]`;
  - SPDX 2.3 `primaryPackagePurpose: OTHER`;
  - C160 `dataset` agreeing across formats;
  - the CycloneDX output passing the shared validator (`tests/common/cdx_schema.rs`);
  - the SPDX 3 output passing `spdx3-validate`, under the existing `WAYBILL_REQUIRE_SPDX3_VALIDATOR` convention.
- [x] T009 [US2] Implement the native kinds for a component whose `waybill:component-kind` is `dataset`:
  - in `waybill-cli/src/generate/cyclonedx/builder.rs`, `type: "data"`;
  - in `waybill-cli/src/generate/spdx/packages.rs`, `primaryPackagePurpose: OTHER`;
  - in `waybill-cli/src/generate/spdx/v3_packages.rs`, element type `dataset_DatasetPackage`, `software_primaryPurpose: "data"` and `dataset_datasetType: ["noAssertion"]`.

  Extend C160's row text in `docs/reference/sbom-format-mapping.md` with the `dataset` value. Make T008 pass.

## Phase 5: Polish and cross-cutting

- [x] T010 [P] Docs:
  - in `docs/ecosystems.md`, a CMake section bullet on declared datasets, noting the #1516 discovery limit;
  - in `CHANGELOG.md`, an `[Unreleased]` Added entry.
- [ ] T011 Mutations, each reverted with `cp` and each required to fail a test. Record them in `specs/1535-declared-datasets/measurements/mutations.md`:
  - **M1:** classify without a paired digest list;
  - **M2:** accept `$ENV` in a URL;
  - **M3:** no fallback de-duplication;
  - **M4:** identity by URL, not element;
  - **M5:** CycloneDX type `library`, not `data`;
  - **M6:** a version taken from the element;
  - **M7:** keep a digest on disagreement.
- [ ] T012 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record it in `measurements/gate-log.md`, with the positive signals counted.
- [ ] T013 Regenerate the corpus goldens in CI (`public-corpus.yml`, `regen_goldens=true`). Byte-compare every file, and record the result in `measurements/golden-review.md`. Byte-identity is predicted: no CMake target (research R2).
- [ ] T014 Measure the ROCm source in CI. Extend the fixture repo's `census-downloads.yml` to build waybill at a given ref and scan `src/rocm-libraries/projects/rocsparse` and `projects/hipsparse`. Report the dataset components (name, URL, checksum) and C229, then compare them with the 24 installed stems (`measurements/README.md`). Record SC-001 in `measurements/rocm-source.md`.
- [ ] T015 Open the PR. The body carries the measurements, the mutations, the gate result and the attribution line. `Closes #1565`.

## Dependencies

- **Setup** is done.
- **Foundational:** T003 → T004.
- **US1:** T005 → T006 → T007. It needs Phase 2.
- **US2:** T008 → T009. It needs T006, which emits the components.
- **Polish:** T010 at any time after T007. Then T011 → T012 → T013 and T014 → T015.

## Parallel examples

- T010 (docs) alongside T011 (mutations).
- T013 (corpus) and T014 (ROCm source) run in CI together.

## Implementation strategy

1. **US1** gives the datasets and their counts. **US2** makes them datasets to consumers, so both are P1 and ship together.
2. The ROCm-source run (T014) is the acceptance test: 24 components from rocSPARSE's declarations, one-to-one with the installed matrices.
