# T016: mutations

`mutate.py` applies each by exact string replacement (each anchor occurs once). `mutate-run.sh` runs `scan_nested_req_scope` and the `requirements_txt::` unit tests, restores the file with `cp` from `HEAD`, and rebuilds after the last mutation. Run at `528288b0`, plus the W-10b test (committed next).

| # | Mutation | Rule | Failed binaries | Compile errors | Restored | Caught by |
|---|---|---|---|---|---|---|
| M1 | drop `doc` from the docs words | FR-001 | 2 | 0 | yes | `m670_scope_classify_filename_signals`, `s1613_the_selector_reads_labelled_names_only`, W-4 |
| M2 | drop the long forms | FR-006 | 2 | 0 | yes | the filename table, the selector table, W-5 |
| M3 | drop `tests` from the test words | FR-006 (R1) | 2 | 0 | yes | the filename table, the selector table, W-5 |
| M4 | drop the docs directory names | FR-002 | 2 | 0 | yes | both `s1615_*` unit tests, W-1/2, W-7, W-8, W-10 |
| M5 | drop the test directory names | FR-002 | 2 | 0 | yes | `s1615_nearest_…`, W-3, W-7, W-13 |
| M6 | farthest ancestor first | FR-003 | 2 | 0 | yes | `s1615_nearest_…`, W-7, W-8 |
| M7 | dot-directories searched before docs/test names | FR-003 | 2 | 0 | yes | `s1615_nearest_…`, W-8 |
| M8 | the scan root's own name counts | FR-004 | 2 | 0 | yes | `s1615_the_scan_root_bounds_the_search`, W-12 |
| M9 | directory names matched by prefix | FR-005 | 2 | 0 | yes | `s1615_nearest_…`, W-14 |
| M10 | the reader keeps the old word list | research R2 | 2 | 0 | yes | the selector table, W-4, W-5 |
| M11 | `ci_scope` keeps C155 on a runtime component | FR-007 (no C155) | 1 | 0 | yes | W-10b |
| M12 | `merge_lifecycle_scope` lets Optional beat a runtime declaration | FR-007 (scope) | 1 | 0 | yes | W-10, W-10b |

**All 12 are caught.**

**M11 survived the first run.** W-10's runtime declaration was always merged first, so its label never reached the merged component and `ci_scope`'s removal was never exercised. W-10b puts the docs file first (`a/docs/sphinx/` before `b/`). It passes on the real code and fails under M11.

The first run's compile-error column was also wrong: the runner's pattern matched cargo's `error: test failed` line. Every mutation compiled, as the red test names show. The pattern is now `^error\[E…\]` and the table is from the re-run.
