# Tasks: a statically linked liblzma is versioned from the version it embeds (#1664)

**Input**: `specs/1647-static-liblzma-anchor/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002 to SC-005).
- **Written red.** S1, S2 and S5 (`contracts/wire.md`) are written before the change that alters them, and fail on the log-only commit. S5 fails because it gives `gmp@5.4.5` there, as on `main`.
- **Guards.** S3, S4 and S6–S8 pass on `main`. Their teeth come from mutations M1–M4.

**Paths**:
- `SC` = `waybill-cli/src/scan_fs/binary/scan.rs` (the anchor read, ~lines 340–367)
- `EN` = `waybill-cli/src/scan_fs/binary/entry.rs` (`BinaryScan.anchor_symbols`, ~line 597)
- `VS` = `waybill-cli/src/scan_fs/binary/version_strings.rs` (`ANCHOR_SYMBOLS` ~520, `Anchor`, `anchored_verdicts` ~757)
- `MD` = `waybill-cli/src/scan_fs/binary/mod.rs` (`prepare`, the census block ~1110)
- `IL` = `waybill-cli/tests/common/installed_lib.rs`
- `IT` = `waybill-cli/tests/version_string_static.rs` (new)
- `MS` = `specs/1647-static-liblzma-anchor/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R7, cases are `contracts/wire.md`'s S1–S8, and mutations are R7's M1–M4.
- Rule A is FR-002 (defined `.symtab` anchors). Rule B is FR-004 (the xz header magic).

## Phase 1: Setup

- [X] T001 Build `main`'s binary at `b6ad3c26` into the scratchpad as `waybill-main-b6ad3c26`. Before building, confirm that `git diff b6ad3c26 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.
- [X] T002 [P] In `IL`, add the test-only `elf_with_symtab(e_type: u16, rodata: &[u8], defined: &[&str], undefined: &[&str]) -> Vec<u8>` (R6).
  - Shape: `.rodata`, `.symtab` and `.strtab`, no `.dynsym`, modelled on `elf_shared_lib_with_dynsym`.
  - `defined` entries are global functions in `.rodata`; `undefined` entries are `SHN_UNDEF`.
  - Add a variant, or a flag, that also writes `.dynsym` with the same defined names, for S6.
  - A unit-level sanity test parses its output with `object` and checks that `symbols()` returns exactly the given names, with their definedness.
  - *As built*: `elf_with_symtab` wraps a general `elf_with_tables(e_type, rodata, dynsym: Option<(defined, undefined)>, symtab: (defined, undefined))`, which S6 uses for both tables. The sanity test is `helper_writes_symtab_only_with_definedness` in `IT`.
- [X] T003 [P] Create `IT`, copying the harness from `waybill-cli/tests/version_string_anchored.rs` (`region`, `tree`, scan in three formats with parity and unique `bom-ref`s, census events from stderr). Add `WAYBILL_VERSION_STATIC_BIN`, to run a case against T001's binary.
  - *As built*: `Scan` keeps the CycloneDX document and the log; `scan_with(bin, dir)` returns the three documents' bytes for the control.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [X] T004 In `VS`, add `AnchorSymbol { name, table: SymbolTable, binding: Binding }`, `enum SymbolTable { Dynsym, Symtab, MachoSymtab, PeExport }` and `enum Binding { Global, Weak, Local, Unknown }` (data-model).
  - In `EN`, change `anchor_symbols` to `Vec<AnchorSymbol>`.
  - In `SC`, fill it from today's three sources (`.dynsym`, Mach-O globals, PE exports), with table and binding, as today: defined only, deduplicated by `(name, table)`.
  - `anchored_verdicts` takes `&[AnchorSymbol]` and keys on names only, so verdicts are unchanged.
  - `Anchor::Symbol` gains `table` and `binding`. The `anchored` event's symbol anchors gain `"table"` and `"binding"`; every other field is unchanged (contracts/wire.md).
  - Unit tests: the existing spec 1644 verdict tests pass after mechanical type changes, and an `anchored` JSON test shows the two new keys.
  - *As built*: the spec 1644 test `defined_anchor_symbol_anchors_elf_and_macho` asserted the old anchor JSON exactly, and failed the first gate. It now asserts the new full form for each format (`dynsym` on ELF, `macho-symtab` on Mach-O).
