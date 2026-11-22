# Tasks: bzip2, GMP and xz are versioned from the bare version they embed, when a second anchor names the library (#1641)

**Input**: `specs/1644-bare-version-anchors/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002 to SC-006).
- **Written red.** Every case in `contracts/wire.md` that expects a component (A-1 to A-4, P-1, P-2) is written before the change that produces it, and must fail on the log-only commit.
- **Guards.** The N cases expect nothing. They pass on `main` trivially, because no pattern exists yet. Their teeth come from the mutations (T015), not from a red run.

**Paths**:
- `VS` = `waybill-cli/src/scan_fs/binary/version_strings.rs`
- `SC` = `waybill-cli/src/scan_fs/binary/scan.rs` (symbol collection, ~line 309)
- `EN` = `waybill-cli/src/scan_fs/binary/entry.rs` (`BinaryScan`, ~line 585)
- `BM` = `waybill-cli/src/scan_fs/binary/mod.rs` (`prepare_held`, ~line 1107)
- `CT` = `waybill-cli/src/generate/cpe_table.toml`
- `IT` = `waybill-cli/tests/version_string_anchored.rs` (new)
- `IL` = `waybill-cli/tests/common/installed_lib.rs`
- `MS` = `specs/1644-bare-version-anchors/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R10. Cases are `contracts/wire.md`'s. Mutations are R9's M1–M7.

## Phase 1: Setup

- [ ] T001 Build `main`'s binary at `110d1a58` into the scratchpad as `waybill-main-110d1a58`, and record the full SHA in `MS/control.md`. It is the control for every "byte-identical to `main`" claim.
- [ ] T002 Add `elf_shared_lib_with_dynsym(rodata, defined: &[&str], undefined: &[&str]) -> Vec<u8>` to `IL`, test-only (R8).
  - Sections: `.rodata`, `.dynsym` (`SHT_DYNSYM`, `sh_link` to `.dynstr`, a null entry first; defined entries `STB_GLOBAL`/`STT_FUNC` with `st_shndx` = the `.rodata` index; undefined entries `SHN_UNDEF`), `.dynstr` and `.shstrtab`.
  - **Control in the same task:** a test in `IL`'s users (or `IT`) reads the output with `object` and asserts that the defined names report `!is_undefined()` and the undefined ones report `is_undefined()`. Without it, a helper that writes every symbol as undefined would make N-3 pass for the wrong reason.
- [ ] T003 Create `IT` with `mod common;` and helpers on `IL`:
  - `lib_dir(bytes, file_name) -> TempDir`;
  - `scan3(dir)`, which honours `WAYBILL_VERSION_ANCHORED_BIN` so a case can run against T001's binary;
  - `scan_with_log(dir, "waybill::version_census=debug") -> (docs, stderr)`;
  - `curated(doc, slug)`, searched recursively;
  - `parity(dir)`.

  Reuse spec 1643's `version_string_boundary.rs` helpers by copying the needed ones, not by importing a test file.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [ ] T004 In `VS`, add `CuratedLibrary::{Bzip2, Gmp, Liblzma}` with slugs `bzip2`, `gmp`, `liblzma`, and `Boundary::WholeString` (data-model).
  - Extend every exhaustive match: `boundary()`, `slug()`, the CPE and emission paths that match on the variant, the test-only `sample()` and spec 1643's `ANCHORED` set (unchanged: the new variants are not anchored).
  - `pattern_at` must never yield a `WholeString` library. Add a unit test that iterates every variant (`strum::EnumIter`), asserting that `scan()` over each new library's own candidate and anchor bytes returns no match for it.
  - Adjust spec 1643's `s1643_every_variant_follows_its_class` and `s1643_census_agrees_with_the_scanner` so the new class is handled explicitly, not skipped by a wildcard.
- [ ] T005 [P] Add `pub const ANCHOR_SYMBOLS: &[&str] = &["lzma_version_string", "lzma_version_number"]` in `VS` (R3). Add `pub anchor_symbols: Vec<String>` to `BinaryScan` in `EN`, initialised empty in every constructor, including the test default at ~line 1387.
  - In `SC`, fill it with the defined names in `ANCHOR_SYMBOLS`: ELF `dynamic_symbols()` where `!is_undefined()`; Mach-O `symbols()` where `is_global() && !is_undefined()`, after the existing underscore strip; PE export names.
  - Leave `symbol_names` byte-for-byte unchanged.
  - Unit test on T002's helper: defines `lzma_version_string` → collected; imports it → not collected; defines `lzma_code` → not collected (not in the list).
