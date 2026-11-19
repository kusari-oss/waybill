# T018: mutations

`mutate.py` applies each by exact string replacement (each anchor occurs once). `mutate-run.sh` runs `installed_record_scope` and the `cmake_search_pair::` unit tests, restores the file with `cp` from `HEAD`, and rebuilds after the last mutation. Run at `ef9b898a`.

| # | Mutation | Rule | Failed binaries | Compile errors | Restored | Caught by |
|---|---|---|---|---|---|---|
| M1 | `ships_library_file` always false | FR-001 | 2 | 0 | yes | I-1, I-2, I-4, I-9, I-10, three unit tests |
| M2 | a shipping record put in `required` | FR-005 | 2 | 0 | yes | I-10 (the link half is switched off), I-1, I-2, I-9, two unit tests |
| M3 | `conditions_only` writes no C236 | FR-002 / FR-006 | 1 | 0 | yes | I-1, I-2, I-9, I-10 |
| M4 | Test records made Optional | FR-004 | 1 | 0 | yes | I-5, I-7 |
| M5 | derivation precedence reversed | FR-004 | 1 | 0 | yes | `s1617_test_pairings` (unit only) |
| M6 | `.a` not a library file | FR-001 | 2 | 0 | yes | I-9, `s1617_library_file_names` |
| M7 | a record with an Optional pairing made Test | FR-004 | 1 | 0 | yes | `s1617_test_pairings` (unit only) |

**All 7 are caught.**

**M5 and M7 are caught only by the unit test.**
- **M5.** No integration case has a record whose pairings mix `source-test-directory` and `cmake-build-testing`.
- **M7.** Spec 1614's name rule gives every search of a name one scope (research R4), so the pairings of a record from one tree are either all Test or all Optional. A record with one Test and one Optional pairing cannot come from a single name, and no integration case can construct it.