- [X] T005 In `SC`, add the census-only read of `.symtab` (R3):
  - one `memchr::memmem` search for `lzma_version_` over the raw `.strtab` section;
  - only on a hit, iterate `file.symbols()`, keeping **defined** entries named in `ANCHOR_SYMBOLS`, with their binding.

  Store them in a new never-emitted `BinaryScan.static_anchor_symbols: Vec<AnchorSymbol>` (table `Symtab`), plus `elf_type`. They do **not** feed `anchor_symbols` on this commit.
  - Unit tests on T002's helper: defined in `.symtab` is collected; undefined is not; a `.strtab` without the needle skips iteration; a non-ELF file gives empty.
  - *As built*: the four checks are integration cases, not unit tests in `SC`, because the `.symtab` builder lives in `tests/common`: S1 and S2 (collected), S3 (undefined), S7 (no needle), S6 (both tables). Recorded in `MS/control.md`.
- [X] T006 In `VS` and `MD`, emit the `liblzma_static` census event (contracts/wire.md, R4), for a binary of any format, when either:
  - rule A applies (ELF only): `static_anchor_symbols` holds a name `anchor_symbols` does not;
  - rule B applies: the region holds `FD 37 7A 58 5A 00` and both lists are empty.

  Each of the following is the **whole** list `anchored_verdicts` returns, every anchored library with its verdict, not only liblzma's:
  - `current`: production's `anchored_verdicts(region, anchor_symbols)`;
  - `rule_a`: `anchored_verdicts(region, anchor_symbols ∪ static_anchor_symbols)`;
  - `rule_b`: `anchored_verdicts(region, anchor_symbols plus a hypothetical Dynsym lzma_version_string)`.

  Keys are sorted, an absent rule is omitted, and `elf_type` is omitted for a non-ELF binary.
  - Unit tests: the event's JSON for a rule A case, for a rule B case, and for S5's shape, where `current` has gmp `versioned` and `rule_a` has gmp and liblzma `ambiguous-shared`.
  - *As built*: `version_strings::liblzma_static_event`, with five unit tests `s1647_*` in `VS`. They run in the binary target: `cargo test -p waybill --bins s1647`.
- [X] T007 Write S1–S8 in `IT` with their expected results:
  - S1, S2 and S5 are red on this commit; S3, S4 and S6–S8 pass.
  - Each case also asserts its census, by the liblzma entry unless stated:
    - S1 and S2: `rule_a` is `versioned` at `5.4.5`;
    - S3: no event;
    - S4: `rule_a` is `ambiguous`;
    - S5: `current` has gmp `versioned` at `5.4.5`, and `rule_a` has gmp and liblzma `ambiguous-shared`;
    - S8: `rule_b` is `versioned` (this commit only).

  Run the cases with this commit's binary and with T001's (`WAYBILL_VERSION_STATIC_BIN`). Record the red/green table in `MS/control.md`, and confirm that T001's binary gives byte-identical documents to this commit's for every case.
  - *As built*: the control is `control_byte_identical_to_main` (ignored unless `WAYBILL_VERSION_STATIC_MAIN_BIN` is set). It masks version-4 UUIDs, because the CycloneDX serial is random per run (`MS/control.md`). The red cases check their census first, so their log-only failure is the component line.
