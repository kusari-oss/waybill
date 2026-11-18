# Mutations (T018, SC-008)

`measurements/mutate.py` applies one named mutation. `mutate-run.sh` applies each in turn and runs the unit tests (`s1542`, `s1541`, `deduplicator`, `graph_completeness`) and the `scan_determinism` and `installed_lib_us1` targets. It records the failing tests and restores every touched file with `cp`. Every file was clean against the commit afterwards. Run at `455a6d1f`.

**6 of 6 caught.** No mutation caused a compile error (0 `error[` lines).

| # | mutation | caught by |
|---|---|---|
| M1 | the content hash dropped from the dedup key (R1) | `d1_symlink_chains_to_different_bytes_stay_apart`, `d3_one_name_different_bytes_is_two_files`, `s1542_one_name_different_hashes_stay_two` |
| M2 | the content-hash parse always `None` (R1) | the same three |
| M3 | the containment snapshot reverted to the live map (R2) | `s1542_reachability_is_the_same_in_every_component_order` |
| M4 | the output sort back to PURL only (R3) | `control_the_comparison_detects_a_difference`, D-1, D-4, D-5, D-7, `s1542_output_is_independent_of_input_order` |
| M5 | the container dropped from the total order (R3) | the control, D-1, D-4, D-5, D-7 |
| M6 | merged path lists left unsorted (R3) | `s1542_output_is_independent_of_input_order` |

**M3 is caught only by the permutation unit test.** That is by design (T007). A scan-level fixture needs a nested copy whose sibling is reachable while it is not, and in D-4 both are unreachable, so containment order cannot change the answer.
