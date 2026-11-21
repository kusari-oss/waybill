# Tasks: Record which readers found a merged component (C56)

**Input**: `specs/1639-also-detected-via/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-002, SC-003) and the plan's Testing section. Each story's tests are written first and must fail on the log-only commit (after T004) before its implementation, except the guard cases named as such.

**Paths**:
- `AD` = `waybill-cli/src/scan_fs/also_detected.rs` (new)
- `DD` = `waybill-cli/src/scan_fs/dedup.rs` (removed in T022)
- `DU` = `waybill-cli/src/resolve/deduplicator.rs`
- `RC` = `waybill-cli/src/resolve/reconciler.rs`
- `CM` = `waybill-cli/src/scan_fs/package_db/cmake.rs`
- `FE` = `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs`
- `SP` = `waybill-cli/src/scan_fs/cmake_search_pair.rs`
- `EV` = `waybill-cli/src/generate/cyclonedx/evidence.rs` (callers in `builder.rs` and `metadata.rs`)
- `BE` = `waybill-cli/src/scan_fs/binary/entry.rs`; `SF` = `…/binary/symbol_fingerprint.rs`; `FM` = `…/binary/fingerprints/matcher.rs`
- `PX` = `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`
- `IT` = `waybill-cli/tests/also_detected_via.rs` (new)
- `FX` = `waybill-cli/tests/fixtures/also_detected_via/` (new)
- `MAP` = `docs/reference/sbom-format-mapping.md`
- `MS` = `specs/1639-also-detected-via/measurements/`
- Sites S1–S9 are as defined in research R3.

## Phase 1: Setup

- [X] T001 Create `IT` from the existing helpers:
  - `common::cmake_fetch` (scan in three formats with `--root-name`/`--root-version`, `prop`);
  - `common::git_fixture::GitFixture` (`add_submodule`, `git`);
  - `common::installed_lib`;
  - `common::cdx_schema::cdx_validation_errors`.

  Add three helpers:
  - `parity(dir)`: compares C55, C56 and C242 through `waybill::parity::extractors::EXTRACTORS` across the three formats, validates the CycloneDX document (no errors), and asserts no repeated `bom-ref`;
  - `assert_repeatable(dir)`: 10 scans, byte-identical after the usual serial/timestamp masking;
  - `c56(doc, purl)`: reads C56 from the CDX property.

  Copy the opkg-DB-plus-image-manifest fixture builder from `waybill-cli/tests/cross_reader_dedup_ecosystem_107.rs` into a helper.
- [X] T002 A-ctl in `IT` (FR-012).
  - **Tree.** `FX/ctl/` holds one merge of two members with the same mechanism, plus unrelated single-reader components: a resolved CMake fetch, a submodule, and a `.pc` file.
  - **Golden.** Generate `FX/ctl-golden/{cdx,spdx-2.3,spdx-3}.json` with **`main`'s** binary, before any source change, using the same masking as `cmake_unresolved_github_fetch.rs`'s U-ctl. Regeneration env var: `WAYBILL_UPDATE_ALSO_DETECTED_GOLDENS`.

**Checkpoint**: A-ctl passes at `main`.

## Phase 2: Foundational (log-only helper, census, extractors)

- [ ] T003 Create `AD` (research R4) and declare it in `waybill-cli/src/scan_fs/mod.rs`. It has three functions over `&mut BTreeMap<String, serde_json::Value>`:
  - `observe(site: &'static str, subject: &str, survivor, absorbed)`, where `subject` is the survivor's PURL:
    - emits one `tracing::debug!(target: "waybill::also_detected", site, subject, kept = <survivor C55 or "-">, absorbed = <absorbed C55 or "-">)`;
    - at site `S1` it also logs `rank_winner`: the member `DD::precedence_rank` would have picked. Map each C55 string to `SourceMechanism` by comparing against `canonical_str()` for every variant; a string with no variant is `unranked`.
  - `record(site, subject, survivor, absorbed)`: calls `observe`, then adds the absorbed C55 and every value of the absorbed C56 to the survivor's C56 set, then calls `normalise`.
  - `normalise(survivor)`:
    - removes the survivor's C55 from the set;
    - removes the key when the set is empty or the survivor has no C55;
    - otherwise writes a sorted, deduplicated JSON array of strings.

  Name the key constant `ANN_ALSO_DETECTED_VIA`. Unit tests in `AD`'s test module (with `#[cfg_attr(test, allow(clippy::unwrap_used))]`):
  - union;
  - self-exclusion;
  - the anchor rule;
  - empty means absent;
  - sort and dedup;
  - transitivity (an absorbed C56 joins);
  - a member without C55 contributes nothing.
- [ ] T004 Call `AD::observe` (**not** `record`) at each site, at the point where the site already removes the absorbed member, with the survivor and absorbed annotation maps:
  - S1 in `DU::deduplicate`'s loser loop;
  - S2 in `DU::fold_declared_not_cached`;
  - S3 in `RC::reconcile_design_source_tiers`, per design match;
  - S4 `FE::first_declaration_wins`;
  - S5 `FE::one_per_purl`;
  - S6 `FE::merge_into_existing`;
  - S7 `CM::fold_vendored_into_projects`;
  - S8 `SP::pair`'s project claim;
  - S9 `SP::pair`'s search pairing, per target; not for `in-tree` removals.

  Then confirm output is unchanged:
  - A-ctl and the existing suites pass;
  - `public-corpus.yml` with `regen_goldens=true` on the branch reports 0 differing files. Enumerate every job's conclusion.

  Record the run ID in `MS/census.md`.
- [ ] T005 **Census at the log-only commit** (SC-001, research R6).
  - **Workflow.** Add `census/merges.py` (stdlib only, run as `python3 -I -B`) and `.github/workflows/census-merges.yml` to `kusari-sandbox/waybill-fixture-therock-sdk`, modelled on `census-unresolved.yml`. Its matrix is TheRock (top-level submodules, measure-source's depth) and the three quality-corpus targets at their `xtask/corpus/quality-corpus.toml` pins. Each job:
    - builds waybill at `waybill_ref`;
    - scans with `RUST_LOG=waybill::also_detected=debug`;
    - writes `refs.txt` and uploads `merges.json` and the CycloneDX output, which T029 traces.
  - **What the script tallies:**
    - events by site;
    - events by (kept, absorbed) pair, where kept ≠ absorbed and kept ≠ `-`;
    - merge groups, keyed by (site, subject);
    - S1 groups in which any event's `rank_winner` is the absorbed member, so the ranking would have chosen a different survivor, plus the `unranked` count;
    - occurrences of the old fingerprint-collision key (`waybill:also-detected-via` with library-name values) in the output.
  - **Dispatch** with `--ref main -f waybill_ref=<full branch sha at T004>`.
  - **Record** in `MS/census.md`: the run ID, the fixture commit, the scanned SHAs read from `refs.txt`, and the per-target, per-site and per-pair tables.

  No prediction is written before the run.
- [ ] T006 Re-check research R3's list of sites that are not wired against today's C55 setters: `grep` for `"waybill:source-mechanism"` and for every `SOURCE_MECHANISM`/mechanism constant, without `head`. Record in `MS/census.md` either "no setter reaches a non-wired site", or the site that needs adding, and add it to T004 if one is found.
- [ ] T007 Replace the C56 extractors in `PX` with `cdx_anno!(c56_cdx, "waybill:also-detected-via", component)`, `spdx23_anno!(…)` and `spdx3_anno!(…)` (research R8). Keep the `C56` registration in `mod.rs` `SymmetricEqual`. Confirm the lib-target parity gate passes.

**Checkpoint**: census recorded; output still byte-identical to `main`.

## Phase 3: User Story 1: a merged component says which other readers found it (P1) 🎯 MVP

**Goal**: FR-001–FR-004. **Independent test**: A-1..A-10 in `IT`.

- [ ] T008 [P] [US1] Integration cases A-1, A-2, A-3, A-4, A-6, A-7, A-9 and A-10 in `IT`, as `contracts/wire.md` specifies them. Each calls `parity` and `assert_repeatable`. Every C56 assertion must fail on the log-only commit; record the failures in the commit message.
- [ ] T009 [P] [US1] Unit test in `DU`'s test module: two same-PURL members with different C55 merge, giving C56 with the loser's mechanism. A survivor without C55 takes the first loser's C55 (the existing fold), and C56 holds only the second loser's mechanism.
- [ ] T010 [US1] S1 and S2 in `DU`:
  - replace `observe` with `record`;
  - in S1, copy the loser's C55 and C56 before the fold loop, which moves the loser's map, and call `record` with the copy **after** the fold;
  - make the generic first-wins fold skip `ANN_ALSO_DETECTED_VIA`, beside the existing pants-key special cases, with a comment citing research R4.
- [ ] T011 [P] [US1] S3 in `RC`: replace `observe` with `record`. A source-tier survivor without C55 records nothing, by `normalise`'s anchor rule (A-8, research R5).
- [ ] T012 [P] [US1] S4–S7 in `FE` and `CM`: replace `observe` with `record`. In S6, keep the existing `superseded` call: C233 and C56 are both recorded (FR-004).
- [ ] T013 [P] [US1] S8 and S9 in `SP`: replace `observe` with `record`. Package searches are included (FR-004).
- [ ] T014 [US1] Run T008, T009, `AD`'s unit tests, A-ctl and the existing suites until they pass:
  - `cross_reader_dedup_ecosystem_107`, `dedup_precedence_determinism`;
  - the CMake suites (`cmake_*`), `git_submodule_reader`;
  - the installed-library suites;
  - the reconciler unit tests.

  An existing test whose expectation changes because C56 now appears is updated with the reason in the commit message; none may be loosened otherwise.

## Phase 4: User Story 2: the CycloneDX document stays valid (P1)

**Goal**: FR-005, FR-006, FR-011. **Independent test**: every A-case validates against the vendored 1.6 schema, and parity agrees on C56.

- [ ] T015 [P] [US2] In `IT`, assert for every A-case that no `evidence.identity[].methods[]` object has a key other than `technique`, `confidence` and `value`. `parity` already validates the whole document.
- [ ] T016 [US2] In `EV`:
  - remove `build_evidence`'s `winning_source_mechanism` and `losing_source_mechanisms` parameters and the `waybill-source-mechanism` key they wrote;
  - update the calls in `builder.rs` and `metadata.rs`;
  - delete the unit tests that assert that key (the `Some("conan-recipe")`, `losers` and `["git-submodule"]` cases);
  - add one unit test: a method object has exactly the schema's keys.
- [ ] T017 [US2] Rewrite `MAP`'s C56 row with the wording in `contracts/wire.md` (CDX property; the R1 audit in the justification column; the merge sites; the anchor rule; C242 is separate). Add the removal note to C207 and the C233 clause. Confirm the lib-target parity gate passes.
- [ ] T018 [US2] Run T015, T016's test, all of `IT`, and `cdx_schema`-using suites (`grep -l cdx_validation_errors waybill-cli/tests`) until they pass.

## Phase 5: User Story 3: one key, one meaning (P2)

**Goal**: FR-007. **Independent test**: F-1 and F-2.

- [ ] T019 [P] [US3] F-1 in `BE`'s test module: a `SymbolFingerprintMatch` with a non-empty collision list maps to an entry carrying `waybill:fingerprint-also-matched` (a sorted array) and no `waybill:also-detected-via`.
- [ ] T020 [P] [US3] F-2: find the existing lib-level test that emits one hand-built `ResolvedComponent` in all three formats and runs the parity extractors, using `grep` across `waybill-cli/src` for a test calling the three generators. Add a case where the component carries C242 and the C242 extractors agree. If no such test exists, add one in `waybill-cli/tests/` using the crate's public generate API, and say so in the commit message.
- [ ] T021 [US3] Make the C242 change:
  - in `BE`, write `waybill:fingerprint-also-matched` instead of `waybill:also-detected-via`;
  - rename `SymbolFingerprintMatch::also_detected_via` in `SF` and `MatchResult::also_detected_via` in `FM` to `also_matched`, with their tests;
  - add row C242 to `MAP`, with its Principle V audit from `contracts/wire.md`, and register `c242_*` in `PX` with the three macros, `SymmetricEqual`;
  - update C58's co-emission sentence.

  Run T019, T020 and the binary-scan suites until they pass.

## Phase 6: User Story 4: nothing else changes (P2)

**Goal**: FR-010, FR-012, FR-013. **Independent test**: A-5, A-8 and A-ctl.

- [ ] T022 [US4] Integration cases A-5 (two members with the same mechanism: no C56) and A-8 (design-tier with C55 reconciled into a C55-less survivor: no C56) in `IT`, both guards that pass at `main`. Re-run A-ctl and A-rep across every A-case. Add two edge-case assertions: A-1 scanned with `--split` keeps C56 on its component in the split document, and a main-module survivor carries C56 (spec Edge Cases). Then remove `DD` and its `mod` declaration (FR-008). T005 has already used `precedence_rank`, so remove `observe`'s `rank_winner` field in the same change. Confirm `grep -rn "scan_fs::dedup\|SourceMechanism\|precedence_rank" waybill-cli/src` counts 0, piped to `wc -l`, not `head`.

## Phase 7: Polish and cross-cutting

- [ ] T023 [P] FR-009: correct the Yocto comments in `waybill-cli/src/scan_fs/package_db/mod.rs`. Correct the headers of `waybill-cli/tests/dedup_precedence_determinism.rs` and `waybill-cli/tests/cross_reader_dedup_ecosystem_107.rs`, and `waybill-cli/tests/fixtures/golden_inputs/dedup_collision/README.md`. Extend `cross_reader_dedup_ecosystem_107.rs` to assert C56 `["yocto-image-manifest"]`.
- [ ] T024 [P] Docs:
  - `docs/reference/identifiers.md` §11.5 renames the collision to C242, with its `jq` recipe updated;
  - `docs/reference/reading-a-mikebom-sbom.md`'s C56 row;
  - `docs/ecosystems.md`'s C56 sentence near the Yocto section.
- [ ] T025 [P] `CHANGELOG.md` `## [Unreleased]`:
  - Fixed: C56 is emitted for merges across readers (#1558), naming the merge sites and the anchor rule;
  - Changed: the symbol-fingerprint collision moves from `waybill:also-detected-via` to `waybill:fingerprint-also-matched` (C242), a wire change with no alias;
  - Fixed: the CycloneDX `methods[]` key milestone 105 specified is no longer produced (#1442).
- [ ] T026 Mutations M1–M8 (plan Testing). Restore with `cp`, rebuild after the last one, and use a `^error\[E…\]` guard so a compile failure is not counted as a kill. Record each mutation and its killing test in `MS/mutations.md`.
- [ ] T027 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`. Read `target/gate/prepr.log` for `GATE_RESULT=` and every `test result:` line, and record the counts in `MS/gate-log.md`.
- [ ] T028 TheRock `measure-source` (SC-005, SC-006), with `--ref main -f waybill_ref=<branch full sha> -f base_ref=<main full sha>`. Read the scanned SHAs from `refs.txt`. Trace every changed component to an FR, and confirm component counts are equal and unexplained orphans do not increase. Record the median of 3 timed runs per side and the ratio in `MS/therock.md`.
- [ ] T029 Corpora:
  - **Public corpus:** `public-corpus.yml` `regen_goldens=true` (SC-004). Trace every changed golden to an FR, and commit them through the workflow's artifact.
  - **Quality corpus:** branch and `main` runs (SC-005), traced component by component with `census-merges` at both SHAs.

  Enumerate every job's conclusion. Record in `MS/corpus.md`.
- [ ] T030 Verify each task's claim against the code and measurements before ticking it. Confirm `git diff --quiet origin/main -- Cargo.lock '*/Cargo.toml' Cargo.toml` (FR-014). Then open the PR with "Closes #1558" and "Refs #1515, #1442", ending with the attribution footer. Merging is the maintainer's.

## Dependencies

- **Setup:** T001 → T002. T002's golden is made before T003.
- **Foundational:** T003 → T004 → T005 (CI, needs T004 pushed). T006 can run with T005. T007 is independent of T003–T006.
- **US1:** needs T003, T004 and T007. T008 and T009 are written first. T010 (after T009, same file), T011, T012 and T013 touch different files and can land in any order → T014.
- **US2:** T016 after T014; T015 any time after T001; T017 after T016; T018 last.
- **US3:** independent of US1 and US2, after T001. T019 and T020 → T021.
- **US4:** T022 after T014, T018 and T021, and after T005 (it removes what T005 used).
- **Polish:**
  - T023–T025 in parallel after T022;
  - T026 after T022;
  - T027 after T023–T026;
  - T028 and T029 after T027;
  - T030 last.

## Parallel examples

- **US1:** T008 (`IT`) and T009 (`DU` tests) together. Then T010 (`DU`), T011 (`RC`), T012 (`FE`/`CM`) and T013 (`SP`) together.
- **US3:** T019 (`BE`) and T020 (lib test) together, and the whole story alongside US1.
- **Polish:** T023, T024 and T025 touch different files.

## Implementation strategy

- **MVP:** Phases 1–3. C56 appears on every cross-reader merge, the census is recorded, and output is otherwise unchanged.
- **Next:** US2, which removes the schema-invalid key and rewrites the row, then US3, the C242 split.
- **Then:** US4's guards and the module removal, then polish. One PR for the whole spec.
