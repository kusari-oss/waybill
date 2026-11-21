# T006: the control before any reader change

The cases ran at the commit that adds them, before any reader calls `sanitize_declared_url`. T005's helper is present but unused by D-row readers, so their output still equals `main`'s.

Command: `cargo test -p waybill --test declared_url_credentials -- --skip corpus_harness`.

**Result: 5 passed (the guards G-1 to G-5, each with its positive control), 22 failed.**

Every D-case fails on its first credential check. The credential reaches the CycloneDX document, which proves the fixture reaches its field. No D-case passed, so no fixture had to be fixed.

| Case | Failure before the fix |
|---|---|
| `d01_cmake_git_repository` | wbfxuser occurs in the CycloneDX |
| `d01u_cmake_url` | wbfxuser occurs in the CycloneDX |
| `d02_cmake_dataset_download` | wbfxuser occurs in the CycloneDX |
| `d03_cmake_github_repository` | wbfxuser occurs in the CycloneDX |
| `d04_bazel_http_archive` | wbfxuser occurs in the CycloneDX |
| `d05_uv_lock_index` | wbfxuser occurs in the CycloneDX |
| `d06_uv_lock_git_source` | wbfxuser occurs in the CycloneDX |
| `d07_flake_lock_input` | wbfxuser occurs in the CycloneDX |
| `d08_pants_pex_lockfile` | wbfxuser occurs in the CycloneDX |
| `d09_pants_coursier_lockfile` | wbfxuser occurs in the CycloneDX |
| `d12_cocoapods_git_source` | wbfxuser occurs in the CycloneDX |
| `d13_dart_hosted_repository` | wbfxuser occurs in the CycloneDX |
| `d13g_dart_git_source` | wbfxuser occurs in the CycloneDX |
| `d14_composer_self_hosted_repository` | wbfxuser occurs in the CycloneDX |
| `d14v_composer_vcs_source` | wbfxuser occurs in the CycloneDX |
| `d15_elixir_lockfile_git` | wbfxuser occurs in the CycloneDX |
| `d15d_elixir_declared_git` | wbfxuser occurs in the CycloneDX |
| `d16_erlang_lockfile_git` | wbfxuser occurs in the CycloneDX |
| `d16d_erlang_declared_git` | wbfxuser occurs in the CycloneDX |
| `d17_requirements_direct_url` | wbfxuser occurs in the CycloneDX |
| `d19_yocto_src_uri` | wbfxuser occurs in the CycloneDX |
| `s1_ssh_git_user_changes_that_url_alone` | a CycloneDX component property is `ssh://git@git.example/org/waybill-fixture-ssh.git`, not `ssh://git.example/org/waybill-fixture-ssh.git` |

## Unit-level cases

Command: `cargo test -p waybill --bin waybill s1640_a_credentialled`. 0 passed, 2 failed:
- `scan_fs::binary::entry::tests::s1640_a_credentialled_git_source_is_stripped` (D-18): the VCS URL kept the userinfo.
- `scan_fs::package_db::nix::closure::emit::m1282_tests::s1640_a_credentialled_derivation_source_is_stripped` (D-10, D-11): the `vcs` location kept the userinfo.

D-10, D-11 and D-18 are unit-level, not directory scans:
- `cargo_auditable_packages_to_entries` and `vcs_source`/`source_url` are the only producers of those fields;
- `vcs_source` alone feeds both the closure `vcs` reference and the Hackage `vcs_url` qualifier (`closure_identity.rs`, the single caller);
- a `nix_stub` scan needs a wiremock Hackage and a GHC-built closure to reach the qualifier.
