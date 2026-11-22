# Research: spec 1643

## R1: The defect reproduces at the current `main`

**Measured** (`measurements/probe.md`), waybill `83657af2`, real Nix-built `libz.1.3.2.dylib` and `libncursesw.6.dylib`, one byte changed per copy, three scans each:
- zlib is versioned only when the byte before `" deflate"` is `0x00`. With `-`, `0xff` or `x` it is the unversioned symbol-fingerprint `pkg:generic/zlib`.
- ncurses is identified only when the byte before `ncurses 6.6.20251230` is `0x00`. With `-`, `0xff` or `x` it is absent.

Results were identical across all three runs of each case. The boundary code (`version_strings.rs:135-151`) is unchanged since `5e4e36b8` (#1526), so the issues' older commits describe the same code.

**Why the layout occurs.** The scanned region is the concatenation of the read-only sections (`scan.rs::collect_string_region`): ELF `.rodata` + `.data.rel.ro`, Mach-O `__cstring` + `__const`, PE `.rdata`. `__cstring` is a NUL-separated table, but `.rodata`, `__const` and `.data.rel.ro` mix string literals with other constant data, so a literal can follow any byte. #1641's ncurses banner sits in `.rodata` behind `0xff`.

## R2: The classification in FR-001 matches the code

**Decision:** keep FR-001's table as written: zlib, expat and ncurses anchored; the other twelve NUL-only.

**Evidence, read from each parser:**

| Library | Parser | Check after the version |
|---|---|---|
| zlib | `parse_zlib_version` | requires ` Copyright` |
| expat | `parse_semver_triple` + `.filter(tail.get(len) == Some(&0))` | requires NUL |
| ncurses | `parse_ncurses_version` | the text up to the first NUL must be exactly `X.Y[.YYYYMMDD]` |
| OpenSSL | `parse_openssl_version` | any byte that is not alphanumeric or `.` |
| BoringSSL | `parse_git_sha` | none (40 lowercase hex) |
| SQLite | `parse_sqlite_version` | NUL or space |
| curl, GnuTLS, LibreSSL, LLVM, PHP, Node.js | `parse_semver_triple` | none |
| PCRE, PCRE2 | `parse_pcre_version` | ` YYYY-MM-DD`, then none |
| OpenJDK | `parse_openjdk_version` | any byte that is not alphanumeric, `.` or `+` |

Only the first three fix the end of the match. A pattern ended by "any non-version byte" matches inside prose ("requires OpenSSL 1.1.1 or later"), and so does one ended by nothing.

**Alternatives considered:**
- **Relax every pattern** to the identifier rule. Rejected: for the twelve unanchored patterns, the NUL rule is the only thing separating a library's own banner from a mention of it in another program's text. The census counts their non-NUL occurrences, so a later spec can decide with evidence.
- **Drop the boundary for anchored patterns entirely.** Rejected: the spec requires an identifier character to still reject (`lncurses …`, `xdeflate …`). A literal fused to a preceding identifier is not the library's banner.
- **Treat PCRE's date as an anchor.** Rejected: nothing is checked after the date, so `PCRE 8.45 2021-06-15` inside a sentence still matches.

## R3: Where the class lives (FR-004)

**Decision:** a `Boundary` enum (`Anchored`, `NulOnly`) and `CuratedLibrary::boundary(self) -> Boundary`, written as an exhaustive `match` with no wildcard arm. A new variant then fails to compile until it is classified. A `Boundary::accepts(region, pos)` predicate replaces the inline `at_boundary` test for expat and ncurses. zlib keeps its own form for the leading space (R4).

`match_prefix` keeps its structure. The expat and ncurses checks move above the `if !at_boundary { return None; }` early return and use `CuratedLibrary::X.boundary().accepts(region, pos)`. The twelve NUL-only patterns stay below it, unchanged.

**Test of the class (SC-003):** a unit test iterates `CuratedLibrary::iter()` (the existing `strum::EnumIter` derive, #1246). For each variant it takes a canonical banner from a test-only `fn sample(lib) -> &[u8]`, which is also an exhaustive `match`. It asserts that the banner behind `-` matches exactly when `boundary()` is `Anchored`, and behind NUL always matches. Re-classing any variant fails this test, and so does adding a variant without a sample.

**Alternative considered:** a list of anchored libraries. Rejected for the reason #1246 was filed: a list can miss a variant added later.

## R4: zlib's leading space

**Decision:** for the form with a leading space, `deflate ` at `pos` is accepted when `region[pos-1] == b' '` and either `pos == 1` or `region[pos-2]` is not an identifier character. The no-space form keeps `main`'s `at_boundary` (spec FR-002). In code, `after_one_space`'s `region[pos - 2] == 0` becomes `!is_identifier(region[pos - 2])`.

NUL is not an identifier character, so every case `main` accepts is still accepted (FR-006).

## R5: Census mechanics (SC-001)

**Decision:** a log-only commit adds `version_strings::census(region) -> Vec<CensusHit>`, which applies every pattern's prefix and version parser at every position **without** a boundary test. Each hit records:
- `library` and `version`;
- `preceding`: `start`, `nul`, `whitespace` (space, tab, CR, LF), `other_non_identifier` or `identifier`. Whitespace is split out because a mention inside another program's sentence is space-preceded; binary data before a literal (`0xff`, `-`) is not (analysis U1);
- `accepted_main`: would `main`'s rule accept it. This is a frozen copy of `main`'s rule in its own function (`main_rule_accepts`), not a call into `match_prefix`, so it keeps meaning `main` after the wiring (analysis I1);
- `accepted_new`: would FR-002/FR-003 accept it;
- 24 bytes before and after the literal, hex-encoded, so each new match can be judged.

`binary/mod.rs::prepare_held` emits one `tracing::debug!(target: "waybill::version_census", …)` per hit with the binary's path. It is guarded by `tracing::enabled!` for that target, so a scan without `RUST_LOG` pays one check per binary. The output is unchanged; the public corpus regenerated at that commit must be byte-identical, as in spec 1642.

The census reads waybill's own events, not a re-implementation of the patterns in Python (the project rule: never model the reader). A fixture-repository script summarises the events.

**Positive control:** an integration test scans a synthetic library holding `-ncurses 6.6.20251230\0` with the census target enabled and asserts that one `preceding=other_non_identifier, accepted_main=false, accepted_new=true` event fires. A zero count can then not mean the census never ran.

**Census agrees with the scanner.** `census` decides acceptance outside `match_prefix`, so the two could drift. A unit test added with the wiring asserts that, for every fixture banner and preceding byte, the census hits with `accepted_new` are exactly `scan()`'s matches (analysis I1).

**Kept after the census.** It is small, costs nothing when disabled, and lets the census be re-run when a pattern is added (as `waybill::owner_census` was kept by spec 1642).

## R6: Census targets

| target | how | why |
|---|---|---|
| TheRock SDK, full tarball (`therock-dist-linux-gfx1150-7.14.0a20260612.tar.gz`, 3.3 GB, sha256 `efeb6b07…`) | new `census-versions.yml` in kusari-sandbox/waybill-fixture-therock-sdk, downloading the tarball as `measure-full` does | the vendor-built libraries #1641 describes |
| 18 quality-corpus targets at their pins | same workflow, matrix copied from `census-owners.yml` | the project's standing corpus; mostly source trees, so few binaries |
| public corpus | `public-corpus.yml` with `regen_goldens=true`, at the log-only commit (must be identical) and at the wired commit (the diff is the change) | container images and Nix closures with distribution-built zlib, expat and ncurses: the false-positive check on real libraries |

`fixture/sdk` in the fixture repository holds only metadata files, so the full tarball is required.

**Census result (T005, `measurements/census.md`).**
- **Run:** 44990809831 at `d3dcd676`; 19 of 19 jobs succeeded.
- **TheRock SDK:** 16 events in 13 binaries.
  - **8 verdicts change:** 6 zlib behind `-` and 2 ncurses behind `0xff`. All are the libraries' own banners.
  - **No identifier-preceded match.** The only whitespace-preceded hits are 3 LLVM ones, inside `AMD LLVM version …`; LLVM is NUL-only, so they stay rejected.
- **Quality corpus:** one target has events (pants-backend-ai: 2 OpenSSL, 2 zlib, all NUL-preceded), and none changes verdict.
- **FR-005 is resolved with no reclassification:** no anchored pattern produced a false positive, and no binary gains a second version or loses one.

## R7: Fixtures

**Decision:** a new integration test file, `waybill-cli/tests/version_string_boundary.rs`, built on `tests/common/installed_lib.rs::elf_shared_lib(rodata)`. It writes `libwaybill-fixture-z.so.1` and `libwaybill-fixture-ncurses.so.6` into a temp directory, one directory per preceding byte. Each case scans in all three formats, asserts the curated component and runs `waybill sbom parity-check`.

Unit tests in `version_strings.rs` cover `scan()` directly: every byte class, for every anchored pattern, plus the R3 iteration over all variants.

The component names stay `zlib`, `ncurses` and `expat`: these are the curated slugs, and the existing #1525/#1526 tests already use those banners. Only file names are synthetic.

## R8: Mutations (SC-003)

| id | mutation | expected to be killed by |
|---|---|---|
| M1 | `Boundary::Anchored` accepts only NUL (`main`'s rule) | `-`/`0xff` fixtures, unit and integration |
| M2 | zlib classed `NulOnly` | the R3 iteration test |
| M3 | expat classed `NulOnly` | the R3 iteration test |
| M4 | ncurses classed `NulOnly` | the R3 iteration test |
| M5 | OpenSSL classed `Anchored` | the R3 iteration test |
| M6 | the identifier test removed (any preceding byte accepted) | `x`/`9`/`_` fixtures |
| M7 | zlib's space form tests `region[pos-1]` instead of `region[pos-2]` | `- deflate` and `x deflate` fixtures |

Each mutation is applied by `measurements/mutate.py`, run, and reverted with `cp`. The binary is rebuilt after each revert.

## R9: Timing (SC-006)

**Decision:** `measure-full.yml` (interleaved base and branch scans of the full SDK tarball), at least 3 runs per arm, ratio of medians. The change adds one byte comparison for three patterns at positions that already match their prefix, so no measurable change is expected. That expectation is not a measurement; SC-006 is.
