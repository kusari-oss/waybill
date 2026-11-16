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

## Clarifications

### Session 2026-11-16

- Q: What does a `RECORD`-claimed binary appear as, and where? → A: Each owned binary appears once, as its own component nested under the owning package. What it identifies nests under it: package → `libz.1.3.2.dylib` → `zlib@1.3.2`. The file tier's separate top-level `file` component for that path is not emitted, so nothing is listed twice, and `site-packages` gains the inventory it lacks today.
- Q: Which real wheel(s) does CI measure? → A: Both of these:
  - a pinned ROCm Python wheel published by TheRock, scanned in the fixture repo's CI;
  - the auditwheel `manylinux` wheel and the delocate macOS wheel of one widely used numerical package from PyPI, pinned and scanned in CI.
- Q: Where do the dependency edges start? → A: In a chain. The package depends on each binary it owns. Each binary depends on the libraries it links and on what was identified in it. Today every edge starts at the document root, and the binary has no edges of its own.
- Q: What counts as "identified" for FR-009's counts? → A: A component derived from the binary's own content: an embedded version string, an ELF note, a symbol fingerprint, embedded-path attribution, or build metadata (cargo-auditable, Go build info). Linkage alone does not count.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A vendored native library is identified under its Python package (Priority: P1)

A consumer scans an unpacked wheel or an installed `site-packages` tree. A shared library the package ships, and lists in its `RECORD`, goes through binary analysis. The library is reported once, as a component nested under the Python package that owns it, with its path as the occurrence. Anything identified in it nests under the library. The package stays the owner.

**Why this priority**: This is the regression. Identification inside wheels was lost for unpacked wheels and never existed for `site-packages`, and vendored native code is exactly what a vulnerability consumer needs to see.

**Independent Test**: The measured fixture, built from synthetic `waybill-fixture-*` names around a binary carrying an embedded zlib version string. Scan it in both layouts with a `.dist-info`, then check:
- the library is reported once, nested under `pkg:pypi/waybill-fixture-pkg@1.2.3`;
- the zlib identification is nested under the library;
- the linkage the no-`.dist-info` scan reports is reported too.

**Acceptance Scenarios**:

1. **Given** an unpacked wheel whose `RECORD` lists a library with an embedded zlib version, **When** it is scanned, **Then** the library appears as a component nested under `pkg:pypi/waybill-fixture-pkg@1.2.3`, with its path as the occurrence. `pkg:generic/zlib@1.3.2` is nested under the library, and the root stays the Python package.
2. **Given** the same package under `usr/lib/python3.12/site-packages/`, **When** it is scanned, **Then** the same nesting appears under the package.
3. **Given** a library with dynamic dependencies outside the package, **When** it is scanned, **Then** those dependencies are reported as they are for an unclaimed binary, and the edges to them start from the library's own component, which the package depends on.
4. **Given** a vendored library that links another library vendored in the same package, **When** it is scanned, **Then** the edge goes to the other library's component, and the linked library is not reported as an external dependency.
5. **Given** a package whose `RECORD` lists only Python files, **When** it is scanned, **Then** the output is unchanged.
6. **Given** an owned library in which analysis identifies nothing, **When** it is scanned, **Then** it still appears once, nested under the package. No top-level `file` component is emitted for its path.

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
3. **Given** an owned library named `libfoo-0123abcd.so.1` directly in `<pkg>.libs/`, **When** its name is derived from its file name, **Then** the hash suffix is not part of that name. The same file name anywhere else keeps the suffix.

### Edge Cases

