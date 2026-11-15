# Feature Specification: Attribute installed files to their package through CMake exported targets

**Feature Branch**: `1532-cmake-exported-targets`
**Created**: 2026-11-15
**Status**: Draft
**Input**: Issue #1563. Read CMake exported-targets files to attribute installed libraries and executables to the package that installed them. This extends the #1531 installed-library reader.

## Background

#1531 (spec 1531) gives an installed C/C++ package a versioned component, `pkg:generic/<name>@<version>`, from its pkg-config file or CMake package version file. The files that package installs (its shared libraries, static archives and executables) are still unattributed, except for a shared library whose file name matches the package name.

A CMake package that exports its targets also installs `<Pkg>Targets.cmake` (or `<pkg>-targets.cmake`, or `*Exports.cmake`) and one `…-<config>.cmake` file per build configuration. Together these state the exact installed file of every library and executable target:

```cmake
set_target_properties(composable_kernel::device_conv_operations PROPERTIES
  IMPORTED_LOCATION_RELEASE "${_IMPORT_PREFIX}/lib/libdevice_conv_operations.a")
```

`_IMPORT_PREFIX` is the directory holding the file, raised one level by each `get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)` line in the package's main targets file. No waybill reader reads these files today.

### Measured on a ROCm SDK install tree

**Fixture:** the public `kusari-sandbox/waybill-fixture-therock-sdk`, extracted in CI from `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz` (SHA-256 `efeb6b07…bc5a`). Unpacked, the tarball is 16.55 GB, of which 10.46 GB is product payload once the test and benchmark payload is excluded.

**What the exported targets name:**
- 150 exported-targets files ship.
- Together they name 914 imported locations, every one written `${_IMPORT_PREFIX}/…`. 910 of those files ship; 4 are named but absent.
- The only build configuration present is `release`.

**How the 910 shipped files would be attributed:**
- 900 are in a package directory whose CMake version file produced a #1531 component.
- 3 more are in a directory with no version file, whose package has a same-name #1531 component from its `.pc` (`amd-dbgapi`).
- 7 belong to packages with no versioned component: `rdc` (empty version), `openmp`, `rocprofiler-systems`, `utf8_range`.

**Product bytes attributed to a versioned component** (measured by script, not yet by waybill):
- 15.8% after #1531;
- 64.6% with exported targets.

The largest gains are `composable_kernel` (2.34 GB, five static archives), `flang` (0.80 GB), `clang` (0.72 GB), `mlir` (0.60 GB) and `llvm` (0.42 GB).

**Test payload.** 0.188 GB of the files named belong to a package that declares itself a test package, `rocprofiler-sdk-tests`. Its test scope is #1564's concern, not this feature's.

## Clarifications

### Session 2026-11-15 (from the feature description and #1563)

- **Executables** are attached the same way as libraries: as evidence on the package's component. No new edge kind is introduced.
- **Static archives** are attached as evidence now, without waiting for #1534.
- **A package with no versioned component** emits nothing. Its named files are counted in the coverage record.
- **Coverage** extends C225 rather than adding a catalog row.

### Session 2026-11-15

- Q: Attributing about 900 installed files (6.95 GB on the ROCm SDK) adds them to each package's evidence, and every evidence file is SHA-256 hashed today (files over 256 MB are listed without a hash). What should evidence carry for these files? → A: **A hash, within a budget.** Each attributed file gets a hash, in #1531's shape. A hash the scan already computed is reused: the binary scan's hash of a shared library or executable. That gives the static archives, which no other part of the scan hashes, a verifiable digest. The added scan time is measured in CI on the full ROCm install tree. Only if it exceeds the SC-006 budget do attributed files fall back to path-only (FR-005a, SC-006).

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Every file a versioned package exports is attributed to it (Priority: P1)

An engineer scans an installed SDK. Its packages install CMake exported targets next to their version files. The engineer expects each library and executable those targets name to appear in the evidence of the package's versioned component, including static archives and executables whose file names never mention the package.

**Why this priority**: This is all of #1563. On the measured SDK it is the single largest gain in attributed bytes, from 15.8% to 64.6% of the product payload, and every attribution comes from the package's own metadata.

**Independent Test**: A synthetic prefix holds a package with a version file, a targets file and a release targets file. The release file names a shared library, a static archive whose name differs from the package, and an executable. All three files exist. The package's component lists all three in its evidence.

**Acceptance Scenarios**:

