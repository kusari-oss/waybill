# Research: Read a CMake build tree to settle what a build actually used

**Spec**: [spec.md](spec.md) · **Measurements**: [measurements/](measurements/README.md)

Each item states its evidence class: **measured** (from the SC-001(a) probe,
24 build trees, `measurements/probe-results.md`), **read** (from CMake's or
TheRock's sources, not observed), or **decision**.

## R1. FetchContent "taken" has four agreeing signals, written at configure time *(measured)*

For every populated declaration, in all 24 runs (CMake 3.25.1, 3.31.6, 4.0.3 ×
Ninja, Make), all four of these are present, and none is present for a
declaration that was not populated:

1. directory `_deps/<lc>-src` (also `-build`, `-subbuild`);
2. cache entry `<lc>_SOURCE_DIR:STATIC` (and `<lc>_BINARY_DIR`);
3. cache entry `FETCHCONTENT_SOURCE_DIR_<UC>:PATH` (empty unless overridden);
4. stamp `_deps/<lc>-subbuild/<lc>-populate-prefix/src/<lc>-populate-stamp/<lc>-populate-done`.

`<lc>`/`<UC>` are the declared name lower- and upper-cased. Population happens
during **configure**: the `confonly` runs (no build) show all four.

- **Decision:** settle FetchContent from the cache entries (2, 3) as the primary
  signal, because `CMakeCache.txt` is one small file per build directory
  (421 lines in the probe), and confirm with (1). The stamp (4) is not needed.
- **Consequence for the spec:** FetchContent settlement needs only a
  *configured* tree, not a built one. FR-003's FetchContent bullet is
  confirmed; its "sub-build or build directory" wording is narrowed to (1)–(3).
- **Mixed-case names** *(measured, T004; 24/24 runs)*: `FetchContent_Declare(MixedCase …)`
  gives `_deps/mixedcase-{src,build,subbuild}`, `mixedcase_SOURCE_DIR`,
  `FETCHCONTENT_SOURCE_DIR_MIXEDCASE` and `mixedcase-populate-*` stamps, so
  FetchContent keys are lower-cased, except the source-dir override, which is
  upper-cased. `ExternalProject_Add(ExtMixed …)` keeps its case everywhere:
  `ExtMixed-prefix/src/ExtMixed-stamp/ExtMixed-download`.

## R2. "Not taken because found" is visible in the cache; "never made available" is not *(measured)*

- `hdronly` with `FIND_PACKAGE_ARGS`, installed on the system (`system` runs,
  6/6): no `_deps/hdronly-*`, no `hdronly_SOURCE_DIR`, no
  `FETCHCONTENT_SOURCE_DIR_HDRONLY`, and `hdronly_DIR:PATH=<install prefix>/lib/cmake/hdronly`.
- The same declaration when fetched (`fetch` runs, 6/6):
  `hdronly_DIR:INTERNAL=<build>/CMakeFiles/pkgRedirects`, which is FetchContent's
  own redirect.
- `neverused` (declared, never made available), and `extra`/`bdep` when their
  gate is off: **no signal at all**, in every run.

**Decision:** *not taken* = no population signals (R1) **and** `<name>_DIR` (any
case variant CMake writes) present and not under `CMakeFiles/pkgRedirects`.
*No evidence* = no population signal and no `<name>_DIR`. A gated-off
declaration is settled through its condition (R5), not through absence.
Absence alone never means *not taken*: the spec's over-reporting rule.

## R3. ExternalProject: the download stamp, not the stamp directory *(measured)*

- After **configure** (`confonly`, 6/6): `<name>-prefix/src/<name>-stamp/`
  exists but holds only `*-info.txt` and helper scripts; no step stamps.
- After a **successful build** (`fetch`, `system`, 12/12): step stamps
  `<name>-mkdir`, `-download`, `-update`/`-patch`, `-configure`, `-build`,
  `-install`, `-done`, and `<name>-prefix/src/<name>/` is non-empty.
- A **failed download** (`extproj` in `failed`, 6/6): only `<name>-mkdir`; no
  `<name>-download`.
- The generator matters for what *else* ran after a failure: under Make,
  `extgit` never downloaded (3/3); under Ninja it did (3/3), because Ninja keeps
  running independent edges. Reading the stamps reports what happened either
  way.

**Decision:** *taken* = `<name>-download` stamp present. *Incomplete* =
stamp directory present without it. The stamp directory's existence alone,
which configure creates, is never *taken*. This corrects the spec's FR-003,
which said "the download (or update) step's stamp shows completion"; the
update stamp is not needed.

**Read, not measured:** ExternalProject's `PREFIX`, `STAMP_DIR` and
`SOURCE_DIR` options move these directories. TheRock's
`therock_subproject_fetch` sets `PREFIX ${CMAKE_CURRENT_BINARY_DIR}/<target>_fetch`
and `SOURCE_DIR ${CMAKE_CURRENT_BINARY_DIR}/source` (`cmake/therock_subproject.cmake:159,162`
at `b1cae390`). **Decision:** locate a declaration's stamp directory by the
stamp file names (`<name>-download`, `<name>-mkdir`) anywhere beneath the build
directory, not by assuming `<name>-prefix/`.

## R4. Info files carry URL and hash, but are labelled internal *(measured)*

`<name>-urlinfo.txt` records `url(s)=` and `hash=SHA256=…`; `<name>-gitinfo.txt`
and `-gitclone-lastrun.txt` record `repository=` but **not** the tag. Each
begins *"its contents are an internal implementation detail … No other meaning
or use of this file is supported."*

**Decision:** do not read info files in this spec. Identity (URL, hash, tag)
already comes from the source declaration (specs 1517, 1538). Using an
unsupported format would add a dependency on CMake internals for no settlement
gain. A build-only component (FR-007) is named from its directory and cache
entries alone. Revisit only if TheRock's census (SC-001(b)) finds build-only
components that need a URL to be identified.

## R5. Configured values settle gated declarations *(measured)*

`system` runs: `WITH_EXTRA:BOOL=ON` and `BACKEND:STRING=B` are in the cache,
and exactly `extra` and `bdep` were populated (6/6). `fetch` runs:
`WITH_EXTRA:BOOL=OFF`, `BACKEND:STRING=A`, neither populated (6/6).

**Decision:** feed the cache's `BOOL`, `STRING` and literal values into spec
1641's evaluator as configured values (FR-005). The evaluator's predicted
outcome for `if(BACKEND STREQUAL "B")` and the population signal agree on every
probe run, which is the test that the plumbing is right. `PATH`, `FILEPATH`,
`INTERNAL` and `STATIC` entries are not condition inputs and are never emitted
(FR-006).

**Implementation note (US2, T024).** The tokenizer strips quotes, so the
evaluator cannot tell `"B"` from `B`. An operand that names a configured
variable is read as that variable, even if it was quoted in the source,
which CMake's CMP0054 would not dereference. This needs a configured
variable whose name equals a quoted literal it is compared with. It is
accepted as a narrow edge. A comparison is decided only when at least one
operand comes from a configured value, so a scan without a build tree is
unchanged: two literals stay *unknown*, as in spec 1641.

## R6. compile_commands.json and the file API *(measured)*

- `CMAKE_EXPORT_COMPILE_COMMANDS ON` wrote `compile_commands.json` in all 24
  runs, under both generators and all three versions. Include directories were
  absolute paths: `<build>/_deps/<lc>-src[/include]` for a populated
  dependency, and `<install prefix>/include` when `hdronly` was found on the
  system.
  - **Decision:** an include directory under a settled `_deps/<lc>-src`
    resolves to that declaration's component. One under a `<name>_DIR`'s
    install prefix resolves to that package's component when the scan has one
    (R2). Anything else is ignored. Unresolved include directories are counted,
    not emitted.
- A file-API reply exists only where a query was placed before configure:
  6/6 `system` runs, 0/18 others. It holds `cache-v2`, `codemodel-v2`, and
  per-directory and per-target JSON.
  - **Decision:** FR-010 stays as written. The reply is read when present, and
    waybill writes no query. Whether to document the query step for operators
    is a docs task. The probe shows the reply is cheap and complete, but no
    real-world build in this research had one.
  - **US4 (implemented):** the query step is documented for operators in
    `docs/user-guide/cli-reference.md` (`--cmake-build-tree`), not automated.
    A reply's `cache-v2` replaces `CMakeCache.txt` and its codemodel targets'
    `compileGroups[].includes[]` and `sources[]` replace
    `compile_commands.json`, per build directory; a malformed reply is
    counted and not used at all.

## R7. TheRock's build layout *(read from source at `b1cae390`, not observed)*

`cmake/therock_subproject.cmake`:
- Subprojects are **not** ExternalProjects. Each is configured by an
  `add_custom_command` that runs CMake into `<subproject>/build`, with
  `-DCMAKE_PROJECT_TOP_LEVEL_INCLUDES=<…>_init.cmake` (lines 989–1074).
  Each subproject build directory therefore has its own `CMakeCache.txt`:
  FR-002's "every CMake build directory beneath the path" is the right
  discovery rule.
- Third-party tarballs go through `therock_subproject_fetch`, an
  `ExternalProject_Add` with a moved `PREFIX` and `SOURCE_DIR` (R3).
- Each subproject writes `compile_commands.json` into its build directory, and
  TheRock merges fragments (`compile_commands_fragment_<target>.json`,
  line 1037).
- Stage, dist and stamp directories (`stage/`, `dist/`, `stamp/`) and
  `.smrev` files are TheRock's own. They are out of scope (#1540).

All of this is a hypothesis until SC-001(b) captures a TheRock build tree.

## R8. Absolute paths are pervasive *(measured)*

In one probe tree, `CMakeCache.txt` has 11 lines containing the build path,
and every `compile_commands.json` include and info-file field is absolute.

**Decision (FR-006):**
- Evidence paths are emitted relative to the named build-tree root (`build:`
  prefix) or the scan root.
- A path outside both is reduced to a fixed marker. The marker is
  `<external>` for a source or include path, and `<prefix>/…` for one under a
  `<name>_DIR` install prefix.
- No cache value of type `PATH`, `FILEPATH`, `INTERNAL` or `STATIC` is ever
  emitted.
- The SC-006 test asserts that no string in any format contains the build
  tree's absolute path or the host's home directory.

## R9. Scale *(measured for the probe; TheRock pending)*

A probe build tree has 198–481 entries and 0.4–1.2 MB of files. Only
`CMakeCache.txt`, `compile_commands.json`, file-API replies and stamp-file
*names* are read (FR-013). Stamp files are tested for existence, not read.
TheRock's size is unknown until SC-001(b), so SC-008's 1.10× bound has to be
re-checked there.

## R10. Open measurement: SC-001(b), TheRock *(not yet run)*

**Decision:** this is the first implementation task. Its outcome changes no
requirement unless it contradicts R1–R3 or R7. In that case the spec is
amended before any reader code merges.

- **Workflow:** `census-buildtree.yml` in `kusari-sandbox/waybill-fixture-therock-sdk`.
- **Source:** `kusari-sandbox/TheRock` at `b1cae390`, never built locally.
- **Build:** a minimal subset, chosen in the task: one sysdep fetch, such as
  zlib through `therock_subproject_fetch`, plus one small math library that
  uses FetchContent internally.
- **Output:** listings and metadata in the same shape as the probe, then the
  probe's `analyze.py` run over them.
