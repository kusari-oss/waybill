# Tasks: Read find_package from every CMake file, with the conditions that reach it

**Input**: `specs/1614-cmake-tree-finds/`:
- spec.md, plan.md;
- research.md (R1–R13);
- data-model.md;
- contracts/wire.md (cases W-1 to W-16, W-ctl, W-rep).

**Tests**: the spec requires them (SC-003 to SC-006, FR-009 to FR-011). Within each story, tests are written first and must fail on `main` before the change. W-ctl is the exception: it must pass on `main` and stay passing.

**Format**: `[ID] [P?] [Story] Description`. `[P]` marks a different file with no dependency on an incomplete task.

**Shipping**: US1 alone is not shippable. Reading every file without its conditions turns 131 conditional names into runtime requirements (spec US2). US1 and US2 merge together; US3–US5 follow in the same PR.

## Phase 1: Setup

- [X] T001 Trace the plan's premises on `main` before any change, and record them in `specs/1614-cmake-tree-finds/measurements/trace.md`:
  - (a) Re-run research R7's probe tree with `main`'s binary. Record the two duplicate pairs (fetch and installed record) by PURL.
  - (b) Confirm a nested `find_package` is absent on `main` (W-1's premise).
  - (c) Confirm `vendor/` is in the walker's default skip set, which is why R7's probe moved the SDK to `sdk/`. Name the constant and file.
  - (d) Confirm `read_fetches` receives every walked `CMakeLists.txt` / `*.cmake` file, and that `read()` (no walker, the binary tier) uses `discover_cmake_files`. Cite the lines.
  - (e) Confirm where C231 is built and emitted (`scan_fs/mod.rs:154`, `generate/mod.rs:174`) as the template for C237.
  - Revert any scratch change with `cp`.
- [X] T002 [P] Create `waybill-cli/tests/scan_cmake_tree_finds.rs` (file-scope `#![cfg(unix)]`), on the pattern of `waybill-cli/tests/scan_dot_directory_manifests.rs`. Add helpers:
  - `scan(root) -> Docs`, asserting `row_all_formats` for C42, C62, C122, C232, C236 and C237;
  - `by_purl(docs, purl)`, failing on a duplicate;
  - `absent_purl`;
  - `scope`, `prop`, `edges_from(docs, ref)`;
  - `cmake(root, rel, body)`;
  - `git_repo_with_submodule(root, rel)`, which builds a real gitlink in a temporary repository for W-14c, with local paths only and no network;
  - `assert_repeatable(root, 20)`.

  Names are `waybill-fixture-*` / `waybill_fixture_*` only.
- [X] T003 [P] W-ctl, the byte-identity control (FR-009):
  - Build the tree in `waybill-cli/tests/fixtures/cmake_tree_finds/ctl/`: the root `CMakeLists.txt` and `cmake/deps.cmake` with unconditional `find_package` (with and without a version, with `EXACT`, a mixed-case name for C103), `pkg_check_modules`, and an unconditional `FetchContent_Declare` with no chain conditions.
  - Commit the masked three-format golden of `main`'s output, generated with `main`'s binary before any source change.
  - Add `w_ctl_is_byte_identical_to_main` to the test file. It compares the documents after removing C237; it must pass on `main`, and from T008 on it also asserts that C237 is present with counts that match the tree (FR-009).

## Phase 2: Foundational

- [X] T004 Register the catalog rows in `docs/reference/sbom-format-mapping.md`, with their extractors in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`, following C232 and C231:
  - **C236** `waybill:cmake-find-conditions`: per component, a sorted JSON array.
  - **C237** `waybill:cmake-find-coverage`: document scope, the object of `contracts/wire.md`.
  - Extend C232's description (conditions include the `include()` / `add_subdirectory()` chain) and C57's (names from every CMake file read for package searches).

  Run the lib target's catalog gate (`cargo test -p waybill --lib sbom_format_mapping`).
- [X] T005 Create `waybill-cli/src/scan_fs/package_db/cmake/searches.rs`, declared in `cmake.rs`'s module list:
  - **`PackageSearch`** (`data-model.md`): kind, declared name, range via `find_package_requirement`, `COMPONENTS` list, file, conditions, in-test-dir.
  - **`fn extract(cmds, blocks_seed, rel, source_path) -> (Vec<PackageSearch>, unresolved: usize)`**, walking one file's commands with a `BlockStack`, as `read_fetches` does: `function`/`macro` bodies, `elseif`/`else`.
  - **Unit tests** under `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`: blocks; a body; `elseif`/`else`; `EXACT`; a `min...<max` range; `COMPONENTS`; `${VAR}` names counted as unresolved; a call inside a comment ignored; a multi-line call.
- [X] T006 Read package searches from every walked file:
  - In `waybill-cli/src/scan_fs/package_db/cmake/fetches.rs::read_fetches`, call `searches::extract` for each parsed file in the existing loop, and return the searches beside the fetch entries.
  - In `waybill-cli/src/scan_fs/package_db/cmake.rs::read_with_downloads`, when `walked` is `Some`, build `FindPackageHit` / `PkgCheckHit` from those searches instead of `parse_find_package_calls` / `parse_pkg_check_modules_calls`, and emit through the existing `emit_find_package_entries` / `emit_pkg_check_module_entries`.
  - With `walked = None`, keep today's narrow path.
  - **Check:** W-ctl and every existing `cmake.rs` m155 unit test pass. If W-ctl differs, trace the difference (regex versus tokenizer) and record it in `measurements/trace.md` before going on.

## Phase 3: User Story 1 — every package a tree searches for is in its SBOM (P1)

**Goal**: package searches in nested files become components (FR-001), attached to their owner (R8), and counted (C237).

**Independent test**: W-1 to W-4 and W-16.

- [X] T007 [P] [US1] Write W-1 (nested `find_package` with a range), W-2 (two files, one component, both ranges), W-3 (nested `pkg_check_modules` / `pkg_search_module`), W-4 (`${VAR}` name, C237 `unresolved`) and W-16 (an edge from the owning submodule, no root anchor) in `waybill-cli/tests/scan_cmake_tree_finds.rs`. All fail on `main`.
- [X] T008 [US1] C237: build the coverage record in `cmake.rs` from the searches (`read`, `unresolved`, `components`, and so on, with the other buckets 0 until US2–US4 fill them). Carry it as C231 is carried: `scan_fs/mod.rs` field, then `generate/mod.rs`, then the three emitters.
- [X] T009 [US1] R8: in `waybill-cli/src/scan_fs/cmake_fetch_attach.rs`, attach components whose C55 mechanism is `cmake-find-package` / `cmake-pkg-check-modules` exactly as fetches attach. Rename `is_fetch` to say what it now selects.
- [X] T010 [US1] Run T007's tests and W-ctl, all green. Record any W-ctl drift in `measurements/trace.md`. The expected drift is none outside C237. R8 moves root-level searches from the graph-top fallback to declared root anchors, so check SPDX relationship provenance specifically: decide by trace whether a difference is expected or a defect, before any golden changes.

## Phase 4: User Story 2 — a conditional search is not a runtime requirement (P1)

**Goal**: the conditions that reach each file, plus the file's own blocks, scope every declaration (FR-002, FR-003).

**Independent test**: W-5 to W-10 and W-12.

- [X] T011 [P] [US2] Write W-5 (in-file `if()`), W-6 (a fetch in a conditionally included `*.cmake`; C232 gains `WIN_SDK`; required on `main`), W-7 (`BUILD_TESTING` through `add_subdirectory()`: Test, C62 `cmake-build-testing`), W-8 (conditions union along a chain), W-9 (one unconditional declaration wins), W-10 (a file reached both ways is unconditional) W-12 (an include cycle terminates) and W-17 (a search in a `function()` body: Optional, `function: <name>` in C236) in `waybill-cli/tests/scan_cmake_tree_finds.rs`. All fail on `main` except W-12's termination, which must also hold.
- [X] T012 [US2] Create `waybill-cli/src/scan_fs/package_db/cmake/reach.rs` (R1):
  - **`enum Reach { Unconditional, Conditional(BTreeSet<String>) }`**.
  - **`fn compute(scan_root, walked) -> (BTreeMap<PathBuf, Reach>, extra_files: Vec<PathBuf>)`**, built from `include()` (resolved with `links.rs::resolve_include` / `substitute`, made `pub(super)`, with source-directory inheritance) and `add_subdirectory()` (to `<dir>/CMakeLists.txt`). It is memoised, with a cycle guard.
  - **Unit tests:** an unconditional edge wins; a union; a cycle; an unresolved path contributes nothing; a chain of 3; an included file resolving relative paths against the includer's source directory.
  - `extra_files` stays empty until T026.
- [X] T013 [US2] In `read_fetches`, compute `reach` once and seed each file's `BlockStack` with its `Reach` conditions for both fetches and searches.
  - `blocks.if_conditions()` and `blocks.conditions()` must include the seed, so that `fallback::classify` and `fallback::build_testing` see it.
  - Add `BlockStack::seeded(conditions)` in `cmake/blocks.rs`, with a unit test.
- [X] T014 [US2] Scope the package-search components (R4) in `cmake.rs`, on the rule of `fetches.rs::mark` and `one_per_purl`:
  - Test, from a test directory or a `BUILD_TESTING` condition, with C62;
  - otherwise Optional, with C122 `cmake-conditional`;
  - the strongest scope across a name's declarations;
  - C236 holds the sorted union of conditions on a non-runtime component.

  Fill C237 `conditional` and `test`.
- [X] T015 [US2] Run T011's tests, T007's and W-ctl, all green.

## Phase 5: User Story 3 — a search for something the tree provides is not a second component (P1)

**Goal**: one identity per package (FR-005, FR-007, FR-008, SC-003).

**Independent test**: W-14a, W-14b, W-14c and W-15.

- [X] T016 [P] [US3] Write W-14a (fetch pair: one component, the search in its evidence), W-14b (installed record pair, plus the conditional variant: Optional with C236), W-14c (submodule pair and C57 `declared-and-used`) and W-15 (`project()` and `Config.cmake.in`: no component, no edge, C237 `in-tree`) in `waybill-cli/tests/scan_cmake_tree_finds.rs`. W-14a and W-14b fail on `main`, which emits both components (research R7 probe).
- [X] T017 [US3] R7.1, fetch pairing, in `cmake.rs`:
  - `read_fetches` also returns the set of fetch declared names (lowercased, after variable expansion: `Declared.name`).
  - A search whose name is in that set yields no component; its files are added to the fetch component's `extra_source_paths`.
  - Count C237 `paired.fetch`.
- [X] T018 [US3] R7.2 and R7.3 in a new `waybill-cli/src/scan_fs/cmake_search_pair.rs`, called from `waybill-cli/src/scan_fs/mod.rs` after `installed_lib::resolve`:
  - **Installed records.** For each `cmake-find-package` component whose lowercased name equals a `cmake-package-version` / `pkg-config` record's, remove it, and move its evidence and declarations to the record.
  - **Submodules.** Do the same for a name equal to a git submodule directory's last segment (C57's rule).
  - **Edges** to the paired component, from the search's owner (`OwnerIndex`).
  - **The record's scope.** Extend `cmake_link_attach`'s conditional decision (`attach`, lines 139–147) so that a record is Optional only when every link (C235) **and** every search (C236) is conditional, nothing else scoped it, and nothing targets it. Write C236 on the record.
  - Count C237 `paired.installed` / `paired.submodule`.
- [X] T019 [US3] R9, built in the tree:
  - The CMake registration in `cmake.rs` gains `**/*Config.cmake.in` and `**/*-config.cmake.in`; these are collected in `CMakeMarkerState` by name only and not passed to the parsers.
  - Collect `project()` first arguments in `read_fetches`'s loop.
  - The reader does **not** drop an in-tree search. It emits the component tagged in-tree (an internal marker, never emitted), and carries the in-tree name set to `cmake_search_pair.rs`.
  - The post-pass applies R7.2, then R7.3, and only then drops each component still tagged in-tree: no component, no edge.
  - Fill C237 `in-tree` (names) and `in-tree-declarations`.
  - Add a W-15 variant: a submodule directory that holds `project(waybill_fixture_sub)` and is named by a search. It pairs with the submodule (W-14c outcome), and is not counted as in-tree.
- [X] T020 [US3] R10, C57 widening:
  - In `waybill-cli/src/scan_fs/package_db/git_submodule/mod.rs:263`, take package-search names from the CMake reader's walked declarations: pass the marker files through `read_all`, or the extracted names.
  - Keep `collect_find_package_targets` for callers without a walker.
- [X] T021 [US3] Run T016's tests, plus all earlier tests and W-ctl, all green.

## Phase 6: User Story 4 — tools and CMake helpers are not dependencies (P2)

**Goal**: FR-006, research R6.

**Independent test**: W-13.

- [X] T022 [P] [US4] Write W-13 in `waybill-cli/tests/scan_cmake_tree_finds.rs`: `Threads`, `Git`, `PkgConfig` and `Doxygen` yield nothing; Python3 as `Interpreter` alone yields nothing; Python3 with `Development` yields a component; `CUDAToolkit` yields a component; C237 `tools` is 5.
- [X] T023 [US4] In `searches.rs`, add `const TOOLS` (`threads`, `pkgconfig`, `git`, `doxygen`, `pythoninterp`) and `fn is_tool(search)`, with the Python rule: `python` / `python3` whose `COMPONENTS` has `Interpreter` and none of `Development`, `Development.Module`, `Development.Embed`. Unit tests cover each name and each Python shape. Apply it before pairing, and count C237 `tools`.
- [X] T024 [US4] Run T022 and every earlier test, all green.

## Phase 7: User Story 5 — a file reached only by include() is read (P3)

**Goal**: FR-004, research R2.

**Independent test**: W-11.

- [ ] T025 [P] [US5] Write W-11 in `waybill-cli/tests/scan_cmake_tree_finds.rs`: `if(WIN_SDK) include(${CMAKE_CURRENT_SOURCE_DIR}/CMakeLists_wsl.txt) endif()`, the file holding `find_package(waybill_fixture_wsl)`. Expected Optional, C236 `["WIN_SDK"]`. Fails on `main`.
- [ ] T026 [US5] `reach::compute` returns `extra_files`: files `include()` reaches inside the scan root that are not in `walked`. `read_fetches` parses them with the walked files, under their `Reach`.
- [ ] T027 [US5] Run T025 and every earlier test, all green.

## Phase 8: Polish and verification

- [ ] T028 Determinism, W-rep: in `waybill-cli/tests/scan_cmake_tree_finds.rs`, scan the W-5..W-15 tree 20 times and assert identical documents. Confirm every W-case asserts in all three formats through `row_all_formats`.
- [ ] T029 Mutations in `specs/1614-cmake-tree-finds/measurements/mutate.py` and `mutate-run.sh`. Use exact-anchor replacement, revert with `cp`, and rebuild after the last restore. Record the results in `measurements/mutations.md`. Each mutation must turn a named test red:
  - **Reach:** M1 an unconditional edge no longer wins; M2 union replaced by the first path's conditions; M3 a cycle treated as conditional.
  - **Reading:** M4 back to the narrow file set; M5 include-reached files not read; M6 the reach seed not applied to fetches; M7 search scope ignores conditions.
  - **Tools:** M8 the tool list emptied; M9 the Python rule ignores `Development`.
  - **Pairing:** M10 fetch pairing removed; M11 installed-record pairing removed; M12 submodule pairing removed; M13 in-tree checked before pairing.
  - **In-tree and C57:** M14 the in-tree rule removed; M15 the `*Config.cmake.in` patterns removed; M17 C57 back to the narrow set.
  - **Edges:** M16 search attachment removed.
  - **Record scope:** M18 the record's scope ignores searches.
- [ ] T030 [P] `docs/ecosystems.md`, CMake section:
  - package searches read from every file;
  - conditions from `if()`, `include()` and `add_subdirectory()`;
  - the tool list;
  - pairing, and the in-tree rule with #1616.

  `CHANGELOG.md` gets `### Fixed` / `### Added` entries for #1591 and #1605, including the duplicates fixed on `main` (R7).
- [ ] T031 Walker audit (`bash scripts/check-walker-audit.sh`) and the gate (`./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`), with no `+toolchain`. Record `GATE_RESULT`, the `ok` count, passed/failed and the failure blocks in `measurements/gate-log.md`. Also record that `git diff main -- Cargo.toml '*/Cargo.toml' Cargo.lock` is empty (FR-013).
- [ ] T032 TheRock, CI only: two measure-source runs at the branch head against `main` (full SHAs). Record in `measurements/therock.md`:
  - **SC-001:** `cmake-find-package` components against `predict.py` (110 ± the Python calls), each difference traced by name.
  - **SC-002:** the conditional-only names are non-runtime, including the 6–9 fetch components named in the spec.
  - **SC-003:** no name with two components; 0 duplicate `bom-ref`s.
  - **SC-004:** 0 unreachable components without an I7 marker.
  - **SC-005:** `waybill sbom parity-check` on both sides; no new gap; C236 and C237 pass.
  - **SC-006:** the two runners' documents identical apart from timestamps.
  - **SC-009:** the interleaved ratio, ≤ 1.05×.
- [ ] T033 Public corpus at the branch (`repeat=2`): trace every golden change by component and annotation, then regenerate the goldens through the workflow. Record in `measurements/corpus.md`. C237 is expected in every golden whose tree has a CMake package search; list those targets, and treat any other change as needing its own trace.
- [ ] T034 Quality corpus against a `main` control: trace every changed target and re-centre only traced bounds in `xtask/corpus/quality-corpus.toml`. Record in `measurements/corpus.md`.
- [ ] T035 Open the PR, with a body citing T029 and T031–T034, `Closes #1591`, `Closes #1605`, and the attribution line. Do not merge.

## Dependencies

- **T001 → T003 → T006.** T003 needs `main`'s binary before any change.
- **T004, T005 → T006 → US1 (T007–T010) → US2 (T011–T015).** US2 needs the search extraction. US1 and US2 ship together.
- **US3 (T016–T021)** needs US2: pairing reads scoped components, and the combined scope rule needs C236.
- **US4 (T022–T024)** needs only T005/T006; it can run beside US2/US3.
- **US5 (T025–T027)** needs T012 (`reach.rs`).
- **Polish (T028–T035)** after every story. T029 needs every mutation target to exist. T032–T034 need the branch pushed.

## Parallel opportunities

- T002 and T003, then T004 and T005, are independent files.
- The test-writing tasks T007, T011, T016, T022 and T025 each touch only the test file, so they run one at a time against each other. Each can run beside its story's code tasks in other files.
- US4 (T022–T024) can proceed in parallel with US3 after T006.
- T030 can be written during T029's run.

## Implementation strategy

- **MVP = US1 + US2,** together: reading and conditions. On TheRock that is about 110 new components, 85 of them non-runtime (research R13).
- **Then US3,** which removes the duplicate identities that exist on `main` (R7).
- **Then US4 and US5.**
- **Each story ends** with every earlier test and W-ctl green.
