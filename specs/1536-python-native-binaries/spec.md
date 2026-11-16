# Feature Specification: Native binaries inside a Python package get binary analysis

**Feature Branch**: `1536-python-native-binaries`
**Created**: 2026-11-16
**Status**: Draft
**Input**: Issue #1567. Native binaries shipped inside a Python package must still get binary analysis. Ownership and identification are separate questions: the package owns the file, and the file can still contain third-party code worth identifying.

## Background

A Python package's `.dist-info/RECORD` lists every file the package installed. waybill treats that list as a claim of ownership, and a claimed file skips binary analysis entirely:
- no dynamic linkage;
- no embedded version strings;
- no symbol fingerprints;
- no embedded-path attribution (spec 1534).

The rule exists for OS packages. A dpkg-owned `/usr/bin/curl` that embeds `curl 7.88.1` would otherwise be reported twice: as the Debian package and as `pkg:generic/curl@7.88.1`. In that case the binary *is* the package.

A Python package is different. Wheels routinely vendor third-party native code:
- auditwheel-repaired Linux wheels copy their shared-library dependencies into `<pkg>.libs/`;
- delocate-repaired macOS wheels copy them into `<pkg>/.dylibs/`;
- some packages are almost entirely native libraries.

A zlib inside `waybill-fixture-pkg` is not `waybill-fixture-pkg`. Today it is not reported at all.

**How the claim works today.** One claim set holds every reader's claims:
- the OS package databases: dpkg, rpm, apk, alpm, opkg;
- pip `RECORD`;
- `.ipk` archives.

The binary scan skips anything in it. Spec 1531 added a second set holding the OS package databases' claims alone, so "does an OS package own this file?" can already be answered on its own.

**Why it regressed.** #1529 made a `.dist-info` at the root of an unpacked wheel readable. That fixed the wheel's identity, but it also brought unpacked wheels under the `RECORD` rule. Packages under `site-packages` were already subject to it.

### Measured (2026-11-16, waybill `main` `1ae157d5`)

The fixture is a minimal wheel, `waybill-fixture-pkg` 1.2.3, holding a Nix-built zlib 1.3.2 shared library (`libz.1.3.2.dylib`). The library is listed in `RECORD`.

| Layout | Root | What is reported for the library |
|---|---|---|
| Unpacked wheel, no `.dist-info` | `pkg:generic/<dir>@0.0.0` | `pkg:generic/zlib@1.3.2` (embedded version string), a file-level component, and its linkage (`libSystem.B.dylib`) |
| Unpacked wheel with `.dist-info` | `pkg:pypi/waybill-fixture-pkg@1.2.3` | one `file` component (file tier); nothing identified, no linkage |
| The package under `usr/lib/python3.12/site-packages/` | `pkg:generic/<dir>@0.0.0` with `pkg:pypi/waybill-fixture-pkg@1.2.3` as a component | nothing; the library is not even a `file` component, because the file tier skips `site-packages` |

The pip reader searches `site-packages` only under `usr/lib`, `usr/local/lib`, `.venv`, `venv` and `opt/app/.venv`. A bare `lib/python3.X/site-packages/` at the scan root is not read, so the package is not found at all. That is outside this feature.

**Quoted from #1567, not re-measured here.** On a real wheel of about 300 native libraries, the build before #1529 emitted 310 binary-reader components and `main` emits 0. The issue reports that only one of those was an embedded-version match. So most of the 310 were file-level components, and `main` still inventories the same files as 312 `file` components. What the regression removed is identification and linkage, not the file inventory.

**The ROCm SDK fixture** (`kusari-sandbox/waybill-fixture-therock-sdk`) contains no `.dist-info` directory: 0 of its listed paths. It cannot exercise this feature. It can only show that nothing changes there.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A vendored native library is identified under its Python package (Priority: P1)

A consumer scans an unpacked wheel or an installed `site-packages` tree. A shared library the package ships, and lists in its `RECORD`, goes through binary analysis. Anything identified is reported nested under the Python package that owns the file, with the library's path as its occurrence. The package stays the owner.

**Why this priority**: This is the regression. Identification inside wheels was lost for unpacked wheels and never existed for `site-packages`, and vendored native code is exactly what a vulnerability consumer needs to see.

