# Feature Specification: bzip2, GMP and xz are versioned from the bare version they embed, when a second anchor names the library (#1641)

**Feature Branch**: `1644-bare-version-anchors`
**Created**: 2026-11-22
**Status**: Draft
**Input**: The half of #1641 that spec 1643 left open. bzip2, GMP and xz carry their versions in the binary, but waybill never reads them. Refs #1641, #1526, #1555, #1556, #1662; tracked in #1543.

## Background

The curated scanner (`waybill-cli/src/scan_fs/binary/version_strings.rs`) reads a binary's read-only string region and emits `pkg:generic/<library>@<version>`, with `waybill:evidence-kind = embedded-version-string`. Every one of its 15 patterns is a single self-anchoring string: the library's name and the version are in the same literal (`OpenSSL 3.0.11`, ` deflate 1.3.2 Copyright`). Spec 1643 classed each pattern as anchored or NUL-only by what fixes the ends of that one literal.

bzip2, GMP and xz do not state their name next to their version. **Measured in #1641** at `786091a8`, on vendor-built Linux shared libraries (`strings -a` for the text; `waybill sbom scan` on each library alone for the result):

| Library | Version text in the binary | Other text naming the library | waybill |
|---|---|---|---|
| bzip2 | `1.0.8, 13-Jul-2019` | `bzip2/libbzip2: internal error number %d.` | not identified |
| GMP | `6.3.0` | `GNU MP: Cannot allocate memory …` | not identified |
| xz | `5.8.1` | none recorded; #1641 suggests an `lzma_` symbol | not identified |

A bare `X.Y.Z` names no library. #1526 and #1641 both conclude that it needs a second, independent anchor in the same file that does. Nothing in the scanner ties two pieces of evidence together today.

**On `main` `110d1a58`, read from the code:**
- No `CuratedLibrary` variant and no symbol fingerprint covers bzip2, GMP or xz. The fingerprints cover openssl, zlib, libcurl, sqlite, pcre, pcre2 and gnutls.
- `cpe_table.toml` has rows for `pkg:generic/bzip2` (`bzip:bzip2`) and `pkg:generic/xz` (`tukaani:xz`, `xz_project:xz`), but none for GMP or `liblzma`.
- The binary scan already collects the read-only string region (capped at 16 MiB) and the dynamic-symbol names (`BinaryScan.symbol_names`). On ELF that list is every named `.dynsym` entry, **imported (undefined) as well as defined**. A program that merely calls `lzma_code` from a shared liblzma therefore lists `lzma_code`, exactly as liblzma itself does.
- The pkg-config reader (spec 1531) names a component after its `.pc` file (`pkg:generic/<name>@<Version>`). An upstream xz install ships `liblzma.pc`, bzip2 ships `bzip2.pc` where its build installs one, and GMP ships `gmp.pc`. A curated match and a `.pc` record merge only when their PURLs are equal. #1662 is what happens when they differ: `ncurses@6.6` and `ncursesw@6.6.20251230` for one library.

## Clarifications

### Session 2026-11-22

- Q: GMP and xz share the bare `X.Y.Z` form. When one candidate string would version two libraries in the same binary, what happens? → A: Neither library gets that version. The census records the binary as `ambiguous-shared`, because one string cannot be the version of two libraries (FR-005a).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A bundled bzip2, GMP or xz gets its version (Priority: P1)

An operator scans an SDK or installed tree containing vendor-built copies of bzip2, GMP and xz. Today none of the three is identified from the binary, so a vulnerability scanner has nothing to match even though each library states its version.

**Why this priority**: This is the defect. It affects the same class of vendor-built libraries as #1641's table and the ROCm SDK (#1543).

**Independent Test**: Scan synthetic `waybill-fixture-*` shared libraries, each holding one library's version string and its anchor. Each gives the library's versioned component.

**Acceptance Scenarios**:

1. **Given** a library whose read-only region holds the whole string `1.0.8, 13-Jul-2019` and the text `bzip2/libbzip2`, **When** it is scanned, **Then** it gives the bzip2 component at version `1.0.8`.
2. **Given** a library whose region holds the whole string `6.3.0` and the text `GNU MP:`, **When** it is scanned, **Then** it gives the GMP component at `6.3.0`.
3. **Given** a library whose region holds the whole string `5.8.1` and which itself defines xz's anchor (FR-003), **When** it is scanned, **Then** it gives the xz component at `5.8.1`.

---

### User Story 2 - A bare version is never a component by itself (Priority: P1)

The new rule must not turn version-shaped strings into components. A version with no anchor names nothing. Neither does a binary that holds an anchor and several candidate versions, nor a program that only links against the library.

