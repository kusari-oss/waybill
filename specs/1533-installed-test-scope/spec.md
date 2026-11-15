# Feature Specification: Report an installed tree's test and benchmark payload with a test scope

**Feature Branch**: `1533-installed-test-scope`
**Created**: 2026-11-15
**Status**: Draft
**Input**: Issue #1564. Report an installed tree's test and benchmark payload with a test scope. Prefer declarations over path and name guesses.

## Background

An installed SDK can ship a large test and benchmark payload alongside its product. Today waybill reports that payload exactly like the libraries it tests: a gtest executable and the shared library it exercises are indistinguishable in the SBOM. A consumer counting what ships, or triaging a finding, cannot tell them apart.

#1524 made a directory scan *skip* binaries under directories named exactly `test`, `tests`, `unittests` or `Inputs`, because a toolchain's source tree keeps fixtures there (core dumps, corrupt objects). It left image scans alone, because an installed system's tests are not fixtures. They are still not product. **Decision (#1564):** in an installed tree, that payload is reported, and it is reported with a test scope.

### Measured on a ROCm SDK install tree

**Fixture:** the public `kusari-sandbox/waybill-fixture-therock-sdk`, a listing of every member of `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz` (16.55 GB, 38,694 files unpacked). `measure-full` scans the extracted tree in CI with `--path`, not `--image`. **So an installed tree is not always an image scan.**

**#1564's census:** 6.09 GB (36.8%) of the tree is test, benchmark and example payload:
- 510 test, benchmark and tuning executables in `bin/` (2.54 GB);
- 6 `*_gtest.data` blobs (1.56 GB). Re-read from the listing on 2026-11-15, the regular files matching `*_gtest.data` are 4, all in `bin/`, totalling 1.45 GB. The census's figure is not reproduced, and research re-derives every bucket with a committed script;
- 43 sparse matrices under `clients/matrices/` (1.49 GB);
- 13,629 test-suite files (0.30 GB);
- 140 examples and samples (0.20 GB).

Those buckets came from the census script's path and name rules. They are not a proposed classifier.

