# Spec 1667 mutations (T012)

Filter: `test(~s1667) | test(~amend_) | test(~root_dir_) | binary(haskell_path_shapes) | binary(project_discovery_haskell)`

| Mutation | Result | Failing tests |
|---|---|---|
| M1 root_relative returns the path unchanged | killed | c200_has_no_host_path, a_nested_manifest_is_relative, a_path_above_the_root_climbs, a_path_under_the_second_spelling_is_relative_to_it, every_key_is_relative_and_the_keys_are_unchanged, the_root_manifest_is_its_file_name, three_components_join_with_a_slash_on_every_host, s1667_finalize_writes_every_c200_path_under_the_canonical_root, us3_f1_positive_control_records_drop, us3_symlinked_root_c200_matches |
| M2 no `..` above the root | killed | a_path_above_the_root_climbs |
| M3 include entries not relativised | killed | c200_has_no_host_path, every_key_is_relative_and_the_keys_are_unchanged |
| M4 subproject_dir drops the file test | killed | root_dir_strips_a_nested_manifest_file, root_dir_strips_a_root_manifest_file, s1667_a_cabal_file_on_disk_is_its_directory, s1667_a_root_package_yaml_is_the_root, split_source_dir_is_a_directory, two_main_modules_in_one_dir_group, us1_f2_root_only_keeps_chain_drops_independent, us1_f2_strict_keeps_chain, us1_unlisted_manifest_name_is_root_level, us2_declaration_only_scope_keeps_root_level_components, us2_f3_cabal_project_sets_scope_without_root_package, us2_f4_cabal_project_members_join_json_root, us2_f5_stack_yaml_sets_scope, us3_f2_drop_one |
| M5 candidates joined to the canonical root again | killed | us3_f1_positive_control_records_drop, us3_symlinked_root_c200_matches |

Control, unmutated: Summary [   0.487s] 40 tests run: 40 passed, 5631 skipped; failing: none