1. **Given** `lib/cmake/WaybillFixtureP/WaybillFixturePConfigVersion.cmake` (`4.5.6`), `WaybillFixturePTargets.cmake` (with two `get_filename_component(... PATH)` lines) and `WaybillFixturePTargets-release.cmake` naming `${_IMPORT_PREFIX}/lib/libwaybill-fixture-p.so.4`, `${_IMPORT_PREFIX}/lib/libwaybill-fixture-ops.a` and `${_IMPORT_PREFIX}/bin/waybill-fixture-tool`, **When** the prefix is scanned, **Then** `pkg:generic/waybillfixturep@4.5.6` lists all three files in its evidence.
2. **Given** the lower-case convention (`waybill-fixture-q-targets.cmake` and `waybill-fixture-q-targets-release.cmake` beside `waybill-fixture-q-config-version.cmake`), **When** it is scanned, **Then** the named files are attributed the same way.
3. **Given** an `*Exports.cmake` / `*Exports-release.cmake` pair beside a version file, **When** it is scanned, **Then** the named files are attributed the same way.
4. **Given** a targets file naming a file that does not ship, **When** it is scanned, **Then** that file is listed nowhere, and the coverage record counts it as not shipped.
5. **Given** a shared library already listed in a component's evidence by #1531's file-name match, **When** an exported target also names it, **Then** it appears once.

---

### User Story 2 — Nothing is attributed without a versioned package, and nothing is guessed (Priority: P1)

The same engineer expects attribution only when it is certain: a versioned package component that the targets file demonstrably belongs to, and a file the targets file names exactly.

**Why this priority**: A wrong attribution is worse than none. The measured tree has packages with no usable version (`rdc` ships an empty `PACKAGE_VERSION`), and targets files are CMake code that must not be evaluated.

**Independent Test**: Several prefixes, each with one kind of uncertainty. None of them produces an attribution, and each is counted under its reason.

**Acceptance Scenarios**:

1. **Given** a targets file in a directory whose version file is computed, **When** it is scanned, **Then** no file is attributed, no component is created, and the files are counted as having no package version.
2. **Given** a targets file in a directory with no version file, but with a #1531 component of the same lowercased name in the same prefix (from a `.pc`), **When** it is scanned, **Then** the named files are attributed to that component.
3. **Given** an imported location written with a variable other than `_IMPORT_PREFIX` (`${CMAKE_INSTALL_PREFIX}/lib/x.so`, `${FOO}/x.so`), **When** it is scanned, **Then** it is not resolved, and it is counted as an unresolved location.
4. **Given** an imported location that resolves outside the scan root, **When** it is scanned, **Then** it is not read or listed, and it is counted as unresolved.
5. **Given** a targets file that an OS package database owns, **When** it is scanned, **Then** it is skipped and counted, as #1531 skips owned files.

---

### Edge Cases

- **Several configurations** (`-release.cmake`, `-debug.cmake`, `-relwithdebinfo.cmake`): every configuration's locations are read, and a file named by more than one is listed once.
- **A symlinked library** (`libfoo.so` → `libfoo.so.1.2`). The location is the path the targets file names, resolved within the scan root, so a symlink whose target exists counts as shipped. Evidence lists the named path.
- **Interface, alias and imported-only targets** with no `IMPORTED_LOCATION` are ignored. They name no file.
- **`IMPORTED_IMPLIB`** (a Windows import library) and **`IMPORTED_SONAME`** name no installed file to attribute beyond the location, and are ignored.
- **Several packages naming the same file** (rare; a file re-exported by two packages). The file is listed under each package that names it, and nothing is deduplicated across packages. The coverage counts are per (directory, location) pair, so such a file counts once per package that names it. *(Amended during analysis: I1.)*
- **Prefixes nested inside prefixes** (`lib/rdc/grpc/`, `lib/rocm_sysdeps/`). Each targets file resolves relative to its own location, so a nested prefix's files attach to the nested package.
- **Test packages** (`rocprofiler-sdk-tests`). Their files attach to their own package's component. Scoping them as tests is #1564.
- **Malformed or truncated targets files.** Statements that cannot be read are skipped and counted. They never abort the scan.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan MUST read every CMake exported-targets file found in a CMake package search location (spec 1531 FR-006):
  - `<Pkg>Targets.cmake` and `<Pkg>Targets-<config>.cmake`;
  - `<pkg>-targets.cmake` and `<pkg>-targets-<config>.cmake`;
  - `<Pkg>Exports.cmake` and `<Pkg>Exports-<config>.cmake`.

  It MUST apply #1531's OS-ownership skip (spec 1531 FR-015) and root-relative symlink resolution (spec 1531 FR-012).
- **FR-002**: The scan MUST read each `set_target_properties(<target> PROPERTIES …)` statement textually. It MUST take each `IMPORTED_LOCATION` and `IMPORTED_LOCATION_<CONFIG>` value, and evaluate nothing.
- **FR-003**: `_IMPORT_PREFIX` MUST be the directory holding the targets file, raised one level per `get_filename_component(_IMPORT_PREFIX "${_IMPORT_PREFIX}" PATH)` line in the package's main targets file. That is the file in the same directory without a configuration suffix. A `<config>` file inherits that value. A `<config>` file with no main file in its directory has no `_IMPORT_PREFIX`, so all its locations are unresolved. A location containing any other variable, or resolving outside the scan root, MUST NOT be resolved, and MUST be counted as unresolved. *(Amended during analysis: I2.)*
- **FR-004**: A targets file's package component MUST be determined as follows. When none is found, its files MUST NOT be attributed, no component may be created, and its files MUST be counted as having no package version.
  - **First:** the #1531 component produced by the CMake version file in the same directory.
  - **Otherwise:** the #1531 component, in the same install prefix, whose name equals the directory's lowercased name.
