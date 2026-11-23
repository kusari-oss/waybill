# Tasks: dynamic linkage and embedded source paths are labelled binary analysis (#1668)

**Input**: `specs/1648-binary-kind-technique/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002 to SC-004).
- **Written red.** K1, K2 and K6 (`contracts/wire.md`) are written before the change that alters them, and fail on the log-only commit.
- **Guards.** K3, K4, K5 and K7 pass on `main`, and their teeth come from mutations.

**Paths**:
- `BM` = `waybill-cli/src/scan_fs/binary/mod.rs` (`BINARY_EVIDENCE_KINDS` ~68, its drift test ~94)
- `SM` = `waybill-cli/src/scan_fs/mod.rs` (`entry_technique` ~270, `census_entry_technique` ~281)
- `NC` = `waybill-cli/src/scan_fs/package_db/nix/closure/contents.rs` (`component_from_entry`)
- `DD` = `waybill-cli/src/resolve/deduplicator.rs` (`BINARY_KINDS` 17, `effective_technique` ~23, merge sites S1 and S2)
- `GB` = `waybill-cli/src/scan_fs/package_db/go_binary.rs` (`emit_entries_from_info` ~934)
- `IL` = `waybill-cli/tests/common/installed_lib.rs`
- `IT` = `waybill-cli/tests/binary_kind_technique.rs` (new)
- `MS` = `specs/1648-binary-kind-technique/measurements/`
- `FR` = kusari-sandbox/waybill-fixture-therock-sdk
- Research decisions are R1–R7, cases are wire.md's K1–K7, and mutations are R7's M1–M5.

## Phase 1: Setup

- [X] T001 Build `main`'s binary at `33c445f6` into the scratchpad as `waybill-main-33c445f6`. Before building, confirm `git diff 33c445f6 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.
  - *As built*: `MS/control.md`.
