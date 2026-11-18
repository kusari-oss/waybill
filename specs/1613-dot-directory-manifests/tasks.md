# Tasks: Read dependency manifests in CI and docs dot-directories

**Input**: `specs/1613-dot-directory-manifests/`:
- spec.md, plan.md;
- research.md (R1–R7);
- data-model.md;
- contracts/wire.md (cases D-1 to D-12).

**Tests**: the spec requires them (FR-012, SC-004). Within each story, tests are written first and must fail on `main` before the fix.

**Format**: `[ID] [P?] [Story] Description`. `[P]` marks a different file with no dependency on an incomplete task.

## Phase 1: Setup

- [X] T001 Trace the plan's predictions on `main` before any change, and record them in `specs/1613-dot-directory-manifests/measurements/trace.md`:
  - Build the quickstart tree (`quickstart.md`), scan it with `main`'s binary, and confirm none of its four packages appear.
  - Add `.github/workflows/test-suite/waybill-fixture.cmake` holding a `FetchContent_Declare`. Then set the CMake registration's `descend_into` to `.github` in a scratch copy and confirm the CMake reader receives the file. This proves R2's claim that CMake discovers through the shared walker, and not only by its registration's pattern.
  - Confirm how the deduplicator merges a pip package declared both in a scoped requirements file (`Optional`, C155) and in an unscoped `requirements.txt`. Use `requirements-ci.txt` beside `requirements.txt`, which needs no dot-directory and so runs on `main`. Record which scope and which C155 value survive. If the runtime declaration doesn't win, FR-006 needs a merge rule of its own, and T010 gains it.
  - Revert the scratch change with `cp`.
- [X] T002 [P] Create `waybill-cli/tests/scan_dot_directory_manifests.rs` (file-scope `#![cfg(unix)]`), including the corpus harness by `#[path]` as `waybill-cli/tests/scan_therock_last_orphans.rs` does. Add helpers:
  - `scan(dir, flags) -> [cdx, spdx23, spdx3]`;
  - `component(cdx, name)`;
  - `prop_all_formats(docs, name, key)`, through the parity extractors for C155 and C62;
  - `scope_all_formats(docs, name)`;
  - `assert_repeatable(dir, flags, 20)`;
  - `requirements(path, lines)` and `package_json(path, deps)`, writing `waybill-fixture-*` names only.

## Phase 2: Foundational

- [X] T003 In `waybill-cli/src/scan_fs/package_db/project_roots.rs`, add:
  - `pub(crate) const LISTED_DOT_DIRECTORIES: [&str; 7]` (`.github`, `.ci`, `.gitlab`, `.circleci`, `.buildkite`, `.devcontainer`, `.sphinx`);
  - `pub(crate) fn listed_dot_dir_class(name: &str) -> Option<DotDirClass>`, case-insensitive, with `DotDirClass::{Ci, Docs}`;
  - `pub(crate) fn listed_ancestor_class(path: &Path, root: &Path) -> Option<DotDirClass>`, giving the nearest listed ancestor between `root` and the file;
  - `pub(crate) fn listed_dot_dir_globset() -> anyhow::Result<globset::GlobSet>`, through `globset_from_patterns_case_insensitive`.
  
  Unit tests: case (`.CI`, `.GitHub`); nesting (the nearest ancestor wins); non-listed names (`.venv`, `.upstream-tests`, `.tox`) give `None`; `should_skip_default_descent` is unchanged for every name.
- [X] T004 Add `ci-directory` and `docs-directory` to C62's value list in `docs/reference/sbom-format-mapping.md`, with one sentence naming spec 1613 and the rule (every evidence source file under a listed directory; `Development`). Confirm that C62's extractor carries any string value, and say so in the commit message. Run `cargo build --all-targets`.

## Phase 3: User Story 1 — CI and docs packages appear, labelled (P1)

**Goal**: packages declared under listed directories appear, with a non-runtime scope and a `ci` or `docs` label.

**Independent test**: D-1 to D-5, D-8, D-10 and D-11 in all three formats.

