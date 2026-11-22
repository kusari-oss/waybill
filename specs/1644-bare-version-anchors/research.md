# Research: spec 1644, bzip2, GMP and xz versioned from a bare version plus a second anchor

Each entry says whether it was **measured** (with the artifact), **read from code** (file and line, at `main` `110d1a58`), or **read from upstream source** (file and tag). Nothing here is extrapolated. Every fact that depends on real builds is left to the census (R7), which runs before any version is emitted.

## R1. Behaviour at `main`: read from code

**Decision**: No re-measurement is needed before design. The failure is structural, not a matter of bytes.

**Evidence**: `CuratedLibrary` (`version_strings.rs:67`) has no bzip2, GMP or xz variant. `symbol_fingerprint.rs` covers openssl, zlib, libcurl, sqlite, pcre, pcre2 and gnutls only. No byte sequence can produce these libraries today. #1641's "not identified" at `786091a8` is the only possible result at `110d1a58`. The fixtures in T-tasks still assert it as the red half of each test.

## R2. Upstream version and anchor forms: read from upstream source

The sources are kept in the session scratchpad, not committed (they are third-party code). The forms below are quoted from them.

| Library | Version literal | Anchor text in the library | Source |
|---|---|---|---|
| bzip2 | `BZ_VERSION "1.0.8, 13-Jul-2019"`, returned by `BZ2_bzlibVersion` | `"\n\nbzip2/libbzip2: internal error number %d.\n"` in `bz_internal_error` | `bzlib_private.h:39`, `bzlib.c:44`, tag `bzip2-1.0.8` (sourceware) |
| GMP | `const char * const gmp_version = VERSION;` (`AC_INIT(GNU MP, GMP_VERSION, …)`) | `"GNU MP: Cannot allocate memory (size=%lu)\n"` and `"GNU MP: Cannot reallocate memory …"` in the default allocators | `version.c:33`, `memory.c:56,104`, GMP 6.3.0 release tarball |
| xz | `LZMA_VERSION_STRING`: `#major "." #minor "." #patch stability commit`, where stability is `"alpha"`, `"beta"` or `""` and commit is `""` unless defined | none; liblzma holds no fixed text naming itself | `src/liblzma/api/lzma/version.h:76-105`, tag `v5.8.1` |

**Decisions**:
- **bzip2's anchor is a substring.** Upstream's text starts with `\n\n`, so the anchor can never be a NUL-bounded string. FR-002 already says "substring anywhere in the region". This confirms it.
- **GMP: `X.Y.Z` only.** `doc/gmp.texi:2302-2304` says `gmp_version` is `"i.j.k"`, and that `"i.j"` was used only before 4.3.0, when k was zero. GMP 4.3.0 shipped in 2009. Accepting `X.Y` would widen the candidate shape, and with it the false-candidate rate, for releases no current build carries. **Spec amendment: FR-004's GMP form becomes `X.Y.Z` only.**
- **xz's candidate grammar is `X.Y.Z` optionally followed by `alpha` or `beta`.** A snapshot's commit suffix (`4.999.8beta-21-g1d92`, `version.h:94-96`) is not accepted, so a snapshot build gets no version rather than a version string no advisory uses. **Spec amendment: FR-004's xz grammar is fixed to this.**
- **GMP's anchor is `GNU MP:`.** It covers both allocator messages. GMP's assertion message (`"GNU MP assertion failed"`, `assert.c:56`) does not contain it and is not needed.

**Alternatives considered**: anchoring GMP on its exported data symbol (`__gmp_version`). It was rejected as the only anchor because a static link hides it, and the text anchor survives a static link. The census (R7) records both, so this can be revisited with evidence.

## R3. xz's anchor symbols: read from upstream source and code

**Decision**: xz's anchor is a **defined** dynamic or exported symbol named `lzma_version_string` or `lzma_version_number`.

**Evidence**:
- Both symbols are in the `XZ_5.0` node of `src/liblzma/liblzma_generic.map:93-94` at `v5.8.1`, so every liblzma since 5.0 exports them.
- They are the functions that return the version, so a library that defines them is liblzma. A program that only imports them shows liblzma is installed, not which file holds it.

**Code fact that shapes the design**: `BinaryScan.symbol_names` (`scan.rs:309-315`) is built from `file.dynamic_symbols()` with no definedness filter. On ELF it holds imported names as well as defined ones. On Mach-O it holds every `N_EXT` symbol (`is_global()`), which also includes undefined externals. Only PE's list, from the export directory, is defined by construction.

**Decision**: `symbol_names` is left unchanged. It feeds the symbol fingerprint, and changing it would change fingerprint matches, which FR-012 forbids. A new, small field, `BinaryScan.anchor_symbols`, collects only the **defined** names that appear in a fixed list owned by `version_strings.rs`:
- ELF: dynamic symbols where `!is_undefined()`;
- Mach-O: global symbols where `!is_undefined()`, with the existing single-underscore strip;
- PE: export names.

The list has two entries, so the field costs a few bytes per binary.

**Alternatives considered**:
- Filtering `symbol_names` itself: rejected, because it changes fingerprint output.
- Storing every defined symbol: rejected, because the memory scales with the symbol table and nothing else needs it.

## R4. CPE rows: measured

