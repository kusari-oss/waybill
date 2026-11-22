# Feature Specification: one library, one component, when the curated scanner and an installed-library record name or version it differently (#1662)

**Feature Branch**: `1645-split-library-identity`
**Created**: 2026-11-22
**Status**: Draft
**Input**: #1662. A curated embedded-version match and an installed-library record (pkg-config, CMake package version, CMake exported targets) describe the same library file under different names or version forms, and waybill reports two components. Refs #1526, #1531, #1543, #1641.

## Background

The curated scanner (#1526, specs 1643 and 1644) reads a version from a library's banner and emits `pkg:generic/<slug>@<version>`. The installed-library reader (#1531, spec 1563) names a component after its `.pc` file or CMake package, and gives it that record's version. When both produce the same PURL, the existing deduplication merges them into one component. expat, gmp, liblzma and zlib already merge this way on TheRock. When the PURLs differ, the library is reported twice.

**Measured at `main`'s code** (`bb9d6cc4`). The sources were:
- TheRock `measure-full` run 45039486997 at `b94a5a19`, which differs from `bb9d6cc4` only by a doc comment;
- the quality-corpus census run 45039489189;
- the 21 public-corpus CycloneDX goldens at `bb9d6cc4`.

Across all of them, exactly **two libraries split, both on TheRock, sharing 3 files**:

| Library | Curated component | Installed-library component | Shared files | Cause |
|---|---|---|---|---|
| ncurses | `pkg:generic/ncurses@6.6`, from the banner `ncurses 6.6.20251230`; the release is kept and the patch date dropped (#1526). Files: `libncursesw.so`, `librocm_sysdeps_ncursesw.so.6`. CPE candidates `invisible-island`, `ncurses_project` and `gnu` `:ncurses:6.6` | `pkg:generic/ncursesw@6.6.20251230`, from `ncursesw.pc`. Files: `libncursesw.so`, `ncursesw.pc`. No CPE: the table has no `ncursesw` row | `lib/rocm_sysdeps/lib/libncursesw.so` | the name differs (`ncurses` and the wide-character build `ncursesw`), and the version form differs |
| llvm | `pkg:generic/llvm@23.0.0`. Files: `libLLVM-23git.so`, `libLLVM.so`, `libLLVM.so.23.0git`. CPE `llvm:llvm:23.0.0` | `pkg:generic/llvm@23.0.0git`, from `LLVMConfigVersion.cmake` (`cmake-package-version`, also detected via `cmake-find-package`). Files: the LLVM executables and the two `libLLVM` files. CPE `llvm:llvm:23.0.0git` | `lib/llvm/lib/libLLVM.so`, `libLLVM.so.23.0git` | the version form only. The binary's banner is literally `LLVM version 23.0.0git\0` (TheRock census run 45039489189), and the curated scanner's `X.Y.Z` parser drops the `git` |

Also measured:
- **The rest of the ncurses family does not conflict.** On TheRock, every other ncurses-family record claims only its own library: `formw.pc` claims `libformw.so`, and likewise `menuw.pc` and `panelw.pc`; `ncurses++w.pc` claims only itself. None of those files carries a curated match.
- **No other target is affected.** No curated component in the quality corpus or the public corpus shares a file with another package identity. The public goldens carry no `embedded-version-string` component at all.
- **The CPE table** has rows for `pkg:generic/ncurses` (`invisible-island` 321 entries, `ncurses_project` 49, `gnu` 21) and `pkg:generic/llvm` (`llvm:llvm`, 55). It has none for `ncursesw`.
- **NVD's version forms**, probed 2026-11-22 (`measurements/nvd-version-forms.sh` and its dated output):
  - ncurses: 28 CVEs with 110 `cpeMatch` entries. 102 put `X.Y` in the CPE version field, and 81 of those carry the patch date in the **update** field (for example `6.5:20251206`). The other 8 are ranges bounded by `X.Y` or `X.Y.Z`. None puts the patch date in the version field.
  - `llvm:llvm`: 8 CVEs, all versioned by dates such as `2023-01-12`. Neither `23.0.0` nor `23.0.0git` matches them.
- **One more component carries a patch-date version.** The public corpus has `pkg:generic/ncurses@6.4.20221231` (`nix-closure-openai`, from the closure tier), whose CPE today is `invisible-island:ncurses:6.4.20221231`. Its version field holds the patch date, which no NVD ncurses entry does.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A library found by both its banner and its install record is one component (Priority: P1)

An operator scans an SDK whose ncurses and LLVM are found both from the library's embedded banner and from the pkg-config or CMake file installed beside it. Today each is reported twice, under two names or two versions, and a consumer counting libraries or matching vulnerabilities sees two products.

**Why this priority**: This is the defect (#1662). It doubles the component and splits its evidence: one copy has the CPE, the other has the exact version.

**Independent Test**: Use synthetic `waybill-fixture-*` trees, one per shape:
- the name variant: a library whose banner reads `ncurses X.Y.YYYYMMDD`, with an `ncursesw.pc` claiming it;
- the version suffix: `LLVM version X.Y.Zgit`, with a CMake package-version file saying `X.Y.Zgit`.

Each must give exactly one component, carrying both readers' files and evidence.

**Acceptance Scenarios**:

1. **Given** a library whose banner is `ncurses 6.6.20251230` and an `ncursesw.pc` with `Version: 6.6.20251230` whose library is that file, **When** scanned, **Then** there is exactly one component, `pkg:generic/ncurses@6.6.20251230`:
   - it lists every file either reader named: the library under both of its file names, and `ncursesw.pc`;
   - it records that it was also detected through pkg-config, in the same form as spec 1644's merged `liblzma@5.8.1`;
   - its CPE candidates are the ncurses row's three vendors, at version `6.6` with update `20251230`;
   - no `ncursesw` component remains.
2. **Given** a library whose banner is `LLVM version 23.0.0git` and a CMake package-version file saying `23.0.0git` whose exported targets include that file, **When** scanned, **Then** there is exactly one `pkg:generic/llvm@23.0.0git` component, listing the files of both.
3. **Given** the same name-variant tree but with an `ncursesw.pc` whose version disagrees (`6.5.20240427` against the banner's `6.6`), **When** scanned, **Then** both components remain, each with its own version, as in spec 1644's P-2.
4. **Given** either tree, **When** the three formats are emitted, **Then** `waybill sbom parity-check` reports no gap the same tree without the second record does not.

---

### User Story 2 - The curated LLVM version is the version the binary states (Priority: P1)

An operator scans an LLVM development build. The banner says `23.0.0git`, and today waybill reports `23.0.0`, which is a release that may not contain the same code.

**Why this priority**: It is the whole cause of the llvm split. Fixing it in the scanner makes the existing PURL merge apply, so llvm needs no new merge logic.

**Independent Test**: Unit and fixture cases for `LLVM version X.Y.Z` followed by a NUL, by a suffix and then a NUL, and by a suffix and then a space.

**Acceptance Scenarios**:

1. **Given** the banner `LLVM version 23.0.0git\0`, **When** scanned, **Then** the curated version is `23.0.0git`.
2. **Given** `LLVM version 17.0.6\0`, **When** scanned, **Then** the version is `17.0.6`, unchanged from `main`.
3. **Given** `LLVM version 18.1.0rc1 (…)`, where a space follows the suffix, **When** scanned, **Then** the version is `18.1.0rc1`.

---

### User Story 3 - Nothing else changes (Priority: P2)

An operator scanning anything where no file is claimed both by a curated match and by a differently named or versioned installed-library record sees the same output as on `main`. The one exception is FR-008's CPE form, on the components it applies to, and those are counted.

**Why this priority**: The merge is narrow by design. A wider effect would mean the rule matched libraries it should not.

**Independent Test**: B-1, spec 1643's byte-identity golden. Also the public corpus, the quality corpus and TheRock, each traced component by component against a `main` control.

**Acceptance Scenarios**:

1. **Given** spec 1643's B-1 tree (no installed-library record), **When** scanned, **Then** the output is byte-identical to `main`.
2. **Given** TheRock, **When** scanned, **Then** the only changes are:
   - ncurses becomes one component and llvm becomes one component;
   - the `ncursesw` and `llvm@23.0.0` components disappear into them;
   - the edges follow the merged components.

   Every other component is unchanged.

---

### Edge Cases

- **Two records claim the shared file**, both with a qualifying name and an agreeing version (for example `ncurses.pc` and `ncursesw.pc` naming one file). Nothing is merged, and the census records the file as `ambiguous`. One file cannot pick between two records by order.
- **The record is a different library that links the curated one statically.** For example `foo.pc` claims `libfoo.so`, which bundles zlib's banner. The record's name is not one of zlib's names (FR-004), so nothing merges, even if the versions happen to agree.
- **The curated version is a prefix of the record's only by string**, not at a component boundary: `6.6` against `6.60`. That does not agree (FR-005).
- **The record's version is the shorter one**: the curated `23.0.0git` against a record's `23.0.0`. That does not agree, because the curated version must be the less exact or equal one (FR-005). Both remain, and the census records it.
- **A curated match with no record**, as in B-1. It stays exactly as on `main`, including `ncurses@X.Y`.
- **An ncurses component with a patch-date version but no curated match**, such as the closure tier's `ncurses@6.4.20221231`. Its CPE takes NVD's form (FR-008). This is counted, and it is the only change of its kind in the corpora.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census first, log-only)**: Before any output changes, a DEBUG census event SHALL be emitted for every file claimed both by a curated match and by an installed-library record whose PURL differs. The event records:
  - the file;
  - the curated library, its version and its banner text;
  - each such record's name, version and reader (pkg-config, CMake package version or CMake exported targets);
  - the verdict FR-003 to FR-006 would give: `merge`, `version-disagrees`, `name-unrelated` or `ambiguous`.

  It SHALL be run on TheRock, the quality corpus and the public corpus, and recorded. If a count contradicts FR-002 to FR-008, the spec is amended with that evidence before anything is wired.
- **FR-002 (LLVM keeps its suffix)**: The curated LLVM pattern SHALL keep the version exactly as the banner writes it. That is `X.Y.Z` followed by any suffix of lowercase ASCII letters and digits attached directly to it (`git`, `rc1`), ending at a NUL, a space or the region's end. A banner without a suffix is unchanged. The census SHALL count every curated LLVM match on the three corpora and report the old and new version of each.
- **FR-003 (merge condition)**: A curated match and an installed-library record SHALL become one component when all of these hold:
  - the record's own library files include a file the curated match was read from;
  - the record's name is one of the curated library's names (FR-004);
  - the versions agree (FR-005);
  - exactly one record satisfies the three conditions above for that file (FR-006).
- **FR-004 (a library's names: a closed list)**: Each curated library SHALL have a fixed set of names, written out for every library, with a source for each entry. The set holds the curated slug, plus any name that upstream's own build gives the same library. For ncurses, that is the build-variant names its `configure` produces: `ncursesw` (wide characters), `ncursest` (threads) and `ncursestw`. Every other curated library has its slug only. No other name ever matches, and there is no pattern-based or free-form alias.
  - *Why a list and not a rule*: the shared file alone cannot tell "the same library" from "a library that bundles it statically" (Edge Cases). A name rule such as "slug plus one letter" would accept unrelated names, and has no source.
- **FR-005 (versions agree)**: The curated version agrees with the record's version when the two are equal, or when the curated version followed by a `.` begins the record's version: `6.6` agrees with `6.6.20251230`, but not with `6.60`. A suffix attached without a `.` does not agree: `23.0.0` with `23.0.0git` does not, which is why FR-002 is needed.
- **FR-006 (ambiguity)**: When two or more records satisfy FR-003's first three conditions for one file, nothing SHALL be merged for that file. The census records `ambiguous`.
- **FR-007 (what survives a merge)**:
  - The merged component's **name** is the curated slug, because it is the name the CPE table has rows for. `ncurses` has three pairs, and `ncursesw` has no row.
  - Its **version** is the record's, the more exact of the two.
  - Its **files and evidence** are the union of both, in the same shape as an existing PURL merge such as spec 1644's `liblzma@5.8.1`.
  - The record's original name (`ncursesw`) is recorded on the component, so that it is not lost. The plan chooses the field, and prefers a native CycloneDX or SPDX construct over a new `waybill:*` annotation (Principle V).
- **FR-008 (ncurses CPE form)**: For every `pkg:generic/ncurses` component whose version is `X.Y.YYYYMMDD`, the CPE candidates SHALL put `X.Y` in the version field and `YYYYMMDD` in the update field. This is NVD's form, measured: 81 of 110 entries put the patch date in the update field, and none puts it in the version field. A version `X.Y` keeps `*` in update, as on `main`. This applies whichever reader produced the component. The census SHALL count the components it changes; the public corpus has exactly one today.
- **FR-009 (disagreement keeps both)**: When FR-003 does not hold, both components SHALL remain exactly as on `main`.
- **FR-010 (formats agree)**: The merged component SHALL be represented equally in CycloneDX, SPDX 2.3 and SPDX 3. `waybill sbom parity-check` SHALL report no gap that `main` does not.
- **FR-011 (scope of change)**: Output SHALL be byte-identical to `main` except for:
  - components merged under FR-003;
  - the LLVM versions FR-002 changes;
  - the ncurses CPEs FR-008 changes.

  Each of these is counted by the census.
- **FR-012 (no new dependencies)**: Zero new Cargo dependencies.

### Key Entities

- **Curated match**: a library, version and the files it was read from, produced by the embedded-version scanner.
- **Installed-library record**: a component from a pkg-config file, a CMake package-version file or CMake exported targets, with a name, a version, and the library files the record itself declares.
- **Library names**: the closed list of a curated library's names (FR-004).
- **Shared-file verdict**: per file, one of `merge`, `version-disagrees`, `name-unrelated` or `ambiguous` (FR-001).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On TheRock, ncurses and llvm are each exactly one component. This is traced component by component against a `main` control, and every other change is accounted for under FR-011.
- **SC-002**: The census (FR-001) is recorded for TheRock, the quality corpus and the public corpus before the merge is wired. Every count is consistent with FR-002 to FR-008, or the spec is amended.
- **SC-003**: The synthetic name-variant and version-suffix trees each give exactly one component, carrying both readers' files. The version-disagreement tree gives two.
- **SC-004**: Each rule is guarded by a test that fails when the rule is removed, shown by mutation:
  - the name list (FR-004);
  - the dotted-prefix agreement (FR-005);
  - the single-record condition (FR-006);
  - the LLVM suffix (FR-002);
  - the ncurses CPE split (FR-008).
- **SC-005**: `waybill sbom parity-check` shows no new gap on any acceptance tree or on TheRock.
- **SC-006**: B-1 is byte-identical to `main`. The public corpus differs from its goldens only in components FR-011 names, each traced.
- **SC-007**: TheRock's scan time is within 1.05× `main`, measured as the median of at least 3 interleaved runs per arm with one warm-up each, and reported with min, median and max.

## Assumptions

- The installed-library reader already records which library files a record declares (spec 1531's `.pc` `Libs`, spec 1563's exported targets). That is the "record's own library files" in FR-003; no new file attribution is introduced.
- NVD's version conventions are observations dated 2026-11-22 (`measurements/nvd-version-forms-2026-11-22.txt`). The probe is committed so they can be re-checked.
- The ncurses build-variant names come from ncurses' own `configure` documentation, `--enable-widec` and `--with-pthread`. The plan records the source.
- TheRock is scanned in CI only, through `kusari-sandbox/waybill-fixture-therock-sdk`, dispatched with `--ref main` and full SHAs, with every `refs.txt` verified.
- Fixtures use synthetic `waybill-fixture-*` names. The curated banners and `.pc` names under test (`ncurses`, `ncursesw`, `LLVM`) are the library identities being tested, embedded in synthetic files.