- [X] T005 [US1] Write the tests first, in `waybill-cli/tests/scan_dot_directory_manifests.rs`:
  - D-1 `.ci/requirements.txt`, D-2 `.ci/metrics/requirements.txt`, D-3 `.github/requirements-tool-ci.txt`, D-4 `docs/.sphinx/requirements.txt`, D-5 `.CI/requirements.txt`: each present, `Optional`, C155 `ci` (D-4 `docs`).
  - D-8: `.ci/all_requirements.txt` gives C155 `ci`; root `dev-requirements.txt` gives `dev`.
  - D-10: `.github/actions/waybill-fixture-act/` with its dependency installed in a committed `node_modules/` gives that dependency `Development` and C62 `ci-directory`. Corrected during T012: the npm reader reads a project's `node_modules/` as its installed tree; see measurements/trace.md.
  - D-11: a package in both `.ci/requirements.txt` and the root `requirements.txt` appears once, with the runtime scope and no C155.
  
  Run them on `main`'s code and record in `measurements/trace.md` that they fail.
- [X] T006 [US1] In `waybill-cli/src/scan_fs/package_db/pip/mod.rs`, set the pip registration's `descend_into: Some(project_roots::listed_dot_dir_globset()?)`.
- [X] T007 [US1] Widen the pip selector (R3, FR-004a) in `waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` (`read_requirements_files`) and `waybill-cli/src/scan_fs/package_db/pip/mod.rs` (`has_python_project_marker`), through one shared predicate `is_requirements_file(path, root)`. A file is read if its name:
  - starts with `requirements` and ends with `.txt`; or
  - is `<scope>-requirements*.txt` with scope ∈ {`dev`, `test`, `docs`, `ci`}; or
  - is `*requirements*.txt` with a listed ancestor (T003).
  
  Unit tests: `dev-requirements.txt` read; `tox-requirements.txt` not read; `all_requirements.txt` read under `.ci` but not at the root; the #1522 rejection still applies to a widened file.
- [X] T008 [US1] Extend `classify_requirements_scope` in `waybill-cli/src/scan_fs/package_db/pip/requirements_txt.rs` with the ancestor rule (R4), after the parent-directory and file-name signals. It needs the scan root, so thread it through from `read_requirements_files`. Unit tests:
  - precedence: `.ci/tests/requirements.txt` gives `test`, and `.github/docs/requirements.txt` gives `docs`;
  - the ancestor rule: `.ci/metrics/requirements.txt` gives `ci`, and `.sphinx/requirements.txt` gives `docs`;
  - unchanged for every existing test case.
- [X] T009 [P] [US1] Set `descend_into: Some(project_roots::listed_dot_dir_globset()?)` on the npm registration (`waybill-cli/src/scan_fs/package_db/npm/mod.rs`) and on the CMake registration (`waybill-cli/src/scan_fs/package_db/cmake.rs`).
- [X] T010 [US1] Create `waybill-cli/src/scan_fs/ci_scope.rs` with `pub(crate) fn apply(components: &mut [ResolvedComponent], root: &Path) -> CiScopeSummary` (R5). For each component that has no `lifecycle_scope` and whose evidence source files are all non-empty and all under a listed directory, it sets:
  - `Development`;
  - C62 = `ci-directory`, or `docs-directory` when every file's nearest listed ancestor is `.sphinx`.
  
  It also removes C155 `waybill:python-req-file-scope` from any component whose final scope is runtime (research R8). It logs one INFO summary. Unit tests: only-under-listed; mixed locations (unchanged, FR-006); already scoped (unchanged); no source files (unchanged); `.sphinx` only (`docs-directory`); a runtime-scoped component carrying C155 loses it.
