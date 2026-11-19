# Tasks: Label pip requirements files under docs and test directories by those directories

**Input**: `specs/1615-nested-req-scope/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested (spec SC-008, plan Testing). Each story's tests are written first and must fail before its implementation.

**Paths**:
- `RT` = `waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs`
- `IT` = `waybill-cli/tests/scan_nested_req_scope.rs`

## Phase 1: Setup

- [ ] T001 Confirm premises at `main` and record them in `specs/1615-nested-req-scope/measurements/trace.md`:
  - (a) `requirements-tests.txt` and `requirements-doc.txt` classify as `Main` today;
  - (b) `listed_ancestor_class`'s three callers are as research R3 lists;
  - (c) spec 1613's `ci_scope::apply` removes C155 from a runtime component.
  Use a throwaway unit test or the existing tests, not a script that re-parses paths.
- [ ] T002 Create `IT` with the sibling suite's helpers (`scan`, `named`, `present`, `scope`, `requirements`, `tree`, `assert_repeatable`, as in `waybill-cli/tests/scan_dot_directory_manifests.rs`), plus a `c155(&Docs, name)` helper. It reads C155 through `row_all_formats`, so each assertion covers all three formats.

## Phase 2: Foundational

- [ ] T003 Write the W-ctl byte-identity control in `IT`: a tree at `waybill-cli/tests/fixtures/nested_req_scope/ctl/` with `requirements.txt`, `requirements-dev.txt`, `docs/requirements.txt` and `.github/requirements.txt`. Generate its golden in `waybill-cli/tests/fixtures/nested_req_scope/ctl-golden/` from **`main`'s** binary, before any source change, and compare masked output (FR-009).

**Checkpoint**: the control passes at `main`.

## Phase 3: User Story 1 — a documentation site's requirements are not runtime (P1) 🎯 MVP

**Goal**: FR-002, FR-003, FR-004 and FR-007: the nearest `docs`/`test` ancestor below the scan root labels a file, in one search with the listed dot-directories.

**Independent Test**: W-1, W-2, W-7, W-8, W-10, W-12, W-13 and W-14 pass.

- [ ] T004 [P] [US1] Unit tests in `RT`'s test module for the ancestor search:
  - nearest wins across both kinds (`tests/docs/sphinx`, `docs/.github/x`, `.github/docs/x`);
  - the root's own name never counts;
  - a path outside the root gets the dot-directory search only (research R3);
  - partial names (`documents`, `testing_utils`, `test_data`) and FR-005's names label nothing;
  - the parent and the name still win (`tests/fixtures/requirements-docs.txt`).
- [ ] T005 [P] [US1] Integration cases in `IT`: W-1, W-2, W-7, W-8, W-10, W-12, W-13, W-14. Run them and confirm they fail at this point.
- [ ] T006 [US1] Implement the ancestor search in `RT`. Add `nearest_ancestor_scope(path, root) -> Option<&'static str>`:
  - It walks the path's ancestors relative to `root`, nearest first.
  - Each is matched case-insensitively against `project_roots::listed_dot_dir_class` and against `docs`/`doc`/`documentation` (`"docs"`) or `test`/`tests` (`"test"`).
  - When `path` is not under `root`, it falls back to `listed_ancestor_class(path, root)` unchanged.

  Call it from `classify_requirements_scope_under` in place of `listed_ancestor_class`. Leave `project_roots.rs` and `scan_fs/ci_scope.rs` unchanged (research R3).
- [ ] T007 [US1] Run T004, T005 and W-ctl until they pass. Also run `scan_dot_directory_manifests` and `cargo test -p waybill --bins requirements_txt::`: spec 1613's dot-directory behaviour must be unchanged.

**Checkpoint**: US1 complete.

## Phase 4: User Story 2 — a file named for documentation is labelled docs (P1)

**Goal**: FR-001, FR-006 and research R2: per-scope word sets in the name rule, and reading of the newly labelled names.

**Independent Test**: W-4, W-5, W-6 pass, and W-11 shows no unreachable component without an orphan reason.

- [ ] T008 [P] [US2] Extend `m670_scope_classify_filename_signals` in `RT`:
  - `requirements-doc.txt`, `doc-requirements.txt`, `requirements-development.txt`, `requirements-documentation.txt`, `requirements-tests.txt`, `testing-requirements.txt` and `requirements-testing-frozen.txt` are scoped;
  - `requirements-docker.txt`, `requirements-testinfra.txt`, `requirements-devel.txt` and `requirements_dev.txt` are not.

  Extend the `is_requirements_file` test table: `doc-requirements.txt`, `development-requirements.txt` and `testing-requirements.txt` are read; `docker-requirements.txt` is not.
