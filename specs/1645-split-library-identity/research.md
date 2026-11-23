# Research: spec 1645

All paths are relative to `waybill-cli/src/`. Code facts were read at `bb9d6cc4`.

## R1. Where the two components come from, and where they could meet

- **Curated matches.** `scan_fs/binary/mod.rs` (~line 826) turns each `EmbeddedVersionMatch` into a `PackageDbEntry` through `entry.rs::version_match_to_entry` (line 260). That entry has `evidence_kind = Some("embedded-version-string")`, `name` = the slug, and `source_path` = the binary.
- **Installed-library records.** `scan_fs/mod.rs` (~line 691) calls `installed_lib::resolve::resolve` after the binary scan, passing it the binary entries (`&db_entries[binary_start..]`, curated entries included). Each record becomes `pkg:generic/<lowercased record name>@<record version>`.
  - The record's library files go into `extra_source_paths`. They are found by `match_binaries` (the file name names the library, within the record's prefix), by exported targets (spec 1563) and by embedded paths (spec 1534).
  - The records are spliced **in front of** the binary entries, so an identical curated PURL merges into the record and keeps the record's tier and mechanism.
- **Merging** is by PURL, in `resolve/deduplicator.rs::deduplicate`.

**Decision**: add one pass, `library_identity::reconcile(&mut db_entries)`, immediately after the splice in `scan_fs/mod.rs`. It sees both kinds of entry, and the record's own declared files.
- It does not merge anything itself. It rewrites the PURL, and the name where needed, of the qualifying record and curated entries to the surviving identity, then leaves the merge to the deduplicator.
- That reuses the one merge path every existing case (expat, gmp, liblzma, zlib) already goes through, so the merged component comes out in the same shape (FR-007).
  - Measured on TheRock (`gmp@6.3.0`, `zlib@1.3.2`): the record's mechanism and tier are kept, and the files are the union.
  - The curated entry's `evidence-kind` and `confidence` do not survive, and C56 gains no entry for it. A curated entry has no source mechanism; C56 on `liblzma@5.8.1` names its second *record*. That loss predates this spec, and #1665 covers it.

**Alternatives rejected**:
- *Merge in the deduplicator by shared file.* The deduplicator has no notion of "a record's own library files" against "a curated match's file", and a file-keyed merge there would reach every reader.
- *Merge inside `resolve`.* It would have to emit the curated entry's evidence, which it does not own.

## R2. Edges that name a rewritten PURL

`resolve_requires` turns each `.pc` `Requires` into an exact PURL string in `depends`. Renaming a record (`ncursesw@…` to `ncurses@…`) would leave any `depends` naming the old PURL dangling.

- **Measured on TheRock**: no record depends on `ncursesw` or on either llvm component. Every edge into them is from the root `sdk@0.0.0`.

**Decision**: `reconcile` rewrites every `depends` string equal to a rewritten PURL, across all entries, in the same pass.
- A test covers it: a `formw.pc` with `Requires: ncursesw`.
- Mutation M6 removes the remap and must break that test.

## R3. The library-name list (FR-004)

