# Tasks: Report an installed tree's test and benchmark payload with a test scope

**Input**: `specs/1533-installed-test-scope/` (spec, plan, research R1–R10, data-model, contracts/wire.md, quickstart).
**Tests**: required. The spec's SC-007 asks for a mutation per rule, and every acceptance scenario becomes an integration test.

Paths are relative to the repository root. `[P]` means a different file with no dependency on an unfinished task.

## Phase 1: Setup (measurement first)

- [x] T001 Extend the fixture capture to `CTestTestfile.cmake` and `*_include.cmake` (fixture repo `capture.yml`, commits `ca91afe` and `951b8f7`; captures `5ded2fd` and `f7935cb`)
- [x] T002 Write and run the census in `specs/1533-installed-test-scope/measurements/test-census.py`, with its output in `measurements/test-census.json` (research R1, R3, R4, R9)

## Phase 2: Foundational (blocks every story)

- [ ] T003 Create `waybill-cli/src/scan_fs/test_scope.rs`, registered in `waybill-cli/src/scan_fs/mod.rs`:
  - `TestRule` (`ExportedTargetsTestPackage`, `CtestDeclaration`, `TestDirectory`), with its strength order and `c62()` value (data-model);
  - `InstalledTree` (`Image`, `Recognised`, `NotRecognised`);
  - `TestScopeCoverage`, with `wire()` producing C226's sorted-key JSON, or `None` when empty.

  Add unit tests for the strength order, the weakest-across-paths rule and the wire shape.
- [ ] T004 Carry the inputs out of `scan_path` in `waybill-cli/src/scan_fs/mod.rs`:
  - `installed_tree: InstalledTree`, which is `Image` for `ScanMode::Image`. For `--path`, it is `Recognised` when #1531's resolve saw at least one non-OS-owned record in a recognised location, or when `os_claimed_paths` is non-empty (research R5). Expose the record bool from `installed_lib/resolve.rs`.
  - `test_declarations` (empty for now);
  - the OS claim set (`os_claimed_paths`, plus `os_claimed_inodes` on unix) for the post-pass's FR-013 check (data-model).

  Follow the `installed_library_coverage` plumbing into `ScanArtifacts` in `waybill-cli/src/cli/scan_cmd.rs`.
