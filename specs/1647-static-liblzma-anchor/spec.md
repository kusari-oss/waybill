# Feature Specification: A statically linked liblzma is versioned from the version it embeds (#1664)

**Feature Branch**: `1647-static-liblzma-anchor`
**Created**: 2026-11-23
**Status**: Draft
**Input**: #1664. Spec 1644 (#1663) versions liblzma from its bare `X.Y.Z` only when the binary defines `lzma_version_string` or `lzma_version_number` as a dynamic symbol. A statically linked liblzma has no such dynamic symbol, so it is never versioned. Refs #1641, #1663; tracked in #1543.

## Background

Spec 1644 added three anchored libraries to the curated scanner (`waybill-cli/src/scan_fs/binary/version_strings.rs`). bzip2 and GMP are anchored by text in the read-only string region, and text survives static linking. liblzma is anchored by a **defined** symbol named `lzma_version_string` or `lzma_version_number` (spec 1644 FR-003). On ELF that symbol is read from `.dynsym` only (`waybill-cli/src/scan_fs/binary/scan.rs:344`). On Mach-O it is read from the full symbol table, filtered to defined externals; on PE, from the export table.

**Measured locally** at `main` `b6ad3c26` (2026-11-23). Ubuntu 24.04, x86_64, package `liblzma-dev` `5.6.1+really5.4.5-1ubuntu0.4`, so the library is xz 5.4.5. Two programs were built with `gcc -O2 -static -llzma`: program A calls only `lzma_easy_buffer_encode`; program B also calls `lzma_version_string`. Each was kept unstripped and run through `strip`. The probe is in `measurements/probe/`.

| Build | `lzma_version_*` defined in `.dynsym` | in `.symtab` | whole string `5.4.5` | text naming liblzma | waybill at `b6ad3c26` |
|---|---|---|---|---|---|
| shared `liblzma.so.5`, same package (positive control) | yes | — | yes | — | `pkg:generic/liblzma@5.4.5` |
| A, static, unstripped | no | 2 of 2 | 1 | symbol names only | no liblzma component |
| B, static, unstripped | no | 2 of 2 | 1 | symbol names only | no liblzma component |
| A, static, stripped | no | 0 | 1 | none | no liblzma component |
| B, static, stripped | no | 0 | 1 | none | no liblzma component |

- **The version string is linked in even when the program never asks for it.** Program A never calls `lzma_version_string`, but its static build still holds the whole string `5.4.5` and defines both symbols.
- **Unstripped, the anchor exists, but in `.symtab`.** A static link puts the library's symbols in the static symbol table, and waybill's ELF anchor never reads it.
- **Stripped, no symbol and no text names the library.** `strings -a` on the stripped builds finds no `lzma_`, no `liblzma`, no `tukaani`, and none of the `lzma_str_to_filters` messages #1664 considered. The only liblzma-specific bytes found are the xz stream header magic, `FD 37 7A 58 5A 00` (`strings` shows `7zXZ`), which liblzma's encoder and decoder both use.
- **Not yet known:** whether the ROCm SDK, the quality corpus or the public corpus contain any statically linked liblzma at all. Every liblzma counted on TheRock by spec 1644 is a shared library and is already versioned (`specs/1644-bare-version-anchors/measurements/census.md`).

The probe covers one xz release, one libc and one compiler. It is a reason to look, not evidence for a rule.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - An unstripped static liblzma gets its version (Priority: P1)

An operator scans a tree containing a program that statically links liblzma and was not stripped. Today the program yields no liblzma component, although it holds both the version and liblzma's own symbols.

**Why this priority**: This is the exact gap. The evidence spec 1644 trusts is present, just in a table it does not read.

**Independent Test**: Scan a synthetic `waybill-fixture-*` ELF program that defines `lzma_version_string` in its static symbol table only and holds one whole-string version. It gives the liblzma component at that version.

**Acceptance Scenarios**:

