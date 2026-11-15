# Mutations (spec 1531, task T034)

Each mutation was applied to the source and the four integration targets
were run with `--no-fail-fast`: `installed_lib_us1`, `installed_lib_us2`,
`installed_lib_us3` and `installed_lib_image`. The file was then restored
with `cp` from a copy saved before the mutation, and the restore was
confirmed with `cmp`. Every mutation fails at least one test, and each
fails for the reason that test exists.

| # | Mutation | Failing tests | Failure |
|---|---|---|---|
| M1 | The post-pass reads the shared claim set (`claimed_paths`), not the OS-only one | `us1_every_declared_library_gets_its_exact_version`, `sc009_an_unowned_copy_of_the_dpkg_file_is_emitted` | The wheel's `waybill-fixture-v@5.0` disappears, because pip's `RECORD` claim is taken as OS ownership. `owned-by-os-package` is 1 where it should be 0. |
| M2 | `.pc` variable expansion skipped | `us1_…`, `sc002_requires_become_edges_…` | `waybill-fixture-a` (`Version: ${version}`) is missing, and its edges go with it. |
| M3 | A `Requires` comparison operator no longer consumes its version | `sc002_requires_become_edges_…` | `unresolved-requires` is 2, not 1: `1.0` was taken as a module name. |
| M4 | Installed-library entries appended after the binary entries instead of before them | `sc003_a_pc_and_an_embedded_version_merge` | The merged zlib has tier `analyzed`, not `deployed`: the embedded-version entry won the merge. |
| M5 | Same-prefix-first `Requires` resolution dropped | `sc002_requires_become_edges_…` | `waybill-fixture-a` loses its edge to `waybill-fixture-c@2.0`, because two prefixes' `c` now make it ambiguous. |
| M6 | A computed CMake `PACKAGE_VERSION` accepted | `us1_…` | `pkg:generic/waybillfixturex@${WAYBILL_FIXTURE_X_VERSION}` is emitted. |
| M7 | Files read at the walker's path, without root-relative symlink resolution | `a_symlink_resolves_inside_the_root_never_on_the_host` | Versions `9.9.9` (outside the root, through a relative link) and `1.1.1` (the host's file, through an absolute link) are read. This also shows the shared walker does dispatch both links, so the resolution is what stops them. |
| M8 | OS-ownership check removed | `an_image_scan_reads_unowned_files_and_skips_dpkg_owned_ones`, `a_homebrew_keg_owns_its_pkgconfig_files`, `us1_…` | The dpkg-owned and Homebrew-keg `.pc` files are emitted. |

## A first attempt that did not test what it claimed

The first run of M1 to M4 reported `failing=[]` for every mutation, which
would have read as "no test catches this". Two mistakes in the harness
caused it, and that output was discarded:

- The test-target arguments were held in a zsh variable. zsh does not
  word-split an unquoted variable, so cargo received the four `--test`
  flags as one argument and rejected it.
- Once that was fixed under bash, the log parser matched `test … FAILED`
  lines, which `cargo test -q` does not print. Without `--no-fail-fast`,
  cargo also stopped at the first failing test binary.

The harness now runs under bash with `--no-fail-fast` and reads the
`---- <name> stdout ----` blocks. The results above come from it.

The unit tests in `installed_lib/{pkgconfig,cmake_version,location,resolve}.rs`
(42 tests) cover the same rules at function level, and passed before and
after.