- [ ] T009 [P] [US2] Integration cases W-4, W-5, W-6 and W-11 in `IT`. Confirm they fail.
- [ ] T010 [US2] Implement R1 in `RT`. Add one typed table, `SCOPE_WORDS: [(&str, &[&str]); 4]`, mapping each C155 value to its words. `classify_requirements_scope` and `is_requirements_file` both iterate it in place of the literal `["dev", "test", "docs", "ci"]` lists. `matches_scope_filename` is unchanged.
- [ ] T011 [US2] Run T008, T009, W-ctl and the whole `requirements_txt::` unit module until they pass. Re-run US1's cases.

**Checkpoint**: US1 and US2 complete.

## Phase 5: User Story 3 — requirements inside a test tree are test-only (P2)

**Goal**: the test half of FR-002, already implemented by T006; this phase proves it on the measured shape.

**Independent Test**: W-3 passes.

- [ ] T012 [US3] Integration case W-3 in `IT` (`proj/tests/corpus/dbt/requirements-x.txt`, `proj/test/integration/requirements.txt`). It passes on T006 alone. Then confirm it fails with T006's `test`/`tests` arm removed, as a positive control.

## Phase 6: Polish & cross-cutting

- [ ] T013 [P] W-rep in `IT`: W-1 to W-14 in one tree, scanned 20 times, identical each time.
- [ ] T014 [P] Docs:
  - `docs/ecosystems.md`, pip section: the three labelling signals in order, the word sets, the directory names, the scan-root bound, and what stays runtime (FR-005).
  - `docs/reference/sbom-format-mapping.md`: C155's description lists the signals; no new row.
- [ ] T015 [P] Add a `CHANGELOG.md` entry under the unreleased section.
- [ ] T016 Mutations, recorded in `specs/1615-nested-req-scope/measurements/mutations.md` with the runner's output; restore with `cp` and rebuild after the last one. One each:
  - (M1) drop `doc`;
  - (M2) drop the long forms;
  - (M3) drop `tests`;
  - (M4) drop FR-002's docs names from the ancestor search;
  - (M5) drop its test names;
  - (M6) farthest-first instead of nearest-first;
  - (M7) dot-directory search before the docs/test names instead of one search;
  - (M8) include the scan root's own name;
  - (M9) match directory names by prefix;
  - (M10) `is_requirements_file` keeps the old word list;
  - (M11) remove `ci_scope`'s C155 removal (FR-007's mechanism).

  Each must turn at least one test red, with 0 compile errors.
- [ ] T017 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_RESULT`, the ok and non-ok line counts, the passed/failed totals and the failure blocks in `specs/1615-nested-req-scope/measurements/gate-log.md`. Regenerate any golden that changes, and trace each change.
- [ ] T018 Push. Dispatch two TheRock measure-source runs against `main` with full SHAs (`quickstart.md`). Record in `specs/1615-nested-req-scope/measurements/therock.md`:
  - scan time (SC-007);
  - the pip components that changed scope or C155, against the 14 files and the predicted 98 (SC-001) and 11 (SC-002);
  - `requirements-development.txt`;
  - `parity-check` gaps against `main`'s, and duplicate `bom-ref`s (SC-003);
  - identity across the two runners (SC-004);
  - unreachable components without an orphan reason.
- [ ] T019 Dispatch the public corpus (`repeat=2`) and the quality corpus at the branch, with `main`'s quality run as control. Trace every change against research R7's prediction (ansible, mongo, nlohmann) in `specs/1615-nested-req-scope/measurements/corpus.md` (SC-005, SC-006). Re-centre a bound only with a traced reason.
- [ ] T020 File a follow-up issue for FR-010: non-pip components declared only under docs or test directories (TheRock: 15 Cargo crates from `rccl/contrib/nccl4rust/tests/cuda-oxide-smoke/Cargo.lock`). Measured counts only; attribution line.
- [ ] T021 Open the PR, with a body citing T016–T019, `Closes #1614` and the attribution line. Do not merge.

## Dependencies

- T001 → T002 → T003 (the golden is made from `main` before any source change).
- **US1** (T004–T007) and **US2** (T008–T011) both edit `RT`. Run them in sequence, US1 first; their tests (T004/T005, T008/T009) can be written in parallel.
- **US3** (T012) needs T006.
- **Polish:** T013–T015 after US1–US3. T016 needs every test. T017 → T018 and T019 → T020 → T021.

## Parallel examples

- T004 and T005, the unit and integration tests, in different files.
- T008 and T009, likewise.
- T013, T014 and T015.

## Implementation strategy

**MVP is US1** (T001–T007): its ancestor rule covers all 14 TheRock files, since `requirements-doc.txt` also sits under `docs/archive/`. US2 adds the name rule, which on TheRock newly labels only `requirements-development.txt`. US3 proves the rule on the measured test shape.
