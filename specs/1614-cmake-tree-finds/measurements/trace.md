# T001 trace: premises on `main`

Code identical to `main` `cd2af654`: `git diff main -- waybill-cli waybill-common` was empty when this was taken.

## (a) Duplicate pairs on `main` (research R7 probe)

| Pair | Components on `main` |
|---|---|
| fetch | `pkg:generic/waybill_fixture_fmt` (`cmake-find-package`) **and** `pkg:github/waybill-fixture/fmt@v1.0.0` (`cmake-fetchcontent-git`, excluded) |
| installed record | `pkg:generic/waybill_fixture_inst` (`cmake-find-package`) **and** `pkg:generic/waybill_fixture_inst@4.5.6` (`cmake-package-version`) |

Both pairs sit in the narrow file set (the root `CMakeLists.txt`), so the duplicates predate this feature.

## (b) A nested `find_package` is absent on `main`

In the same probe, `projects/app/CMakeLists.txt` declared `find_package(waybill_fixture_nested 2.0)` and `find_package(waybill_fixture_lib)`. Neither appears. The only search components are the 4 from the root file.

## (c) `vendor/` is skipped by default

`should_skip_default_descent` in `waybill-cli/src/scan_fs/package_db/project_roots.rs` (lines 38–54) lists `vendor`. A version file under `vendor/` is never seen, which is why the probe moved it to `sdk/`.

## (d) Which files each path reads

`waybill-cli/src/scan_fs/package_db/cmake.rs`:
- **`read_with_downloads` with a walker:** `discover_cmake_files` (line 201) gives the narrow set, used for `find_package`, `pkg_check_modules`, `file(DOWNLOAD)` and vendored directories. Fetches and links read `walked.unwrap_or(cmake_files)` (lines 244–248).
- **`read()` without a walker:** calls `read_with_downloads(…, None)`, so fetches read the narrow set too. Its only caller is the binary tier (`waybill-cli/src/scan_fs/binary/mod.rs:366`), for milestone 109's source binding. It stays on the narrow set (plan R3).
- **`finalize()`**, the shared-walker path called from `package_db/mod.rs:3162`, passes `Some(marker.files)`.

## (e) C231, the template for C237

- **Built** in `read_with_downloads`'s `FetchCoverage`.
- **Carried** as `scan_fs::mod.rs` `cmake_fetch_coverage: Option<String>` (line 154), then `generate::mod.rs` `cmake_fetch_coverage: Option<&str>` (line 174), then into the three emitters.

C237 follows the same route.

## T006 findings

**W-ctl passes on the new path.** Proved by mutation: with the parser's `find_package` arm renamed, W-ctl fails (its components disappear), so the control exercises the tokenized reading, not the old regexes.

**Three milestone-156 tests failed. One was a real regression; two encoded the narrow set.**

| Test | Cause | Resolution |
|---|---|---|
| `cmake_walker_depth_deep_emission` | Its fixture is `cmake/modules/vendor/Extra.cmake`. The narrow set walked `cmake/**` with its own walker, `vendor/` included; the shared walker skips `vendor/` (`should_skip_default_descent`). A search the old reader saw disappeared. **FR-009 regression.** | Package searches also read every narrow-set file the walker did not hand the reader, through the same tokenized path, with conditions (`searches::read_file`). Fetches still read the walked files only, unchanged. |
| `cmake_walker_depth_exclude_path` | Asserted `find_package(NotEmitted)` in `src/CMakeLists.txt` is not read (milestone 156 FR-017). | FR-001's intended change: now 1. The test's `--exclude-path` assertions are unchanged and still pass. |
| `cmake_walker_depth_third_party_opt_in` (default case) | Asserted a search at `third_party/somedep/cmake/` depth 3 is not read without `--cmake-third-party-recursive` (milestone 156 FR-019). | FR-001's intended change: now read by default. The flag still governs vendored-directory and `file(DOWNLOAD)` discovery, and its opt-in test is unchanged. |

## T010: US1 against W-ctl

After T007–T009, W-ctl is still identical to `main`'s golden once C237 is removed. Research R8 moves root-level package searches from the graph-top fallback to declared root anchors; the root's edges to them and their SPDX relationships came out unchanged, so T010's provenance check found no drift.