- [ ] T006 In `VS`, add the pure function `anchored_verdicts(region: &[u8], anchor_symbols: &[String]) -> Vec<AnchoredVerdict>` (data-model).
  1. Find the text anchors with `memchr::memmem`, and the symbol anchor in `anchor_symbols`.
  2. Return early, with nothing, when no anchor is found.
  3. Otherwise split the region on NUL once and classify each whole string against each anchored library's shape (FR-004).
  4. Dedupe the candidates, apply FR-005a's removal, then decide the verdict.

  Unit tests cover every row of data-model's verdict table, including:
  - a duplicate candidate counted once;
  - a candidate at the region's start;
  - a candidate at the region's end without a trailing NUL, which is not a whole string and is rejected;
  - the bzip2 date form with each malformed part (month `Foo`, a 3-digit year);
  - xz's `alpha`/`beta` suffixes, and a rejected commit suffix.
- [ ] T007 In `BM`'s `prepare_held`, inside the existing `tracing::enabled!(target: "waybill::version_census", DEBUG)` block, emit one `anchored` event per verdict with the JSON shape in `contracts/wire.md`. No matches are appended yet.
  - In `IT`, the positive control **C-1**: fixture A-2 run with `scan_with_log` logs exactly one `anchored` event, with `"library":"gmp"` and `"verdict":"versioned"`. Its documents still have **no** gmp component (log-only).
  - **C-2**: a library with no anchor logs no `anchored` event.
- [ ] T008 Write the cases of `contracts/wire.md` in `IT`, with their expected results:
  - A-1 to A-4, P-1 and P-2 are red on this commit;
  - the N cases and B-1 pass.

  Record the red/green table in `MS/control.md`, run with both this commit's binary and T001's. B-1 reuses spec 1643's golden (`waybill-cli/tests/fixtures/version_string_boundary/b1-golden/`), compared byte for byte with per-run fields masked.
- [ ] T009 Run the gate, `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, expecting only the planned red cases to fail. Mark the red cases `#[ignore = "spec 1644: red until T011"]` for this commit, and list them in `MS/control.md`. Commit the log-only change.
- [ ] T010 Census (FR-001, R7). In `FR`:
  - Extend `census/versions.py` to parse `anchored` events, and to read each target's main-control CycloneDX document for components named `bzip2`, `gmp`, `liblzma` or `xz`, with their versions, evidence kinds and source files.
  - Add a committed synthetic positive-control tree (A-1, A-2, A-3) to `census-versions.yml`.

  Dispatch with `--ref main -f waybill_ref=<T009's full SHA>`, then verify every job's `refs.txt`. Dispatch the public corpus with `-f branch=1644-bare-version-anchors`, and confirm the summary names T009's commit and that the goldens are byte-identical. Record in `MS/census.md`:
  - per target: anchored binaries, each anchor and where it sits, the candidates, the verdict, and existing components with their names (FR-001 bullets 1–3);
  - xz: whether `lzma_version_string`/`lzma_version_number` are defined in every liblzma counted, and whether any string-region text is present in all of them (FR-003);
  - bzip2: libraries carrying the date form without the anchor, and other binaries carrying the form at all (FR-006);
  - the ambiguous and ambiguous-shared counts, with the binaries named;
  - every verdict judged against the surrounding bytes: is the version the library's own?

  **Checkpoint**: if any count contradicts FR-002 to FR-007, amend the spec with the evidence before Phase 3.

## Phase 3: User Story 1 — a bundled bzip2, GMP or xz gets its version (P1) 🎯 MVP

**Goal**: anchored binaries with one candidate emit `pkg:generic/<slug>@<version>`.
**Independent test**: A-1 to A-4 pass; the N cases still pass.

