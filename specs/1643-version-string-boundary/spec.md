# Feature Specification: An embedded version string is found when a byte other than NUL precedes it (#1568, #1641)

**Feature Branch**: `1643-version-string-boundary`
**Created**: 2026-11-22
**Status**: Draft
**Input**: #1568 and the boundary half of #1641. The curated embedded-version scanner (`waybill-cli/src/scan_fs/binary/version_strings.rs`) accepts a match only when the byte before it is NUL, so version text in vendor-built libraries is missed when another byte precedes it. Closes #1568; Refs #1641, #1525, #1526, #1543.

## Background

The curated scanner reads a binary's read-only string sections and looks for libraries that state their own version: `pkg:generic/<library>@<version>`, with `waybill:evidence-kind = embedded-version-string`. To avoid matching a library name inside unrelated text, it accepts a match only at the start of a string. The byte before the match must be NUL, or the match must start the region (`at_boundary`, `version_strings.rs:135`). zlib's literal begins with a space, so #1525 also accepts one leading space, but only when the byte before that space is NUL (lines 139-141).

String tables are not always NUL-separated at the point where a library's literal begins. **Measured in the issues** (re-verified at the current `main` as the first research step):

| Library | Byte before the literal | Result | Source |
|---|---|---|---|
| zlib (Nix-built `libz.1.3.2.dylib`) | `0x00`, unmodified | `pkg:generic/zlib@1.3.2` | #1568, waybill `cba55f7b` |
| zlib | `0x2D` (`-`) before `" deflate"` | `pkg:generic/zlib`, no version (symbol fingerprint only) | #1568 |
| ncurses (Nix-built `libncursesw.6.dylib`) | `0x00`, unmodified | `pkg:generic/ncurses@6.6` | #1641, waybill `786091a8` |
| ncurses | `0xFF` before `ncurses 6.6.20251230` | not identified | #1641 |

