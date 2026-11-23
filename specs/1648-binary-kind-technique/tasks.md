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

- [ ] T001 Build `main`'s binary at `33c445f6` into the scratchpad as `waybill-main-33c445f6`. Before building, confirm `git diff 33c445f6 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.
- [ ] T002 [P] In `IL`, add the test-only `elf_with_needed(needed: &[&str], rodata: &[u8]) -> Vec<u8>` (R6): an ELF64 `ET_DYN` file with `.rodata`, a `.dynamic` section holding one `DT_NEEDED` per name, and a `.dynstr`.
  - Before writing it, check how `scan_fs/binary/linkage.rs` reads `DT_NEEDED` (through `object`'s dynamic-section accessors, or through program headers), and build exactly what that reader needs.
  - A sanity test in `IT` scans one such file and finds the `dynamic-linkage` component for each name. That test is the helper's positive control.
- [ ] T003 [P] Create `IT` with the harness from `waybill-cli/tests/version_string_static.rs`: `tree`, `scan_with(bin, dir)` returning the three documents' bytes, CycloneDX and the log, `cdx_components`, parity and unique `bom-ref`s.
  - Add `technique(cdx, purl) -> Vec<String>` (the CycloneDX methods' techniques) and `d1(doc, purl)` for SPDX 2.3 and SPDX 3.
  - Add `census(stderr, event) -> Vec<Value>`.
  - Add the control `control_byte_identical_to_main`. It is ignored unless `WAYBILL_BINARY_KIND_MAIN_BIN` is set, and masks version-4 UUIDs. `WAYBILL_FIXED_TIMESTAMP` already fixes timestamps. The case list comes from `WAYBILL_BINARY_KIND_IDENTICAL`.
  - Bring in the fixture builders R6 names: `elf_with_package_note`, the `.dep-v0` cargo-auditable fixture, the embedded-source-path tree, and a synthetic Go BuildInfo binary. Move a builder into `tests/common` only where `IT` needs it from another test file, and keep a comment naming where it came from.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [ ] T004 In `BM`, add `pub(crate) fn binary_kind_technique(kind: &str) -> Option<ResolutionTechnique>` (data-model).
  - It returns `BinaryAnalysis` for `embedded-version-string`, `symbol-fingerprint`, `dynamic-linkage` and `embedded-source-path`.
  - It returns `PackageDatabase` for `cargo-auditable`, `elf-note-package`, `jdk-runtime-collapsed` and `python-stdlib-collapsed`.
  - It returns `None` for anything else.
  - Unit tests `s1648_*`: every entry of `BINARY_EVIDENCE_KINDS` maps to `Some` (totality, FR-006); each kind maps to its decided technique; a non-binary kind (`requirements-txt`) maps to `None`.
  - Nothing calls it on this commit except the census (T005).
- [ ] T005 In `SM` and `NC`, at the conversion sites, emit the census `technique_change` event (spec 1646's shape: `evidence_kind`, `purl`, `old`, `new`) for each entry whose `binary_kind_technique` differs from the technique it is converted with. The technique given is unchanged.
  - In `census_entry_technique`, rename the `detected_go` label to `detected-go` (R3).
- [ ] T006 [P] In `GB::emit_entries_from_info`, emit a `binary_derived_kind` census event with `evidence_kind: "go-buildinfo"`, the entry's PURL, the binary's path and `technique: "package-database"`, for each entry it creates (R3, FR-005). Off unless `waybill::version_census=debug` is enabled.
- [ ] T007 In `DD`, at S1 and S2, emit `kind_merge` (data-model) when either side's evidence kind is `dynamic-linkage` or `embedded-source-path`. Do not change `best`.
- [ ] T008 Write K1–K7 in `IT` with their expected results (wire.md):
  - K1, K2 and K6 are red on this commit; K3, K4, K5 and K7 pass.
  - Each case also asserts its census:
    - K1 and K2: one `technique_change` with `new: binary-analysis`;
    - K3, K4 and K5: no `technique_change`;
    - K5: one `binary_derived_kind` `go-buildinfo` per BuildInfo entry, and none labelled `go-buildinfo` from `detected_go`;
    - K6: one `kind_merge`.
  - **K6 feasibility**: first establish which pair can merge at all. A `dynamic-linkage` PURL carries no version, while a pkg-config record's does. Use the first pair that gives equal PURLs. If none does, record that in `MS/control.md` and make K6 a unit test in `DD` on constructed components (wire.md, K6 note).
  - Run the cases with this commit's binary and with T001's. Record the red/green table in `MS/control.md`, and confirm that the documents are identical to `main`'s for every case (UUIDs masked).
- [ ] T009 Mark K1, K2 and K6 `#[ignore = "spec 1648: red until T012"]`. Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, then check `GATE_RESULT=pass`, the expected line and the per-target totals. Commit the log-only change and push.
- [ ] T010 Census (FR-001, R5).

  In `FR`:
  - extend `census/versions.py` to parse `kind_merge` and to summarise `technique_change` by kind, `binary_derived_kind` by kind (with `detected-go` and `go-buildinfo` side by side), and `kind_merge` with both sides;
  - add to the `control` job: compile a dynamically linked C program (`gcc`, no `-static`) and a minimal Go program (`go build`), scan them, and assert:
    - log-only: the C program's `dynamic-linkage` components have `technique_change` events, and the Go binary has `go-buildinfo` events;
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
- [ ] T011 **Checkpoint.** If a count contradicts FR-002 to FR-005, amend the spec first. Expected, as a prediction from spec 1646's census:
  - TheRock: about 49 `dynamic-linkage` and 15 `embedded-source-path` PURLs;
  - pants-backend-ai 8 and cpp-mongo 3 `dynamic-linkage`;
  - few or no `kind_merge` events.

  Record whether `detected-go` and `go-buildinfo` differ, and by how much.

## Phase 3: User Story 1 — a library inferred from a binary's structure says so (P1) 🎯 MVP

**Goal**: `dynamic-linkage` and `embedded-source-path` report `binary-analysis`.
**Independent test**: K1 and K2 pass; K7 is unchanged.

- [ ] T012 [US1] Wire:
  - `SM::entry_technique` and `NC` use `binary_kind_technique(kind).unwrap_or(PackageDatabase)`;
  - `DD::effective_technique` and `is_binary_side` use `binary_kind_technique(kind) == Some(BinaryAnalysis)`;
  - remove `DD::BINARY_KINDS`, the `technique_change` event and the `kind_merge` event (data-model).

  Then remove the `#[ignore]` from K1, K2 and K6, update their census assertions to the wired state, and update unit tests that referred to `BINARY_KINDS`.

## Phase 4: User Story 2 — embedded manifests keep manifest analysis (P1)

**Goal**: cargo-auditable, ELF package notes and Go BuildInfo are unchanged.
**Independent test**: K3, K4 and K5 are byte-identical to `main` at the wired commit.

- [ ] T013 [US2] Run `control_byte_identical_to_main` at the wired commit with `WAYBILL_BINARY_KIND_IDENTICAL=K3,K4,K5,K7`. It must pass. Also run it for K1 and confirm that it fails, which shows the control can detect a difference. Record both runs in `MS/control.md`.

## Phase 5: User Story 3 — a merge keeps the binary method visible (P2)

**Goal**: a merge with a newly binary-analysis kind lists both methods.
**Independent test**: K6 passes, as an integration case or as the unit test T008 decided on.

- [ ] T014 [US3] K6 asserts CycloneDX methods `manifest-analysis` then `binary-analysis`, and the SPDX D1 `methods` array in both formats. If K6 is a unit test (T008), it asserts `absorb_methods` returns one `BinaryAnalysis` method for a record merged with a `dynamic-linkage` component.

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: every difference is traced.
**Independent test**: the public corpus, the quality corpus and TheRock, each against a `main` control.

- [ ] T015 [US4] Mutations M1–M5 (R7). Copy spec 1647's `measurements/mutate.py` to `MS/mutate.py` and replace its table. Record the killing tests in `MS/mutations.md`. Every mutation must be killed. Afterwards, `diff -r` the source against a copy taken before the run, and rebuild.
- [ ] T016 [US4] Public corpus at the final code commit (`-f branch=1648-binary-kind-technique`), with the scanned commit confirmed. Expected: no drift (spec Background). If there is drift, trace each line to a component of a changed kind, regenerate with `regen_goldens=true`, `cmp` the regenerated files with the traced output, commit them, and confirm 166 of 166. Record in `MS/corpus.md`.
- [ ] T017 [US4] Quality corpus (`census-versions.yml`) and TheRock (`measure-full`, base `33c445f6` against the final code commit), at full SHAs, with every `refs.txt` verified. Record in `MS/therock.md`:
  - every component that changed, traced to its kind;
  - with UUIDs, timestamps and those components' technique masked, 0 differing lines;
  - `waybill sbom parity-check` on both arms, with the D1 row traced;
  - SC-006's timing: one warm-up per arm, 3 interleaved runs, min/median/max and the median ratio.

## Phase 7: Polish

- [ ] T018 [P] Docs:
  - `docs/reference/binary-identity.md`: which evidence kinds report `binary-analysis` and which keep `manifest-analysis`, with the CycloneDX definitions' reasoning in one sentence each;
  - `docs/reference/sbom-format-mapping.md` D1 row, if it names the kinds that report `binary-analysis`.
- [ ] T019 [P] `CHANGELOG.md`: a Fixed entry citing #1668, built from T010, T016 and T017.
- [ ] T020 Run the full gate on the final commit, and record in `MS/gate-log.md` the per-target totals, `GATE_RESULT`, the expected line, and that `git diff 33c445f6 -- Cargo.lock '*Cargo.toml'` is empty.
- [ ] T021 Close-out. Check each task's claim against the committed tree and the measurements. Note every as-built difference inline, then tick the task.
  - File a follow-up issue for the two path-identified collapse kinds (`jdk-runtime-collapsed`, `python-stdlib-collapsed`), with T010's counts if any, and the question of CycloneDX's `filename` technique.

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