- [X] T008 Mark S1, S2 and S5 `#[ignore = "spec 1647: red until T011"]`. Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, then check `GATE_RESULT=pass`, the expected line, and the per-target totals. Commit the log-only change and push.
  - *As built*: the first gate failed at clippy (an unread field) and the second on the spec 1644 assertion (T004's note). The third passed at `7c78ac6c`: 430 suites, 11,057 passed, 0 failed, 51 ignored (47 before, plus S1, S2, S5 and the control).
- [X] T009 Census (FR-001, R5).

  In `FR`:
  - extend `census/versions.py` to parse `liblzma_static`, summarising per target:
    - files by rule, by ELF type and by binding;
    - each rule's verdict counts, per library;
    - the components each rule would newly version, and the versions it would remove, found by comparing the rule's list with `current`;
    - for each newly versioned liblzma, any `liblzma` or `xz` component the same document already has from another reader (pkg-config, CMake, a package database), with its version;
  - add to the `control` job: `apt-get install liblzma-dev`, then compile `MS/probe/a.c` (copied into the fixture repository with its provenance) with `gcc -O2 -static -llzma`, unstripped and stripped. Assert that the unstripped build gives `rule_a.verdict = versioned` at the runner's xz version, and that the stripped one gives `rule_b` (log-only) or no event (wired, rule B removed). The assertion accepts both states.

  Then:
  - dispatch `census-versions.yml` with `--ref main -f waybill_ref=<T008's full SHA>`, and verify every `refs.txt`;
  - dispatch `public-corpus.yml` with `-f branch=1647-static-liblzma-anchor`, and confirm 166 of 166 at T008's commit. Its harness keeps no census log, so on the public corpus FR-001 is measured as output: unchanged here, and traced in T015;
  - locally (not TheRock): extract the 13 magic-carrying files listed in `MS/magic-population-2026-11-23.md` from `ubuntu:24.04` with the same packages, scan each alone with T008's build and the census on, and record every `liblzma_static` event.

  Record everything in `MS/census.md`. State plainly any count that is zero, TheRock's above all.
  - *As built*: run 45201916920 (20 of 20 jobs, every `refs.txt` at `7c78ac6c`) and public corpus run 45201924694 (166 of 166, scanned commit `7c78ac6c`). TheRock: rule A **0 files**; rule B 5 files, all `no-candidate`. Quality corpus: 0 events. Ubuntu: `7zr` and `7za` get rule B `liblzma@13.2.0`. Static control: both rules fired on the runner's real build at 5.4.5. Everything is in `MS/census.md`.

- [X] T010 **Checkpoint: decide FR-004.**
  - If T009 records `7zr` (or any non-liblzma file) with `rule_b.verdict = versioned`: amend spec FR-004 to **rejected**, citing `MS/census.md` and `MS/magic-population-2026-11-23.md`, and amend wire S8 and data-model accordingly.
  - If no non-liblzma file is versioned by rule B: stop. Report to the user that FR-004's first condition (the build matrix) is now needed, and do not wire rule B without it.
  - If a rule A count contradicts FR-002 (for example a local-binding or relocatable-object case that is not liblzma), amend the spec first.
  - *As built*: FR-004 is **rejected** (`7zr`, `7za`) and amended in spec.md, contracts/wire.md (S8) and data-model.md. FR-002 is confirmed. This spec does not change TheRock's output.

## Phase 3: User Story 1 — an unstripped static liblzma gets its version (P1) 🎯 MVP

**Goal**: defined `.symtab` anchors version liblzma.
**Independent test**: S1 and S2 pass; S6 and S7 are unchanged.

- [X] T011 [US1] Wire rule A:
  - in `SC`, merge `static_anchor_symbols` into `anchor_symbols` (keeping table and binding), and remove the separate field;
  - in `MD`, keep the `liblzma_static` event for files anchored only by `.symtab`, with `rule_a` reporting the production verdict;
  - if T010 rejected FR-004, remove rule B's evaluation, the magic search and the `magic` field.

  Then:
  - remove the `#[ignore]` from S1, S2 and S5;
  - update the census assertions to the wired state: `current` now equals `rule_a`;
  - if rule B is removed, drop S8's `rule_b` assertion and the `magic` key from T006's unit tests, and assert that S8 has no event at all.
  - *As built*: `scan.rs` appends the `.symtab` anchors to `anchor_symbols` (`symtab_anchor_symbols` is local), and the `static_anchor_symbols` field is gone. `liblzma_static_event(anchor_symbols, elf_type, current)` fires for a binary with a `.symtab` anchor that no other table defines. `rule_a` equals `current`, kept so the census reads one shape. Rule B, the magic constant and its search are removed. The unit test `s1647_rule_b_from_magic` became `s1647_magic_alone_anchors_nothing`, and S8 asserts no event. The control still passes on S3, S4 and S6–S8, and S1 now differs from `main`, which is the control's teeth.

## Phase 4: User Story 2 — a program that only uses liblzma is never anchored (P1)

**Goal**: definedness and spec 1644's rules hold for `.symtab` anchors.
**Independent test**: S3, S4 and S5 pass; mutations M1–M4 are killed.

- [X] T012 [US2] Add a unit test in `SC` for a program whose `.dynsym` **and** `.symtab` both list `lzma_version_string` as undefined. It gives no anchor in either list.
  - *As built*: an integration test, `importer_undefined_in_both_tables_never_anchors` in `IT`, because the ELF builder is test-side (see T005).

- [X] T013 [US2] Run mutations M1–M4 (R7). Copy spec 1646's `measurements/mutate.py` to `MS/mutate.py`, and write each mutation as an exact-string replacement. Record the killing tests in `MS/mutations.md`. Every mutation must be killed. Afterwards, `diff -r` the source against a copy taken before the run, and rebuild.
  - *As built*: 5 of 5 killed, including an added M5 (a `.dynsym` anchor labelled `symtab`). `MS/mutations.md`.

## Phase 5: User Story 3 — a stripped static liblzma: decided by measurement (P2)

**Goal**: FR-004's outcome is recorded and visible.
**Independent test**: S8 gives no liblzma component (if rejected), and #1664 records why.

- [X] T014 [US3] Comment on #1664 with FR-004's measured outcome: the magic's population, the `7zr` false version with waybill's census line, and that a stripped static liblzma remains unversioned. End the comment with `---` and the attribution line. No session URLs.
  - *As built*: posted on #1664 (issue comment 7105769275), with the attribution line.

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: every difference is traced.
**Independent test**: the public corpus, the quality corpus and TheRock, each against a `main` control.

- [X] T015 [US4] Public corpus at the final code commit (`-f branch=1647-static-liblzma-anchor`), with the summary's commit confirmed.
  - **No drift**: record the 166 of 166 run.
  - **Drift**: trace every changed line to a newly versioned liblzma component, regenerate with `regen_goldens=true`, `cmp` the regenerated files with the traced output, commit them, and confirm 166 of 166.

  Record all of it in `MS/corpus.md`.
  - *As built*: no drift. Run 45206557193 scanned `541e2bff` and passed 166 of 166 (`MS/corpus.md`).

- [X] T016 [US4] Quality corpus (`census-versions.yml`) and TheRock (`measure-full`, base `b6ad3c26` against the final code commit), at full SHAs, with every `refs.txt` verified. Record in `MS/therock.md`:
  - every component that changed, traced to one file and one anchor, including any GMP version removed by a shared candidate. If T009 found none on TheRock, the expected result is no change, and it is stated as such;
  - the recursive component counts and the edge counts, compared;
  - `waybill sbom parity-check` on both arms;
  - SC-007's timing: one warm-up per arm, 3 interleaved runs, the median ratio, and min/median/max.
  - *As built*: TheRock's documents are identical to `main`'s in all three formats with UUIDs and timestamps masked. parity-check gives 260 of 261 on both arms, with the same A5 row failing. All 17 quality-corpus documents are identical. Only the static control gained `liblzma@5.4.5`. Median ratio 1.001 (`MS/therock.md`).

## Phase 7: Polish

- [X] T017 [P] `docs/reference/binary-identity.md`:
  - the liblzma anchor row: a defined symbol in the ELF dynamic or static symbol table, a Mach-O defined external, or a PE export;
  - replace "A statically linked liblzma whose symbols are hidden … gets no liblzma version" with what holds now: unstripped static builds are versioned; stripped ones are not, with FR-004's reason.
  - *As built*: the liblzma anchor row, and three bullets replacing the hidden-symbols sentence: importers from either table, unstripped static builds versioned, stripped ones not (with the 7-Zip reason).

- [X] T018 [P] `CHANGELOG.md`: a Fixed entry citing #1664, built from T009, T015 and T016's measurements.
  - *As built*: a Fixed entry above #1665's, built from T009, T015 and T016.

- [ ] T019 Run the full gate on the final commit, and record in `MS/gate-log.md`:
  - `GATE_RESULT`, the `>>> all pre-PR checks passed.` line, and the per-target totals;
  - that `git diff b6ad3c26 -- Cargo.lock '*Cargo.toml'` is empty.
- [ ] T020 Close-out. For every task above, check that its claim is true of the committed tree and measurements. Where the implementation differs from the task text, note it inline rather than tick a false claim. Then tick it.

## Dependencies

- **Setup**: T001 → T007. T002 → T005 and T007. T003 → T007.
- **Foundational**: T004 → T005 → T006 → T007 → T008 → T009 → T010.
- **US1**: T011 needs T010.
- **US2**: T012 can follow T005. T013 needs T011.
- **US3**: T014 needs T010 and T011.
- **US4**: T015 and T016 need every code change (T011).
- **Polish**: T017 and T018 after T011, with T018's numbers after T015 and T016. T019 and T020 last.

## Parallel opportunities

- T002 (`IL`) and T003 (`IT`) together.
- T012 alongside T006–T008, since it only adds a unit test.
- T017 with T018.
- The CI dispatches (T009, T015, T016) run while local work continues.

## Implementation strategy

- **MVP**: Phases 1–3. The census, then rule A wired.
- **The scope may shrink.** If T009 finds nothing on TheRock, the change still fixes unstripped static liblzma generally, but #1543 gains nothing from it. The CHANGELOG and the #1543 tracker say so plainly.
