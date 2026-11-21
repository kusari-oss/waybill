# T022: mutations

Script: `mutate.py` (this directory), run as `python3 -I -B mutate.py <repo> <outdir>`. For each mutation it:
- backs up the file with `cp`;
- applies one exact string replacement (each anchor checked to occur once before the run);
- runs the killing test;
- restores with `cp`.

A log containing `error[E…]` is classified as a compile error, never as a kill. Run at `34f1f1c9`; `git status` showed no source file modified after the run, and the next build rebuilt from the restored source (the whole integration file then passed, 171 of 171).

**28 of 28 killed.** Each `sanitize_declared_url`/`sanitize_declared_text` call site is replaced by its raw argument (M01–M24). The helper's own behaviour is mutated in M25–M28.

| Mutation | Result |
|---|---|
| M01 cmake fetch | KILLED by d01_cmake_git_repository |
| M02 cmake dataset | KILLED by d02_cmake_dataset_download |
| M03 bazel archive | KILLED by d04_bazel_http_archive |
| M04 bazel remote | KILLED by d04g_bazel_git_repository |
| M05 uv sources | KILLED by d06u_uv_lock_url_source |
| M06 uv artifact (S3) | KILLED by scan_fs::package_db::pip::uv_lock::tests::m800_private_index_credentials_are_stripped |
| M07 flake | KILLED by d07_flake_lock_input |
| M08 closure source_url | KILLED by scan_fs::package_db::nix::closure::emit::m1282_tests::s1640_a_credentialled_derivation_source_is_stripped |
| M09 closure vcs_source | KILLED by scan_fs::package_db::nix::closure::emit::m1282_tests::s1640_a_credentialled_derivation_source_is_stripped |
| M10 pants pex | KILLED by d08_pants_pex_lockfile |
| M11 pants coursier | KILLED by d09_pants_coursier_lockfile |
| M12 cocoapods | KILLED by d12_cocoapods_git_source |
| M13 dart | KILLED by d13g_dart_git_source |
| M14 composer | KILLED by d14_composer_self_hosted_repository |
| M15 elixir lock | KILLED by d15_elixir_lockfile_git |
| M16 elixir declared | KILLED by d15d_elixir_declared_git |
| M17 erlang lock | KILLED by d16_erlang_lockfile_git |
| M18 erlang declared | KILLED by d16d_erlang_declared_git |
| M19 pip direct url | KILLED by d17_requirements_direct_url |
| M20 cargo-auditable | KILLED by scan_fs::binary::entry::tests::s1640_a_credentialled_git_source_is_stripped |
| M21 yocto | KILLED by d19_yocto_src_uri |
| M22 swift | KILLED by d22_swift_package_resolved_location |
| M23 requirement ranges | KILLED by d20_npm_url_dependency_range |
| M24 pyproject constraint | KILLED by d21_pyproject_direct_url_constraint |
| M25 splice reverted | KILLED by i3_stripping_keeps_the_rest_of_the_url |
| M26 ssh exception removed | KILLED by s1_ssh_git_user_changes_that_url_alone |
| M27 warning removed | KILLED by d05_uv_lock_index |
| M28 text sanitiser inert | KILLED by d21_pyproject_direct_url_constraint |

## The cases written after their fix

D-4g, D-6u and I-1 to I-3 were written after the code they test, so none of them had a failing run before its fix. These mutations stand in for one:
- **D-4g:** M04.
- **D-6u:** M05.
- **I-3:** M25.
- **I-1 and I-2:** M01 was applied by hand and both were run. Both failed (`i1_a_credentialled…`: 0 passed, 1 failed; `i2_a_credentialled…`: 0 passed, 1 failed). The file was then restored with `cp` and `git diff --quiet` confirmed it clean.