1. **Given** an ELF program whose static symbol table defines `lzma_version_string` and whose read-only region holds the whole string `5.4.5`, **When** it is scanned, **Then** it gives `pkg:generic/liblzma@5.4.5`.
2. **Given** the same program defining `lzma_version_number` instead, **When** it is scanned, **Then** the result is the same.
3. **Given** a shared liblzma that defines the symbols in both tables, **When** it is scanned, **Then** the result is unchanged from `main`.

---

### User Story 2 - A program that only uses liblzma is never anchored (Priority: P1)

A dynamically linked program that calls liblzma lists `lzma_version_string` as an **undefined** symbol, in `.dynsym` and often in `.symtab` too. It does not contain liblzma.

**Why this priority**: Reading a second symbol table must not turn every user of liblzma into a copy of it. A false component is worse than a missing version.

**Independent Test**: The User Story 1 fixture with the symbol undefined in both tables. It gives no liblzma component.

**Acceptance Scenarios**:

1. **Given** an ELF program whose static symbol table lists `lzma_version_string` as undefined, and which holds the whole string `5.4.5` for another reason, **When** it is scanned, **Then** no liblzma component is produced.
2. **Given** a program defining the symbol in its static symbol table and holding two whole-string candidates, **When** it is scanned, **Then** no liblzma component is produced, and the census records it as ambiguous (spec 1644 FR-005, unchanged).
3. **Given** a program defining the symbol in `.symtab` and holding `GNU MP:`, with `5.4.5` its only candidate, **When** it is scanned, **Then** neither library is versioned (spec 1644 FR-005a, unchanged).
4. **Given** that same program, **When** it is scanned on `main`, **Then** it gives `pkg:generic/gmp@5.4.5`, because `main` does not read the `.symtab` anchor and GMP is the only anchored library. This change removes that version. That is intended: once both libraries are anchored, the string fits both, and spec 1644 gives a shared string to neither. FR-001 counts such losses, and SC-006 traces each one.

---

### User Story 3 - A stripped static liblzma: decided by measurement (Priority: P2)

A stripped static build keeps the version and the xz header magic but no symbol. Whether the magic can anchor liblzma depends on two facts this spec must measure: whether every static liblzma build carries it with exactly one candidate, and whether programs that are not liblzma carry it too.

**Why this priority**: Stripped static binaries are probably the more common case in shipped software, but a byte sequence any xz-aware program may hold is a much weaker anchor than a symbol. It is adopted only if the measurements support it (FR-004).

**Independent Test**: If adopted, a synthetic stripped `waybill-fixture-*` program holding the magic and one candidate gives the liblzma component, and one holding the magic with two candidates gives none. If rejected, `measurements/` records why, and no output changes.

**Acceptance Scenarios**:

1. **Given** the rule is adopted, and a stripped program holds `FD 37 7A 58 5A 00` and the single whole string `5.4.5`, **When** it is scanned, **Then** it gives `pkg:generic/liblzma@5.4.5`.
2. **Given** the rule is rejected, **When** the same program is scanned, **Then** no liblzma component is produced, as on `main`.

---

### User Story 4 - Nothing else changes (Priority: P2)

Binaries whose liblzma verdict does not change produce the same documents as on `main`.

**Why this priority**: The symbol tables are read for every ELF binary in every scan. A change in what is read must be traceable component by component.

**Independent Test**: Existing version-string, spec 1643 boundary, spec 1644 anchor and spec 1646 evidence tests pass unchanged. Every corpus and TheRock difference against a `main` control is traced to one binary and one anchor.

**Acceptance Scenarios**:

1. **Given** a binary that defines no liblzma anchor symbol in any table, and (if FR-004 adopts it) holds no xz header magic, **When** it is scanned, **Then** the output is byte-identical to `main`'s.
2. **Given** the public corpus, the quality corpus and TheRock's SDK fixture, **When** each is scanned at this branch and at `main`, **Then** every difference is listed and traced.

