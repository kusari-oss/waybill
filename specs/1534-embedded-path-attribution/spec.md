# Feature Specification: Attribute installed binaries to their source through embedded build and source paths

**Feature Branch**: `1534-embedded-path-attribution`
**Created**: 2026-11-16
**Status**: Draft
**Input**: Issue #1532. Attribute installed binaries to their source using the build and source paths embedded in them. Measure first, in CI only. Prefer evidence that names a package over path heuristics. Never fabricate a version.

## Background

Unstripped and debug-info builds embed absolute source paths: assertion messages, `__FILE__`, and DWARF compilation units. Those paths name the project a file was compiled from, and often the vendored third-party trees compiled into it. Today an installed binary that no package metadata names becomes a file-level component with no identity.

### Measured on a ROCm SDK install tree (CI only)

**Fixture:** `kusari-sandbox/waybill-fixture-therock-sdk`, the pinned `therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz` (16.55 GB).

**The input.** After #1563, #1564 and #1572, 869 binary file components are neither attributed to a versioned component nor test-scoped. Of those:
- 708 are regular files, 3.015 GB;
- 161 are known only by a symlink path, such as LLVM's multi-call aliases (`llvm-strip`, `llvm-readelf`).

**The census** (`census-binaries` run 43784750746, workflow `91e4096`) read those 708 files with binutils.

| What the file carries | Files | Size |
|---|---|---|
| Absolute source paths in its strings | 545 | 2.977 GB |
| DWARF compilation units with absolute paths | 42 | 0.260 GB |
| Either | 546 | 2.983 GB |
| A GNU build-id | 122 | 0.115 GB |
| No path evidence | 162 | 0.032 GB |

**Which source tree the paths name.** A file names the source tree that built it, and also the toolchain compiled into it. On this tree, 419 files carry the toolchain's own RPM build paths (`/builddir/build/BUILD/…`); the Red Hat GCC 8.5 and 13.3 stamps in `.comment` match that. Such a path says nothing about the file's own source.

Classified against the TheRock checkout layout:
- **A TheRock submodule:** 423 files, 2.283 GB (76% of the bytes).
  - 422 name exactly one submodule project.
  - One names two. `lib/libMIOpenCKGroupedConv_gfx1150.so` (0.353 GB) names `miopen` and `composablekernel`, because Composable Kernel is compiled into MIOpen.
- **The project directory's name equals a versioned component already in the document** (#1531/#1563): 291 files, 1.349 GB. For example `rocthrust@4.4.0` (197 files, 0.466 GB), `miopen@3.5.2` (2, 0.359 GB), `rocblas@5.5.0` (12, 0.142 GB), `rocshmem@3.4.0`, `rocprofiler-sdk@1.3.1`.
  - Many of these are test and benchmark executables. rocThrust is header-only, so every one of its 197 binaries is a test or a benchmark.
  - So a path says which project built a file, not whether the file is product.
