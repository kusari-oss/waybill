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