- [X] T011 [US1] Call `ci_scope::apply` after deduplication, beside the spec 1533 test-scope pass. That call site is `waybill-cli/src/cli/scan_cmd.rs`, where deduplication and propagation finish (a deviation from the plan's `scan_fs/mod.rs`, which only declares `mod ci_scope;`).
- [X] T012 [US1] Run T005's tests until they pass, in all three formats. Record the run in `measurements/trace.md`.

## Phase 4: User Story 2 — caches and test suites stay skipped (P1)

**Goal**: no content from caches, virtualenvs or `.upstream-tests`; byte-identical output where nothing is listed.

**Independent test**: D-6, D-7, D-9, D-12a, and 20 repeated scans.

- [X] T013 [US2] Add tests to `waybill-cli/tests/scan_dot_directory_manifests.rs`:
  - D-6: requirements under `.venv/`, `.tox/`, `.cache/` and `.upstream-tests/` are absent.
  - D-7: `.ci/.venv/requirements.txt` is absent.
  - D-9: root `tox-requirements.txt` is not read.
  - D-12a: a tree with a root `requirements.txt` gives, after masking (`masked_difference`), the same document as the same tree plus `.cache/requirements.txt`, `.venv/requirements.txt` and `.tox/requirements.txt`. Non-listed dot-directories change nothing.
  - D-12b, byte-identity against `main` for trees with no listed directory: this is not repeated in this file. It is asserted by the existing byte-identity goldens in the gate (T020) and by the public corpus at the branch and at `main` (T022, SC-003).
  - `assert_repeatable` over the US1 tree, 20 scans.
- [X] T014 [US2] Run T013's tests until they pass. Fix any nested-skip leak in the registrations, not in `should_skip_default_descent`.

## Phase 5: User Story 3 — the file tier does not report CI files (P2)

**Goal**: with a file inventory on, nothing under a listed directory becomes a file-tier component.

**Independent test**: D-12c.

- [X] T015 [US3] Add D-12c to `waybill-cli/tests/scan_dot_directory_manifests.rs`: scan a tree holding `.github/workflows/ci.yml` and `.ci/script.sh` with the file inventory on, and assert that no file-tier component's path lies under `.github` or `.ci`. Run it until it passes. No implementation is expected (R1: the file tier keeps its own walker); if it fails, the fix belongs in the registrations.

## Phase 6: Polish and cross-cutting

- [X] T016 Mutations, in `specs/1613-dot-directory-manifests/measurements/mutate.py` and `mutate-run.sh`, each applied, tested and reverted with `cp`, with the results in `measurements/mutations.md`. Each must turn a named test red:
  - M1: no `descend_into` on pip;
  - M2: case-sensitive glob;
  - M3: `.venv` added to the listed set;
  - M4: no ancestor rule;
  - M5: selector not widened;
  - M6: `tox-requirements.txt` accepted;
  - M7: R5 pass removed;
  - M8: R5 "all files" made "any file";
  - M9: R5 overwrites an existing scope.
- [X] T017 [P] Document in `docs/ecosystems.md` (pip and CI-directory sections):
  - which requirements names are read;
  - the listed directories;
  - the `ci`/`docs` labels;
  - the `tox-requirements.txt` gap.
- [X] T018 [P] Add a `### Fixed` entry to `CHANGELOG.md` for #1610: what was hidden, what is read now, and the labels.
- [X] T019 Reproduce the walker-audit gate locally (the grep and diff from the CI workflow against `waybill-cli/src/scan_fs/walk.audit-allowlist.txt`) and confirm no change.
- [X] T020 Run the gate on its own: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record the `ok` count, passed/failed totals and failure blocks in `measurements/gate-log.md`.
- [ ] T021 TheRock in CI: run `measure-source` with `waybill_ref=<branch sha>` and `base_ref=main`. Record in `measurements/therock.md`:
  - SC-001: the 8 files' packages present, labelled;
  - SC-001a: the `dev-requirements.txt` packages present, labelled `dev`; `tox-requirements.txt` absent;
  - SC-002: nothing from `.upstream-tests`, `._upstream`, `.gitlab-ci`, `.jenkins` or `.claude`, and no file-tier entry under a dot-directory;
  - SC-004: the formats agree on C155 and C62, and repeated scans are identical;
  - SC-005: interleaved medians, with the ratio ≤ 1.05;
  - every other difference from `main`, from a normalised, `LC_ALL=C`-sorted diff, including what the 4 `.github` CMake scripts produced.
- [ ] T022 [P] Public corpus: `public-corpus.yml` at the branch and at `main` (`repeat=2`). SC-003 predicts no golden change; trace any that occurs.
- [ ] T023 [P] Quality corpus at the branch and at `main` (SC-006). Trace every changed target.
- [ ] T024 Open the PR, with a body citing T020–T023's evidence and the attribution line. Do not merge.

## Dependencies

- T001 → everything (it confirms R2's CMake route).
- T002, T003 → US1.
- T004 → T010.
- **US1** (T005 → T006–T011 → T012) → **US2** (T013–T014) → **US3** (T015). US2 and US3 assert properties of US1's change.
- **Polish** (T016–T024) after US3. T021–T023 can run in parallel; T024 last.

## Parallel examples

- T002 ∥ T003, which touch different files.
- T009 ∥ T010, which touch different files, once T003 is done.
- T017 ∥ T018 ∥ T022 ∥ T023.

## Implementation strategy

- **MVP:** US1 with T006–T008 (pip only), which is what reaches TheRock's 8 + 2 requirements files.
- **Then:**
  - npm and CMake opt-in (T009);
  - the R5 pass (T010, T011);
  - US2's guards, US3's check, mutations and measurement.