---

### Edge Cases

- **A static program that bundles several libraries.** It may hold many `X.Y.Z` strings. With two or more candidates it is ambiguous and gets no version (spec 1644 FR-005). The census counts how often a newly anchored binary ends ambiguous, so the gain is measured, not assumed.
- **A static program that bundles GMP as well.** A candidate both libraries accept versions neither (spec 1644 FR-005a). On `main` such a program may carry a GMP version taken from a string liblzma also accepts; reading `.symtab` removes it (User Story 2, scenario 4). So this change can remove components as well as add them.
- **A local or hidden symbol.** With link-time optimisation or `-fvisibility=hidden`, `lzma_version_string` may be in `.symtab` with local binding. Any defined symbol of that name anchors, whatever its binding; the census reports the binding, so a surprise is visible.
- **A separate debug-info file** (`.debug`, `*.dbg`): its `.symtab` defines the symbols, but its data sections hold no bytes. It has no candidate and gets no version.
- **A relocatable object or a static archive member** (`version.o` from `liblzma.a`). If the scanner reads it as ELF, it defines the symbols and holds the version, and it is liblzma. The census counts such files separately.
- **A program that defines its own `lzma_version_string`.** Not seen; the census lists every anchored file, so one would be found.
- **A large unstripped static symbol table** (an unstripped static `libLLVM`). Reading it costs time on every scan. SC-007 bounds the cost on TheRock, where such files exist.
- **Mach-O and PE.** Mach-O already reads the full symbol table, filtered to defined externals, so an unstripped static Mach-O program should be anchored on `main` (inference from the code, research R1; no static Mach-O fixture tests it). A static PE program exports nothing. A stripped static build on either format is covered only if FR-004 adopts the magic.
- **The 16 MiB region cap.** A candidate or magic beyond it is not seen, as for every curated pattern.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (census before design)**: Before any output changes, a log-only build MUST count the facts the remaining requirements depend on. It runs on TheRock's SDK fixture (CI only, through kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs and every `refs.txt` verified), the quality corpus and the public corpus. It counts:
  - ELF files that define an anchor symbol in `.symtab` but not in `.dynsym`, with the symbol's binding and the file's ELF type;
  - files that hold the xz header magic but define no anchor symbol in any table;
  - for each such file, its whole-string candidates of liblzma's shape, and the verdict each rule would give **for every anchored library in the file**, not only liblzma;
  - which of them already get a liblzma version from another reader (pkg-config, CMake, a package database), and under which name and version;
  - how many components each rule would newly version, and how many existing versions it would remove (a GMP version lost to a shared candidate, User Story 2 scenario 4).

  The census states plainly if TheRock has none. On the public corpus, whose harness keeps no census log, what is measured is output: unchanged at the log-only commit, and every golden difference traced at the wired commit. A positive control, a static liblzma program compiled on the CI runner and kept unstripped and stripped, shows each count firing, so a zero means absence. Where a count contradicts a requirement below, the requirement MUST be amended in this spec with the evidence before implementation.

- **FR-002 (static symbol table)**: On ELF, a symbol named `lzma_version_string` or `lzma_version_number` anchors liblzma when it is **defined** in `.dynsym` or in `.symtab`, whatever its binding. An undefined symbol in either table MUST NOT anchor. The symbol list used by the symbol fingerprints is unchanged; only the anchor reads `.symtab`.

- **FR-003 (rules kept)**: Spec 1644's rules apply unchanged to a binary anchored under FR-002 or FR-004: the whole-string candidate shape (FR-004), one candidate or none (FR-005), and a candidate shared with another anchored library versions neither (FR-005a).

