# Feature Specification: Report declared third-party datasets as data components

**Feature Branch**: `1535-declared-datasets`
**Created**: 2026-11-16
**Status**: Draft
**Input**: Issue #1565. Report third-party datasets as data components (CycloneDX `type: data`; SPDX 3 `dataset_DatasetPackage`; SPDX 2.3 through a bridging row). Prefer declarations over guesses.

## Background

An installed SDK can ship third-party datasets: benchmark inputs, test matrices and sample data. They have an origin and a licence of their own, and both CycloneDX and SPDX 3 have a native component kind for them. waybill reports them at best as anonymous files. Data files are not a file-tier shape, so on the ROCm SDK they get no component at all.

### Measured (2026-11-16)

**Installed tree.** `kusari-sandbox/waybill-fixture-therock-sdk`, the listing of the pinned ROCm 7.14 SDK tarball.
- `clients/matrices/` holds 43 files, 1.49 GB: 24 sparse matrices in `.csr` and `.bin` encodings. The `.bin` files are 19 of the tree's 21, 0.632 GB.
- Nothing in the installed tree records where the matrices came from.

**Source tree.** `ROCm/rocm-libraries`, read through the GitHub API at the commit TheRock pins, `9811f2ea5235`. Nothing was cloned.
- `projects/rocsparse/cmake/ClientMatrices.cmake` and `rocsparse_clientmatrices.cmake` each declare two lists:
  - `TEST_MATRICES`, holding 24 SuiteSparse Matrix Collection coordinates (`Group/Name`, for example `Williams/mac_econ_fwd500`, `HB/nos1`);
  - `TEST_MD5HASH`, holding 24 archive digests.
- They then download each matrix:
  - an index loop, `foreach(i RANGE 0 ${len1})`, takes `list(GET TEST_MATRICES ${i} m)` and `list(GET TEST_MD5HASH ${i} md5)`;
  - it calls `file(DOWNLOAD https://sparse.tamu.edu/MM/${m}.tar.gz …)`, with a second URL as fallback;
  - it checks `file(MD5 …)` against the declared digest.
- **The declared names are exactly the 24 installed matrix stems.** hipSPARSE's `ClientMatrices.cmake` declares 19 of them.
- waybill's CMake reader does not read `file(DOWNLOAD)` today.

**What is not a dataset:** `*_gtest.data` (4 files, 1.45 GB) are test inputs generated for the projects' own test clients, not third-party data.

### Native carriers (Constitution Principle V)

- **CycloneDX 1.6:** component `type: "data"`.
- **SPDX 3.0.1:** `dataset_DatasetPackage`, a subclass of `software_Package`, with `software_primaryPurpose: "data"` and a required `dataset_datasetType`.
- **SPDX 2.3:** no data kind (`primaryPackagePurpose` has no `DATA`). `OTHER` plus a waybill annotation is the bridge. Research R4 found the existing C160 `waybill:component-kind` row can carry it with a new value.

## Clarifications

### Session 2026-11-16

- Q: Are datasets emitted from an installed tree without a declaration? → A: (A) No. Datasets come only from a declaration, read by a source-tree scan. An installed tree's data files are unchanged: no component is created from a directory name or file shape. Linking installed files to a declaration is out of scope.
- Q: What happens to a resolved download the dataset rule does not classify as data? → A: It is counted in the document's download coverage and emits nothing. Only datasets become components; a general `file(DOWNLOAD)` reader is #1518's scope.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A declared download list becomes data components (Priority: P1)

A consumer scans a source tree whose CMake files declare datasets to download. Each declared dataset is reported as a data component in all three formats, with:
- the identity the declaration gives (where it comes from);
- the declared digest;
- no version, because none is declared.

**Why this priority**: The declaration is the only record of where these datasets come from and what their bytes should be. On ROCm it names all 24 matrices. Nothing has to be guessed.

**Independent Test**: A synthetic CMake file in rocSPARSE's shape: two literal lists of three `waybill-fixture-*` entries, an index loop and a `file(DOWNLOAD https://example.invalid/MM/${m}.tar.gz …)`. Scan it and check:
- three data components in CycloneDX, SPDX 2.3 and SPDX 3;
- each with its URL and its MD5;
- a variant with a fourth URL variable the file does not define, which gives nothing and is counted.

**Acceptance Scenarios**:

1. **Given** a `file(DOWNLOAD <url>)` whose URL is a literal, **When** the tree is scanned, **Then** it is resolved but not classified as a dataset (research R2: only a list loop with a paired digest list is), so it is counted and emits nothing. On TheRock's pins these are three tool installers and one undigested data file.
2. **Given** a dataset download inside a loop over literal lists defined in the same file, where the URL uses only elements of those lists, **When** the tree is scanned, **Then** one data component is reported per list element, with the element substituted into the URL.
3. **Given** a digest list taken element by element in the same loop and checked with `file(MD5|SHA256 …)`, **When** the tree is scanned, **Then** each component carries the digest at its own index.
4. **Given** a URL using any variable not defined as a literal in the same file (`$ENV{…}`, a cache variable, a function argument), **When** the tree is scanned, **Then** that download yields no component, and it is counted as unresolved.
5. **Given** a fallback download (a second `file(DOWNLOAD)` of the same element after the first fails), **When** the tree is scanned, **Then** it adds no second component.

---

### User Story 2 - Data components are typed natively (Priority: P1)

Every data component this feature emits carries each format's native kind:
- CycloneDX `type: "data"`;
- SPDX 3 `dataset_DatasetPackage` with `software_primaryPurpose: "data"` and a `dataset_datasetType`;
- SPDX 2.3 `primaryPackagePurpose: OTHER` plus the bridging annotation.