- **No such component:** 132 files, 0.934 GB.
  - `hip-tests` has 56 files, 0.488 GB: HIP's run-time-discovered `catch_tests`.
  - `rocprofiler-systems` has 34 files, 0.287 GB. It has no versioned component (#1563).
  - `rocgdb` has 7 files.
  - `llvm-project` has 15 files. It is the submodule's name; its installed packages are named `llvm`, `clang` and so on.
- **Bundled third-party trees** (`third_party/<name>`, `external/<name>`, `_deps/<name>-src`), with the number of files naming each: `timemory` 11, `fmt` 9, `googletest` 8 (+3 FetchContent), `abseil-cpp` 8, `pybind11` 8, `dyninst` 8, `ptl` 4, `grpc` 2, `fftw3` 2, `perfetto` 2, `gotcha`, `json`, `papi` 1 each. Some directory names are not projects (`packages`, `sysdeps`).

The paths carry no versions. On this tree, a version can only come from a component that already has one, or from the build's own record of source pins (`share/therock/therock_manifest.json`, which is project-specific: #1541).

## Clarifications

### Session 2026-11-16

- Q: How does a path become a component? → A: [NEEDS CLARIFICATION: Generic rule only, or also TheRock's manifest? (A) a path's source-root-relative directory names a versioned component already in the document, and only that one; this reaches 291 files, 1.349 GB, and is project-agnostic. (B) A plus TheRock's `therock_manifest.json` mapping submodule paths to pins, which would reach `llvm-project`, `rocgdb`, `rocprofiler-systems` and the other submodules without a versioned component, but is project-specific and is what #1541 (TheRock cataloger, installed artifacts) and the #1545 plugin interface are for.]
- Q: Bundled third-party trees? → A: [NEEDS CLARIFICATION: Report each bundled tree a binary names (`fmt`, `abseil-cpp`, `pybind11`, …) as a component with no version, nested under the binary (the issue's original scope), or record the names on the binary only, or defer? A versionless component per bundled library is new inventory nothing else finds, but `packages`, `sysdeps` and `json` show that directory names are not always project names.]

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A binary built from a known package's source is attributed to it (Priority: P1)

A consumer scans an installed tree. A binary that no package metadata names carries embedded source paths, all of whose project directories name one versioned component already in the document. The binary is listed in that component's evidence, the way #1563 lists the files a package's exported targets name. The evidence kind is recorded.

**Why this priority**: It is the largest measured gain that invents nothing. On ROCm it covers 291 files, 1.349 GB, including the 0.353 GB MIOpen library. The version comes from the component, never from a path.

**Independent Test**: A synthetic prefix with:
- a versioned `waybill-fixture-p` (from its `.pc`);
- an unattributed shared library whose strings hold `/src/waybill-fixture-p/lib/x.cc`;
- one whose paths name two packages;
- one whose paths name only the toolchain.

Only the first is attributed.

**Acceptance Scenarios**:

1. **Given** a binary whose embedded project paths name exactly one versioned component, **When** the tree is scanned, **Then** the binary appears in that component's evidence, with its digest, and is recorded as attributed through embedded paths.
2. **Given** a binary whose paths name two or more such components, **When** the tree is scanned, **Then** it is not attributed, and it is counted.
3. **Given** a binary whose only paths are toolchain or system paths (`/usr/include`, `/builddir/build/BUILD`, `/opt/…`), **When** the tree is scanned, **Then** it is not attributed.
4. **Given** a binary already attributed by package metadata (#1531/#1563), or owned by an OS package, **When** the tree is scanned, **Then** this feature does not attribute it again.
5. **Given** a test-scoped binary (#1564), **When** the tree is scanned, **Then** it may be attributed to the project that built it, and its scope is unchanged.

---

### User Story 2 - Bundled third-party code is visible (Priority: P2)

A consumer scans an installed tree whose binaries compile in vendored libraries. Each library a binary's paths name under a vendoring convention is reported per the clarification above. No version is invented.

**Why this priority**: This is the issue's original motivation: bundled libraries nothing else finds. On ROCm it is 13 named trees across about 30 files. It is smaller in bytes than US1, and its precision depends on directory names.

**Independent Test**: A synthetic binary whose strings name `/src/p/third_party/waybill-fixture-fmt/format.cc` and `/src/p/third_party/packages/x.cc`. The scenarios fix which of these is reported, and how.

**Acceptance Scenarios**:

1. **Given** a binary naming `third_party/<name>/`, `external/<name>/` or `_deps/<name>-src/`, **When** the tree is scanned, **Then** `<name>` is recorded for that binary, in the form the clarification chooses, with no version.
2. **Given** a vendoring directory whose name is a known non-project (`packages`, `sysdeps`, `src`, `include`), **When** the tree is scanned, **Then** nothing is recorded for it.

---

### Edge Cases

- **Remapped paths** (`-ffile-prefix-map`, `-fdebug-prefix-map`, or relative paths): no evidence, and nothing is attributed.
- **A static library compiled into another project.** `libMIOpenCKGroupedConv` names MIOpen and Composable Kernel. Under US1's one-package rule it is not attributed, unless a rule for this case is chosen. That decision is recorded with the measurement.
- **Toolchain paths** (`/builddir/build/BUILD/gcc-…`, `/usr/include`, `/opt/python/…`) are never a file's source.
- **A binary known only by a symlink.** Evidence is read from the file the link resolves to inside the root; the 161 such components on ROCm are counted, not re-read.
- **Very large binaries** (350 MB): reading must stay within the scan-time budget (SC-004).
- **A path that names a package's source but is a test of it** (rocThrust): attribution says which project built the file. Whether it is product is #1564's scope, which this feature does not change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The scan MUST read absolute source paths from an installed binary's read-only data, and MAY read them from its debug information, without new Cargo dependencies.
- **FR-002**: A path MUST be classified before use:
  - a project source path;
  - a vendored or fetched third-party tree (`third_party/<name>`, `third-party/<name>`, `external/<name>`, `3rdparty/<name>`, `_deps/<name>-src`);
  - a toolchain or system path, which is never a file's source.
- **FR-003**: A binary MUST be attributed to a versioned component only when its project paths name that component, and only that one, under the rule the clarification chooses. The version MUST come from the component, never from a path.
- **FR-004**: An attributed binary MUST be listed in the component's evidence, with its digest, the way #1563 lists exported-targets files. The evidence kind MUST distinguish embedded-path attribution from package metadata.
- **FR-005**: A binary that package metadata already attributes (#1531/#1563), or that an OS package owns, MUST NOT be attributed by this feature.
- **FR-006**: Bundled third-party trees MUST be handled as the clarification chooses. No version may be invented.
- **FR-007**: The document MUST record, at document scope, how many binaries were read, carried paths, were attributed, named several packages, named only toolchain paths, or named a project with no versioned component.
- **FR-008**: A scan in which no binary carries a usable path MUST be byte-identical to today's.
- **FR-009**: The feature MUST add zero Cargo dependencies, no subprocess and no network access, and MUST use only synthetic `waybill-fixture-*` names in committed fixtures.

### Key Entities

- **Embedded path**: an absolute path string found in a binary, with its classification (project, third-party tree, toolchain).
- **Path attribution**: a binary → versioned component link, with its evidence kind.
- **Embedded-path coverage**: the document-scope counts (FR-007).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the ROCm fixture, the binaries attributed through embedded paths reconcile with the committed census for the chosen rule, by count and bytes. Under rule (A) that is 291 files and 1.349 GB, or fewer if the one-package rule excludes MIOpen's library.
- **SC-002**: No binary is attributed to a component whose project its paths do not name. This is checked on ROCm by listing every attribution with the project directory that justified it.
- **SC-003**: Across the public corpus, every golden that changes is explained by a rule in this spec.
- **SC-004**: Scan time on the full ROCm tree is at most 1.25× the main-branch median, over one warm-up and three interleaved runs. Reading strings from 3 GB of binaries is the cost to bound.
- **SC-005**: Each rule (one-package, toolchain exclusion, already-attributed exclusion, OS ownership) has a mutation that fails a test.

## Assumptions

- Measurements run only in CI, through the public fixture repository. The census script and its input list are committed with the spec.
- Path classification uses only the paths themselves and the components already in the document. No source tree is read.
- `therock_manifest.json` is not read unless the first clarification chooses (B).
- The 161 symlink-only binary components on ROCm are a separate observation: binary components keyed on a symlink path whose target is also scanned. They are counted here and not changed.
- Binaries built from a `*-tests` project (`hip-tests`) are evidence for test scope, but changing #1564's rules is out of scope. A follow-up issue records the measurement.