**Independent Test**: The measured fixture, built from synthetic `waybill-fixture-*` names around a binary carrying an embedded zlib version string. Scan it in both layouts with a `.dist-info`, then check:
- the zlib identification is reported, nested under `pkg:pypi/waybill-fixture-pkg@1.2.3`;
- the linkage the no-`.dist-info` scan reports is reported too.

**Acceptance Scenarios**:

1. **Given** an unpacked wheel whose `RECORD` lists a library with an embedded zlib version, **When** it is scanned, **Then** `pkg:generic/zlib@1.3.2` appears nested under `pkg:pypi/waybill-fixture-pkg@1.2.3`, its occurrence is the library's path, and the root stays the Python package.
2. **Given** the same package under `usr/lib/python3.12/site-packages/`, **When** it is scanned, **Then** the same nested component appears under the package.
3. **Given** a library with dynamic dependencies outside the package, **When** it is scanned, **Then** those dependencies are reported as they are for an unclaimed binary, and the dependency edges start from the owning package.
4. **Given** a vendored library that links another library vendored in the same package, **When** it is scanned, **Then** the linked library is not reported as an external dependency.
5. **Given** a package whose `RECORD` lists only Python files, **When** it is scanned, **Then** the output is unchanged.

---

### User Story 2 - OS-package-owned binaries keep today's behaviour (Priority: P1)

A consumer scans a container image or rootfs. Binaries owned by an OS package database (dpkg, rpm, apk, alpm, opkg) are reported exactly as today. That includes Python packages the OS package manager installed: a `python3-*` Debian package has a `.dist-info` with a `RECORD`, and dpkg also lists the same files.

**Why this priority**: The claim rule exists to stop OS packages double-reporting themselves. Relaxing it for Python must not reopen that.

**Independent Test**: A synthetic rootfs with a dpkg database that lists a library inside a `site-packages` package which also has a `RECORD`. Scan it and compare with `main`: byte-identical.

**Acceptance Scenarios**:

1. **Given** a binary claimed by an OS package database, **When** it is scanned, **Then** its output is unchanged, whether or not a `RECORD` also lists it.
2. **Given** the corpus of public scan targets, **When** its goldens are regenerated, **Then** every difference is in a binary claimed by a `RECORD` and no OS package database.

---

### User Story 3 - Repaired-wheel layouts (Priority: P2)

A consumer scans wheels repaired by auditwheel and delocate:
- auditwheel puts libraries in a sibling `<pkg>.libs/` directory, with a content hash in the file name (`libgfortran-<hash>.so.5.0.0`);
- delocate puts them in `<pkg>/.dylibs/`.

Both are listed in `RECORD`, so both are owned by the package and analysed.

**Why this priority**: These are the two layouts through which most third-party native code reaches a wheel. US1 covers the mechanism; this story makes sure the layouts' details do not defeat it.

**Independent Test**: Synthetic wheels in each layout, with a hashed-name library carrying an embedded version string. Check that each library is identified, nested under its package, and named without the auditwheel hash suffix.

**Acceptance Scenarios**:

1. **Given** an auditwheel layout, **When** it is scanned, **Then** the libraries in `<pkg>.libs/` are owned by the package and analysed as in US1.
2. **Given** a delocate layout, **When** it is scanned, **Then** the libraries in `<pkg>/.dylibs/` are owned by the package and analysed as in US1.
3. **Given** a library named `libfoo-0123abcd.so.1`, **When** a name is derived from its file name, **Then** the hash suffix is not part of that name.

### Edge Cases

