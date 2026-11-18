# Feature Specification: Open static archives on installed trees

**Feature Branch**: `1612-static-archives`
**Created**: 2026-11-18
**Status**: Draft
**Input**: User description: "#1534: open static archives (.a) on installed trees, so third-party code compiled into a shipped static library is identified. Measured in CI only … .a files are 822 files and 4.58 GB, 27.6% of the tree's 16.55 GB … 758 of them already appear in the SBOM as files of a package … the binary tier drops every .o/.a before reading it as a build intermediate … trace before designing: extend the fixture's census … Decisions to make: (1) where archives are opened … (2) how an archive is represented … (3) which per-member analyses run … (4) the ar format … (5) bounds … Acceptance: on the TheRock SDK, the census's archive-only third-party trees appear as components nested under their archive with evidence; documents identical across repeated scans …" (full text in the conversation that created this branch)

The spec number (1612) is the next in sequence. It is unrelated to issue or PR #1612.

This branch is cut from `binary-tier-parallel` (PR #1613), because archive work runs inside the parallel per-file stage that PR adds. It is rebased onto `main` once #1609, #1612 and #1613 land.

## Background

### What is known (measured)

All figures are from the pinned ROCm TheRock SDK tarball (`therock-dist-linux-gfx1150-7.14.0a20260612`, 16.55 GB unpacked, 38,694 files). They were measured in CI in `kusari-sandbox/waybill-fixture-therock-sdk` (its file listing, `listing/all-files.txt`) and in that repository's `measure-full` and `compare` runs of 2026-11-18. The tarball is never fetched or scanned locally.

- **Static archives are the largest share of shipped bytes waybill never looks inside:** 822 `.a` files, 4.58 GB, 27.6% of the tree. Executables are larger (6.12 GB), but waybill already analyses those.
- **The bytes are concentrated:**
  - 10 top-level `lib/*.a` hold 2.47 GB;
  - `lib/llvm/` holds 687 archives and 1.96 GB;
  - `lib/rdc/` holds 122 archives;
  - `lib/rocm_sysdeps/` holds 3.
- **758 of the 822 already appear in the document**, as files of a package. They are attributed by CMake exported targets (spec 1532, #1563) or by version-file records (spec 1531). Nothing inside them is examined.
- **Archives are dropped before they are read.** The binary tier's early build-intermediate gate drops every `.o` and `.a` before reading it, unless the path could fold into a CPython or JDK umbrella. That is right for a source or build tree, where these are compilation intermediates, and wrong for an installed tree that ships them.

### What is not known yet

What is inside the archives:
- how many members each holds;
- what source paths, version strings and symbols those members carry;
- which third-party code (googletest, abseil, fmt, zlib, code LLVM vendors, …) appears **only** inside archives, and not also in a shared library or executable that waybill already analyses.

Only that last group is new information, and its size decides this feature's value. The spec's success criteria are therefore stated against a census taken first (FR-001), not against figures guessed here.

### What exists to build on

- **Spec 1534 (#1532)**:
  - reads the source paths embedded in an installed binary;
  - attributes the binary to the package they name;
  - nests the third-party trees they reveal under the binary as bundled components.
- **The curated version-string scanner and the symbol-fingerprint scanner** (milestones 096 and 099; #1527 extends fingerprint coverage).
- **Spec 1533 recognises an installed tree** (C226, `installed-tree: recognised`). Source and build trees are not recognised as installed.
- **Compiler and debugger test inputs** are skipped by the binary tier (#1524). TheRock's source tree has many `.o` and `.a` files under `test/**/Inputs/`.
- **An `ar` reader** exists in the ipk reader (milestone 187). It reads only short BSD member names. Archives built by GNU and LLVM toolchains also use:
  - a symbol index (`/`, `/SYM64/`);
  - a long-name table (`//`, with members named `/N`);
  - sometimes the thin format (`!<thin>\n`), whose members are external files.
- **Mach-O universal (fat) archives** occur in the Nix closure tier (`lib_links.rs`). They do not occur on the TheRock SDK, which is Linux-only.
- **The binary tier is parallel** (#1613): files are read, parsed and hashed on several threads, and stateful work stays in walk order. Two recent fixes removed whole-file re-reads (#1608, #1611), and this feature must not bring them back.

## Clarifications

### Session 2026-11-18

- Q: Which opened archives become components? → A: Every opened archive becomes one binary component with a `file-sha256` identity, like a shared library, whether or not its members reveal anything. Findings nest under it. On the SDK that is up to 822 new components.
- Q: Should an archive owned by an OS package (dpkg, rpm, apk) be opened? → A: No. It is reported through its package, as a claimed binary is, and counted in the coverage record as `owned-by-os-package`.
- Q: On by default, or opt-in? → A: On by default in installed trees. `--no-binary-scan all` turns it off with the rest of the binary tier; there is no separate flag.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Third-party code compiled into a shipped static library is identified (Priority: P1)

A consumer scans an installed SDK that ships static libraries. Third-party code that the vendor compiled into one of those archives appears in the document as a component, nested under the archive that contains it, with the evidence that identified it. This holds even when that code appears nowhere else in the tree.

**Why this priority**: this is the feature. A consumer who scans an SDK for vulnerable third-party code, or for licence obligations, today gets nothing for 27.6% of its bytes.

**Independent Test**: a synthetic installed tree (recognised as installed under spec 1533's rules) containing a static archive. One member carries embedded source paths into a vendored third-party tree, a curated version string, and that library's exported symbols. The document shows the archive, the bundled third-party component nested under it, and the evidence for each.

**Acceptance Scenarios**:

1. **Given** an installed tree with an archive whose members' embedded source paths name a vendored third-party tree, **When** it is scanned, **Then** that tree appears as one bundled component nested under the archive, carrying the evidence kind and the members that support it. It does not appear once per member.
2. **Given** an archive whose members carry a curated version string for a library, **When** it is scanned, **Then** that library appears under the archive with the version the string gives.
3. **Given** an archive already attributed to a package (exported targets or a version-file record), **When** it is scanned, **Then** that attribution is unchanged, and what the members reveal is added beneath the archive.
4. **Given** the TheRock SDK, **When** it is scanned, **Then** every third-party tree that the census (FR-001) found only inside archives appears as a component nested under at least one archive that contains it.

---

### User Story 2 - Source and build trees, and test inputs, are unaffected (Priority: P1)

A consumer scans a source checkout or a build tree full of `.o` and `.a` intermediates and compiler test inputs. The document is the same as before this feature.

**Why this priority**: in a source or build tree, opening intermediates would flood the document with build products and test fixtures, the defect #1524 fixed. The same protection must hold in the new path.

**Independent Test**: a synthetic source tree with a `.a` in a build directory and a `.a` under a compiler test-input directory, scanned before and after this feature. The documents are identical.

**Acceptance Scenarios**:

1. **Given** a tree not recognised as installed, **When** it is scanned, **Then** no archive is opened, and the document is identical to the one produced without this feature.
2. **Given** an installed tree with an archive under a compiler test-input directory (#1524's rule), **When** it is scanned, **Then** that archive is not opened.
3. **Given** TheRock's source tree and the public corpus, **When** they are scanned, **Then** their documents are unchanged, except for differences that have been traced and explained.

---

### User Story 3 - Every archive is accounted for (Priority: P2)

A consumer reading the document can tell how many archives were found, how many were opened, and why each of the others was not. No archive disappears silently.

**Why this priority**: without this, an archive skipped for size, format or a bounds limit looks the same as an archive that contained nothing worth reporting. That is the "a zero can mean broken" failure this project has repeatedly guarded against.

**Independent Test**: a synthetic installed tree with:
- one archive opened normally;
- one over the size bound;
- one with more members than the member bound;
- one thin archive;
- one malformed archive.

The document's archive coverage record counts each against its reason, and the counts add up to the number of archives found.

**Acceptance Scenarios**:

1. **Given** archives that were opened, skipped by a bound, skipped for format, or failed to parse, **When** the document is read, **Then** a document-scope coverage record gives each count by reason, and the counts sum to the archives found.
2. **Given** an archive that was opened only partly because a member exceeded the per-member bound, **When** the document is read, **Then** the partial opening is recorded as such.

---

### User Story 4 - Archives in the formats real toolchains write are read (Priority: P2)

Archives produced by the GNU and LLVM toolchains are read correctly: symbol index, long member names and all. A member's name is its real name, not a truncated or table-relative one.

**Why this priority**: toolchain archives with long member names are the common case. LLVM's own libraries have members such as `AArch64TargetMachine.cpp.o`. A reader that mishandles them misattributes members or skips the archive.

**Independent Test**: synthetic GNU-format and BSD-format archives built at test time, with short names, long names, a symbol index, and a thin archive. Each member is listed with its full name; the symbol index is not mistaken for a member; the thin archive's external members are recorded, not read.

**Acceptance Scenarios**:

1. **Given** a GNU-format archive with a symbol index and a long-name table, **When** it is read, **Then** every object member is found with its full name, and the index and name table are not reported as members.
2. **Given** a BSD-format archive with inline long names (`#1/N`), **When** it is read, **Then** every member is found with its full name.
3. **Given** a thin archive, **When** it is read, **Then** it is counted as thin in the coverage record. Its external members are not followed through the archive: they are separate files, and the walk examines them on its own terms.

### Edge Cases

- **An archive whose members are not object files** (data, nested archives, LLVM bitcode). Only object members recognised as ELF, Mach-O or PE are analysed. The others are counted, not opened. Nested archives are not recursed into.
- **The same third-party tree found in many members of one archive.** It becomes one finding per archive: evidence merges, it is not repeated per member.
- **The same third-party tree in many archives**, or in an archive and in a shared library. It is nested under each container where it was found, as #1532 does for binaries. Deduplication across containers follows the existing rules for bundled components.
- **An archive that is also a symlink target or a hard link.** It is opened once, consistent with how the binary tier treats linked binaries today.
- **An archive larger than the per-archive bound, or with more members than the member bound.** It is recorded and not opened, or opened up to the bound and recorded as partial.
- **A malformed archive** (truncated header, size field past end of file). It is recorded as failed. The scan continues, and nothing partial from that archive is emitted unless the members read before the fault are well-formed.
- **A Mach-O universal (fat) archive.** It is out of scope for this feature; it is counted as skipped by format.
- **An `.a` that is not an archive** (wrong magic). It is counted under its own reason and not opened.
- **An archive owned by an OS package** (for example `libc.a` from a `.deb` in a container image). It is not opened; it is counted as `owned-by-os-package`.
- **Archives in an installed tree's test payload** (spec 1533's test scope). They are opened, and their components inherit the test scope, as other test-scoped content does.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census first)**: Before design, the fixture repository MUST gain a census of every `.a` in the pinned SDK. It records:
  - for each archive: its format, member count, and the members' total size;
  - for each member: its embedded source paths (spec 1534's rules), curated version strings, and defined and dynamic symbols.
  
  It MUST report which third-party trees appear only inside archives, and which also appear in shared libraries or executables. The success criteria's targets are taken from this census. The census is a measuring script, not a model of the reader.
- **FR-002**: Static archives MUST be opened only in a tree recognised as installed (spec 1533). Elsewhere the existing build-intermediate gate applies, unchanged.
- **FR-003**: An archive under a compiler test-input directory (#1524's rule) MUST NOT be opened, even in an installed tree.
- **FR-003a**: An archive claimed by an OS package (dpkg, rpm, apk) MUST NOT be opened. This is the same claim check the binary tier applies to claimed binaries; it prevents the package's content from being reported a second time under another identity.
- **FR-004**: The archive reader MUST read:
  - BSD-format archives (short names and `#1/N` inline long names);
  - GNU-format archives (`/` and `/SYM64/` symbol indexes, the `//` long-name table, and `/N` member references).
  
  It MUST list each object member under its full name, and MUST NOT report index or name-table entries as members.
- **FR-005**: A thin archive MUST be recognised and counted. Its external members MUST NOT be read through the archive.
- **FR-006**: Mach-O universal (fat) archives are out of scope for this feature. They MUST be counted as skipped by format.
- **FR-007**: For each object member, the system MUST run the same analyses it runs on an installed binary today:
  - embedded source paths;
  - curated version strings;
  - symbol fingerprints.
  
  It MUST merge the results per archive into at most one finding per third-party package or tree.
- **FR-008**: Every archive that is opened MUST be represented as one binary component, whether or not its members reveal anything. Its identity is the file's SHA-256, as a shared library's is today. What its members reveal MUST be nested under it, as bundled components, with the evidence kind and the supporting member names. Members MUST NOT become components of their own.
- **FR-009**: An archive already attributed to a package MUST keep that attribution unchanged.
- **FR-010**: The system MUST enforce:
  - a per-archive size bound;
  - a per-member size bound;
  - a member-count bound.
  
  It MUST record every archive skipped or partly opened because of one, with the reason.
- **FR-011**: The document MUST carry a document-scope archive coverage record. It counts the archives found, opened, partly opened, and skipped by each reason (bound, thin, fat, not an archive, malformed, test input, owned by an OS package), and the counts MUST sum to the archives found.
- **FR-012**: Every new annotation MUST be registered as a catalog row, with parity extractors for all three formats, before it is emitted. All three formats MUST agree on it.
- **FR-013**: Opening archives MUST NOT add whole-file re-reads. Each archive is read once, in the binary tier's per-file stage.
- **FR-014**: Documents MUST be identical across repeated scans of the same tree (spec 1542's repeat check), whatever the thread scheduling.
- **FR-015**: Tests MUST use synthetic `waybill-fixture-*` archives built at test time:
  - GNU and BSD formats;
  - short and long names;
  - a symbol index;
  - a thin archive;
  - an archive under a compiler test-input directory;
  - an archive in a tree not recognised as installed.
- **FR-015a**: Archive opening MUST be on by default, in trees recognised as installed. `--no-binary-scan all` MUST turn it off along with the rest of the binary tier. No new flag is added.
- **FR-016**: No new Cargo dependencies.

### Key Entities

- **Static archive**: a shipped `.a` file, identified by its content hash, as a shared library is. It is emitted as a component whenever it is opened. Attributes: format (BSD, GNU, thin, fat), member count, and whether it was opened, partly opened or skipped, with the reason.
- **Member**: an object file inside an archive, identified by its full name within the archive. It is analysed, never emitted as a component.
- **Archive finding**: a third-party package or tree revealed by one or more members of one archive. It carries the evidence kind, version where known, and the supporting member names. It is nested under the archive.
- **Archive coverage record**: the document-scope counts of archives found, opened, partly opened, and skipped, by reason.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the TheRock SDK, 100% of the third-party trees the census (FR-001) finds only inside archives appear in the document, nested under an archive that contains them, with evidence.
- **SC-002**: On the TheRock SDK, the archive coverage record accounts for all 822 `.a` files: the counts by reason sum to 822, and every skipped archive has a recorded reason.
- **SC-003**: TheRock's source tree and the public corpus produce documents identical to `main`'s, except for differences traced and explained in the feature's measurements. No archive under a compiler test-input directory is opened.
- **SC-004**: Repeated scans of the TheRock SDK produce identical documents in all three formats.
- **SC-005**: The three formats agree on every new annotation on the TheRock SDK.
- **SC-006**: The TheRock SDK scan's median time, interleaved against `main` on the same runner class, grows by no more than the census-measured cost of reading the archives' bytes once.
  - The archives are 4.58 GB of the tree. The bound is expressed as a ratio against `main`, set in the plan from the census. It is not an absolute guessed here.
- **SC-007**: The quality corpus is traced against a `main` control, and every change in it is explained.

## Assumptions

- **Installed trees only.** "Installed tree" means spec 1533's recognition. Archives in trees it does not recognise keep today's behaviour, even if some are genuinely shipped. Widening that is a separate decision.
- **Per-member analysis reuses existing scanners.** The embedded-path, version-string and symbol-fingerprint scanners apply to object members as to whole binaries. Gaps in their coverage (#1527) are out of scope here.
- **One component per archive.** Representing the archive as a single binary component, with findings nested under it, is consistent with how installed binaries and their bundled trees are represented today (spec 1534). Shared libraries get a component each whether or not anything is found inside them (508 on the SDK), and archives are treated the same.
- **Fat archives are deferred.** They occur only in the Nix closure tier, not on the TheRock SDK, so they are excluded and counted.
- **Thin archives are not followed** through the archive. Their members are separate files, which the walk already examines.
- **Bounds are set in the plan.** The values are chosen from the census's observed distribution of archive and member sizes, not here.
