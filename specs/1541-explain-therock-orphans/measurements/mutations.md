# Mutations (T016, SC-008)

`measurements/mutate.py` applies one named mutation; `mutate-run.sh` applies each, runs the unit tests (`s1541`, `uv_lock`, `graph_completeness`, `orphan_reason`, `discover`) and the `orphan_explanations` and `public_corpus` targets, records the failing tests, and restores every touched file with `cp`. Every file was clean against the commit afterwards.

**19 of 19 caught.** No mutation caused a compile error: the `error:` lines in the logs are cargo's per-target failure summaries (0 `error[` lines across all logs).

| # | mutation | failing tests | caught by |
|---|---|---|---|
| M1 | classifier without the operator's root (FR-002) | 1 | `o1_an_operator_root_classifies_against_the_documents_graph` |
| M2 | containment edges removed | 2 | `a_child_of_a_reachable_container_is_reachable`, `containment_is_transitive` |
| M3 | containment edges reversed | 3 | `a_child_of_a_reachable_container_is_reachable`, `a_reachable_child_does_not_reach_its_container`, `containment_is_transitive` |
| M4 | I7 check reads top-level components only | 2 | `s1541_a_nested_child_of_an_unreachable_container_is_checked` |
| M5 | I7 check: containment not reachability | 6 | `s1541_a_nested_child_of_a_reachable_container_is_not_an_offender`, `s1541_a_nested_child_of_an_unreachable_container_is_checked`, `s1541_a_nested_childs_dependencies_are_reachable` |
| M6 | `test-data-binary` arm removed | 4 | `s1541_each_test_data_directory_is_recognised`, `s1541_the_test_data_match_is_exact_per_segment`, `o3_a_binary_under_tests_is_test_data`, `o4_the_other_test_data_spellings_are_test_data` |
| M7 | `unowned-binary` arm removed | 5 | `s1541_every_file_must_be_under_test_data`, `s1541_the_test_data_match_is_exact_per_segment`, `o5_a_binary_elsewhere_is_unowned`, `o6_a_binary_partly_outside_test_data_is_unowned`, `o9_a_library_nested_in_an_unreachable_binary_is_explained` |
| M8 | `test_assets` dropped from the directory list | 1 | `o4_the_other_test_data_spellings_are_test_data` |
| M9 | every file → any file | 2 | `s1541_every_file_must_be_under_test_data`, `o6_a_binary_partly_outside_test_data_is_unowned` |
| M10 | binary arm moved above every existing code | 5 | `s1541_an_existing_code_wins_over_the_binary_codes`, `s1541_each_test_data_directory_is_recognised`, `s1541_the_test_data_match_is_exact_per_segment`, `o3_a_binary_under_tests_is_test_data`, `o4_the_other_test_data_spellings_are_test_data` |
| M11 | directory match case-insensitive | 1 | `s1541_the_test_data_match_is_exact_per_segment` |
| M12 | versionless editable project skipped again | 2 | `o10_a_versionless_editable_root_is_emitted_and_wired`, `s1541_a_versionless_editable_project_is_emitted` |
| M13 | dev-group edges dropped | 1 | `dev_dependencies_produce_edges_and_scope_issue_783` |
| M14 | forks wired by bare name | 2 | `o11_every_forked_version_is_wired`, `s1541_a_forked_dependency_targets_each_version` |
| M15 | the project's own unrequested extras get no edge | 1 | `s1541_the_projects_unrequested_extra_is_an_optional_edge` |
| M16 | every package's unrequested extras get edges (#1163) | 1 | `requested_extra_children_are_dependencies` |
| M17 | C151 not emitted on the versionless project | 1 | `s1541_a_versionless_editable_project_is_emitted` |
| M18 | #1524 skip back to the whole-tree rule (FR-011) | 1 | `o14_a_vendored_sdk_does_not_unskip_the_trees_test_inputs` |
| M19 | installed prefix ignored in the per-binary skip | 2 | `o14_a_vendored_sdk_does_not_unskip_the_trees_test_inputs`, `s1541_only_test_inputs_under_an_installed_prefix_are_kept` |