**Why this priority**: Without it, a dataset is indistinguishable from a library to every consumer. That is the issue's whole point.

**Independent Test**: The US1 fixture, checked field by field in all three formats, and against the SPDX 3 schema and validator.

**Acceptance Scenarios**:

1. **Given** a declared dataset, **When** it is emitted, **Then** CycloneDX has `type: "data"`, and SPDX 3 has a `dataset_DatasetPackage` that passes the conformance validator.
2. **Given** SPDX 2.3, **When** the dataset is emitted, **Then** its package has `primaryPackagePurpose: OTHER` and the bridging annotation, and the parity extractors agree across formats.

---

### Edge Cases

- **Two files declare the same dataset.** rocSPARSE's two CMake files and hipSPARSE's declare overlapping matrices. One component per distinct identity, with every declaring file as evidence.
- **Mirror downloads** (`$ENV{…_TEST_MIRROR}/…`) are unresolved by FR-004 and do not duplicate the primary URL.
- **The lists have different lengths.** The digest is not attached to any element, and the case is counted in C229's `digest-mismatches`.
- **A download that is not data** (a source archive, a tool) via `file(DOWNLOAD)`. What is declared is a download, not a purpose. The plan must decide how a dataset download is told from any other, by measurement on the corpus. A download not classified as a dataset is counted and emits nothing (clarified). This feature MUST NOT re-type an existing FetchContent or ExternalProject component.
- **The test scope** (#1564). These matrices are test data. A dataset declared by a test client's CMake file is not test-scoped by this feature; #1564's rules apply to installed files.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CMake reader MUST read `file(DOWNLOAD <url> …)` commands. A URL is resolved only from:
  - literals;
  - elements of literal lists defined in the same file, taken element by element in a loop over those lists or over an index range up to their length.
- **FR-002**: Each resolved download that FR-008 classifies as a dataset MUST be reported as a data component. Its identity is the list element (for example `Williams/mac_econ_fwd500`). The first resolved URL and the paired digest are recorded on the component. It has no version unless one is declared. A fallback URL or a repeat declaration of the same element joins the same component.
- **FR-003**: A digest list paired element by element with the download list, and checked by `file(MD5|SHA1|SHA256|SHA512 …)` in the same loop, MUST be recorded on the component as its declared hash, with the algorithm.
- **FR-004**: A download whose URL uses anything outside FR-001's literals MUST yield no component, and MUST be counted.
- **FR-005**: The native kinds of US2 MUST be emitted. SPDX 2.3 MUST carry `primaryPackagePurpose: OTHER` plus C160 `waybill:component-kind: dataset`. C160 is an existing closed enum documented as extensible without new rows (research R4), so no new catalog row is needed for the kind.
- **FR-006**: The dataset type MUST be chosen from SPDX 3's `DatasetType` vocabulary by a documented rule. When the declaration does not support a specific value, it MUST be `noAssertion`.
- **FR-007**: The document MUST record the downloads read, the datasets emitted, the resolved downloads not classified as datasets, the unresolved downloads, and the digest mismatches.
- **FR-008**: A download MUST be classified as a dataset only by a rule the plan measures on the ROCm source and the public corpus. The rule MUST NOT classify any download the corpus shows to be source or tooling. A resolved download it does not classify emits nothing and is counted (clarified): a misclassification is a gap, never a wrong type.
- **FR-009**: A data component MUST come only from a declaration. An installed tree's data files without one (for example the ROCm install's `clients/matrices/`) MUST be unchanged: no component from a directory name or file shape (clarified).
- **FR-010**: Zero new Cargo dependencies, no network access at scan time, synthetic `waybill-fixture-*` names in committed fixtures, and ROCm measured in CI only.

### Key Entities

- **Declared download**: a `file(DOWNLOAD)` command with its URL template, its loop context and its paired digest list.
- **Dataset component**: identity from the declared list element, with the first resolved URL and the declared digest recorded; no version; native data kind.
- **Download coverage**: the document-scope counts (FR-007).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In CI, at `rocm-libraries@9811f2ea5235` (sparse-checked-out):
  - a scan rooted at `projects/rocsparse` reports **24** dataset components, matching the 24 installed matrix stems one-to-one, each with its `sparse.tamu.edu` URL and declared MD5;
  - a scan rooted at `projects/hipsparse` reports 19, all among those 24.

  Separate roots are needed because of the reader's discovery limit (research R7, #1516).
- **SC-002**: SPDX 3 output with datasets passes the conformance validator, and CycloneDX passes its schema.
- **SC-003**: Across the public corpus, every golden that changes is explained. A component that becomes a dataset, or a new one, is justified by its declaration.
- **SC-004**: Each rule (literal URL, list substitution, digest pairing, unresolved variable, fallback de-duplication, dataset classification) has a mutation that fails a test.

## Assumptions

- The ROCm source is read in CI only, through a sparse checkout of the pinned `rocm-libraries` commit. TheRock is never cloned locally.
- A dataset has no version unless one is declared. A SuiteSparse URL carries none.
- The SuiteSparse coordinates (`Group/Name`) are the upstream identity. A PURL type for them does not exist, so the plan chooses the representation, for example `pkg:generic` with `download_url` and `checksum` qualifiers.
- Licensing of the matrices is out of scope. The declaration states none.
- Linking installed data files to a declaration seen in another scan is out of scope (clarified); cross-tier binding (milestone 072) is the existing route if it is ever wanted.