- **Claimed by both a `RECORD` and an OS package:** the OS claim wins, and the output is unchanged (US2).
- **A `RECORD` entry for a missing file:** it is skipped, as today.
- **A `RECORD` path outside the package directory.** pip installs scripts and data through `../../bin/<name>`-style entries. Some wheels ship their whole tool as a native executable this way, for example Rust and Go command-line tools. Such a file is owned by the package and analysed. Its embedded build metadata (cargo-auditable crates, Go build info) nests under the Python package.
- **A binary that identifies the package itself.** A native library may carry the same project's name and version. It is still reported nested under the package, because it describes what the package contains. It is not merged into the package.
- **Two `RECORD`s claim one file:** one owner is chosen deterministically, and the choice does not depend on scan order.
- **A package without a `RECORD`** (egg-info, or a `.dist-info` missing the file): nothing claims its files today. Unchanged.
- **CPython's own extension modules** (`lib-dynload`) are not `RECORD`-claimed. The CPython collapse is unchanged.
- **`.ipk` archive claims** keep today's behaviour. Only `RECORD` claims change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A file claimed by a Python package's `RECORD`, and by no OS package database, MUST go through binary analysis as an unclaimed file does. That covers dynamic linkage, ELF notes, embedded version strings, symbol fingerprints, embedded-path attribution, binary role, and build metadata (cargo-auditable, Go build info).
- **FR-002**: Every component identified from such a file MUST nest under the Python package that owns it, the way cargo-auditable crates nest under their binary. Its occurrence MUST be the file's path.
- **FR-003**: The owning Python package MUST remain the owner, and the document root MUST NOT change. What the binary's own file-level component becomes is [NEEDS CLARIFICATION: when a `RECORD`-claimed binary's own component is emitted, and where it sits — see Q1].
- **FR-004**: Dependency edges from linkage MUST start at the owning package. A library resolved to another file the same package owns MUST NOT be reported as an external dependency.
- **FR-005**: A file claimed by an OS package database (dpkg, rpm, apk, alpm, opkg) MUST keep today's behaviour, whether or not a `RECORD` also claims it.
- **FR-006**: An unpacked wheel's root `.dist-info` and a `site-packages` `.dist-info` MUST take the same path. The layout does not change what is analysed or how it is reported.
- **FR-007**: Claims other than `RECORD` and the OS package databases (for example `.ipk` archives) MUST keep today's behaviour.
- **FR-008**: A name derived from a file name MUST drop an auditwheel content-hash suffix (`-<8 hex>` before `.so`).
- **FR-009**: When one or more `RECORD`-claimed binaries are analysed, the document MUST say how many there were and how many yielded at least one identification, so a consumer can tell "analysed, nothing found" from "not analysed".
- **FR-010**: Output MUST be deterministic, including the choice of owner when two `RECORD`s claim one file.
- **FR-011**: No network access at scan time, and zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names.

### Key Entities

- **Owned binary**: a file a Python package's `RECORD` claims, that no OS package database claims, and that binary analysis recognises as ELF, Mach-O or PE. Its attributes are the owning package, its path, and what analysis identified in it.
- **Owning package**: the `pkg:pypi` component whose `RECORD` lists the file. It is the parent of everything identified in its owned binaries.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the synthetic wheel, in both layouts with a `.dist-info`, the zlib identification and the linkage that the no-`.dist-info` scan reports are all present. Each is nested under, or an edge from, `pkg:pypi/waybill-fixture-pkg@1.2.3`.
- **SC-002**: On the public corpus, with goldens regenerated in CI, every changed file is explained by a `RECORD`-only claimed binary. No component of an OS package changes.
- **SC-003**: On the ROCm SDK fixture in CI, which contains no `.dist-info`, the output is unchanged.
- **SC-004**: On [NEEDS CLARIFICATION: which real wheel(s) CI measures — see Q2], measured in CI, the identifications and linkage that the same tree yields without its `.dist-info` are all present, nested under the package. No binary is reported twice.
- **SC-005**: Each rule has a mutation that makes a test fail: the OS claim winning, nesting, the owner-rooted edges, the internal-link exclusion, the hash-suffix strip, the same path for both layouts, and the FR-009 counts.
- **SC-006**: Every output passes the CycloneDX 1.6 schema and `spdx3-validate`.

## Assumptions

- The binary scan's existing analysis is reused unchanged. This feature changes which files reach it and where its results attach, not what it finds.
- The nesting construct is the one cargo-auditable already uses. No new format carrier is needed for nesting; FR-009 may need a document-scope row.
- `site-packages` locations stay as the pip reader finds them today.
- #1528 (wheels over `--max-file-size`) and #1530 (archives inside packages) are separate.
- Measurements on TheRock's artifacts run in CI only. Nothing from TheRock is cloned or downloaded locally.
