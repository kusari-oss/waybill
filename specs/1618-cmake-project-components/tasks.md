# Tasks: A component for each CMake project the scanned tree builds

**Input**: `specs/1618-cmake-project-components/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested (spec SC-001–SC-008, plan Testing). Each story's tests are written first and must fail before its implementation.

**Paths**:
- `PR` = `waybill-cli/src/scan_fs/package_db/cmake/projects.rs` (new)
- `FE` = `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs`
- `RE` = `waybill-cli/src/scan_fs/package_db/cmake/reach.rs`
- `CM` = `waybill-cli/src/scan_fs/package_db/cmake.rs`
- `SP` = `waybill-cli/src/scan_fs/cmake_search_pair.rs`
- `DA` = `waybill-cli/src/scan_fs/declared_attach.rs`
- `MOD` = `waybill-cli/src/scan_fs/mod.rs`
- `RS` = `waybill-cli/src/generate/root_selector.rs`
- `IT` = `waybill-cli/tests/cmake_project_components.rs`
- `FX` = `waybill-cli/tests/fixtures/cmake_project_components/`
- `MAP` = `docs/reference/sbom-format-mapping.md`

## Phase 1: Setup

- [X] T001 Census, before any code. Add `census/projects.py` and `.github/workflows/census-projects.yml` to `kusari-sandbox/waybill-fixture-therock-sdk`, following `census/dotdirs.py` and `census-dotdirs.yml`; run TheRock only there, and the three quality-corpus targets (`cmake-nlohmann-json`, `python-pytorch`, `cpp-mongo`) at their pinned SHAs from `xtask/corpus/quality-corpus.toml`, in the same workflow. For each tree record, in `specs/1618-cmake-project-components/measurements/census.md`:
  - (a) every literal `project(<name> …)` in a `CMakeLists.txt`, with directory, `VERSION` form (literal / `${}` / none) and whether a same-file literal `set()` resolves it;
  - (b) the population SC-001 measures: searched names that have a project declaration, re-counting #1616's 63 on the current pin;
  - (c) how many project directories each of R8's example/benchmark names and `TEST_DIRS` catches, listing any first-party library caught;
  - (d) for each R10 candidate signal, the projects it marks;
  - (e) names declared in more than one directory, and with which versions;
  - (f) projects whose name equals a fetch, installed record or submodule.

  Adjust R8's name list from (c) before T009.
- [X] T002 Vendored sample, after T001(d) and **before computing any precision**. Draw a random sample of ≥ 30 projects across the four trees, seeded and listed in the file. Label each `vendored` or `first-party` from the project's own files (licence holder, upstream URL, README), and commit `specs/1618-cmake-project-components/measurements/vendored-sample.tsv` with the evidence column. Then compute each signal's and two-signal conjunction's precision and recall in `measurements/vendored-rule.md`, and apply R10's fixed criterion (precision ≥ 0.90, ties to recall). If nothing qualifies, stop US2's C240 work and report to the maintainer.
- [X] T003 Create `IT` with helpers: `scan` (three formats, parity extractors for C42, C55, C62, C197, C236, C237, C238–C241), `by_purl`, `prop`, `edges_from`, `root_of`, `assert_repeatable` (as in `waybill-cli/tests/installed_record_scope.rs`), and `write_project(root, dir, name, version_form)`.

## Phase 2: Foundational

- [X] T004 P-ctl in `IT`: a tree with a FetchContent declaration and a `find_package` but **no** `project()`, in `FX/ctl/`. Generate its golden in `FX/ctl-golden/` from **`main`'s** binary before any source change; compare masked output (FR-012).
- [X] T005 Unit tests in `FE`/`PR` for `ProjectDeclaration` parsing: `VERSION 1.2.3`, `VERSION ${V}` with and without same-file `set(V …)`, no `VERSION`, `project(${X})` (skipped), `project(n LANGUAGES CXX VERSION 2)`, comments and multi-line calls.
- [X] T006 Implement `DeclaredVersion` and `ProjectDeclaration` in `PR`, and collect them in `FE::read_declarations`'s existing loop (replacing the `projects` name set, which is derived from them for `built_in_tree`), using the file's `Vars`.
- [X] T007 Reached-from-root set in `RE`: BFS from `<scan root>/CMakeLists.txt` over `compute`'s `into` edges, exposed on `ReachMap`; empty-means-undefined when there is no scan-root listfile. Unit-test: reached child, unreached sibling, no root listfile.

**Checkpoint**: P-ctl passes at `main`; declarations parse.

## Phase 3: User Story 1 — a monorepo's projects and the edges between them (P1) 🎯 MVP

**Goal**: FR-001–FR-007 (minus root selection), FR-009–FR-016: project components, search edges, enclosing edges, ownership, scope, reach.

**Independent Test**: P-1–P-8b, P-10–P-12, P-16–P-17 pass in three formats, and P-ctl stays byte-identical.

- [X] T008 [P] [US1] Integration cases P-1, P-2, P-3, P-4, P-5, P-6, P-7, P-8, P-8b, P-10, P-11, P-12, P-16, P-16b, P-16c, P-17 in `IT`. Confirm each fails at `main`, except P-17 (a guard).
- [X] T009 [P] [US1] Unit tests in `PR`: merge rules (R5: same version merges; different versions split; versionless merges into versioned), directory scope (T001-adjusted lists), C197/C238 choice, C239 from the reach set.
- [X] T010 [P] [US1] Unit tests in `DA`: `RANK_CMAKE_PROJECT` loses to a PyPI and another main module in the same directory and beats a submodule; `owner_above(dir)` never returns the directory's own owner.
- [X] T011 [US1] Implement `PR`: from the declarations, build one `PackageDbEntry` per (name, effective version) with C55 `cmake-project`, `main-module`, `source`, sorted evidence, C197/C238, directory scope and C62 (`TEST_DIRS` → `source-test-directory`; example/benchmark → Development, `source-example-directory`), C239. Skip declarations under excluded or skipped directories (FR-009). Emit from `CM`.
- [X] T012 [US1] In `CM`, apply R5's one-identity order before emitting: a project whose name a fetch declares is dropped and its file joins the fetch's evidence; with `--include-vendored`, a `parse_vendored` entry whose directory holds a project declaration is dropped into the project's evidence.
- [X] T013 [US1] In `SP::pair`, after installed records and submodules, pair a search whose name is a project component's with that component (`Pairing.project = true`); count it in C237 `paired.project`, serialised only when non-zero (FR-012); leave config-only names in `in-tree` (R6). Also apply FR-005's later identities here, where they are known: drop a project component whose name an installed-library record or a submodule directory claims, and move its file into that component's evidence (P-16b, P-16c). In `record_scopes`, route project targets to `conditions_only` for conditional searches and to `test` when every pairing is Test and the project has no directory scope (R4).
- [X] T014 [US1] In `DA`, add `RANK_CMAKE_PROJECT` and `owner_above`. In `MOD`, at the existing attach site, give every project component no search targets an edge from `owner_above(its dir)` or a root anchor (FR-006), with provenance `cmake-project-enclosure`.
- [X] T015 [US1] Run T008–T010, P-ctl, and the existing CMake suites (`scan_cmake_tree_finds`, `scan_therock_last_orphans`, `installed_record_scope`, `installed_lib_targets`, `propagation_seeds`, `scan_nested_req_scope`, `cargo test -p waybill --bins cmake`) until they pass; trace any existing test that changes.

**Checkpoint**: US1 complete; P-ctl still byte-identical.

## Phase 4: User Story 2 — vendored code is visible and marked (P1)

**Goal**: FR-008, SC-006.

**Independent Test**: P-9 passes; signals and verdict match T002's rule.

- [X] T016 [P] [US2] Integration cases P-9 and P-9b in `IT`, plus one fixture per candidate signal and one first-party control (no C240, no C241). Confirm they fail at `main`.
- [X] T017 [US2] Implement the signals in `PR` (`path-segment`, `submodule-boundary` from the submodule directories, `licence-file`, `not-reached`) as C241, and C240 `waybill:vendored = true` per T002's chosen rule. If T002 found no rule, implement C241 only and mark T016's C240 assertions as the follow-up the maintainer decides.

## Phase 5: User Story 3 — a CMake-only repository describes itself (P2)

**Goal**: FR-007's root selection and override behaviour, SC-008.

**Independent Test**: P-13, P-14, P-15, P-18, P-19 pass.

- [X] T018 [P] [US3] Integration cases P-13, P-14, P-15, P-18, P-19 in `IT`. P-14 and P-15 must pass at `main` and on the branch with root and root-selection heuristic byte-identical (guards for SC-008); P-13 fails at `main`.
- [X] T019 [US3] In `RS::select_root`, drop main modules whose C55 is `cmake-project` from `main_modules` whenever any other main module exists (R2). Unit-test: CMake-only one project; CMake-only several with one at the root (`RepoRoot`); one Python module plus CMake projects (fast path, no heuristic, identical losers).

## Phase 6: Polish & cross-cutting

- [X] T020 [P] P-rep in `IT`: P-1–P-17 in one tree, scanned 20 times, identical.
- [X] T021 [P] Catalog in `MAP`: C55 `cmake-project`; C62 `source-example-directory`; C197 `not-declared`; C237 `paired.project` and the narrowed `in-tree`; new C238–C241 with the Principle V audits from `contracts/wire.md`. Add the extractors for C238–C241 in `waybill-cli/src/parity/extractors/` in the same change (the lib-target gate).
- [X] T022 [P] `docs/ecosystems.md`, CMake section: project components, ownership rank, root candidacy, scope by directory, reach, vendored signals; replace "CMake, Conan and vcpkg emit no project component" in the pip section.
- [X] T023 [P] `CHANGELOG.md` entry under Unreleased / Added.
- [ ] T024 Mutations in `specs/1618-cmake-project-components/measurements/{mutate.py,mutate-run.sh,mutations.md}`, adapted from `specs/1617-installed-record-scope/measurements/`: restore with `cp`, rebuild after the last, `^error\[E…\]` compile pattern. One each: (M1) CMake projects always root candidates; (M2) CMake rank above other main modules; (M3) no enclosing edge; (M4) no search pairing; (M5) C238 emitted as C197; (M6) test-directory projects unscoped; (M7) reach set ignored; (M8) `--include-vendored` entry not merged; (M9) conditional search makes the project Optional; (M10) `paired.project` serialised at 0 (P-ctl must turn red); (M11) installed-record and submodule collisions not dropped. Each must turn at least one test red with 0 compile errors.
- [ ] T025 Gate: `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`. Record `GATE_RESULT`, ok/non-ok counts, passed/failed, failure blocks, and an empty `git diff --stat main -- '*Cargo.toml' Cargo.lock` in `measurements/gate-log.md`.
- [ ] T026 Push. Dispatch measure-source (TheRock) against `main` with full SHAs. In `measurements/therock.md` record: project components and edges, every ownership change traced, SC-001 against T001(b), unexplained orphans (SC-002), parity, duplicates, repeat identity on two runners (SC-005), scan time median of ≥ 3 (SC-007).
- [ ] T027 Dispatch the public corpus with `regen_goldens=true` (expected byte-identical, R12) and the quality corpus at the branch with a `main` control. Trace every change component by component for the three affected targets (SC-003, SC-004, SC-008) in `measurements/corpus.md`; commit regenerated goldens if any.
- [ ] T028 Open the PR with a body citing T001, T002, T024–T027, `Closes #1616` and the attribution line. Do not merge.

## Dependencies

- T001 → T002 (sample after census, before precision) → T017.
- T001(c) → T009/T011 (directory lists).
- T003 → T004 (P-ctl golden from `main` before any source change) → T005–T007.
- **US1** (T008–T015) after Foundational. **US2** (T016–T017) after US1 and T002. **US3** (T018–T019) after US1; independent of US2.
- Polish: T020–T023 in parallel after the stories; then T024 → T025 → T026 ∥ T027 → T028.

## Parallel examples

- T008, T009, T010 (different files).
- T016 and T018 once US1 is in.
- T020, T021, T022, T023.

## Implementation strategy

**MVP is US1** (T001, T003–T015): projects exist, searches between them are edges, everything is owned and reachable, and no other ecosystem's output moves. US3 then gives CMake-only repositories a real root. US2's verdict waits on T002's measurement and may stop at signals only.