One unit test asserted spec 1538's rule that only fetches attach: `cmake_fetch_attach::tests::only_fetch_components_are_attached`. It is rewritten for R8 as `fetches_and_package_searches_are_attached_and_nothing_else`: a package search attaches, and a `cmake-package-version` record still does not.

## T012: a defect in the first reach algorithm

The first `reach.rs` computed reach as a memoised depth-first fold. The unit test `a_cycle_terminates_and_an_unresolved_path_contributes_nothing` failed: `a.cmake`, included only under `if(A)`, came out unconditional. A back edge (`b.cmake` → `a.cmake` while `a` was still being computed) returned "unconditional" and absorbed the whole cycle. `b`'s reach had also been memoised from `a`'s partial state.

**Replaced by a fixed point:**
- files nothing reaches start unconditional;
- every other file takes the union over edges from sources whose reach is known;
- "unconditional" absorbs;
- the iteration is capped at one pass per file plus one;
- a file still unknown afterwards is reached only by a cycle with no way in, and counts as unconditional.

All four reach unit tests pass. Contract W-12 (a cycle reached from an unconditional root) stays required.

## T021: US3 and existing tests

One existing test asserted the duplicate that FR-005 removes. `cmake_find_package_dedup_integration::cmake_find_package_requirement_and_fetch_in_different_directories_stay_apart` expected a `find_package(openssl 1.1.0)` in the root `CMakeLists.txt` and `FetchContent_Declare(openssl URL …-1.1.0.tar.gz)` in `cmake/deps.cmake` to stay two components: the milestone-191 reconciler only matches within one directory. Research R7.1 pairs them in the reader, in any directory. Renamed `…_pair`, it now expects one component, the fetch. The same-directory test beside it is unchanged and passes.

All 3,778 `scan_fs` unit tests pass, as do the CMake, installed-library, submodule, TheRock, graph, orphan, root and declared-requirements integration suites.

## T027: US4 and an existing test

`cmake_find_package_kamailio_shape_integration` asserted at least 5 `cmake-find-package` components from a fixture whose five searches include `find_package(Threads REQUIRED)`. FR-006 (US4) makes `Threads` a CMake helper with no component, so the fixture gives 4. The test now asserts the four library searches by PURL, and that `pkg:generic/threads` is absent. The failure appeared in the run after US5's commit (`de243e74`); the fix follows it.

## T032 round 1 (`93fc2cb0`): two fixes

**SC-009 failed:** TheRock scan time was 1.114× and 1.082× `main` (runs 44463579565, 44463591426). The scan logs put the extra time in the CMake reader's phase: walker completion to `cmake: fetch declarations` took 2.95 s against 1.08 s on `main`; the rest of the timeline matched. Research R12 planned for the reach pre-pass to share the fetch reader's parsed commands, and the first implementation did not. It had four costs `main` doesn't pay:
- **a second tokenization** of every walked CMake file in `reach.rs` (more than 6,000 on TheRock);
- **a whole-list clone** of each file's commands on every traversal;
- **`fs::canonicalize` per walked file**, twice: once in `reach.rs`, once in the narrow-set check;
- **a quadratic `Vec::contains`** over the `CMakeLists.txt` list.

Now each file is tokenized once into `Rc`-shared commands, `reach::compute` takes them, walked paths are rebased onto the canonical root by prefix swap, the narrow-set check compares paths as given, and the lookup uses a set. W-ctl gained an assertion that C237 `read` is exactly 7, which would catch a file read twice. Local timing on this machine was too noisy to judge (0.8 s to 5 s for one binary on one tree); the measurement is TheRock in CI.

**FR-006 dropped a library search.** pytorch's `find_package(Python COMPONENTS Interpreter OPTIONAL_COMPONENTS ${PYTHON_COMPONENTS})` was taken for an interpreter-only tool search, but `PYTHON_COMPONENTS` is `Development.Module` (`cmake/Dependencies.cmake:866`). A component list holding a variable is no longer taken to lack `Development` (Principle III). Unit-tested in `tools_and_python_as_an_interpreter`.