- [ ] T005 Add the post-pass call in `waybill-cli/src/cli/scan_cmd.rs`, after file-tier components join (about line 5720) and before `apply_exclude_scope_filter` (about line 5794), with a comment citing research R2. With no rules wired, it must change nothing. Add a unit test asserting a component list passes through unchanged.
- [ ] T006 [P] Emit C226 as a document annotation in all three formats, the C225 pattern:
  - `waybill-cli/src/generate/cyclonedx/{metadata,builder,mod}.rs`;
  - `waybill-cli/src/generate/spdx/{annotations,v3_annotations}.rs`, with their test constructors.

  Add the C226 extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs` and the C226 row in `docs/reference/sbom-format-mapping.md` in the same change. The lib-target gate requires the row and its extractor together.
- [ ] T007 SPDX 3 purpose (research R8). This follows T006, because both edit `parity/extractors/spdx3.rs`:
  - in `waybill-cli/src/generate/spdx/v3_packages.rs`, an element with `lifecycle_scope == Some(Test)` and a C62 value from this feature gets `software_primaryPurpose: "test"`, and its `BinaryRole` purpose moves to `software_additionalPurpose: [role]`;
  - in `waybill-cli/src/parity/extractors/spdx3.rs`, `spdx3_binary_role` falls back to `software_additionalPurpose` when the primary purpose is `test`.

  Unit tests cover both. An unscoped element is byte-identical.

**Checkpoint:** the gate is green and output is unchanged, because no rule fires yet.

## Phase 3: User Story 1, declared test packages (P1, the MVP)

**Goal:** files named by a test package's exported targets or by an installed CTest file are test-scoped in all three formats.
**Independent test:** a synthetic prefix with a `waybill-fixture-p-tests` export set, a CTest file and a product library (spec US1).

- [ ] T008 [P] [US1] Write the CTest parser tests first in `waybill-cli/src/scan_fs/package_db/installed_lib/ctest.rs`, with synthetic `testdata/` snippets in the R1 forms:
  - positional `add_test(<name> "../<exe>" args)` and `NAME … COMMAND …`;
  - a bracket-argument name;
  - `python3 -m pytest … tests/x.py`, where the interpreter is not a location and the script is;
  - a `${VAR}` word and an absolute path, both unresolved;
  - `include()` counted, not followed;
  - comments and a truncated file.
- [ ] T009 [US1] Implement `ctest.rs`: parse into `CtestFile { add_tests: Vec<Vec<String>>, includes: usize }`, using `cmake_version::commands` and `strip_comments`. Make T008 pass.
- [ ] T010 [US1] In `waybill-cli/src/scan_fs/package_db/installed_lib/mod.rs`:
  - add `**/CTestTestfile.cmake` to `registration()` (not the venv probe: CTest files are not installed into site-packages);
  - collect CTest records into `Phase1`;
  - classify exported-targets files whose base package name ends in `-tests`, `_tests`, `-test` or `_test` (case-insensitive) as test packages. `targets::classify_with_base` already returns the base name.
- [ ] T011 [US1] In `waybill-cli/src/scan_fs/package_db/installed_lib/resolve.rs`:
  - resolve CTest locations relative to each file's directory with `location::resolve_in_root`, recording the outcomes `resolved`, `not-shipped`, `unresolved` and `outside-root`, plus OS-owned skips;
  - collect the resolved locations of test-package export sets from the #1563 per-directory resolution, without resolving them a second time;
  - return `TestDeclarations { paths: BTreeMap<root-relative path, TestRule>, package_purls: BTreeSet<purl>, coverage }`.

  Feed it into T004's `test_declarations`.
- [ ] T012 [US1] Implement the declaration rules in `waybill-cli/src/scan_fs/test_scope.rs::apply`:
  - a component's paths come from `evidence.source_file_paths` (binary) or `waybill:file-paths` (file tier), made root-relative;
  - every path must be declared;
  - an installed-library component whose PURL is in `package_purls` is scoped as a whole;
  - skip any component that already has a scope, that is OS-owned (any path in the OS claim set from T004), that is the subject, or that is in another ecosystem (data-model);
  - set `lifecycle_scope = Test` and C62 (`ANN_SCOPE_DERIVATION`), and count into the coverage.
- [ ] T013 [US1] Integration tests in `waybill-cli/tests/installed_test_scope.rs`. Share helpers through `waybill-cli/tests/common/installed_lib.rs`. Test:
  - acceptance scenarios 1–4, checked in CycloneDX, SPDX 2.3 and SPDX 3;
  - the control: the same prefix with its declarations removed scopes nothing;
  - **no propagation:** `libwaybill-fixture-p.so` is linked only by a declared test executable and stays unscoped;
  - an OS-owned declared file is not scoped;
  - `--root-name` pointing at a declared test executable leaves the subject unscoped;
  - a declaration in a `--path` tree that is not recognised still scopes;
  - C226's `declarations`, `locations` and `scoped` counts.

**Checkpoint:** US1 is complete and the MVP is shippable.

## Phase 4: User Story 2, conventional test directories (P2)

**Goal:** in an installed tree, files under `test`, `tests`, `unittests`, `test_plugins` or `.upstream-tests` are test-scoped.
**Independent test:** spec US2. The control is an unrecognised source tree with `tests/`, which must stay byte-identical.

- [ ] T014 [US2] In `waybill-cli/src/scan_fs/test_scope.rs`, add the path-convention rule:
  - exact directory names, applied only when `installed_tree` is `Image` or `Recognised`;
  - a declaration wins over the convention for the same file;
  - across several paths, the weakest rule is recorded (research R6);
  - set `installed-tree` in C226.

  Unit tests cover `testing`, `attestation` and `contest`, none of which match.
- [ ] T015 [US2] Implement FR-012 in `waybill-cli/src/scan_fs/test_scope.rs`: under `--include-test-binaries` (the `WAYBILL_INCLUDE_TEST_BINARIES` env var `scan_fs` already reads), a binary file component whose every path lies under `binary::discover::TEST_INPUT_DIRS` is scoped `test-directory`, whether or not the tree is recognised (research R10).
- [ ] T016 [US2] Add integration tests to `waybill-cli/tests/installed_test_scope.rs`:
  - US2 scenarios 1–3;
  - an image-shaped rootfs;
  - an extracted rootfs scanned with `--path`, with a dpkg `status` and `.list` and no `.pc`. It is recognised; an unclaimed `tests/` file is scoped and a dpkg-claimed one is not;
  - the unrecognised source-tree control, byte-identical to a scan with the pass disabled, and with no C226;
  - `--include-test-binaries` in an unrecognised tree;
  - the all-paths rule: identical content under `tests/` and in `lib/` stays unscoped.

## Phase 5: User Story 3, name patterns (P3)

**Goal:** per research R4, no pattern is adopted. The story ships the committed measurement and a guard against a name rule appearing without one.

- [ ] T017 [US3] Add a test to `waybill-cli/tests/installed_test_scope.rs`. In a recognised prefix, the undeclared `bin/test_waybill_fixture_op`, `bin/benchmark_waybill_fixture_op`, `bin/waybill-fixture-r-bench` and `bin/waybill-fixture-attest` all stay unscoped.

## Phase 6: Polish and cross-cutting

- [ ] T018 [P] Docs:
  - in `docs/reference/sbom-format-mapping.md`: the three new C62 values in C62's row; an A13 note on `software_additionalPurpose`; C55 unchanged;
  - in `docs/ecosystems.md`: an "installed trees: test payload" paragraph;
  - in `CHANGELOG.md`: the `[Unreleased]` entry.
- [ ] T019 Mutations, each reverted with `cp` and each required to fail a test. Record them in `specs/1533-installed-test-scope/measurements/mutations.md`:
  - **M1:** any path instead of all paths;
  - **M2:** run the pass before scope propagation;
  - **M3:** substring directory matching;
  - **M4:** drop the recognition gate, and separately drop the OS-database signal;
  - **M5:** scope OS-owned files;
  - **M6:** treat the interpreter as the location;
  - **M7:** re-scope an already-scoped component;
  - **M8:** drop the A13 `additionalPurpose` fallback.
- [ ] T020 Run the gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record the result in `measurements/gate-log.md`.
- [ ] T021 Regenerate the corpus goldens in CI (`public-corpus.yml` dispatch on the branch). Review the diff normalised and sorted. Record it in `measurements/golden-review.md` against research R5's prediction.
- [ ] T022 In the fixture repo's `measure-full.yml`, report C226, the scoped components and bytes per C62 value, and the SC-003 check (no installed-library component of a non-test package, and no shared library a non-test component links, is scoped). Run it against `main` and this branch. Record SC-001 to SC-004 and SC-006 in `specs/1533-installed-test-scope/measurements/therock-full.md`.
- [ ] T023 Open the PR. The body carries the measurements, the mutations, the gate result, and the attribution line.

## Dependencies

- **Setup** (T001, T002) is done.
- **Foundational:** T003 → T004 → T005. T006 → T007 run after T003, alongside T004 and T005.
- **US1:** T008 → T009 → T010 → T011 → T012 → T013. It needs Phase 2.
- **US2:** T014 → T015 → T016. It needs T012's matching scaffold, so it follows US1.
- **US3:** T017 needs T014 (recognition), and is otherwise independent.
- **Polish:** T018 can run in parallel from US1 on. T019 → T020 → T021 and T022 → T023.

## Parallel examples

- After T003: T006 (C226 emission and extractors) runs alongside T004 and T005. T007 follows T006, because they share `parity/extractors/spdx3.rs`.
- In US1: T008 (parser tests) can be written while T004 and T005 land.
- In Polish: T018 (docs) alongside T019 (mutations).

## Implementation strategy

1. **MVP is US1.** Declarations alone scope 2.82 GB of ROCm's measured payload (research R3) and carry no guess.
2. **US2** adds 0.236 GB, and is gated by recognition so source trees stay byte-identical.
3. **US3** records a decision rather than adding behaviour.
4. **Measure in CI** before the PR (T021, T022). A measured number that disagrees with research R3 is investigated, not explained away.
