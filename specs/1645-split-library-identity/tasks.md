# Tasks: one library, one component, when the curated scanner and an installed-library record name or version it differently (#1662)

**Input**: `specs/1645-split-library-identity/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-003 to SC-006).
- **Written red.** Every case in `contracts/wire.md` that expects a change (M-1, M-2, M-3, E-1, and B-1's new golden) is written before the change that produces it, and must fail on the log-only commit.
- **Guards.** The N cases expect no merge. They pass on `main` trivially. Their teeth come from the mutations (T016).

**Paths**:
- `VS` = `waybill-cli/src/scan_fs/binary/version_strings.rs`
- `LI` = `waybill-cli/src/scan_fs/library_identity.rs` (new)
- `SM` = `waybill-cli/src/scan_fs/mod.rs` (the installed-library splice, ~line 696)
- `CP` = `waybill-cli/src/generate/cpe.rs`
- `IT` = `waybill-cli/tests/library_identity.rs` (new)
- `IL` = `waybill-cli/tests/common/installed_lib.rs`
- `VB` = `waybill-cli/tests/version_string_boundary.rs`, and its B-1 golden at `waybill-cli/tests/fixtures/version_string_boundary/b1-golden/`
- `MS` = `specs/1645-split-library-identity/measurements/`
- `FR` = the TheRock fixture repository, `kusari-sandbox/waybill-fixture-therock-sdk`
- Research decisions are R1–R10. Cases are `contracts/wire.md`'s. Mutations are R9's M1–M8.

**Story order.** US2 (LLVM) is implemented before US1 because US1's M-2 case depends on it. Both are P1.

## Phase 1: Setup

- [ ] T001 Build `main`'s binary at `bb9d6cc4` into the scratchpad as `waybill-main-bb9d6cc4`, and record the full SHA in `MS/control.md`. It is the control for every "equal to `main`" claim.
- [ ] T002 Create `IT` with `mod common;` and these helpers:
  - `lib_tree(files: &[(&str, Vec<u8>)]) -> TempDir`, writing each relative path;
  - `pc(name, version, extra_lines) -> Vec<u8>`;
  - `cmake_version(package, version) -> Vec<u8>`;
  - `scan3(dir)`, which honours `WAYBILL_LIBRARY_IDENTITY_BIN` so a case can run against T001's binary;
  - `scan_with_log(dir, "waybill::version_census=debug") -> (docs, stderr)`, which also runs `parity` and checks that `bom-ref`s are unique;
  - `components(doc, name_regex)`, searched recursively;
  - `events(stderr, "shared_file")`, matching ` shared_file=` with its leading space (spec 1644's `unanchored=` lesson).

  Reuse `IL::elf_shared_lib` for the library files. Copy the rest from `tests/version_string_anchored.rs`; do not import a test file.

## Phase 2: Foundational (log-only census)

Output does not change in this phase.

- [ ] T003 In `VS`, add `CuratedLibrary::names(self) -> &'static [&'static str]` and `CuratedLibrary::from_slug(&str) -> Option<Self>` (R3).
  - Both are exhaustive matches with no wildcard. `Ncurses` has `["ncurses", "ncursesw", "ncursest", "ncursestw"]`; every other variant has `[slug]`.
  - Add a doc comment on `Ncurses`'s arm quoting ncurses `INSTALL` v1.267 (research R3), with its URL.
  - Unit tests: for every variant (`strum::EnumIter`), `from_slug(v.slug()) == Some(v)` and `names()[0] == slug()`; and `from_slug("ncursesw") == None`, since names are not slugs.
- [ ] T004 In `VS`, add `parse_llvm_version(tail)` (R4) and a version of `parse_ncurses_version` that keeps the patch date (R5), **without** switching the scanner to either yet.
  - Both scanner sites keep calling the old parsers. When the new parser would give a different version, they record it for the census.
  - Unit tests for both parsers, with `contracts/wire.md`'s cases: `17.0.6\0`, `23.0.0git\0`, `18.1.0rc1 (`, `23.0.0` at the region's end, and `23.0.0-rc1` rejected; ncurses keeps the date and keeps its existing rejections.
- [ ] T005 Create `LI` with `pub(crate) fn reconcile(entries: &mut [PackageDbEntry], log_only: bool)`, and wire it into `SM` right after `db_entries.splice(binary_start..binary_start, resolved.entries)`.
  - Implement data-model's verdict for each curated entry (`evidence_kind == "embedded-version-string"`, library from `from_slug(name)`) and each file it was read from (`source_path`). The candidates are the record entries (`waybill:source-mechanism` is `pkg-config` or `cmake-package-version`, the only two the installed-library reader emits) whose `extra_source_paths` contain the file and whose PURL differs.
  - Emit the `shared_file` census event (`contracts/wire.md`) inside `tracing::enabled!(target: "waybill::version_census", DEBUG)`.
  - With `log_only = true`, rewrite nothing.
  - Unit tests in `LI` on constructed entries, one per verdict: `merge`, `version-disagrees`, `name-unrelated` and `ambiguous` (N-4's unit form); the dotted-prefix agreement; the `depends` remap (R2), which is computed even when `log_only`.
- [ ] T006 In `CP`, add `format_cpe_with_update(vendor, product, version, update)` and the ncurses split (R6), computed but not used.
  - When the split would change a component's CPEs, emit `cpe_form_change` inside the census guard.
  - Unit tests: `ncurses@6.4.20221231` gives version `6.4` and update `20221231` for all three vendors; `ncurses@6.6` is unchanged; `zlib@1.3.2` is unchanged.
- [ ] T007 In `VS`, emit `curated_version_change` (`contracts/wire.md`) where T004 recorded a difference, inside the existing census guard.
- [ ] T008 Write every case of `contracts/wire.md` in `IT`, with its expected result:
  - M-1, M-2, M-3 and E-1 are red on this commit;
  - N-1, N-2, N-3, C-1, C-2 and P-1 pass;
  - the integration form of N-4 is attempted. Record whether the readers attach both records to the file; if not, `MS/control.md` says the state is reachable only in the unit test.

  C-1 must see exactly one `shared_file` event with verdict `merge`, and the log-only documents must still hold two components. That is the positive control: the census fires on the case it is meant to count.

  Record the red/green table in `MS/control.md`, run with both this commit's binary and T001's.
- [ ] T009 Run `IT` with the red cases unignored, and record in `MS/control.md` that exactly the planned cases fail, each by name. Mark them `#[ignore = "spec 1645: red until T0NN"]`, naming the task that turns each green. Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Commit the log-only change.
- [ ] T010 Census (FR-001, R8). In `FR`, extend `census/versions.py` to parse `shared_file`, `curated_version_change` and `cpe_form_change`, and add the M-1 and M-2 trees to the `control` job as a positive control.
  - Dispatch `census-versions.yml` with `--ref main -f waybill_ref=<T009's full SHA>`, and verify every job's `refs.txt`.
  - Dispatch the public corpus with `-f branch=1645-split-library-identity`, confirm its summary names T009's commit and that it passes 166 of 166, and collect the census events from its log or artifacts.

  Record in `MS/census.md`, per target:
  - every `shared_file` event with its verdict;
  - every `curated_version_change`, old and new;
  - every `cpe_form_change`.

  **Checkpoint**: if any count contradicts FR-002 to FR-013, amend the spec with the evidence before Phase 3. The expected counts at the log-only commit are:
  - `shared_file`: TheRock's `libncursesw.so` is `merge` (`6.6` is a dotted prefix of `6.6.20251230`, FR-005); TheRock's two `libLLVM` files are `version-disagrees` (`23.0.0` against `23.0.0git` is not at a `.`) until FR-002;
  - `curated_version_change`: TheRock only, for 2 ncurses files and 3 LLVM files;
  - `cpe_form_change`: `nix-closure-openai`'s `ncurses@6.4.20221231` only. TheRock's curated ncurses is still `6.6`, and `ncursesw` has no CPE row, so no TheRock CPE changes until FR-013.

## Phase 3: User Story 2 — the curated LLVM version is the version the binary states (P1)

**Goal**: `LLVM version 23.0.0git` gives `llvm@23.0.0git`.
**Independent test**: M-2 passes; the LLVM unit tests pass.

- [ ] T011 [US2] In `VS`, switch the LLVM site to `parse_llvm_version`, and remove its `curated_version_change` emission. Remove M-2's `#[ignore]`. M-2 passes: one `llvm@23.0.0git`, through the existing PURL merge, with `reconcile` still log-only.

## Phase 4: User Story 1 — a library found by both its banner and its install record is one component (P1) 🎯 MVP

**Goal**: ncurses becomes one component; a qualifying name and an agreeing version merge.
**Independent test**: M-1, M-3 and E-1 pass; every N case still passes.

- [ ] T012 [US1] In `VS`, switch the ncurses site to keep the patch date (FR-013), and remove its `curated_version_change` emission. Update `VB`'s `NCURSES_V` to `pkg:generic/ncurses@6.6.20251230`, and the unit tests that pinned `6.6`.
- [ ] T013 [US1] After T012, in `CP`, switch the ncurses CPEs to the split form (FR-008), and remove `cpe_form_change`. Only now, with both T012 and T013 applied, regenerate B-1's golden with `WAYBILL_UPDATE_VERSION_BOUNDARY_GOLDENS=1`. Diff it against the old golden: the only changes are the ncurses component's version and CPE strings. Record the diff in `MS/control.md`.
- [ ] T014 [US1] In `LI`, set `log_only = false`: on `merge`, rewrite the PURL, name and `depends` (data-model "Rewrite"). Remove the `#[ignore]` from M-1, M-3 and E-1.
  - M-1 asserts C18 contains the library and `ncursesw.pc`, `waybill:source-mechanism` is `pkg-config` (R7), and the CPE candidates are `…:ncurses:6.6:20251230` for all three vendors. C56 and the curated evidence kind are not asserted: a curated-plus-record merge keeps neither on `main` (R1, #1665).
  - All N cases still pass.
- [ ] T015 [US1] `parity(dir)` for M-1, M-2, M-3 and N-1 (FR-010).
- [ ] T016 [US1] Mutations M1–M8 (R9). Copy spec 1644's `measurements/mutate.py` to `MS/mutate.py`, keeping its full-path test filter, its "no test result line → HARNESS ERROR" guard and its `cp` restore.
  - Write each mutation as an exact-string replacement, run it, and record the killing tests in `MS/mutations.md`.
  - Every mutation must be killed. A survivor means a missing test: add it and re-run.
  - Afterwards, confirm `git status --short waybill-cli` shows only intended changes, and rebuild.

## Phase 5: User Story 3 — nothing else changes (P2)

**Goal**: only FR-011's counted classes change.
**Independent test**: the public corpus, the quality corpus and TheRock, each traced against `main`.

- [ ] T017 [US3] Run the public corpus at the final code commit with `-f branch=1645-split-library-identity`, and confirm the summary's scanned commit. Expected drift: `nix-closure-openai`'s ncurses CPEs only.
  - Trace every changed line, as in spec 1644's `corpus.md`.
  - Regenerate with `regen_goldens=true`, and confirm the regenerated files are byte-identical to the traced output.
  - Commit them, and confirm with a run that passes 166 of 166.

  Record all of it in `MS/corpus.md`.
- [ ] T018 [US3] Run the quality corpus (`census-versions.yml`) and TheRock (`measure-full`, base `bb9d6cc4` against the final code commit) at full SHAs, and verify `refs.txt`. Record in `MS/therock.md`:
  - every component added, removed or changed, traced (expected: ncurses and llvm become one component each, and the edges follow);
  - that no PURL appears twice where it did not on `main`;
  - `waybill sbom parity-check` on both arms;
  - SC-007's timing: one warm-up per arm, 3 interleaved runs, the median ratio, and min/median/max per arm.

  A ratio above 1.05 is diagnosed by measurement, not re-run away.

## Phase 6: Polish

- [ ] T019 [P] In `docs/reference/binary-identity.md`, document:
  - the LLVM suffix and the ncurses patch date;
  - the merge rule (shared file, closed name list, dotted-prefix agreement, single record) and its census;
  - the ncurses CPE form.

  Update `VS`'s module doc table rows for LLVM and ncurses.
- [ ] T020 [P] `CHANGELOG.md`: a Fixed entry citing #1662. Its TheRock and corpus sentences are written from T017 and T018's measurements, not predicted.
- [ ] T021 Run the full gate on the final commit, and record in `MS/gate-log.md` the per-target `test result: ok. N passed; 0 failed` totals and the `>>> all pre-PR checks passed.` line. Name every failing test if any fail. Also record that `git diff bb9d6cc4 -- Cargo.lock '*Cargo.toml'` is empty (FR-012).
- [ ] T022 Close-out. For every task above, check that its claim is true of the committed tree and measurements, not merely that its box is ticked. Then tick it.

## Dependencies

- **Setup**: T001 → T008. T002 → T008.
- **Foundational**: T003 → T005 (`reconcile` uses `names`/`from_slug`). T004 → T007. T005, T006 and T007 → T008 → T009 → T010. T004 and T006 can run beside T003.
- **US2** (T011) needs T010's checkpoint.
- **US1**: T012 needs T010. T013 needs T012, because B-1 is regenerated only once both are applied. T014 needs T011 (for M-2's agreement), T012 and T013. T015 needs T014. T016 needs T011–T015.
- **US3** (T017, T018) needs every code change.
- **Polish**: T019 and T020 any time after T014. T021 and T022 last.

## Parallel opportunities

- T003, T004 and T006 touch different code: `VS`'s enum, `VS`'s parsers and `CP`.
- T013's CPE code in `CP` can be written while T012 is in progress. B-1's regeneration waits for both.
- T019 with T020.
- The census (T010) and the final runs (T017, T018) are CI dispatches. Local work continues while they run.

## Implementation strategy

The MVP is US2 plus US1 behind the census:
1. Setup, then the log-only foundational phase, with the census read and the spec amended if needed.
2. US2 (LLVM, one line of behaviour), then US1 (ncurses date, CPE form, the merge).
3. US3 traces every change against `main` before the PR.
