# Site review (T007)

All 23 sites from `check-before.txt`, reviewed by hand. Every one is **dropped**: it takes `NamedTempFile::new().{expect|unwrap}().path().to_path_buf()` in one statement, passes the path as `--output`, and the file name has no extension. The format comes from `--format` or the scan default, never from the name. Fixed as `let <var>_dir = tempfile::tempdir()…; let <var> = <var>_dir.path().join("out");`, with the guard bound until the output is read.

| site | helper | reads output | verdict | note |
|---|---|---|---|---|
| `cargo_workspace_root_lifecycle_m200.rs:38` | `scan_path` | yes | dropped → held |  |
| `ipk_license_splitter_m202.rs:27` | `scan_path_with_format` | yes | dropped → held |  |
| `npm_phantom_edges.rs:113` | `scan_fixture` | yes | dropped → held |  |
| `orphan_reason_expand.rs:63` | `scan_fixture` | yes | dropped → held |  |
| `pnpm_multi_version.rs:126` | `scan_fixture` | yes | dropped → held |  |
| `pnpm_multi_version_audit.rs:32` | `scan` | yes | dropped → held |  |
| `scan_bazel.rs:19` | `scan_fixture` | yes | dropped → held |  |
| `scan_cmake.rs:16` | `scan_fixture` | yes | dropped → held |  |
| `scan_cmake_findpackage_negative.rs:23` | `scan_fixture` | yes | dropped → held |  |
| `scan_cmake_vendored.rs:19` | `scan_fixture` | yes | dropped → held |  |
| `scan_conan.rs:19` | `scan_fixture` | yes | dropped → held |  |
| `scan_gem.rs:431` | `scan_path_format` | yes | dropped → held |  |
| `scan_image.rs:75` | `scan_image` | yes | dropped → held |  |
| `scan_image.rs:408` | `image_subject_records_the_main_modules_it_outranked_issue_1170` | no (stderr) | dropped → held | test reads stderr only; the output path was still unreserved |
| `scan_maven.rs:837` | `scan_path_format` | yes | dropped → held |  |
| `scan_npm.rs:50` | `output_path_hint` | yes | dropped → held | path read later through the LAST_OUT_PATH thread-local; the TempDir is kept beside it in LAST_OUT_DIR until the next scan |
| `scan_npm.rs:389` | `cli_local_fixture` | yes | dropped → held |  |
| `scan_pip.rs:15` | `scan_path` | yes | dropped → held |  |
| `scan_polyglot_monorepo.rs:19` | `scan` | yes | dropped → held |  |
| `scan_python.rs:31` | `scan` | yes | dropped → held |  |
| `scan_vcpkg.rs:18` | `scan_fixture` | yes | dropped → held |  |
| `scan_vcpkg_conan_cross.rs:20` | `scan_fixture` | yes | dropped → held |  |
| `spdx3_annotation_dedup.rs:37` | `scan_fixture` | yes | dropped → held |  |

23 sites in **21** files (`scan_image.rs` and `scan_npm.rs` have two each). Research R1 said 22 files, a miscount, corrected there.
