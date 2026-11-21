# T026: mutations

Script: `mutate.py` (this directory), run as `python3 -I -B mutate.py <repo> <outdir>`. For each mutation it:
- backs up the file with `cp`;
- applies one exact string replacement;
- runs the killing test;
- restores with `cp`.

A log containing `error[E…]` is classified as a compile error, never as a kill. Run on the tree at `ebbffb21`, which was clean before and after the run (`git status --short` counted 0 lines both times). The next build after the run rebuilt the binary from the restored source.

| ID | Mutation | Result |
|---|---|---|
| M1 | S1's `record` call removed (`resolve/deduplicator.rs`) | killed by `s1639_a_merge_records_the_losers_mechanism` |
| M2 | S6's `record` call removed (`cmake/fetches.rs`) | killed by `a06_conditional_fetch_under_its_submodule` |
| M3 | `write` keeps the survivor's own C55 (`set.remove(&own)` deleted) | killed by `the_survivors_own_mechanism_is_never_recorded` |
| M4 | the anchor rule removed (a survivor with no C55 still records) | killed by `s1639_a_survivor_without_c55_records_nothing` (reconciler, A-8) |
| M5 | C242 written back under `waybill:also-detected-via` | killed by `s1639_a_fingerprint_collision_is_c242_not_c56` (F-1) |
| M6 | the C56 CycloneDX extractor reads the wrong field | killed by `also_detected_via_and_fingerprint_collisions_reach_all_three_formats` (F-2) |
| M7 | C56 written in reverse order | killed by `values_are_sorted_and_unique` |
| M8 | S9's `record` call removed (`cmake_search_pair.rs`) | killed by `a10_package_search_paired_with_a_submodule` |
| M9 | the S1 fold's skip of the C56 key removed | **survived** (all 7 `s1639_` tests pass) |

**M9 surviving was the predicted outcome, and it led to a code change.**
- **Why it survives:** whatever the first-wins fold copies into the survivor's C56, `record` then unions the loser's C55 and C56 into it, so the result is identical.
- **What changed:** the skip is unobservable, so it was removed. Research R4 and T010 record that.
- **Re-run after removal:** the `s1639_` unit tests (7 passed) and A-7, the two-site union (passed).