**Artifacts**: `measurements/cpe-probe-2026-11-22.txt` (`scripts/probe-cpe-dictionary.sh`), `measurements/nvd-titles-2026-11-22.txt` (spec 1375's `cpe-titles.py`).

| Pair | Dictionary entries (2026-11-22) |
|---|---|
| `gmplib:gmp` | **35**; title "GMPlib GMP (GNU MP Bignum Library)" |
| `gnu:gmp` | 0 |
| `tukaani:xz` | 62 |
| `tukaani:liblzma` / any vendor, product `liblzma` | 0 |
| controls: `openssl:openssl` / `waybill-fixture-vendor:waybill-fixture-product` | 387 / 0 |

**Decisions**:
- **`pkg:generic/gmp` → `gmplib:gmp`.** The evidence files are committed under this spec's `measurements/`.
- **`pkg:generic/liblzma` → `tukaani:xz` (62) and `xz_project:xz` (32, spec 1375's 2026-11-13 evidence).** NVD files liblzma's advisories under the `xz` product, and no `liblzma` product exists. A row whose product differs from its name is already allowed: `node`'s product is `node.js`, and `cpe.rs::m1375_every_product_has_a_committed_title_query` keys on the product.
- **`pkg:generic/xz` stays.** It is keyed for any reader that names the project `xz`, and nothing here touches it.
- **`pkg:generic/bzip2` is unchanged.** Its row exists.

## R5. Names against the pkg-config reader: read from upstream source

| Library | Upstream installs | Name the pkg-config reader would give |
|---|---|---|
| GMP | `gmp.pc` (and `gmpxx.pc`) since 6.2.0 (`NEWS`, `Makefile.am:121-122`) | `gmp` |
| xz | `liblzma.pc` (`src/liblzma/Makefile.am:104-105`) | `liblzma` |
| bzip2 | none at 1.0.8 (`Makefile`, 0 occurrences of `bzip2.pc`); distributions and vendors add `bzip2.pc` | `bzip2` where present |

**Decision**: the slugs are `gmp`, `liblzma` and `bzip2`, which confirms FR-007. Whether TheRock ships these `.pc` files, and under which names, is counted by the census (R7). A vendor-renamed `.pc` file (TheRock renames sonames, `librocm_sysdeps_z.so`) is recorded as found.

## R6. Where the work happens, and what it costs: read from code

**Decision**:
1. In `prepare_held` (`mod.rs:1107`), next to the existing `version_strings::scan`, call a new `version_strings::scan_anchored(region, anchor_symbols)`. It returns the matches to append and the verdicts for the census.
2. Inside it, check the anchors first: one `memchr::memmem` search per text anchor (`bzip2/libbzip2`, `GNU MP:`), plus a lookup in `anchor_symbols`. `memchr = "2"` is already a direct dependency (`waybill-cli/Cargo.toml:135`).
3. Only a binary with at least one anchor goes on to the candidate pass, which splits the region on NUL once and classifies each whole string against the anchored libraries' shapes.

**Rationale**: almost every binary has no anchor and pays only the two `memmem` searches. Spec 1643's SC-006 failure came from a per-byte loop over all patterns, so this design keeps any new per-byte loop off the unanchored path. SC-007 measures the result rather than assuming it.

**Exhaustiveness**: the three variants join `CuratedLibrary`. `Boundary` gains a third class, `WholeString`, meaning the candidate is NUL-bounded at both ends and needs an anchor. `pattern_at` never yields a `WholeString` library. The exhaustive matches (`boundary()`, `sample()`, the census) must name the new variants (FR-009, #1246).

**Alternatives considered**: a separate enum for anchored libraries. It was rejected because emission, CPE lookup and the slug uniqueness test all key on `CuratedLibrary`. A second enum would duplicate that plumbing, or let it drift.

## R7. The census: design

**Decision**: a log-only commit first. For every binary that has at least one of the three anchors, `waybill::version_census` emits one `anchored` event with the library, each anchor found (text with its offset, or symbol with its name), every candidate with its offset, the shared candidates (FR-005a) and the verdict. It is dispatched on TheRock (full SDK) and the 18 quality-corpus targets through the fixture repository's existing `census-versions.yml`. `census/versions.py` is extended to summarise the new events and to look up, in each target's main-control document, the components named `bzip2`, `gmp`, `liblzma` or `xz` with their source files and versions (FR-001's third bullet). The public corpus has no census. Its goldens must be byte-identical at the log-only commit, and its diff at the final commit is its census (as in spec 1643 SC-005).

**Positive control**: a synthetic fixture with each anchor and one candidate runs in the integration suite. On CI, the census job also scans a committed synthetic tree with all three libraries, so a zero for every corpus target means absence, not a broken probe.

## R8. Fixtures: read from code

**Decision**: `tests/common/installed_lib.rs::elf_shared_lib` writes only `.rodata` and `.shstrtab`. A test-only sibling, `elf_shared_lib_with_dynsym(rodata, defined, undefined)`, adds `.dynsym` and `.dynstr`, with `SHN_UNDEF` for the undefined names. `object`'s reader resolves those from section headers alone. No new crate is needed (`object` has only the `read` feature) and no real library is committed. All names are synthetic `waybill-fixture-*` paths. The anchors and versions are the upstream literals, which is the point of the test.

## R9. Mutations

Each must fail at least one test (SC-003). The runner is spec 1643's `measurements/mutate.py` pattern: one full-path test filter, a "no test result line → harness error" guard, and restore with `cp`.

| ID | Mutation |
|---|---|
| M1 | bzip2 versioned without its anchor |
| M2 | GMP versioned without its anchor |
| M3 | xz anchored by an imported symbol (definedness check removed) |
| M4 | ambiguity rule: the first of two candidates taken |
| M5 | FR-005a removed: a shared candidate versions both libraries |
| M6 | whole-string rule removed: a candidate inside a longer string accepted |
| M7 | xz grammar widened to accept a commit suffix |

## R10. Timing

SC-007 uses spec 1643's method: `measure-full` in the fixture repository, base `main` against the branch, one warm-up per arm, then 3 interleaved runs per arm. The ratio of medians is reported, and the runs are read from the "Build both" lines.