- **FR-004 (the xz header magic, decided by measurement)**: The six bytes `FD 37 7A 58 5A 00`, anywhere in the read-only region, are adopted as a second liblzma anchor **only if both** hold:
  1. **Present in liblzma builds.** Static liblzma builds covering at least xz 5.2.x, 5.4.x and 5.6.x or 5.8.x, glibc and musl, encoder-only and decoder-only, stripped and unstripped, each carry the magic and exactly one whole-string candidate equal to the build's version.
  2. **No false version.** No binary in the false-positive population yields a version it does not contain. That population is: every file FR-001 finds holding the magic without an anchor symbol, plus programs that recognise xz files without being liblzma, at least one format sniffer (for example libarchive) and one independent xz implementation (for example BusyBox's `unxz`). Each one's whole-string candidates of liblzma's shape are recorded.

  If either fails, the magic is not adopted. The failing build or program and the reason go in `measurements/`, and #1664 is updated with them.

- **FR-005 (census reporting)**: `waybill::version_census` MUST report, for each liblzma-anchored binary, which anchor fired: a defined `.dynsym` symbol, a defined `.symtab` symbol with its binding, or (if adopted) the header magic with its offset. The existing event fields keep their shape.

- **FR-006 (no new output field)**: A version gained this way MUST be emitted exactly as spec 1644's liblzma match: same PURL name (`pkg:generic/liblzma`), same evidence kind, same technique (spec 1646's `binary-analysis`), same annotations. No new annotation, catalog row or output field.

- **FR-007 (byte identity)**: Output MUST be byte-identical to `main` for any binary in which no anchored library's verdict changes.

- **FR-008 (probe kept)**: The local probe (source, build script and results) MUST be committed under `measurements/probe/`, so the finding can be re-run when xz changes. No real liblzma binary is committed.

### Key Entities

- **Anchor location**: where a liblzma anchor was found: a defined `.dynsym` symbol, a defined `.symtab` symbol (with binding), a Mach-O defined external, a PE export, or, if FR-004 adopts it, the header magic.
- **Static liblzma build**: a binary into which liblzma's object code was linked, as opposed to a program that loads the shared library.
- **False-positive population**: files that hold the xz header magic but are not liblzma.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: FR-001's census is recorded in `measurements/`, listing every newly anchored file in TheRock, the quality corpus and the public corpus with its anchor location, candidates and verdict, and stating plainly where a count is zero. The positive control shows each count firing.
- **SC-002**: A synthetic unstripped static `waybill-fixture-*` program gives `pkg:generic/liblzma` at its version. The same program with the symbol undefined, or with a second candidate, gives none.
- **SC-003**: FR-004's decision is recorded in `measurements/` with a row for every build and every false-positive program examined. If adopted, synthetic stripped fixtures meet User Story 3; if rejected, #1664 records why.
- **SC-004**: Each new rule has a mutation record: accepting an undefined `.symtab` symbol, dropping the `.symtab` read, and (if adopted) accepting the magic with two candidates each fail at least one test.
- **SC-005**: `waybill sbom parity-check` agrees for every fixture.
- **SC-006**: Every public-corpus, quality-corpus and TheRock difference against a `main` control is traced, component by component, to one binary and one anchor.
- **SC-007**: Scan time on TheRock's SDK fixture is within 1.05× `main`'s, median of at least 3 interleaved runs per arm.

## Assumptions

- The probe ran on an x86_64 Ubuntu 24.04 container on an aarch64 host (`--platform linux/amd64`). Its findings are re-verified at the current `main` before design.
- The liblzma builds for FR-004 are made in containers from upstream xz release tarballs or distribution packages. They are measurement inputs only and are never committed. Committed fixtures are synthetic.
- The existing synthetic ELF helper may need to emit a `.symtab` with a defined or undefined symbol. That extension is test-only.
- TheRock is scanned on CI only, never cloned or scanned locally.
- Out of scope: other bare-version libraries; reading `.symtab` for any other purpose, including the symbol fingerprints; the other binary-derived evidence kinds (#1668); and any change to bzip2's or GMP's anchors.
- Zero new Cargo dependencies.