- [ ] T011 [US1] In `BM`, append to `p.version_matches` one `EmbeddedVersionMatch` per `Versioned` verdict from `anchored_verdicts(&scan.string_region, &scan.anchor_symbols)`. Compute the verdicts once and use them for both the matches and the census. Remove T009's `#[ignore]` from A-1 to A-4. A-1 to A-4 pass, and all N cases still pass.
- [ ] T012 [P] [US1] In `CT`, add the rows:
  - `pkg:generic/gmp`: `gmplib:gmp`, with `dictionary_entries = 35`, `observed = "2026-11-22"`, `dictionary_evidence = "specs/1644-bare-version-anchors/measurements/cpe-probe-2026-11-22.txt"` and `title_evidence = "specs/1644-bare-version-anchors/measurements/nvd-titles-2026-11-22.txt"`.
  - `pkg:generic/liblzma`: `tukaani:xz` (62, `MS/cpe-probe-2026-11-22.txt`) and `xz_project:xz` (32, spec 1375's 2026-11-13 evidence), each with `title_evidence = "specs/1375-generic-cpe-table/measurements/nvd-titles.txt"`, which holds `=== xz (`. Comment the row the way the existing `bzip2` row is commented: NVD has no `liblzma` product.

  Confirm `cpe.rs`'s table tests pass, including `m1375_every_product_has_a_committed_title_query`. If any evidence file is not in the format the test reads, re-run the probe into the right format; never edit evidence by hand. In `IT`, assert the CPE candidates of A-1, A-2 and A-3 (`contracts/wire.md`).
- [ ] T013 [US1] `parity(dir)` for A-1 to A-4 (SC-005).

## Phase 4: User Story 2 — a bare version is never a component by itself (P1)

**Goal**: no unanchored, ambiguous, shared, imported or partial-string case emits anything.
**Independent test**: the N cases pass, and every mutation is killed.

- [ ] T014 [US2] In `IT`, assert the census verdicts for:
  - N-4: `ambiguous`, with both candidates;
  - N-5: `ambiguous-shared`, for both gmp and liblzma;
  - N-7: `no-candidate`.

  This uses `scan_with_log`, so the absence of a component is tied to the stated reason (FR-011).
- [ ] T015 [US2] Copy spec 1643's mutation runner to `MS/mutate.py`, with its full-path test filter, its "no test result line → HARNESS ERROR" guard and its `cp` restore. Write M1–M7 (R9) as exact-string replacements in `VS`/`SC`, run each, and record the killing tests in `MS/mutations.md`. Every mutation must be killed (SC-003). A survivor means a missing test: add the test and re-run. Then confirm the source is restored (`git diff --stat` shows only intended changes) and rebuild.

## Phase 5: User Story 3 — one library, one component (P2)

**Goal**: an embedded version and an agreeing `.pc` file give one component.
**Independent test**: P-1 has exactly one `pkg:generic/liblzma@5.8.1`; P-2 has both versions.

- [ ] T016 [US3] Remove P-1's and P-2's `#[ignore]`. For P-1, also assert that the single component's evidence carries both the embedded match and the `.pc` source file, with the same `waybill:source-files` shape as spec 1643's merged `zlib@1.3.2`. If the merge does not happen, stop and trace it: that is a finding, not a test to loosen.

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: byte-identity off the anchored path; every corpus difference traced.
**Independent test**: B-1, the public corpus, and per-component traces against the `main` control.

- [ ] T017 [US4] Run the public corpus at the final commit with `-f branch=1644-bare-version-anchors`, and confirm the summary's scanned commit. Record the result in `MS/corpus.md`: either 66 of 66 byte-identical, or every changed component traced to one binary, one library and one anchor, and judged as T010 judges.
- [ ] T018 [US4] Run the quality corpus and TheRock at the final commit against T001's `main` control, through `FR`'s `census-versions.yml` (documents) and `measure-full` (timing), verifying `refs.txt`. Record in `MS/therock.md`:
  - every component added, removed or changed, traced component by component;
  - any second component for one library (FR-007), with a follow-up issue filed if one appears;
  - `waybill sbom parity-check` on the base and branch documents;
  - SC-007's timing: one warm-up per arm, 3 interleaved runs per arm, the median ratio, and min/median/max per arm.

  A ratio above 1.05 is a failure to diagnose by measurement (as in spec 1643's SC-006), not to re-run away.

## Phase 7: Polish

- [ ] T019 [P] In `docs/reference/binary-identity.md`, add the three libraries and the anchored rule (anchor, whole-string candidate, one candidate or none, a shared candidate versions neither, defined symbols only). Update `VS`'s module doc table with a row per library and the `WholeString` class.
- [ ] T020 [P] `CHANGELOG.md`: an Added entry for bzip2, GMP and xz (liblzma) versions from embedded strings, citing #1641.
- [ ] T021 Run the full gate on the final commit and record the per-target `test result: ok. N passed; 0 failed` totals and the `>>> all pre-PR checks passed.` line in `MS/gate-log.md`. Enumerate every failing test by name if any fail; never call a failure a flake without the 30× isolation loop.
- [ ] T022 Close-out. For every task above, check that its claim is true of the committed tree and measurements, not merely that its box is ticked. Then tick it.

## Dependencies

- **Setup.** T001 → T008. T002 → T005, T008. T003 → T007, T008.
- **Foundational, in order.** T004 → T006 → T007 → T008 → T009 → T010. T005 can run beside T004.
- **US1** (T011–T013) needs T010's checkpoint. T012 can run beside T011.
- **US2** (T014–T015) needs T011.
- **US3** (T016) needs T011 and T012.
- **US4** (T017–T018) needs every code change, so it runs after T016.
- **Polish.** T019 and T020 can run any time after T011. T021 and T022 come last.

## Parallel opportunities

- T004 with T005 (different files: `VS` versus `SC`/`EN`; T005 adds only a constant to `VS`, at its end).
- T011 with T012 (`BM` versus `CT`).
- T019 with T020.
- The census (T010) and the final runs (T017, T018) are CI dispatches. While one runs, the next local task can proceed.

## Implementation strategy

The MVP is US1 behind the census:
1. Setup.
2. The log-only foundational phase, with the census read and the spec amended if needed.
3. T011–T013.

US2's mutations are what make US1 trustworthy, so they ship in the same PR, as do US3 and US4. One PR covers the whole spec, the way spec 1643 shipped.
