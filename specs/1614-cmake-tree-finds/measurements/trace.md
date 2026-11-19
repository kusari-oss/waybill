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