**Why this priority**: A fabricated component, or a correct library with the wrong version, is worse than a missing version. It sends a scanner after a vulnerability that is not there, or hides one that is.

**Independent Test**: The fixtures of User Story 1 with the anchor removed, with a second candidate version added, or with the xz symbol imported instead of defined. None gives a component for that library.

**Acceptance Scenarios**:

1. **Given** a library holding `6.3.0` with no `GNU MP:` text (and likewise `5.8.1` with no xz anchor, and `1.0.8, 13-Jul-2019` with no `bzip2/libbzip2`), **When** it is scanned, **Then** no component for that library is produced.
2. **Given** a library holding `GNU MP:` and the two whole strings `6.3.0` and `6.2.1`, **When** it is scanned, **Then** no GMP component is produced, and the census records the binary as ambiguous with both candidates.
3. **Given** an executable that imports xz's anchor symbols from a shared liblzma and holds the whole string `5.8.1` for some other reason, **When** it is scanned, **Then** no xz component is produced.
4. **Given** a binary holding `GNU MP:`, defining xz's anchor symbol, and containing the single whole string `5.8.1`, **When** it is scanned, **Then** neither a GMP nor an xz component is produced, and the census records the binary as `ambiguous-shared`.
5. **Given** the text `GNU MP:` and the string `6.3.0` inside a longer string (`requires 6.3.0 or later`), **When** it is scanned, **Then** no GMP component is produced, because the version must be a whole string (FR-004).

---

### User Story 3 - One library, one component (Priority: P2)

When the same tree also ships the library's `.pc` file or CMake package config, the embedded version must land on the same component, not beside it.

**Why this priority**: #1662 showed that a curated slug different from the `.pc` name reports one library twice. The new patterns must not add three more such pairs.

**Independent Test**: A synthetic tree with the library and its `.pc` file at the same version. The output has one component for the library, carrying both pieces of evidence.

**Acceptance Scenarios**:

1. **Given** a tree with a synthetic liblzma (version `5.8.1`, anchor present) and `lib/pkgconfig/liblzma.pc` declaring `Version: 5.8.1`, **When** it is scanned, **Then** exactly one component names the library at `5.8.1`.
2. **Given** the same tree with the `.pc` file declaring `5.8.0`, **When** it is scanned, **Then** both versions are reported, under one name, as two components. The disagreement is visible rather than collapsed.

---

### User Story 4 - Nothing else changes (Priority: P2)

Binaries that hold none of the three anchors produce the same documents as on `main`.

**Why this priority**: The scanner runs on every binary in every scan, and a check against a bare version shape could fire on almost any binary. Every change must be traceable component by component.

**Independent Test**: The existing version-string, installed-library and spec 1643 boundary tests and the public-corpus goldens pass unchanged. Every corpus and TheRock difference against a `main` control is traced to one binary, one library and one anchor.

**Acceptance Scenarios**:

1. **Given** a binary holding no bzip2, GMP or xz anchor, **When** it is scanned, **Then** the output is byte-identical to `main`'s.
2. **Given** the public corpus, the quality corpus and TheRock's SDK fixture, **When** each is scanned at this branch and at `main`, **Then** every difference is listed and traced.

---

### Edge Cases

- **The anchor and the version in different binaries.** Each binary is judged alone. A version in `libfoo.so` and an anchor in `libbar.so` give nothing.
- **A binary that bundles several libraries** (a static `libLLVM`, a Python extension, a compiler). It may hold an anchor and many version-shaped strings from other code. With two or more candidates it is ambiguous and gets no version (FR-005). The census counts how often this happens, so the cost of the rule is measured rather than assumed.
- **Two anchored libraries with the same version shape in one binary** (GMP and xz). A candidate is attributed to a library only when no other library anchored in the same binary also accepts it (FR-005a).
- **The same version string twice.** Two occurrences of one string are one candidate, not two.
- **bzip2's version without its date** (a build that defines `BZ_VERSION` differently). It is not a candidate. Only the `X.Y.Z, DD-Mon-YYYY` form is (FR-004).
- **A stripped or symbol-less build.** An xz library with no exported symbols has no xz anchor and gets no version. On Mach-O the anchor is a defined external symbol (`_lzma_version_string`), and on PE an export (research R3). A Mach-O build is fixture-tested; PE is not, and the docs say so.
- **The anchor text inside another program's message.** `GNU MP:` or `bzip2/libbzip2` quoted by a program that does not bundle the library. That binary would need a whole-string candidate as well to produce anything, and the census lists every match with its context so such a case is seen.
- **A library already identified by another signal** (pkg-config, a CMake config, a fingerprint, a package database). The embedded match is emitted as for any curated library, and merges when the PURL is equal (User Story 3).
- **The 16 MiB region cap.** An anchor or candidate beyond the cap is not seen, as with every curated pattern today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census before design)**: Before any version is emitted, a log-only build MUST count the facts that the remaining requirements depend on, on TheRock's SDK fixture (CI only, through kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs and `refs.txt` verified) and on the quality corpus:
  - which binaries carry each library's candidate anchors, and where each anchor sits: string region, defined dynamic symbol, or imported dynamic symbol;
  - for each anchored binary, the distinct whole-string version candidates of the library's shape (FR-004), with their count;
  - which of those binaries already get a version for that library from another reader (pkg-config, CMake, symbol fingerprint, package database), and under which name and version;
  - for xz, whether any string-region text is present in every liblzma build counted.

  Where a count contradicts a decision below, the decision MUST be amended in this spec with the evidence before implementation.