`CuratedLibrary` gains `fn names(self) -> &'static [&'static str]`, an exhaustive `match` with no wildcard (the project's rule against stale enum allowlists). Each arm lists the slug first.

- The `Ncurses` arm adds `ncursesw`, `ncursest` and `ncursestw`. The source is ncurses `INSTALL` (`$Id: INSTALL,v 1.267 2026/05/30`, https://invisible-island.net/ncurses/INSTALL.html):
  - "If you configure using the --enable-widec option, a "w" is appended to the library names (e.g., libncursesw.a)", the default since ncurses 6.5;
  - "If you configure using the --with-pthread option, a "t" is appended to the library names (e.g., libncursest.a, libncursestw.a)";
  - `--disable-lib-suffixes` "Suppress the "w", "t" or "tw" suffixes".
- **Not included**: `--with-extra-suffix`, `--with-pc-suffix` and the `_g`/`_p` debug and profile names. These are operator-chosen or non-release builds. A name they produce stays unmerged, which is the safe direction.
- Every other arm is `&["<slug>"]`.
- Slug lookup: `CuratedLibrary::from_slug(&str) -> Option<Self>`, also an exhaustive match. A unit test round-trips every variant (test-only `strum::EnumIter`).

## R4. LLVM suffix (FR-002)

Today `version_strings.rs` (~line 386) calls `parse_semver_triple(tail)`, which stops after `X.Y.Z`. TheRock's banner is `LLVM version 23.0.0git\0`; a second occurrence, `LLVM version 23.0.0git (52226beb…)`, is preceded by whitespace and rejected by spec 1643's boundary.

**Decision**: a dedicated `parse_llvm_version(tail)`. It takes `X.Y.Z`, then `[a-z0-9]*`, and requires the next byte to be NUL, a space or the end of the region; otherwise there is no match. `17.0.6\0` is unchanged.
- A tail such as `23.0.0-rc1` is rejected, because `-` is not a terminator. Today it would match as `23.0.0`. The census (FR-002) counts any such case before the change is wired; if one exists, the spec is amended.

## R5. ncurses patch date (FR-013)

`parse_ncurses_version` (~line 950) returns `X.Y` from `X.Y.YYYYMMDD`.

**Decision**: return the whole `X.Y.YYYYMMDD`. Its validation stays: 8 digits, then NUL.

Known consumers that pin `6.6`, and change as intended:
- unit tests at ~lines 1235 and 1587;
- `tests/version_string_boundary.rs` `NCURSES_V`;
- spec 1643's B-1 golden, which is regenerated and diffed.

## R6. ncurses CPE form (FR-008)

`generate/cpe.rs::synthesize_cpes`, `generic` arm (~line 255), formats every table pair with `format_cpe(vendor, product, version)`. That writes `*` into update. There is a precedent for per-product version shaping: OpenJDK's build suffix is stripped before formatting.

**Decision**:
- Add `format_cpe_with_update(vendor, product, version, update)`.
- In the `generic` arm, when `name == "ncurses"` and the version is `X.Y.YYYYMMDD` (8 digits), emit `version = X.Y` and `update = YYYYMMDD`.
- Every other library, and ncurses `X.Y`, is unchanged.

This reaches every `pkg:generic/ncurses` component whichever reader produced it (clarification 2), because every reader's components go through `synthesize_cpes`.
- **Inference, not traced in code**: the closure tier's `ncurses@6.4.20221231` also reaches it, through spec 1367's call to `synthesize_cpes`. The basis is its current CPE, which has exactly the table's form (`invisible-island:ncurses:6.4.20221231`).
- T010's `cpe_form_change` event on `nix-closure-openai` is the measurement that confirms or refutes this.

**Operator CPE map** (`cli/scan_cmd.rs` ~line 7545) is **not** changed. An operator's row is applied as written, and the operator owns its form.

**Evidence**: `measurements/nvd-version-forms-2026-11-22.txt`. Of 110 ncurses `cpeMatch` entries, 102 have `X.Y` in version, and 81 of those carry the date in update. None puts the date in version.

## R7. Recording the record's original name (FR-007)

There is no native construct for an alternative name: CycloneDX 1.6 `component` has no alias field, and `evidence.identity` records how a field was concluded, not another name.

**Decision**: no new annotation. The merged component keeps the original name in its evidence:
- its occurrences (C18 `waybill:source-files`) include `ncursesw.pc`, and a pkg-config package's name *is* its `.pc` basename (pkg-config's definition);
- it keeps the record's `waybill:source-mechanism` (`pkg-config`), because the record is first in the merge (R1).

For a CMake record, the config file is listed the same way, and C103 already carries a declared name that differs only by case. That satisfies FR-007 and Principle V without a new `waybill:*` row.

**Test**: the merged component's C18 contains the `.pc` path, and its source mechanism is `pkg-config`. C56 is **not** asserted: a curated entry adds nothing to it (R1, #1665).

## R8. Census (FR-001)

A DEBUG event on target `waybill::version_census`, field `shared_file`, emitted from `reconcile` when `tracing::enabled!`. It is one JSON object per file claimed by a curated entry and at least one record with a different PURL. The shape is in `contracts/wire.md`.

The log-only commit emits the event and rewrites nothing. The census is a run of the fixture repository's `census-versions.yml`, with `census/versions.py` extended to parse `shared_file`, plus the public corpus run.
- The same commit also logs the LLVM and ncurses version changes (R4, R5) as `curated_version_change` events: old and new version per match, with nothing emitted from them yet.
- It also logs the ncurses CPE changes as `cpe_form_change` events, so the census counts FR-002, FR-008 and FR-013 before any of them is wired.

## R9. Mutations (SC-004)

| ID | Mutation | Must be killed by |
|---|---|---|
| M1 | `names()` for ncurses returns the slug only | the name-variant case |
| M2 | any record name accepted (name check removed) | the bundled-library negative case |
| M3 | version agreement made a plain string prefix | N-3 (`2.7.5` against `2.7.50`) |
| M4 | the single-record condition removed (first qualifying record taken) | the two-records case |
| M5 | LLVM suffix dropped (`parse_semver_triple` restored) | the version-suffix case, and the parser unit test |
| M6 | `depends` remap removed | the `formw` `Requires` case |
| M7 | ncurses patch date dropped (`X.Y` restored) | the B-1 golden, and the parser unit test |
| M8 | ncurses CPE update split removed | the CPE case |

Every mutation must be killed. A survivor means a missing test.

## R10. Timing

The pass is linear in entries and in each curated entry's files. TheRock has 3,284 components. SC-007 is measured by `measure-full` (one warm-up and 3 interleaved runs per arm). No prediction is made here.
