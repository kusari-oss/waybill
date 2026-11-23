# Research: a statically linked liblzma is versioned from the version it embeds

All observations are at `main` `b6ad3c26` unless stated otherwise. "Measured" means executed in this session, with the evidence file named. "Read" means read from code. Anything else is labelled inference.

## R1. Where the anchor is read today (read)

- **ELF.** `scan.rs:340-367` collects `anchor_symbols` from `file.dynamic_symbols()` only, filtered to `!is_undefined()` and to `ANCHOR_SYMBOLS`.
- **Mach-O.** `file.symbols()` filtered to `is_global() && !is_undefined()`. Mach-O has one symbol table, so an unstripped static Mach-O program is already anchored on `main` (inference from the code; not fixture-tested here).
- **PE.** Export names only.
- **Use.** `mod.rs:1110` passes the names to `version_strings::anchored_verdicts`. `symbol_names`, the symbol fingerprints' input, is a separate list (`scan.rs:309`) and is not touched.

`object` 0.36.7 (`read/elf/file.rs:349`, `:363`): on ELF, `symbols()` iterates `.symtab` and `dynamic_symbols()` iterates `.dynsym`. `ElfSymbol` provides `is_undefined()`, `is_weak()`, `is_local()`, `is_global()` and `scope()`.

**Decision**: read defined anchor symbols from `file.symbols()` as well, on ELF only. Mach-O and PE are unchanged.

## R2. FR-004, the xz header magic: the false-positive population (measured)

`measurements/magic-population-2026-11-23.md`, with scripts in `measurements/probe/`. On an Ubuntu 24.04 install with xz-aware tools added:
- 1,042 ELF files, 13 holding `FD 37 7A 58 5A 00`, and 1 of those 13 is liblzma.
- `7zr` and `7za` (7-Zip, its own xz implementation, no liblzma) hold the magic and exactly one bare `X.Y.Z` in `.rodata`: `13.2.0`. That is the GCC version from 7-Zip's "Compiler:" banner, a NUL-terminated string preceded by NUL.

**Inference, to be confirmed by waybill's own code in the census**: with the magic as an anchor, `7zr` would be emitted as `pkg:generic/liblzma@13.2.0`. FR-004's second condition ("no false version") then fails.

**Decision**: the census computes the magic's would-be verdict with waybill's own `anchored_verdicts` (R4). If `7zr` is `versioned 13.2.0` there, FR-004 is amended to **rejected**, citing this file and the census. #1664 is updated, and the build matrix for FR-004's first condition is not built. One failing condition suffices, and building it would only add cost.

**Alternatives considered**:
- Magic plus "imports no `lzma_*` symbol": 7-Zip imports none, so it would still be anchored. Rejected.
- Magic plus a second magic, the stream footer `YZ`: two bytes, so it names nothing. Rejected.

## R3. The cost of reading `.symtab` (read; bounded by measurement in SC-007)

An unstripped binary's `.symtab` can be far larger than `.dynsym`. An unstripped static `libLLVM`, for example, has entries for every internal function (inference about TheRock's contents; the census reports actual stripped and unstripped counts). Iterating it for every ELF to find two names is wasted work in almost every file.

**Decision**: run one `memchr::memmem` search for `lzma_version_` over the raw `.strtab` section first. Only when it is found, iterate `file.symbols()` and keep the defined entries named in `ANCHOR_SYMBOLS`.
- A file without `.strtab`, or without the substring, costs one search and no iteration.
- If a `.symtab` links a string table with another name, its anchors are missed. The census lists every anchored file, so a miss in a counted tree would show as a `.symtab` anchor the reader did not report. No such layout is expected from standard linkers (inference).

**Alternatives considered**: iterating `.symtab` unconditionally is simpler, but its cost scales with symbol count on every ELF. Reading `.symtab` only when `.dynsym` gives no anchor saves nothing for the common case, which has neither.

## R4. Census design: waybill's rules, not a second model (decision)

The log-only commit computes what each rule would do with the existing `anchored_verdicts`, so the counts come from the code that will emit, not from a script modelling it:
- **Rule A (`.symtab`)**: `anchored_verdicts(region, dynsym_anchors ∪ symtab_anchors)`.
- **Rule B (magic)**: when the magic is in the region and no anchor symbol is defined in either table, `anchored_verdicts(region, ["lzma_version_string"])`. A hypothetical defined anchor gives exactly the verdict the magic would give, because `anchored_verdicts` treats every liblzma anchor alike.

A new DEBUG event `liblzma_static` (contracts/wire.md) is emitted when rule A or rule B would apply. It carries each rule's verdict, the anchors with their table and binding, the ELF type and the magic's offset. Production verdicts are unchanged on the log-only commit.

At the wired commit:
- rule A becomes production;
- if FR-004 is rejected, rule B's evaluation and the `magic` field are removed;
- the `liblzma_static` event stays for rule A, reporting which table anchored, as spec 1646 kept `binary_merge`.

## R5. Where to count (decision, following specs 1644–1646)

- **TheRock**: `census-versions.yml` in kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs. `census/versions.py` gains the `liblzma_static` event. Every `refs.txt` is verified.
- **Quality corpus**: the same workflow's corpus jobs.
- **Public corpus**: counted as specs 1644 and 1645 did. At the log-only commit, a `public-corpus.yml` dispatch with `-f branch=` must pass 166 of 166 (no output change). At the wired commit, every golden difference is a newly versioned component and is traced. The harness does not capture the census log (spec 1645, `measurements/census.md`).
- **The Ubuntu population** (R2): the 13 magic-carrying files are copied out of the container and scanned locally with the log-only build, which is permitted because they are not TheRock. This is the census's run of rule B on the known false positives.
- **Positive control**: the control job compiles `measurements/probe/a.c` with `gcc -static -llzma` on the runner, unstripped and stripped, so rule A and rule B fire on real static builds. Its expected result: unstripped gives rule A `versioned <runner's xz version>`; stripped gives rule B the same version, and rule A nothing. That shows the counters fire, so zeros elsewhere mean absence.

## R6. Fixtures (decision)

Committed fixtures are synthetic. `tests/common/installed_lib.rs` has `elf_shared_lib_with_dynsym(rodata, defined, undefined)`. It gains a test-only sibling that writes a `.symtab` and `.strtab` (with no `.dynsym`), with defined and undefined entries, and an `e_type` choice so a fixture can be `ET_EXEC` like a static program.

Discovery is by magic bytes (`discover.rs:140`), so an extensionless `waybill-fixture-xzstatic` is found without an executable bit. The local probe confirms this: `a.static` and `a.stripped` each produced a file-level component, so their missing liblzma component is real, not a skipped file.

## R7. Mutations (decision)

- **M1**: accept undefined `.symtab` symbols (drop the `!is_undefined()` filter on the new read). Killed by US2's undefined fixture.
- **M2**: drop the `.symtab` read. Killed by US1's fixtures.
- **M3**: change the `.strtab` gate's needle so it never matches. Killed by US1's fixtures. This proves the gate is on the path, not dead.
- **M4**: report an anchor found in both tables as `.symtab` instead of `.dynsym`. Killed by the census contract test.
- **M5** (only if FR-004 were adopted): accept the magic with two candidates.