- [X] T002 [P] In `IL`, add the test-only `elf_with_needed(needed: &[&str], rodata: &[u8]) -> Vec<u8>` (R6): an ELF64 `ET_DYN` file with `.rodata`, a `.dynamic` section holding one `DT_NEEDED` per name, and a `.dynstr`.
  - Before writing it, check how `scan_fs/binary/linkage.rs` reads `DT_NEEDED` (through `object`'s dynamic-section accessors, or through program headers), and build exactly what that reader needs.
  - A sanity test in `IT` scans one such file and finds the `dynamic-linkage` component for each name. That test is the helper's positive control.
  - *As built*: **no ELF builder.** waybill reads ELF imports through `object`'s `imports()`, which names a library only through symbol versioning, not `DT_NEEDED` (`MS/control.md`). K1 uses the existing `macho_dylib` helper, whose `LC_LOAD_DYLIB` gives a real `dynamic-linkage` component; `IL` is unchanged.
- [X] T003 [P] Create `IT` with the harness from `waybill-cli/tests/version_string_static.rs`: `tree`, `scan_with(bin, dir)` returning the three documents' bytes, CycloneDX and the log, `cdx_components`, parity and unique `bom-ref`s.
  - Add `technique(cdx, purl) -> Vec<String>` (the CycloneDX methods' techniques) and `d1(doc, purl)` for SPDX 2.3 and SPDX 3.
  - Add `census(stderr, event) -> Vec<Value>`.
  - Add the control `control_byte_identical_to_main`. It is ignored unless `WAYBILL_BINARY_KIND_MAIN_BIN` is set, and masks version-4 UUIDs. `WAYBILL_FIXED_TIMESTAMP` already fixes timestamps. The case list comes from `WAYBILL_BINARY_KIND_IDENTICAL`.
  - Bring in the fixture builders R6 names: `elf_with_package_note`, the `.dep-v0` cargo-auditable fixture, the embedded-source-path tree, and a synthetic Go BuildInfo binary. Move a builder into `tests/common` only where `IT` needs it from another test file, and keep a comment naming where it came from.
  - *As built*: the builders were copied, with comments naming their sources: `elf_with_section`, `elf_with_package_note`, `cargo_auditable_elf` and `go_binary`. The SPDX D1 readers were copied from `tests/merged_binary_evidence.rs`. A temporary probe test listed each case's output on `main`, and it was removed once the expectations were written.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [X] T004 In `BM`, add `pub(crate) fn binary_kind_technique(kind: &str) -> Option<ResolutionTechnique>` (data-model).
  - It returns `BinaryAnalysis` for `embedded-version-string`, `symbol-fingerprint`, `dynamic-linkage` and `embedded-source-path`.
  - It returns `PackageDatabase` for `cargo-auditable`, `elf-note-package`, `jdk-runtime-collapsed` and `python-stdlib-collapsed`.
  - It returns `None` for anything else.
  - Unit tests `s1648_*`: every entry of `BINARY_EVIDENCE_KINDS` maps to `Some` (totality, FR-006); each kind maps to its decided technique; a non-binary kind (`requirements-txt`) maps to `None`.
  - Nothing calls it on this commit except the census (T005).
  - *As built*: plus `s1648_each_kind_has_its_decided_technique`. The doc comment quotes the CycloneDX definitions.
- [X] T005 In `SM` and `NC`, at the conversion sites, emit the census `kind_technique_change` event (spec 1646's `technique_change` shape, `evidence_kind`, `purl`, `old`, `new`, under its own name so spec 1646's live control is not affected) for each entry whose `binary_kind_technique` differs from the technique it is converted with. The technique given is unchanged.
  - In `census_entry_technique`, rename the `detected_go` label to `detected-go` (R3).
  - *As built*: both in `SM::census_entry_technique`, which both conversion sites already call, so `NC` needed no edit.
- [X] T006 [P] In `GB::emit_entries_from_info`, emit a `binary_derived_kind` census event with `evidence_kind: "go-buildinfo"`, the entry's PURL, the binary's path and `technique: "package-database"`, for each entry it creates (R3, FR-005). Off unless `waybill::version_census=debug` is enabled.
  - *As built*: `emit_entries_from_info` is now a wrapper around `emit_entries_from_info_inner`, so every caller is counted. The count includes the binary's `(devel)` main module, which the document later leaves out.
- [X] T007 In `DD`, at S1 and S2, emit `kind_merge` (data-model) when either side's evidence kind is `dynamic-linkage` or `embedded-source-path`. Do not change `best`.
  - *As built*: `census_kind_merge` beside `census_binary_merge` at S1 and S2.
- [X] T008 Write K1–K7 in `IT` with their expected results (wire.md):
  - K1, K2 and K6 are red on this commit; K3, K4, K5 and K7 pass.
  - Each case also asserts its census:
    - K1 and K2: one `kind_technique_change` with `new: binary-analysis`;
    - K3, K4 and K5: no `kind_technique_change`;
    - K5: one `binary_derived_kind` `go-buildinfo` per BuildInfo entry, and none labelled `go-buildinfo` from `detected_go`;
    - K6: one `kind_merge`.
  - **K6 feasibility**: first establish which pair can merge at all. A `dynamic-linkage` PURL carries no version, while a pkg-config record's does. Use the first pair that gives equal PURLs. If none does, record that in `MS/control.md` and make K6 a unit test in `DD` on constructed components (wire.md, K6 note).
  - Run the cases with this commit's binary and with T001's. Record the red/green table in `MS/control.md`, and confirm that the documents are identical to `main`'s for every case (UUIDs masked).
  - *As built*: **K6 has no record merge.** These kinds' PURLs carry no version, and a record's always does. Two imports of one library are joined by the linkage aggregator before deduplication (K6b, no `kind_merge`). So K6 is an integration check that the joined component lists one method (K6b), plus T014's unit test. Red: K1, K2, K6. The control matches `main` on all 7 cases (`MS/control.md`).
- [X] T009 Mark K1, K2 and K6 `#[ignore = "spec 1648: red until T012"]`. Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, then check `GATE_RESULT=pass`, the expected line and the per-target totals. Commit the log-only change and push.
  - *As built*: the gate passed: 431 suites, 11,068 passed, 0 failed, 52 ignored (48 before, plus K1, K2, K6 and the control).
- [X] T010 Census (FR-001, R5).

  In `FR`:
  - extend `census/versions.py` to parse `kind_merge` and to summarise `kind_technique_change` by kind, `binary_derived_kind` by kind (with `detected-go` and `go-buildinfo` side by side), and `kind_merge` with both sides;
  - add to the `control` job: compile a dynamically linked C program (`gcc`, no `-static`) and a minimal Go program (`go build`), scan them, and assert:
    - log-only: the C program's `dynamic-linkage` components have `kind_technique_change` events, and the Go binary has `go-buildinfo` events;
    - wired: those `dynamic-linkage` components are `binary-analysis` in the document;
    - the assertion accepts both states.

  Then:
  - dispatch `census-versions.yml` with `--ref main -f waybill_ref=<T009's full SHA>`, and verify every `refs.txt`;
  - dispatch `public-corpus.yml` with `-f branch=1648-binary-kind-technique`, and confirm 166 of 166 at T009's commit.

  Record in `MS/census.md`, per target:
  - components whose technique would change, by kind;
  - `kind_merge` events;
  - `detected-go` against `go-buildinfo`, beside spec 1646's figures;
  - every zero stated plainly.

  Also confirm that spec 1646's and 1647's controls in the same job still pass.
  - *As built*: census run 45240732755 (20 of 20 jobs, every `refs.txt` at `56d12eea`) and public corpus run 45240738665 (166 of 166). The runner's real builds fired both counters. TheRock: `dynamic-linkage` 49 and `embedded-source-path` 645 events / 15 PURLs; pants-backend-ai 8; cpp-mongo 3; `kind_merge` 0 everywhere. Go: `detected-go` 38 and 1 against `go-buildinfo` 0 and 0 (`MS/census.md`).

- [X] T011 **Checkpoint.** If a count contradicts FR-002 to FR-005, amend the spec first. Expected, as a prediction from spec 1646's census:
  - TheRock: about 49 `dynamic-linkage` and 15 `embedded-source-path` PURLs;
  - pants-backend-ai 8 and cpp-mongo 3 `dynamic-linkage`;
  - few or no `kind_merge` events.

  Record whether `detected-go` and `go-buildinfo` differ, and by how much.
  - *As built*: no contradiction and no amendment. Every count matches spec 1646's census, and FR-005's correction is confirmed.

## Phase 3: User Story 1 — a library inferred from a binary's structure says so (P1) 🎯 MVP

**Goal**: `dynamic-linkage` and `embedded-source-path` report `binary-analysis`.
**Independent test**: K1 and K2 pass; K7 is unchanged.

- [X] T012 [US1] Wire:
  - `SM::entry_technique` and `NC` use `binary_kind_technique(kind).unwrap_or(PackageDatabase)`;
  - `DD::effective_technique` and `is_binary_side` use `binary_kind_technique(kind) == Some(BinaryAnalysis)`;
  - remove `DD::BINARY_KINDS`, the `kind_technique_change` event and the `kind_merge` event (data-model).

  Then remove the `#[ignore]` from K1, K2 and K6, update their census assertions to the wired state, and update unit tests that referred to `BINARY_KINDS`.
  - *As built*: `entry_technique` reads the decision, so `NC`, which calls it, follows without an edit. The log-only test `log_only_census_reports_what_will_change` became `wired_census_has_no_log_only_events`, which also checks that `binary_derived_kind` reports `binary-analysis` for K1, K2 and K6b. The related suites (specs 1643–1647) pass unchanged.

## Phase 4: User Story 2 — embedded manifests keep manifest analysis (P1)

**Goal**: cargo-auditable, ELF package notes and Go BuildInfo are unchanged.
**Independent test**: K3, K4 and K5 are byte-identical to `main` at the wired commit.

- [X] T013 [US2] Run `control_byte_identical_to_main` at the wired commit with `WAYBILL_BINARY_KIND_IDENTICAL=K3,K4,K5,K7`. It must pass. Also run it for K1 and confirm that it fails, which shows the control can detect a difference. Record both runs in `MS/control.md`.
  - *As built*: the control passes for K3, K4, K5 and K7 against `main`'s binary, and fails for K1 (`K1: cdx differs`), so it can detect a difference.

## Phase 5: User Story 3 — a merge keeps the binary method visible (P2)

**Goal**: a merge with a newly binary-analysis kind lists both methods.
**Independent test**: K6 passes, as an integration case or as the unit test T008 decided on.

- [X] T014 [US3] K6 asserts CycloneDX methods `manifest-analysis` then `binary-analysis`, and the SPDX D1 `methods` array in both formats. If K6 is a unit test (T008), it asserts `absorb_methods` returns one `BinaryAnalysis` method for a record merged with a `dynamic-linkage` component.
  - *As built*: `s1648_a_record_absorbs_the_newly_binary_kinds` in `DD`. It covers both kinds as loser and as winner, and the four unchanged kinds absorbing nothing. K6b is the integration half: a joined import lists one method.

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: every difference is traced.
**Independent test**: the public corpus, the quality corpus and TheRock, each against a `main` control.

- [X] T015 [US4] Mutations M1–M5 (R7). Copy spec 1647's `measurements/mutate.py` to `MS/mutate.py` and replace its table. Record the killing tests in `MS/mutations.md`. Every mutation must be killed. Afterwards, `diff -r` the source against a copy taken before the run, and rebuild.
  - *As built*: 5 of 5 killed (`MS/mutations.md`). M4 is killed by the unit test only, because no tree reaches the merge.

- [X] T016 [US4] Public corpus at the final code commit (`-f branch=1648-binary-kind-technique`), with the scanned commit confirmed. Expected: no drift (spec Background). If there is drift, trace each line to a component of a changed kind, regenerate with `regen_goldens=true`, `cmp` the regenerated files with the traced output, commit them, and confirm 166 of 166. Record in `MS/corpus.md`.
  - *As built*: no drift. Run 45245331467 scanned `bc7d589c` and passed 166 of 166 (`MS/corpus.md`).

- [X] T017 [US4] Quality corpus (`census-versions.yml`) and TheRock (`measure-full`, base `33c445f6` against the final code commit), at full SHAs, with every `refs.txt` verified. Record in `MS/therock.md`:
  - every component that changed, traced to its kind;
  - with UUIDs, timestamps and those components' technique masked, 0 differing lines;
  - `waybill sbom parity-check` on both arms, with the D1 row traced;
  - SC-006's timing: one warm-up per arm, 3 interleaved runs, min/median/max and the median ratio;
  - the change in spec 1646's `binary_merge` debug-event count. Every dedup of one soname across binaries now has a binary side, so a large rise is expected. It is log volume, not output, and is stated as such (analyze L2).
  - *As built*: TheRock: 694 components change technique only (49 `dynamic-linkage`; 645 `embedded-source-path`, 15 PURLs), with 0 other differences, and every format at 0 lines with technique masked. parity-check gives 260 of 261 on both arms (A5), with D1 identical. Quality corpus: pants-backend-ai 8 and cpp-mongo 3, nothing else. `binary_merge` unchanged at 23, 2 and 0. Median ratio 1.000 (`MS/therock.md`).

## Phase 7: Polish

- [X] T018 [P] Docs:
  - `docs/reference/binary-identity.md`: which evidence kinds report `binary-analysis` and which keep `manifest-analysis`, with the CycloneDX definitions' reasoning in one sentence each;
  - `docs/reference/sbom-format-mapping.md` D1 row, if it names the kinds that report `binary-analysis`.
  - *As built*: the D1 row names the kinds, and `binary-identity.md` gains a per-kind table with the definitions.

- [X] T019 [P] `CHANGELOG.md`: a Fixed entry citing #1668, built from T010, T016 and T017.
  - *As built*: a Fixed entry above #1664's, from T010, T016 and T017.

- [ ] T020 Run the full gate on the final commit, and record in `MS/gate-log.md` the per-target totals, `GATE_RESULT`, the expected line, and that `git diff 33c445f6 -- Cargo.lock '*Cargo.toml'` is empty.
- [ ] T021 Close-out. Check each task's claim against the committed tree and the measurements. Note every as-built difference inline, then tick the task.
  - File a follow-up issue for the two path-identified collapse kinds (`jdk-runtime-collapsed`, `python-stdlib-collapsed`), with T010's counts if any, and the question of CycloneDX's `filename` technique. `ResolutionTechnique::FilePathPattern` already emits `filename` (`generate/cyclonedx/evidence.rs:64`), so no new variant would be needed.

## Dependencies

- **Setup**: T001 → T008. T002 and T003 → T008.
- **Foundational**: T004 → T005 → T007 → T008 → T009 → T010 → T011. T006 is independent of T005 and T007.
- **US1**: T012 needs T011.
- **US2 and US3**: T013 and T014 need T012.
- **US4**: T015 needs T012–T014. T016 and T017 need every code change.
- **Polish**: T018 after T012. T019 after T016 and T017. T020 and T021 last.

## Parallel opportunities

- T002 (`IL`) with T003 (`IT`).
- T006 (`GB`) alongside T005 and T007.
- T018 with T019.
- The CI dispatches (T010, T016, T017) run while local work continues.

## Implementation strategy

- **MVP**: Phases 1–3: the census, then the two kinds wired.
- **Expected scale**: about 64 components on TheRock change technique, and nothing else changes there. If T010 finds merges, US3's case carries the evidence.
