# Tasks: A package search's conditions do not mark a shipped library non-runtime

**Input**: `specs/1617-installed-record-scope/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested (spec SC-007, plan Testing). Each story's tests are written first and must fail before its implementation.

**Paths**:
- `SP` = `waybill-cli/src/scan_fs/cmake_search_pair.rs`
- `MOD` = `waybill-cli/src/scan_fs/mod.rs`
- `IT` = `waybill-cli/tests/installed_record_scope.rs`
- `FX` = `waybill-cli/tests/fixtures/installed_record_scope/`

## Phase 1: Setup

- [X] T001 Confirm the premises at `main` and record them in `specs/1617-installed-record-scope/measurements/trace.md`. Use a throwaway debug log at the `record_scopes` call in `MOD`, removed after:
  - (a) at that point an installed record's `evidence.source_file_paths` holds its shared-library files and, from an exported-targets file, its static archives (research R2). Check the archives on a fixture shaped like the SDK's `absl`: a `*Targets-<config>.cmake` naming a `.a` under a raised `_IMPORT_PREFIX`, as `waybill-cli/tests/installed_lib_targets.rs` builds;
  - (b) a search in a tests directory is Test with C62 `source-test-directory`, and its pairing is `conditional` with empty `conditions` (research R1, R4);
  - (c) on spec 1614's W-14b fixture (no library file) the record is Optional with C236, the I-6 baseline.
- [X] T002 Create `IT` with helpers:
  - `scan` asserts C42, C62, C122, C235 and C236 across the three formats via `common::installed_lib::row_all_formats`;
  - `by_purl`, `scope`, `prop`, `conds`;
  - `assert_repeatable`, as in `waybill-cli/tests/propagation_seeds.rs`;
  - `shipped_record(root, prefix, name, version)` writes `<prefix>/lib/lib<name>.so.<version>` with `common::installed_lib::elf_shared_lib`, its `.so` symlink, and `<prefix>/lib/cmake/<name>/<name>ConfigVersion.cmake`;
  - `bare_record(root, prefix, name, version)` writes only the version file.

## Phase 2: Foundational

- [X] T003 Write I-ctl in `IT`:
  - the tree, in `FX/ctl/`, is an installed prefix with one shipped record, searched unconditionally from `share/waybill-fixture/consumer-config.cmake`;
  - generate its golden in `FX/ctl-golden/` from **`main`'s** binary before any source change, and compare masked output (FR-007).

**Checkpoint**: I-ctl passes at `main`.

## Phase 3: User Story 1 — a library an SDK ships is a runtime component (P1) 🎯 MVP

**Goal**: FR-001, FR-002, FR-005, FR-006: the search half never scopes a record that ships a library file. C236 is kept, and the link half is untouched.

**Independent Test**: I-1, I-2, I-3, I-9 and I-10 pass.

- [X] T004 [P] [US1] Unit tests in `SP`'s test module for `record_scopes`:
  - a shipping all-conditional record lands in `conditions_only` (not `optional`, not `required`) with its conditions;
  - a shipping record that is unconditionally linked, already scoped or targeted is in no set (no C236, as today);
  - a non-shipping all-conditional record is in `optional`, as today;
  - `ships_library_file` accepts `x.so`, `x.so.1.2`, `x.a`, `x.dylib`, `x.dll`, `x.lib`, case-insensitively, and rejects `x.cmake`, `x.pc`, `x.so.txt`, `x.so.1a`.
- [X] T005 [P] [US1] Integration cases I-1, I-2, I-3, I-9 and I-10 in `IT`.
  - I-9 builds a static-archive-only record from an exported-targets file, as `waybill-cli/tests/installed_lib_targets.rs` does.
  - I-10 builds a conditional link (C235), as `waybill-cli/tests/scan_therock_last_orphans.rs` does, and asserts C122 `cmake-conditional-link`, C235 and C236.
  - Confirm that I-1, I-2 and I-9 fail at `main`, and that I-3 and I-10 pass as guards.
- [X] T006 [US1] Implement in `SP`:
  - `fn ships_library_file(c: &ResolvedComponent) -> bool`, over `c.evidence.source_file_paths` file names, case-insensitive (data model);
  - `RecordScopes` gains `conditions_only: BTreeMap<String, BTreeSet<String>>`;
  - in `record_scopes`, where a record would be inserted into `optional`, a shipping record goes into `conditions_only` instead;
  - `apply_scope` writes C236 for `conditions_only` without touching the scope or C122.
- [X] T007 [US1] Wire `conditions_only` at the call site in `MOD`. It must not add the record to `scopes.required`, so the `link_conditional.retain(...)` line still lets the link half apply (FR-005).
- [X] T008 [US1] Run T004, T005, I-ctl, `scan_cmake_tree_finds`, `scan_therock_last_orphans`, `installed_lib_targets`, `installed_test_scope` and `cargo test -p waybill --bins cmake_search_pair::` until they pass.

**Checkpoint**: US1 complete.

## Phase 4: User Story 2 — a library searched only from a shipped test project is still shipped (P1)

**Goal**: FR-001 for test-only searches, and FR-004: a non-shipping record searched only from test contexts is Test.

**Independent Test**: I-4, I-5 and I-7 pass.

- [X] T009 [P] [US2] Unit tests in `SP`:
  - a non-shipping record whose pairings are all Test lands in `test`, with `source-test-directory` when any pairing has it, else `cmake-build-testing`;
  - a record with an Optional (non-Test) pairing lands in `optional`;
  - a shipping all-Test record is in `conditions_only` with no conditions, so it is not in `test`.
- [X] T010 [P] [US2] Integration cases I-4, I-5 and I-7 in `IT`. Confirm that I-4 and I-5 fail at `main` and that I-7 passes as a guard.
- [X] T011 [US2] Implement in `SP`:
  - `Pairing` gains `scope: Option<LifecycleScope>` and `test_derivation: Option<String>`, read in `pair` from the search component's `lifecycle_scope` and C62 annotation;
  - `RecordScopes` gains `test: BTreeMap<String, (&'static str, BTreeSet<String>)>`;
  - `record_scopes` routes a non-shipping record with every pairing Test there (research R4);
  - `apply_scope` sets `LifecycleScope::Test`, C62 to the derivation, and C236 when there are conditions, and writes no C122.
- [X] T012 [US2] Wire `test` at the call site in `MOD`, and run T009, T010 and every US1 test.

**Checkpoint**: US1 and US2 complete.

## Phase 5: User Story 3 — what the tree does not ship stays optional (P1)

**Goal**: FR-003: unpaired conditional searches and non-shipping records keep spec 1614's behaviour.

**Independent Test**: I-6 and I-8 pass on the branch.

- [X] T013 [US3] Integration cases I-6 (a bare record searched under `if(NOT GPU_TARGETS)`: Optional, C122, C236) and I-8 (a conditional search for an absent library: the search component Optional with C236) in `IT`. Both pass at `main` and on the branch, as guards of FR-003.
- [X] T014 [US3] I-11 in `IT`: a shipped library under `share/waybill-fixture/tests/lib/`, in an installed tree, is Test through spec 1533 (`test-directory`), on `main` and on the branch.

## Phase 6: Polish & cross-cutting

- [X] T015 [P] I-rep in `IT`: I-1 to I-11 in one tree, scanned 20 times, identical each time.
- [X] T016 [P] Docs:
  - `docs/reference/sbom-format-mapping.md`: C236 may sit on a required record (a shipped library searched only conditionally); C122 `cmake-conditional` is never set by searches on a record that ships a library file; a record searched only from test contexts is Test.
  - `docs/ecosystems.md`, CMake section: how an installed record's scope is decided.
- [X] T017 [P] Add a `CHANGELOG.md` entry under Unreleased / Fixed.
- [X] T018 Mutations, in `specs/1617-installed-record-scope/measurements/{mutate.py,mutate-run.sh,mutations.md}`, adapted from `specs/1616-propagation-seeds/measurements/`: restore with `cp`, rebuild after the last one, and use the `^error\[E…\]` compile pattern. One each:
  - (M1) `ships_library_file` always false;
  - (M2) a shipping record put in `required` (the link half switched off; I-10 must turn red);
  - (M3) `conditions_only` writes no C236;
  - (M4) Test records made Optional;
  - (M5) derivation precedence reversed;
  - (M6) `.a` not counted;
  - (M7) a record with an Optional pairing made Test.

  Each must turn at least one test red, with 0 compile errors.
- [X] T019 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_RESULT`, the ok and non-ok counts, passed/failed, failure blocks, and an empty `git diff --stat main -- '*Cargo.toml' Cargo.lock` in `specs/1617-installed-record-scope/measurements/gate-log.md`. Trace any repository golden that changes.
- [ ] T020 Push. Dispatch measure-full (the SDK) and measure-source (TheRock source) against `main` with full SHAs. In `specs/1617-installed-record-scope/measurements/sdk.md`, record:
  - scan time (SC-006);
  - the 9 records against research R6 (SC-001);
  - every other component whose C42, C62 or C122 changes, traced (SC-002);
  - TheRock source unchanged (SC-003);
  - parity, duplicates and runner identity (SC-005).
- [ ] T021 Dispatch the public corpus with `regen_goldens=true`, and the quality corpus at the branch with a `main` control. Trace every change (SC-004), commit regenerated goldens if any, and record it all in `specs/1617-installed-record-scope/measurements/corpus.md`.
- [ ] T022 Open the PR, with a body citing T018–T021, `Closes #1623` and the attribution line. Do not merge.

## Dependencies

- T001 → T002 → T003 (the I-ctl golden is made from `main` before any source change).
- **US1** (T004–T008) → **US2** (T009–T012): both edit `SP` and `MOD`.
- **US3** (T013–T014) needs nothing new; it can run once T002 exists.
- **Polish:** T015–T017 in parallel after US1–US3. Then T018 → T019 → T020 ∥ T021 → T022.

## Parallel examples

- T004 and T005, in different files.
- T009 and T010.
- T015, T016 and T017.

## Implementation strategy

**MVP is US1** (T001–T008): it makes the 7 shipped SDK libraries, `hsa-runtime64` among them, required. US2 adds the Test scope for test-only records, which on the SDK is `rocprofiler-sdk-tests`; `rocattach` is already fixed by US1, because it ships a library. US3 is guards only.
