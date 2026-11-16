# Mutations (T017)

Each mutation was applied by `mutate.py`, then the integration suite `python_owned_binaries` (22 tests) was run. The file was restored with `cp` from a copy taken before the edit, and `git diff --quiet` confirmed it matched the committed tree (`c690b8d8`). The runner is `mutate-run.sh`. Every row's suite exited 101 with tests failing; no mutation stopped compilation (the one `^error` line in each log is cargo's `error: test failed`).

| Mutation | What it breaks | Failing tests | Restored |
|---|---|---|---|
| M1 | OS claim no longer wins: the shared claim set drops paths a `RECORD` lists, and the per-binary check ignores an OS claim when there is an owner | 2: `us2_ipk_claim_wins_over_record`, `us2_os_claim_wins_over_record` | clean |
| M2 | the owned binary is not nested under its package | 9: `t008_note_and_bundled_tree_nest_under_the_owned_binary`, `t011_one_file_in_two_records_goes_to_the_first_sorted_owner`, `t011_one_library_vendored_by_two_packages`, `t011_script_entry_with_cargo_auditable_crates`, `us1_owned_library_nests_under_its_package_with_its_identification`, `us1_unidentified_owned_library_still_appears_once`, `us2_owned_go_binary_gets_no_second_diagnostic_component`, `us2_unowned_binary_linking_a_pip_library_reports_linkage`, `us3_delocate_layout` | clean |
| M3 | identifications are not nested under the owned binary | 3: `t011_one_library_vendored_by_two_packages`, `us1_owned_library_nests_under_its_package_with_its_identification`, `us3_auditwheel_layout` | clean |
| M4 | the binary's edges are not added (left to the root fallback) | 5: `us1_both_layouts_give_the_same_tree`, `us1_edges_form_a_chain_from_the_package`, `us1_internal_link_is_an_edge_not_a_linkage_component`, `us3_auditwheel_layout`, `us3_delocate_layout` | clean |
| M5 | an internal link is also fed to the linkage aggregator | 2: `us1_internal_link_is_an_edge_not_a_linkage_component`, `us3_auditwheel_layout` | clean |
| M6 | no auditwheel hash strip | 1: `us3_auditwheel_layout` | clean |
| M7 | the strip applies outside `*.libs/` | 1: `us3_hash_kept_outside_a_libs_directory` | clean |
| M8 | only a `.dist-info` at the scan root is read for owners (the two layouts diverge) | 13: `t008_note_and_bundled_tree_nest_under_the_owned_binary`, `t011_one_file_in_two_records_goes_to_the_first_sorted_owner`, `t011_one_library_vendored_by_two_packages`, `t011_script_entry_with_cargo_auditable_crates`, `us1_both_layouts_give_the_same_tree`, `us1_edges_form_a_chain_from_the_package`, `us1_internal_link_is_an_edge_not_a_linkage_component`, `us1_owned_library_nests_under_its_package_with_its_identification`, `us1_unidentified_owned_library_still_appears_once`, `us2_os_claim_wins_over_record`, `us2_owned_go_binary_gets_no_second_diagnostic_component`, `us2_unowned_binary_linking_a_pip_library_reports_linkage`, `us3_delocate_layout` | clean |
| M9 | a nested CycloneDX component is also emitted at the top level | 13: `t008_note_and_bundled_tree_nest_under_the_owned_binary`, `t011_one_file_in_two_records_goes_to_the_first_sorted_owner`, `t011_one_library_vendored_by_two_packages`, `t011_script_entry_with_cargo_auditable_crates`, `t011_self_identifying_binary_is_nested_not_merged`, `us1_both_layouts_give_the_same_tree`, `us1_edges_form_a_chain_from_the_package`, `us1_internal_link_is_an_edge_not_a_linkage_component`, `us1_owned_library_nests_under_its_package_with_its_identification`, `us1_unidentified_owned_library_still_appears_once`, `us2_os_claim_wins_over_record`, `us2_owned_go_binary_gets_no_second_diagnostic_component`, `us3_delocate_layout` | clean |
| M10 | linkage counts as an identification | 1: `us1_unidentified_owned_library_still_appears_once` | clean |
| M11 | two `RECORD`s claiming one file: the last owner wins | 1: `t011_one_file_in_two_records_goes_to_the_first_sorted_owner` | clean |
| M12 | SPDX 2.3 emits one package per copy, not per PURL | 1: `t011_one_library_vendored_by_two_packages` | clean |
| M13 | CycloneDX dependency refs are not mapped to `bom-ref`s | 3: `us1_edges_form_a_chain_from_the_package`, `us1_internal_link_is_an_edge_not_a_linkage_component`, `us3_delocate_layout` | clean |

## M1 took four runs

The first three forms of M1 did not kill both US2 tests. That is a finding about the code under test, not the tests:
1. **Lift the per-binary check only.** Make `path_claimed` ignore an OS claim when a `RECORD` also lists the file. Survived: `discover_binaries` filters claimed paths before that check runs, so the binary never reached it.
2. **Drop the paths from the shared claim set only,** where the `RECORD` owners are collected. Survived: the per-binary check also matches by inode, and the inode set was untouched.
3. **Both of the above.** Only the `.ipk` test failed. dpkg's claims are merged into the shared set *after* the point where the paths were dropped; `.ipk` claims are inserted earlier.
4. **The form recorded above** drops the paths just before `read_all` returns, and lifts the per-binary check. Both US2 tests fail.

So the OS claim is enforced at two independent points, discovery and the per-binary check (path, canonical path, or inode). R1's split leaves both untouched, which is why FR-005 holds by construction. Only a mutation that removes both shows `us2_os_claim_wins_over_record` can fail.
