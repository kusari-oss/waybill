# T029 mutations

Run with `measurements/mutate-run.sh` from the repository root, at `d123b615` plus the runner commit. Each mutation was applied by exact string replacement (`mutate.py`, which refuses an anchor that does not occur exactly once), and then:
- the integration file, the `cmake::` unit tests and the `cmake_fetch_attach` unit tests ran;
- the file was restored with `cp` from `git show HEAD:<file>`, and checked with `cmp`;
- the binary was rebuilt after the last restore.

| Mutation | What it breaks | Failed binaries | Compile errors | Restored | Tests turned red |
|---|---|---|---|---|---|
| M1 | an unconditional edge no longer wins | 2 | 0 | yes | `an_unconditional_edge_wins…`, W-10 |
| M2 | union replaced by the first path's conditions | 1 | 0 | yes | `an_unconditional_edge_wins_and_conditions_union` |
| M3 | a cycle nothing enters counts as conditional | 1 | 0 | yes | `a_cycle_nothing_enters_is_unconditional` |
| M4 | back to the narrow file set | 1 | 0 | yes | W-1, W-2, W-3, W-4, W-5, W-7, W-8, W-9, W-11, W-13, W-14a, W-14b (both), W-14c, W-15 (both), W-16, W-17 |
| M5 | files reached only by `include()` not read | 1 | 0 | yes | W-11 |
| M6 | the reach seed not applied | 1 | 0 | yes | W-6, W-7, W-8, W-11 |
| M7 | search scope ignores conditions | 1 | 0 | yes | W-5, W-8, W-11, W-14b (conditional), W-17 |
| M8 | the tool list emptied | 2 | 0 | yes | `tools_and_python_as_an_interpreter`, W-13 |
| M9 | the Python rule ignores `Development` | 2 | 0 | yes | `tools_and_python_as_an_interpreter`, W-13 |
| M10 | fetch pairing removed | 1 | 0 | yes | W-14a |
| M11 | installed-record pairing removed | 1 | 0 | yes | W-14b (both) |
| M12 | submodule pairing removed | 1 | 0 | yes | W-14c, W-15 (submodule variant) |
| M13 | in-tree checked before pairing | 1 | 0 | yes | W-15 (submodule variant) |
| M14 | the in-tree rule removed | 1 | 0 | yes | W-15 |
| M15 | the `*Config.cmake.in` walker patterns removed | 1 | 0 | yes | W-15 |
| M16 | search attachment removed | 2 | 0 | yes | `fetches_and_package_searches_are_attached_and_nothing_else`, W-16 |
| M17 | C57 not widened | 1 | 0 | yes | W-14c |
| M18 | the record's scope ignores searches | 1 | 0 | yes | W-14b (conditional) |

**All 18 are caught.** Every mutation turns at least one named test red, with 0 compile errors each time, so the failures are behavioural, not build breaks. `git status --short waybill-cli/src` was empty after the run.
