# T019: mutations

`mutate.py` applies each by exact string replacement (each anchor occurs once). `mutate-run.sh` runs `propagation_seeds` and the `scope_propagation::` unit tests, restores the file with `cp` from `HEAD`, and rebuilds after the last mutation. Run at `3b194947`.

| # | Mutation | Rule | Failed binaries | Compile errors | Restored | Caught by |
|---|---|---|---|---|---|---|
| M1 | no added seeds | FR-001 | 2 | 0 | yes | P-1/P-3, P-8, P-10, `s1616_non_runtime_main_modules_are_not_seed_candidates` |
| M2 | non-runtime main modules seeded | FR-002 | 2 | 0 | yes | P-4, the candidate unit test |
| M3 | dot-directory main modules seeded | FR-002 | 2 | 0 | yes | P-5, the candidate unit test |
| M4 | the base immunity search starts from the added seeds | FR-005 / FR-006 | 2 | 0 | yes | P-8, `s1616_an_added_seed_protects_its_runtime_closure`, `s1616_the_count_excludes_what_base_seeds_protect` |
| M4b | the base seeds are dropped from both searches | FR-005 | 2 | 0 | yes | P-1/P-3 (`a-runtime`), P-8, P-10, the closure and count unit tests |
| M5 | the count includes what the base seeds already protect | FR-006 | 2 | 0 | yes | P-8, `s1616_the_count_excludes_what_base_seeds_protect` |
| M6 | the added seeds cross excluded edges | FR-003 | 1 | 0 | yes | `s1616_an_added_seed_does_not_cross_an_excluded_edge` |

**All 7 are caught.**

**M4 was too weak for its purpose.** The analysis added it to test FR-005 through P-1's `a-runtime` guard, but the union with the full seed set (`added_only`) put the base protection back. So M4 was caught only by the count tests, and `a-runtime` stayed required. M4b also drops the base seeds from the union, and P-1 turns red, so the guard is exercised.