- **FR-002 (bzip2 and GMP anchors)**: bzip2's anchor is the text `bzip2/libbzip2`, and GMP's is the text `GNU MP:`. Each may appear anywhere in the binary's read-only string region, as a substring of a longer string. An anchor is evidence that the library is present, not a match, so no boundary class applies to it.

- **FR-003 (xz anchor)**: xz's anchor is a **defined** (not imported) dynamic or exported symbol named `lzma_version_string` or `lzma_version_number`. Both are in liblzma's `XZ_5.0` symbol node, so every liblzma since 5.0 exports them (research R3, read from upstream `v5.8.1`). FR-001's census confirms they are present in the builds counted. An imported symbol MUST NOT count. If the census finds a string-region text present in every liblzma build counted, that text MAY replace or supplement the symbol anchor, by amendment. Without either, xz gets no version, and the formats where the anchor is unavailable are stated in the documentation.

- **FR-004 (candidates)**: A version candidate is a whole string in the read-only region, preceded by NUL or the region's start and followed by NUL:
  - bzip2: `X.Y.Z, DD-Mon-YYYY` (digits for X, Y, Z and DD; a three-letter English month abbreviation; four digits for YYYY). The version is `X.Y.Z`.
  - GMP: `X.Y.Z` (digits only). GMP's manual says `gmp_version` has been `i.j.k` since 4.3.0, and that `i.j` was used only before that, when k was zero (research R2).
  - xz: `X.Y.Z`, optionally followed directly by `alpha` or `beta` (upstream's `LZMA_VERSION_STRING`, research R2). A snapshot's commit suffix (`-21-g1d92`) is not accepted.

  Under spec 1643's classes, every candidate is NUL-only at both ends. That is stricter than any existing pattern, because a bare version has no name to anchor it.

- **FR-005 (one version or none)**: When the library's anchor is present and exactly one distinct candidate exists, the library is emitted at that version. When two or more distinct candidates exist, no component for that library is emitted from this evidence, and the census records the binary as ambiguous with every candidate. No candidate is ever chosen by position, order or frequency.

- **FR-005a (a candidate serves one library)**: Candidates are counted per library only after removing every candidate that another library, anchored in the same binary, also accepts under FR-004. Such a shared candidate versions neither library. When removing it leaves a library with no candidate, that library's verdict is `ambiguous-shared`, and the census records the shared candidate and both libraries. A candidate accepted by only one anchored library is unaffected. For example, in a binary anchoring GMP and xz, the whole strings `6.3.0` and `5.8.1` both still have the shape of both libraries, so both libraries are `ambiguous-shared`.

- **FR-006 (bzip2's date form)**: bzip2's candidate still requires the `bzip2/libbzip2` anchor. The `X.Y.Z, DD-Mon-YYYY` form is bzip2's convention, but it is not exclusive to bzip2, and requiring the anchor costs a version only on a build that holds the version string without the error message. FR-001 counts such builds. If the census finds bzip2 libraries carrying the date form without the anchor, and no other binary carrying the form, this requirement MAY be amended to let the form stand alone.

- **FR-007 (names)**: Each library's PURL name MUST equal the name the pkg-config reader gives the same library's upstream `.pc` file, so that the two pieces of evidence merge (User Story 3):
  - bzip2: `pkg:generic/bzip2`;
  - GMP: `pkg:generic/gmp`;
  - xz: `pkg:generic/liblzma`, the name of upstream's `.pc` file.

  FR-001 states, as counted, whether TheRock or the quality corpus gains a second component for any of the three libraries. If a counted tree names the library otherwise (a CMake config, a vendor-renamed `.pc`), the spec records it, and any resulting duplicate is listed as a follow-up like #1662, not hidden.

- **FR-008 (CPE rows)**: `cpe_table.toml` MUST carry a row for every name FR-007 emits.
  - GMP and `liblzma` rows MUST be added in the form specs 1375 and #1556 use: the vendor and product as found in the CPE dictionary, the dictionary entry count, the observation date and the evidence file.
  - If the dictionary has no entry for a library, it gets no row, and the spec says so.
  - Whether `pkg:generic/xz`'s existing row stays, and why, is recorded.
  - **Consequence beyond anchored binaries** (found at implementation, `measurements/control.md`): a curated row applies to every `pkg:generic/<slug>` component, whatever reader emitted it. A `gmp` or `liblzma` component from a `.pc` file or the closure tier therefore gains these CPEs too, with or without an anchored binary. This is intended, since it is the same library under the same name, and FR-012's byte identity is about binaries without anchors, not trees with these libraries. T018 traces every such change. For the same reason the rows land with the change that versions the libraries (T012), not on the log-only commit.

- **FR-009 (totality)**: Every new `CuratedLibrary` variant MUST appear in each existing exhaustive match (the boundary class, the census, the test sample). Adding a variant without its anchor MUST fail to compile or fail a test that iterates every variant (the #1246 convention).

- **FR-010 (census reporting)**: `waybill::version_census` MUST report, for each binary carrying any of the three anchors: the library, the anchor or anchors found and where, every candidate with its offset, and the verdict (versioned, ambiguous, ambiguous-shared, or no candidate). The existing per-pattern hits keep their current shape.

- **FR-011 (no new output field)**: A version gained this way MUST be emitted exactly as any curated embedded-version match: same evidence kind, same annotations. No new annotation, catalog row or output field, so the three formats agree without parity changes. The reason a binary gets no version (no anchor, no candidate, ambiguous) is recorded in the census event (FR-010), not in the document. An SBOM field would need a catalog row and extractors in all three formats, and the document already shows the absence.

- **FR-012 (byte identity)**: Output MUST be byte-identical to `main` for any binary that holds none of the three anchors.

### Key Entities

- **Anchor**: evidence in a binary that names one of the three libraries without stating its version: a string-region text, or a defined exported symbol.
- **Candidate**: a whole NUL-delimited string in the read-only region with the library's version shape.
- **Verdict**: per binary and library, one of:
  - versioned: anchor present, one candidate;
  - ambiguous: anchor present, two or more candidates;
  - ambiguous-shared: anchor present, but every candidate is also accepted by another library anchored in the same binary (FR-005a);
  - no candidate: anchor present, none found;
  - not considered: no anchor.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: FR-001's census is recorded in the spec's measurements, with every anchored binary in TheRock and the quality corpus listed by library, anchor location, candidate count and verdict. A positive control (a target known to hold an anchor and one candidate) shows the census firing, so its zeros elsewhere mean absence, not a broken probe.
- **SC-002**: Synthetic `waybill-fixture-*` libraries for each of the three libraries give the versioned component with the anchor and one candidate. They give none with the anchor removed, with a second candidate, with a candidate shared with another anchored library, or (for xz) with the anchor symbol imported instead of defined.
- **SC-003**: Each new rule has a mutation record: dropping the anchor requirement, accepting an imported symbol, taking the first of two candidates, giving a shared candidate to both libraries, and accepting a candidate that is not a whole string each fail at least one test.
- **SC-004**: The User Story 3 fixture gives exactly one component per library when the `.pc` version agrees.
- **SC-005**: `waybill sbom parity-check` agrees for every fixture.
- **SC-006**: Every public-corpus, quality-corpus and TheRock difference against a `main` control is traced, component by component, to one binary, one library and one anchor. Any second component for one library is listed under FR-007.
- **SC-007**: Scan time on TheRock's SDK fixture is within 1.05× `main`'s, median of at least 3 interleaved runs per arm.

## Assumptions

- #1641's measurements were taken at `786091a8`. Research re-verifies them on synthetic fixtures at the current `main` before design. Spec 1643 changed only the boundary of existing patterns, so the behaviour for these three libraries is not expected to differ.
- Fixtures are built with the existing synthetic ELF helper (`waybill-cli/tests/common/installed_lib.rs::elf_shared_lib`). If it cannot place a defined dynamic symbol, extending it is test-only work. No real library is committed.
- The census runs on CI only for TheRock. TheRock is never cloned or scanned locally.
- Out of scope: SQLite's bare version (#1555), libcap (#1556), the ncurses/ncursesw pair (#1662), other bare-version libraries, and any change to the existing 15 patterns.
- Zero new Cargo dependencies.
