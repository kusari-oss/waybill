# Mutations (T016, SC-008)

`measurements/mutate.py` applies one named mutation. `mutate-run.sh` applies each in turn and runs the unit tests (`s1541`, `uv_lock`, `graph_completeness`, `orphan_reason`, `discover`) and the `orphan_explanations` and `public_corpus` targets. It records the failing tests and restores every touched file with `cp`. Every file was clean against the commit afterwards.

Run at `2aecdf6e`, the final code. The removal of the stale haskell declaration (#1237) was committed before its regenerated goldens, so `golden_matrix_is_evaluable_and_gated` failed in every run, in both test binaries. It is excluded below. Every mutation fails at least one *other* test.

**25 of 25 caught.** No mutation caused a compile error: 0 `error[` lines across all logs; the `error:` lines are cargo's per-target failure summaries.

| # | mutation | caught by |
|---|---|---|
| M1 | classifier without the operator's root (FR-002) | `o1_an_operator_root_classifies_against_the_documents_graph` |
| M2 | containment edges removed | `a_child_of_a_reachable_container_is_reachable`, `a_nested_components_dependencies_are_reached`, `a_reachable_child_does_not_reach_its_container`, `a_top_level_copy_of_a_nested_purl_is_not_reached_through_containment`, `containment_is_transitive`, `o13_a_claimed_binary_gets_no_binary_code`, `o8_a_library_nested_in_a_reachable_binary_is_reachable` |
| M3 | containment edges reversed | `a_child_of_a_reachable_container_is_reachable`, `a_nested_components_dependencies_are_reached`, `a_reachable_child_does_not_reach_its_container`, `a_top_level_copy_of_a_nested_purl_is_not_reached_through_containment`, `containment_is_transitive` |
| M4 | I7 check reads top-level components only | `s1541_a_nested_child_of_an_unreachable_container_is_checked` |
| M5 | I7 check: containment not reachability | `s1541_a_nested_child_of_a_reachable_container_is_not_an_offender`, `s1541_a_nested_child_of_an_unreachable_container_is_checked`, `s1541_a_nested_childs_dependencies_are_reachable` |
| M6 | `test-data-binary` arm removed | `o3_a_binary_under_tests_is_test_data`, `o4_the_other_test_data_spellings_are_test_data`, `s1541_an_elf_note_package_takes_a_binary_code`, `s1541_each_test_data_directory_is_recognised`, `s1541_the_test_data_match_is_exact_per_segment` |
| M7 | `unowned-binary` arm removed | `o5_a_binary_elsewhere_is_unowned`, `o6_a_binary_partly_outside_test_data_is_unowned`, `o9_a_library_nested_in_an_unreachable_binary_is_explained`, `s1541_every_file_must_be_under_test_data`, `s1541_the_test_data_match_is_exact_per_segment` |
| M8 | `test_assets` dropped from the directory list | `o4_the_other_test_data_spellings_are_test_data`, `s1541_an_elf_note_package_takes_a_binary_code` |
| M9 | every file → any file | `o6_a_binary_partly_outside_test_data_is_unowned`, `s1541_every_file_must_be_under_test_data` |
| M10 | binary arm moved above every existing code | `o3_a_binary_under_tests_is_test_data`, `o4_the_other_test_data_spellings_are_test_data`, `s1541_an_existing_code_wins_over_the_binary_codes`, `s1541_each_test_data_directory_is_recognised`, `s1541_the_test_data_match_is_exact_per_segment` |
| M11 | directory match case-insensitive | `s1541_the_test_data_match_is_exact_per_segment` |
| M12 | versionless editable project skipped again | `s1541_a_versionless_editable_project_is_emitted` |
| M13 | dev-group edges dropped | `dev_dependencies_produce_edges_and_scope_issue_783`, `o10_a_versionless_editable_root_is_emitted_and_wired`, `o15_a_uv_edge_stays_inside_its_lock` |
| M14 | forks wired by bare name | `dev_dependencies_diamond_shape_runtime_wins`, `dev_dependencies_produce_edges_and_scope_issue_783`, `emits_dependency_edges`, `emits_workspace_root_and_members`, `o11_every_forked_version_is_wired`, `o15_a_uv_edge_stays_inside_its_lock`, `requested_extra_children_are_dependencies`, `s1541_a_forked_dependency_targets_each_version`, `s1541_a_versionless_editable_project_is_emitted`, `s1541_an_extra_requested_by_the_projects_extra_is_optional`, `s1541_the_projects_unrequested_extra_is_an_optional_edge` |
| M15 | the project's own extras do not seed optional activation | `s1541_an_extra_requested_by_the_projects_extra_is_optional` |
| M16 | every package's unrequested extras get edges (#1163) | `requested_extra_children_are_dependencies` |
| M17 | C151 not emitted on the versionless project | `s1541_a_versionless_editable_project_is_emitted` |
| M18 | #1524 skip back to the whole-tree rule (FR-011) | `o14_a_vendored_sdk_does_not_unskip_the_trees_test_inputs` |
| M19 | installed prefix ignored in the per-binary skip | `o14_a_vendored_sdk_does_not_unskip_the_trees_test_inputs`, `s1541_only_test_inputs_under_an_installed_prefix_are_kept` |
| M20 | reachability keyed by PURL again (`gotcha`) | `a_top_level_copy_of_a_nested_purl_is_not_reached_through_containment` |
| M21 | a name the lock lists once resolves across the scan again | `dev_dependencies_diamond_shape_runtime_wins`, `dev_dependencies_produce_edges_and_scope_issue_783`, `emits_dependency_edges`, `emits_workspace_root_and_members`, `o15_a_uv_edge_stays_inside_its_lock`, `requested_extra_children_are_dependencies`, `s1541_a_forked_dependency_targets_each_version`, `s1541_a_versionless_editable_project_is_emitted`, `s1541_an_extra_requested_by_the_projects_extra_is_optional`, `s1541_the_projects_unrequested_extra_is_an_optional_edge` |
| M22 | an extra requested by the project's extra stays inactive | `s1541_an_extra_requested_by_the_projects_extra_is_optional` |
| M23 | `conanfile.py` not a build manifest | `s1541_a_conan_recipe_is_a_build_manifest` |
| M24 | only the `analyzed` tier counts as binary-derived | `s1541_an_elf_note_package_takes_a_binary_code` |
| M25 | workspace root lists bare member names again | `emits_workspace_root_and_members`, `s1541_a_workspace_root_reaches_its_members_and_its_own_project` |
