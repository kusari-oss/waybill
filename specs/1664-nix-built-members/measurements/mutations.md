# Mutations (FR-008, SC-004, T015)

Script: `measurements/mutate.py`. Each mutation makes one exact string replacement, then runs the 14 tests (`nix_refs::tests`, `nix_admit_tests`, and the `nix_built_membership` binary) under nextest with `--no-fail-fast`. The file is then restored from a `cp` backup and checked with `cmp`. The final `cmp` of both files against the backups passed.

Baseline before the run: 14 of 14 pass.

| ID | Mutation | Result | Killed by |
|---|---|---|---|
| M2 | accept a quoted path (`"./x"`) | killed | `non_literal_paths_admit_nothing` |
| M3 | skip comment stripping | killed | `comments_admit_nothing`, `a_package_nix_builds_is_a_member_and_nothing_else_is` (the commented-out `waybill-fixture-attr` call) |
| M4 | drop the `${` check | killed | `non_literal_paths_admit_nothing` (`./${name}`) |
| M5 | drop the root-containment check | killed | `resolution_outside_the_root_is_refused` |
| M6 | omit C248 | killed | `a_package_nix_builds_is_a_member_and_nothing_else_is` |
| M7 | omit the `.nix` files from C18 | killed | `a_package_nix_builds_is_a_member_and_nothing_else_is` |
| M8 | let a directory `cabal.project` already declares count as Nix-admitted | killed | `a_package_nix_builds_is_a_member_and_nothing_else_is` (lib carries no C248), `nix_files_change_nothing_when_nothing_is_rejected` |
| M9 | drop the lazy guard | killed | `nothing_rejected_reads_no_nix_file` (the `.nix` listing closure was called) |
| M10 | stop accepting newlines between tokens | killed | `a_call_split_over_lines_is_recognised_with_its_line` |
| M11 | require a bare `callCabal2nix` with no attribute prefix | killed | 5 tests, including the integration test |
| M12 | put C248 on every component of the manifest, not just the main module | killed | `a_package_nix_builds_is_a_member_and_nothing_else_is` (the dependency carries no C248) |
| M13 | drop the quoted-name requirement | killed | 4 tests |

**M1, retired as equivalent.** The first draft had a separate check that the character after `callCabal2nix` is not an identifier character, meant to exclude `callCabal2nixWithOptions`. Removing that check changed no test result, because the quoted-name requirement already excludes `WithOptions`. The check was dead code, so it was removed. M13 mutates the requirement that does the excluding.

**A finding from the first run.** The first mutation run reported `non_literal_paths_admit_nothing` failing under every mutation, including ones in another file. The unmutated baseline was red: a path ended at `{`, so `./${name}` became `./$` and was accepted. That was a bug in the code, fixed in `nix_refs.rs` (research R3). Both runs above started from a green baseline.

**SC-005 (no Nix at scan time).** Two pieces of evidence:
- `grep -cE 'Command|process::' nix_refs.rs` returns 0;
- the integration test scans with `PATH` emptied, so no `nix` binary could be reached.