Both non-NUL layouts occur in real vendor-built Linux shared libraries (#1641's table: `- deflate 1.3.2 Copyright …`, and `ncurses 6.6.20251230` preceded by `0xff` in `.rodata`).

The NUL rule protects against a library name inside other text, such as "requires OpenSSL 1.1.1 or later". That protection is only needed where the pattern itself does not fix both ends of the match. Three patterns already do: the library name starts the match, and a fixed suffix or a NUL terminator ends it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A bundled zlib or ncurses is versioned whatever byte precedes its banner (Priority: P1)

An operator scans an SDK or installed tree containing vendor-built copies of zlib and ncurses. Today these come out unversioned or missing, depending only on the byte that happens to precede the library's banner. A vulnerability scanner cannot match a component that has no version.

**Why this priority**: This is the defect in both issues. It affects real ROCm SDK libraries (#1543), and a missing version is a missed vulnerability match.

**Independent Test**: Scan synthetic shared libraries whose read-only section holds the zlib or ncurses banner preceded by each of NUL, `-` and `0xFF`. All three give the versioned component.

**Acceptance Scenarios**:

1. **Given** a synthetic library whose read-only section holds `\0 deflate 1.3.2 Copyright …`, `- deflate 1.3.2 Copyright …` or `\xFF deflate 1.3.2 Copyright …`, **When** it is scanned, **Then** each gives `pkg:generic/zlib@1.3.2`.
2. **Given** a synthetic library whose read-only section holds `ncurses 6.6.20251230\0` preceded by NUL, `-` or `0xFF`, **When** it is scanned, **Then** each gives `pkg:generic/ncurses@6.6`.
3. **Given** `expat_2.7.5\0` preceded by `-` or `0xFF`, **When** it is scanned, **Then** it gives `pkg:generic/expat@2.7.5`.

---

### User Story 2 - A library name inside other text is still not a match (Priority: P1)

The relaxed rule must not turn mentions into components. A name that is part of a longer identifier (`libncurses 6.6…`, `xdeflate 1.3.2 Copyright`) is not the library's own banner. Neither is a version in a pattern whose end is not fixed (`requires OpenSSL 1.1.1 or later`).

**Why this priority**: A fabricated component is worse than a missing version. It sends a scanner after a library that is not there.

**Independent Test**: The same fixtures with an identifier character (a letter, a digit, `_`) before the literal give no versioned component. A pattern classed NUL-only still requires NUL.

**Acceptance Scenarios**:

1. **Given** `x deflate 1.3.2 Copyright …` (an identifier character before zlib's leading space), **When** it is scanned, **Then** no `zlib@1.3.2` is produced.
2. **Given** `lncurses 6.6.20251230\0`, `9ncurses …` or `_ncurses …`, **When** it is scanned, **Then** no ncurses component is produced.
3. **Given** `-OpenSSL 3.0.11\0` or any other NUL-only pattern behind a non-NUL byte, **When** it is scanned, **Then** the result is unchanged from `main`: no match.

---

### User Story 3 - Nothing else changes (Priority: P2)

Binaries whose curated matches are already NUL-preceded produce the same documents as on `main`.

**Why this priority**: The scanner runs on every binary in every scan. A change here must be traceable component by component.

**Independent Test**: The existing version-string tests, the installed-library integration tests and the public corpus goldens pass unchanged. Every corpus and TheRock difference against a `main` control is a gained or changed version on one of the three relaxed patterns, and is traced.

**Acceptance Scenarios**:

1. **Given** a binary whose zlib, expat and ncurses banners are NUL-preceded, **When** it is scanned, **Then** the output is byte-identical to `main`'s.
2. **Given** the public corpus, the quality corpus and TheRock's SDK fixture, **When** each is scanned at this branch and at `main`, **Then** every difference is listed and traced to one binary and one pattern.

---

### Edge Cases

- **zlib without the leading space.** A banner that starts a string at `deflate X.Y.Z Copyright` is accepted on `main` and stays accepted.
- **Two different versions of one library in one binary.** The relaxed rule could find a second banner that the NUL rule hid. Each distinct version is already emitted as its own match, so the census counts any binary where this newly happens; each case is traced, not assumed.
- **The literal at offset 0 of a region.** It has no preceding byte. It is accepted, as on `main`.
- **A non-ASCII byte before the literal** (`0x80`–`0xFF`). It is not an identifier character, so it is accepted. #1641's ncurses case is exactly this.
- **Whitespace or punctuation before the literal** (` `, `/`, `.`, `-`, `:`). Accepted. For zlib this is the byte before its leading space. A space is where a mention inside another program's sentence would come from ("built with ncurses 6.6.20251230"), so the census counts whitespace-preceded matches separately and judges each one (SC-001, FR-005).
- **A literal at the start of a later section.** The scanned region joins the read-only sections end to end, so a literal that starts a section is preceded by the last byte of the section before it. It is judged like any other preceding byte; the census records it with the rest.
- **NUL-only patterns behind a non-NUL byte.** Unchanged. The census counts such occurrences per pattern as information for later work. No behaviour depends on the count.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each curated pattern MUST be classified as either **anchored** or **NUL-only**. A pattern is anchored only when both ends of its match are fixed: the library's name starts it, and after the version comes a fixed literal suffix or a required NUL terminator. The classification, decided here:

  | Library | Pattern | What ends the match | Class |
  |---|---|---|---|
  | zlib | ` deflate X.Y.Z[a-z] Copyright` | fixed suffix ` Copyright` | anchored |
  | expat | `expat_X.Y.Z` | required NUL | anchored |
  | ncurses | `ncurses X.Y[.YYYYMMDD]` | required NUL | anchored |
  | OpenSSL | `OpenSSL X.Y.Z[a-z]` | any non-version byte | NUL-only |
  | BoringSSL | `BoringSSL <40 hex>` | nothing checked | NUL-only |
  | SQLite | `SQLite version X.Y.Z[.W]` | NUL or space | NUL-only |
  | curl | `libcurl/X.Y.Z` | nothing checked | NUL-only |
  | PCRE | `PCRE X.Y YYYY-MM-DD` | nothing checked after the date | NUL-only |
  | PCRE2 | `PCRE2 X.Y YYYY-MM-DD` | nothing checked after the date | NUL-only |
  | GnuTLS | `GnuTLS X.Y.Z` | nothing checked | NUL-only |
  | LibreSSL | `LibreSSL X.Y.Z` | nothing checked | NUL-only |
  | LLVM | `LLVM version X.Y.Z` | nothing checked | NUL-only |
  | OpenJDK | `OpenJDK X.Y.Z[+B]` / `OpenJDK 8uX[-bY]` | any non-version byte | NUL-only |
  | PHP | `X-Powered-By: PHP/X.Y.Z` | nothing checked | NUL-only |
  | Node.js | `node.js/vX.Y.Z` | nothing checked | NUL-only |

- **FR-002**: An anchored pattern MUST be accepted when the byte before its first character is not an identifier character (`[A-Za-z0-9_]`), or when the match starts the region. For zlib the first character is its leading space, so the test applies to the byte before that space. zlib's existing form that starts at `deflate` with no leading space MUST still require a string boundary, as on `main`.
- **FR-003**: A NUL-only pattern MUST keep `main`'s rule unchanged: the byte before the match is NUL, or the match starts the region.
- **FR-004**: The classification MUST be total over `CuratedLibrary`. Adding a variant without classifying it MUST fail to compile or fail a test that iterates every variant (the #1246 convention), not silently default to either class.
- **FR-005**: If the census (SC-001) finds a false positive on an anchored pattern, meaning a new match that is not the library's own banner, that pattern MUST move to NUL-only and this spec MUST be amended with the evidence before implementation.
- **FR-006**: Output MUST be byte-identical to `main` for any binary in which every curated match is NUL-preceded or starts its region.
- **FR-007**: No new annotation, catalog row or output field. A version gained this way is emitted exactly as a NUL-preceded match of the same pattern would be, so the three formats agree without parity changes.

### Key Entities

- **Curated pattern**: one `CuratedLibrary` variant's signature, its version parser and its class (anchored or NUL-only).
- **Preceding byte**: the byte immediately before a pattern's first character in the read-only region, if any.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (census before design)**: Before the rule changes, a log-only build counts, on TheRock's SDK fixture (CI only, through kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs and `refs.txt` verified) and on the quality corpus:
  - per pattern, the occurrences preceded by NUL, by whitespace, by another non-identifier byte and by an identifier character;
  - the binaries that would gain or change a version under FR-002, each listed with its library and version;
  - for each new match, whether it is the library's own banner or a false positive, judged from the surrounding bytes.
- **SC-002**: Synthetic `waybill-fixture-*` libraries, one per preceding byte (NUL, `-`, `0xFF`, and an identifier character), for zlib and ncurses. The first three give the versioned component and the identifier-character case gives none.
- **SC-003**: Restoring `main`'s NUL-only rule for anchored patterns fails at least one test, and classing an anchored pattern as NUL-only fails at least one test.
- **SC-004**: `waybill sbom parity-check` agrees for every fixture.
- **SC-005**: The public corpus has no census of its own: at the log-only commit its goldens must be byte-identical, and its diff at the final commit is its census. Each changed public-corpus component is judged as SC-001 judges a new match, and traced to one binary and one anchored pattern. Every quality-corpus and TheRock difference against a `main` control is traced, component by component, to one binary and one anchored pattern.
- **SC-006**: Scan time on TheRock's SDK fixture is within 1.05× `main`'s, median of at least 3 runs per arm.

## Assumptions

- The issue measurements were taken at `cba55f7b` and `786091a8`. The scanner's boundary code has not changed since #1526, and research re-verifies the behaviour at the current `main` before design.
- Fixtures are built with the existing synthetic ELF helper (`waybill-cli/tests/common/installed_lib.rs::elf_shared_lib`), which places bytes in `.rodata`. No real library is committed.
- A fixed suffix (` Copyright`) or a required NUL after the version makes a match specific enough that the preceding byte only needs to rule out a longer identifier. Patterns whose end is not checked keep the NUL rule, whatever their prefix.
- Out of scope: new patterns for bzip2, GMP and xz (#1641's second half), SQLite's bare version (#1555), libcap (#1556), and changing the terminator checks of NUL-only patterns.
- Zero new Cargo dependencies.