- **FR-005**: Each resolved location that exists MUST be listed in the package component's evidence, whatever kind of file it is (shared library, static archive, executable). It uses the same evidence mechanism as #1531's shared-library match. A location that does not exist MUST NOT be listed, and MUST be counted as not shipped.
- **FR-005a**: Each attributed file's evidence MUST carry its SHA-256, subject to the existing 256 MB cap.
  - **Reuse.** A hash the scan already computed for that exact file MUST be reused rather than recomputed: the binary scan's hash of a shared library or executable.
  - **Observable.** Reuse MUST be visible: one log line per scan gives the number of hashes seeded and the number reused, so a test can tell reuse from re-hashing. A recomputed hash of the same file gives the same digest, so the digest alone cannot show it. *(Amended during analysis: U1.)*
  - **Fallback.** Only if SC-006's budget is exceeded may attributed files be listed path-only. That choice must be recorded with its measurement.
- **FR-006**: A file listed by #1531's shared-library match and named by an exported target MUST appear once in that component's evidence. Binary file components MUST NOT change, as in spec 1531 FR-010.
- **FR-007**: C225 `waybill:installed-library-coverage` MUST gain an `exported-targets` object. It is present whenever at least one exported-targets file was matched; when none was, C225 is unchanged in shape. It MUST be identical across the three formats, with keys sorted. Its counts are:
  - `read`: files read;
  - `locations`: distinct imported locations;
  - `attached`: files listed in evidence;
  - `not-shipped`;
  - `no-package-version`;
  - `unresolved-location`;
  - `owned-by-os-package`: exported-targets files skipped as OS-owned.
- **FR-008**: The reader MUST work offline, run no subprocess, evaluate no CMake, and read only files under the scan root.
- **FR-009**: Output MUST be byte-identical across repeated scans. A tree with no exported-targets file MUST produce output identical to the base commit's.

### Key Entities

- **Exported-targets file**: a package's statement of its installed targets. The main file sets `_IMPORT_PREFIX`; each `<config>` file names locations.
- **Imported location**: one installed file a target names, resolved to a path under the scan root.
- **Package component**: the #1531 `pkg:generic/<name>@<version>` component that a targets file belongs to (FR-004).
- **Exported-targets coverage**: the FR-007 counts inside C225.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the US1 fixture, all three named files (a shared library, a static archive with an unrelated name, and an executable) appear in the package component's evidence in all three formats. They do not appear when the targets files are removed.
- **SC-002**: In the US2 fixtures:
  - a computed version, an unrecognised variable and a location escaping the root each yield zero attributions and one count under the matching reason;
  - a `.pc`-only package (no version file) gets its files attributed by FR-004's same-name rule.
- **SC-003**: Over the ROCm SDK install tree, measured in CI only, 903 shipped files appear in a versioned package component's evidence. *(Measured: C225 reports `attached: 906`, which is these 903 files plus 3 that two packages each name, because FR-007 counts (directory, location) pairs. See `measurements/therock-full.md`.)* That is 900 through a same-directory version file and 3 through FR-004's same-name rule, out of 910 shipped files named by exported targets. The other 7 are counted under `no-package-version`, and the 4 named but absent files under `not-shipped`. The measurement runs on the full install tree, because the metadata-only fixture does not contain the binaries.
- **SC-004**: Ten repeated scans are byte-identical, and a tree with no exported-targets file is byte-identical to the base commit.
- **SC-005**: Committed goldens change only where a target tree contains an exported-targets file or a matched #1531 file. `image-postgres16`'s C225 record is the only expected change; whether its image ships an exported-targets file is measured during planning.
- **SC-006**: On the full ROCm SDK install tree, measured in CI only, the median scan time with this feature is at most **1.25×** the median without it, at the same waybill base. Each arm gets one warm-up run, then at least 3 interleaved runs. The ratio is against a baseline measured in the same job, not an absolute time.

## Assumptions

- `pkg:generic` identity, the `deployed` tier, the search locations and OS ownership are all as spec 1531 defines them. Nothing about them changes.
- Evidence is the attribution mechanism. A new edge kind or a nested component per file is out of scope; whether each file should become its own component is #1534's and #1541's question.
- Fixtures use synthetic `waybill-fixture-*` names. The ROCm SDK measurement runs only in CI, through the fixture repository's workflows, extended to scan the full tarball.
- Zero new Cargo dependencies, and no subprocesses.
- Scoping a test package's files as tests is #1564.