- **Claimed by both a `RECORD` and an OS package:** the OS claim wins, and the output is unchanged (US2).
- **A `RECORD` entry for a missing file:** it is skipped, as today.
- **A `RECORD` path outside the package directory.** pip installs scripts and data through `../../bin/<name>`-style entries. Some wheels ship their whole tool as a native executable this way, for example Rust and Go command-line tools. Such a file is owned by the package and analysed. Its embedded build metadata (cargo-auditable crates, Go build info) nests under the Python package.
- **A binary that identifies the package itself.** A native library may carry the same project's name and version. It is still reported nested under the binary, because it describes what the package contains. It is not merged into the package.
- **Two `RECORD`s claim one file:** one owner is chosen deterministically, and the choice does not depend on scan order.
- **A package without a `RECORD`** (egg-info, or a `.dist-info` missing the file): nothing claims its files today. Unchanged.
- **CPython's own extension modules** (`lib-dynload`) are not `RECORD`-claimed. The CPython collapse is unchanged.
- **`.ipk` archive claims** keep today's behaviour. Only `RECORD` claims change.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A file claimed by a Python package's `RECORD`, and by no OS package database, MUST go through binary analysis as an unclaimed file does. That covers dynamic linkage, ELF notes, embedded version strings, symbol fingerprints, embedded-path attribution, binary role, and build metadata (cargo-auditable, Go build info).
- **FR-002**: Each such file that analysis recognises as a binary MUST appear exactly once, as its own component nested under the Python package that owns it, with the file's path as its occurrence. Every component identified from it MUST nest under that binary component (package → binary → identified), the way cargo-auditable crates nest under their binary.
- **FR-003**: The owning Python package MUST remain the owner, and the document root MUST NOT change. An owned binary MUST NOT also appear anywhere else; in particular, the file tier MUST NOT emit a top-level `file` component for its path. An owned binary in which nothing is identified still appears, nested.
- **FR-004**: The owning package MUST depend on each binary it owns. Each owned binary MUST depend on the libraries it links and on each component identified in it. When a linked library resolves to another binary the same package owns, the edge MUST go to that binary's component, and the library MUST NOT be reported as an external dependency.
- **FR-005**: A file claimed by an OS package database (dpkg, rpm, apk, alpm, opkg) MUST keep today's behaviour, whether or not a `RECORD` also claims it.
- **FR-006**: An unpacked wheel's root `.dist-info` and a `site-packages` `.dist-info` MUST take the same path. The layout does not change what is analysed or how it is reported.
- **FR-007**: Claims other than `RECORD` and the OS package databases (for example `.ipk` archives) MUST keep today's behaviour.
- **FR-008**: For an owned binary directly in an auditwheel `<pkg>.libs/` directory, the name derived from its file name MUST drop the content-hash suffix (`-<8 hex>` before `.so`). A name anywhere else MUST NOT be changed.
- **FR-009**: When one or more `RECORD`-claimed binaries are analysed, the document MUST say how many there were and how many yielded at least one identification, so a consumer can tell "analysed, nothing found" from "not analysed". An identification is a component derived from the binary's own content: an embedded version string, an ELF note, a symbol fingerprint, embedded-path attribution, or build metadata (cargo-auditable, Go build info). Linkage alone is not an identification.
- **FR-010**: Output MUST be deterministic, including the choice of owner when two `RECORD`s claim one file.
- **FR-011**: No network access at scan time, and zero new Cargo dependencies. Test fixtures use synthetic `waybill-fixture-*` names.

### Key Entities

- **Owned binary**: a file a Python package's `RECORD` claims, that no OS package database claims, and that binary analysis recognises as ELF, Mach-O or PE. It is reported as one component nested under the owning package, with its path as the occurrence, and it is the parent of what analysis identified in it.
- **Owning package**: the `pkg:pypi` component whose `RECORD` lists the file. It is the parent of its owned binaries.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the synthetic wheel, in both layouts with a `.dist-info`, the zlib identification and the linkage that the no-`.dist-info` scan reports are all present. The library appears once, nested under `pkg:pypi/waybill-fixture-pkg@1.2.3`; the zlib identification is nested under the library. The edges form the chain package → library → {the zlib identification, the fixture's linked libraries}. The two layouts give the same components and edges. The measured Mach-O case (`zlib@1.3.2`, `libSystem.B.dylib`) is checked by hand in `quickstart.md`.
- **SC-002**: On the public corpus, with goldens regenerated in CI, every changed file is explained by a `RECORD`-only claimed binary. No component of an OS package changes.
- **SC-003**: On the ROCm SDK fixture in CI, which contains no `.dist-info`, the output is unchanged.
- **SC-004**: Measured in CI on real wheels, each pinned:
  - a ROCm Python wheel published by TheRock, scanned in the fixture repo's CI, raising `--max-file-size` where its libraries need it (#1528);
  - the auditwheel `manylinux` wheel and the delocate macOS wheel of one widely used numerical package from PyPI, chosen at plan time.

  For each wheel, the identifications and linkage that the same tree yields without its `.dist-info` are all present, nested as FR-002 says. No binary is reported twice.
- **SC-005**: Each rule has a mutation that makes a test fail: the OS claim winning, nesting, the edge chain, the internal-link exclusion, the hash-suffix strip, the same path for both layouts, the single listing of each owned binary, and the FR-009 counts.
- **SC-006**: Every output passes the CycloneDX 1.6 schema and `spdx3-validate`.

## Assumptions

- The binary scan's existing analysis is reused unchanged. This feature changes which files reach it and where its results attach, not what it finds.
- The nesting construct is the one cargo-auditable already uses, CycloneDX nested components and SPDX `CONTAINS`, so no new format carrier is needed; FR-009 needs a document-scope row. The emitters carry only one level of nesting today, so they gain depth (research R5). That also changes how *existing* nested components are emitted: CycloneDX `dependencies` refs become their `bom-ref`s, and SPDX emits one element per PURL, so a coordinate nested under two parents no longer produces a duplicate identifier. No corpus golden holds a nested component (measured), and the CHANGELOG states the change.
- `site-packages` locations stay as the pip reader finds them today.
- #1528 (wheels over `--max-file-size`) and #1530 (archives inside packages) are separate.
- Measurements on TheRock's artifacts run in CI only. Nothing from TheRock is cloned or downloaded locally.