**What declares itself test payload** (from the listing, 2026-11-15):
- **CMake exported targets (#1563).** One package declares itself a test package: `rocprofiler-sdk-tests`, which names 0.188 GB. Almost all of it lies under `share/rocprofiler-sdk/tests/` (698 files, 0.191 GB), so the declaration and the path convention agree there.
- **Installed CTest files.** 28 `CTestTestfile.cmake` files ship, for example `bin/rocblas/`, `bin/rocsparse/`, `bin/rocprim/` and `share/hip/` (whose `catch_tests/` holds 254 files, 0.514 GB). Most sit alone in a per-package directory under `bin/`, so the executables they declare are elsewhere. Measured after the capture was extended (research R1): 1,153 of their 1,665 `add_test` commands name an executable by a relative path written for the installed location, together naming 432 shipped files (2.63 GB). 474 use variables set from the environment. HIP's `catch_tests` declares its tests only through 54 `include()`d discovery scripts that run each executable at `ctest` time.

**What path conventions alone reach:** 41 directories named `test`, `tests`, `test_plugins`, `.upstream-tests` or `unittests` hold 0.4 GB. The largest is `share/rocprofiler-sdk/tests` (0.191 GB). So path conventions alone miss most of the payload.

**What names alone would reach,** among the 809 regular files directly under `bin/`:
- 418 contain `test` (including the four largest `*_gtest.data` blobs, 1.45 GB);
- 95 contain `bench`;
- 1 contains `tune` (`rocblas-gemm-tune`).

The `bench` names fall into two shapes. 82 are `benchmark_<op>` executables from the primitive libraries. 13 are `<lib>-bench` drivers (`rocblas-bench`, `hipblaslt-bench`, `rocfft-bench`, …), which users run against their own hardware. Whether those 13 drivers are test payload or a shipped tool is exactly the question #1564 raised.

### Native carriers (Constitution Principle V)

- **CycloneDX 1.6:** the component's `scope: "excluded"`, which the schema describes as "for test and other non-runtime purposes". C42 `waybill:lifecycle-scope: test` carries the finer distinction. This is the shape test dependencies already take.
- **SPDX 3.0.1:** `software_primaryPurpose: "test"` on the element. `test` is in the `SoftwarePurpose` vocabulary. C42 as well.
- **SPDX 2.3:** `primaryPackagePurpose` has no `TEST` value, so there is no element-level carrier. C42 is the bridge, as it already is for test dependencies.
- **Provenance:** C62 `waybill:lifecycle-scope-derivation`, an open enum, records *why* a component was scoped: which declaration or rule. No new catalog row is needed for the scope itself.

## Clarifications

### Session 2026-11-15

- Q: In a `--path` scan, where do the path-convention and name rules apply? → A: Only in an installed tree: every image scan, and a `--path` scan that is recognised as an installed tree (FR-015). Declarations apply wherever they are found.
- Q: Is a `--path` scan of an extracted rootfs an installed tree when an OS package database was read? → A: Yes. Reading a dpkg, rpm, apk, alpm or opkg database is recognition evidence alongside #1531 records (FR-015). Its own files stay excluded by FR-013.
- Q: In SPDX 3, what does a test-scoped element carry when its binary role would set `software_primaryPurpose`? → A: `software_primaryPurpose: "test"`, with the role (`application`, `library`, `file`) moved to `software_additionalPurpose`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A declared test package's files are test-scoped (Priority: P1)

A consumer scans an installed tree containing a CMake package that declares itself a test package (its export set is a `*-tests` package), and a package whose installed CTest file declares test executables. Every component for a file those declarations name carries the test scope in all three formats, with the declaration recorded as the reason.

**Why this priority**: A declaration is the strongest evidence available and cannot misclassify a product file the package did not itself name as a test. It is the #1032 principle: prefer what the project says about itself.

**Independent Test**: A synthetic prefix with a `waybill-fixture-p-tests` export set naming two executables, a CTest file declaring a third, and a product library alongside. Scan, and check that exactly the three named files' components are test-scoped in CycloneDX, SPDX 2.3 and SPDX 3, and that the library is not.

**Acceptance Scenarios**:

1. **Given** an exported-targets file whose package name ends in `-tests` or `_tests`, **When** the tree is scanned, **Then** each shipped file it names is reported with CycloneDX `scope: "excluded"`, C42 `test` in every format, SPDX 3 `software_primaryPurpose: "test"`, and a C62 value naming the exported-targets declaration.
2. **Given** an installed `CTestTestfile.cmake` whose `add_test` names an executable that resolves inside the scan root, **When** the tree is scanned, **Then** that executable's component is test-scoped, with a C62 value naming the CTest declaration.
3. **Given** a CTest command that names an interpreter (`python3`, `bash`, `sh`) with a script argument, **When** the tree is scanned, **Then** the script, if it ships, is the test. The interpreter is never scoped.
4. **Given** a location that is not shipped, escapes the scan root, or uses a variable other than the ones the file itself defines, **When** the tree is scanned, **Then** nothing is scoped through it, and it is counted.

---

### User Story 2 - Well-established test directories are test-scoped (Priority: P2)

A consumer scans an installed tree whose test suites live under conventional directories (`tests/`, `test/`, `test_plugins/`, `.upstream-tests/`, `unittests/`) with no declaration covering them. Each component for a file in such a directory is test-scoped, with the path convention recorded as the reason.

**Why this priority**: The conventions are near-universal and the directories are not product. But a path is weaker evidence than a declaration, and it reaches only about 0.4 GB of the ROCm tree.

**Independent Test**: A synthetic prefix with `share/waybill-fixture-q/tests/`, `lib/test_plugins/` and `libfoo/.upstream-tests/`, each holding an executable and a data file, plus a product file in a sibling directory named `testing-utils/` that must not match.

**Acceptance Scenarios**:

1. **Given** a file whose path below the scan root passes through a directory named exactly one of the conventional names, **When** the tree is scanned, **Then** its component is test-scoped, with a C62 value naming the path convention.
2. **Given** a directory whose name only contains a conventional name (`testing`, `attestation`, `contest`), **When** the tree is scanned, **Then** nothing under it is scoped by this rule.
3. **Given** a file covered by both a declaration and a path convention, **When** the tree is scanned, **Then** the declaration is the recorded reason.

---

### User Story 3 - Name patterns, only where measured safe (Priority: P3)

A consumer scans an installed tree whose test executables sit directly in `bin/` beside its tools, with no declaration. A name pattern would scope them only if the measurement showed it matches nothing that ships as a product. **Outcome (research R4):** no candidate passed FR-009, so no name pattern is adopted. Name-only files stay unscoped, and the measurement is committed for a later feature.

**Why this priority**: Names reach the largest share of the payload (`bin/*test*`, `benchmark_*`, `*_gtest*`), but they are guesses. `rocblas-bench` is arguably a shipped tool. A pattern that misclassifies one product file under-reports what ships, which is worse than leaving test payload unscoped.

**Independent Test**: A synthetic `bin/` with `waybill-fixture-r-test`, `test_waybill_fixture_op`, `benchmark_waybill_fixture_op`, `waybill-fixture-r-bench` and `waybill-fixture-attest` (a product tool whose name contains `test`). Only the patterns adopted by FR-009 scope anything, and `waybill-fixture-attest` is never scoped.

**Acceptance Scenarios**:

1. **Given** an undeclared executable outside any test directory whose name matches a candidate pattern (`test_*`, `benchmark_*`, `*-bench`), **When** an installed tree is scanned, **Then** it is not scoped.
2. **Given** a name that merely contains `test` inside another word, **When** the tree is scanned, **Then** it is not scoped.
3. **Given** a candidate pattern, **When** the feature ships, **Then** its ROCm measurement and the reason it was not adopted are committed with the spec.

---

### Edge Cases

- **Tests discovered at run time.** A CTest file that declares its tests through `include()`d discovery scripts (Catch2 or GoogleTest discovery, as in HIP's `catch_tests`) names no executable without evaluating CMake. Each `include()` is counted as an unresolved location and nothing is scoped through it.
- **A product library used only by test executables.** `librocblas.so` may be linked only by `rocblas-test` and `rocblas-bench` within the tree. It must not inherit the test scope through the dependency graph. Scope never flows from a test-scoped component to what it links.
- **A test-scoped component nested under, or deduplicated with, a package component.** #1563 lists the files a package installed as evidence on its versioned component. A test file named by the `rocprofiler-sdk-tests` export set is evidence of *that* package's component, which is itself test-scoped. A product package's component is never scoped because one of its evidence files is a test.
- **Data files.** `*_gtest.data` and sparse matrices are not executables. Under a test convention, or named by a declaration, their file-tier components are scoped like any other. Three of the four `*_gtest.data` files (109 MB to 887 MB) exceed the 100 MB file-inventory size limit, so no component exists to scope them. That is unchanged, and typing data files is #1565.
- **A source tree with `tests/`.** A `--path` scan not recognised as an installed tree applies no path or name rule, so its `tests/` files are unscoped, as today. Only a declaration found there would scope anything.
- **`--path` scan of an installed tree containing `tests/`.** #1524 still skips binaries under `test`, `tests`, `unittests` and `Inputs` in a directory scan, unless `--include-test-binaries` is given. With that flag they are reported, and they carry the test scope (#1564's third question).
- **An OS-package-owned file.** A file a dpkg, rpm, apk, alpm or opkg database claims belongs to that package. Its scope comes from the package manager, not from this feature.
- **A test executable that is also the subject of the scan** (`--root-name` pointing at it). The subject is never scoped.
- **A scoped component an operator excludes.** `--exclude-scope test` drops it, as it does any test-scoped component.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A component whose file is named as a test by a declaration in the scanned tree MUST be reported with the test scope.
- **FR-002**: The test scope MUST be carried natively where a format has a carrier: CycloneDX `scope: "excluded"`, SPDX 3 `software_primaryPurpose: "test"`. C42 `waybill:lifecycle-scope: test` MUST appear in all three formats. For SPDX 3, a binary role that would otherwise set `software_primaryPurpose` (`application`, `library`, `file`) MUST move to `software_additionalPurpose`, so no information is lost. In SPDX 2.3, `primaryPackagePurpose` keeps the binary role unchanged, since it has no test value.
- **FR-003**: Every test-scoped component MUST carry C62 `waybill:lifecycle-scope-derivation`, naming the rule that scoped it. The values MUST distinguish a declaration from a path convention, and from a name rule if one is ever adopted (none is: research R4).
- **FR-004**: Declarations MUST be:
  - an exported-targets file (#1563) whose package name ends in `-tests`, `_tests`, `-test` or `_test`, scoping every shipped file it names;
  - an installed `CTestTestfile.cmake`, scoping each shipped executable or script its `add_test` commands name. The executable is resolved relative to the file's directory, inside the scan root, with no variable evaluation.
- **FR-005**: In an installed tree (FR-015), path conventions MUST be directory names matched exactly: `test`, `tests`, `unittests`, `test_plugins`, `.upstream-tests`. Research may add a name only with a measurement showing it covers no product file on the ROCm fixture.
- **FR-006**: When more than one rule applies to a file, the recorded reason MUST be the strongest: declaration, then path convention, then a name rule if one is ever adopted.
- **FR-007**: The scope MUST NOT propagate from a test-scoped component to the components it depends on or links. A component is test-scoped only by a rule matching its own file.
- **FR-008**: The scope MUST apply to the components a scan already emits for the file: binary file components, file-tier components, and the installed-library components of a declared test package. This feature MUST NOT create components that would not otherwise exist.
- **FR-009**: Name patterns apply only in an installed tree (FR-015). A name pattern MUST be adopted only if, on the ROCm fixture, every file it matches is test payload by an independent check (a declaration, a path convention, or a recorded reason). The measurement and every rejected pattern MUST be committed with the spec. The candidate patterns to measure are `test_*`, `*-test`, `*_test`, `*-tests`, `*_tests`, `*_gtest`, `*_gtest.data`, `benchmark_*`, `*-bench` and `*-tune`.
- **FR-010**: The scan MUST record, in the document, how many components each rule scoped and how many declared locations were unresolved, not shipped, or outside the root. This MUST extend an existing document-scope row, C225 or C223, unless research shows neither fits.
- **FR-011**: A scan in which no declaration is read and no component is scoped MUST be byte-identical to today's output.
- **FR-012**: `--include-test-binaries` in a directory scan MUST report the binaries #1524 skips with the test scope, recorded by the path convention. This holds whether or not the tree is recognised as installed (FR-015): #1524 already classified these files as test inputs, and the operator opted in to seeing them.
- **FR-013**: A component an OS package database claims MUST NOT be scoped by this feature.
- **FR-014**: The feature MUST add zero Cargo dependencies, use no subprocess or network access, and use only synthetic `waybill-fixture-*` names in committed fixtures.
- **FR-015**: Path-convention and name rules MUST apply only in an installed tree: every image scan, and a `--path` scan that is recognised as an installed tree. Recognition MUST rest on evidence the scan already reads. That evidence is either a #1531 installed-library record (a pkg-config or CMake version file in an install prefix) that is not OS-owned, or an OS package database (dpkg, rpm, apk, alpm, opkg) that was read. The rule MUST be measured to recognise the ROCm fixture and to recognise none of the public corpus's source-tree targets. A `--path` scan that is not recognised gets declarations only. The document MUST record whether the tree was recognised as installed.

### Key Entities

- **Test-scope rule**: what makes a file test payload. One of: a declaration (exported targets of a test package; an installed CTest file), a path convention (an exactly named directory), or a name pattern (adopted per FR-009). Has a strength order and a C62 value.
- **Scoped component**: an existing component (binary file, file-tier, or installed-library) whose own file a rule matches. Carries the test scope in each format and one C62 reason.
- **Test-scope coverage**: document-scope counts per rule, and of declared locations by outcome (scoped, not shipped, unresolved, outside the root).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the ROCm fixture, every file named by the `rocprofiler-sdk-tests` export set that has a component is test-scoped. Its total is reconciled against the 0.188 GB #1563 measured.
- **SC-002**: On the ROCm fixture, every executable named by an installed CTest file that has a component is test-scoped. The count is reconciled against a census script reading the same CTest files.
- **SC-003**: On the ROCm fixture, no product library is test-scoped. Checked: no installed-library component of a package that is not itself a declared test package, and no shared library that a non-test component links, carries the scope.
- **SC-004**: On the ROCm fixture, the share of the #1564 census's test, benchmark and example payload that is test-scoped is reported. Bytes are reported per rule, with each adopted and rejected name pattern's measurement.
- **SC-005**: Across the public corpus, every golden that changes is explained by a rule in this spec. Every source-tree target is byte-identical, because path and name rules do not run there (FR-015). Any change in one is a defect.
- **SC-006**: Scan time on the full ROCm tree is at most 1.10× the main-branch median, over one warm-up and three interleaved runs.
- **SC-007**: Each rule (exported-targets declaration, CTest declaration, path convention) and the no-propagation guarantee (FR-007) is covered by a mutation that fails a test when the rule is removed or widened.

## Assumptions

- The ROCm measurements run only in CI, in the public fixture repository. The capture is extended to include `CTestTestfile.cmake` files so their contents can be measured.
- #1563's exported-targets reading is reused, not re-implemented. Its per-file attribution is where a test package's files are known.
- The 100 MB file-inventory size limit is unchanged. Payload with no component stays unreported, and #1565 covers data files.
- `--exclude-scope test` and the other existing scope filters need no change. They act on the scope this feature sets.
- Examples and samples (`examples/`, `samples/`, 0.20 GB) are not test payload and are out of scope. They may be a separate scope later.
- Benchmark payload is treated as test payload. CycloneDX `excluded` covers "other non-runtime purposes", and neither SPDX version has a benchmark purpose. Whether the `<lib>-bench` drivers qualify is decided by FR-009's measurement, not assumed.
