# T024 mutations

Harness: `mutate.py` and `mutate-run.sh` in this directory. Each mutation is restored with `cp` from `HEAD`, and the binary is rebuilt after the last one. The tests run are `cmake_project_components` and the unit tests of `cmake::projects`, `declared_attach`, `root_selector` and `cmake_search_pair`. Run against `HEAD` = the P-20 commit.

| Mutation | Failed test binaries | Compile errors | Restored | Red |
|---|---|---|---|---|
|`M1-cmake-always-root-candidate ` | 2 | 0 | yes | p03_found_only_from_tests_is_test p14_a_scan_root_python_project_keeps_the_root p15_a_nested_python_project_keeps_the_root s1618_a_scoped_project_is_not_a_candidate s1618_another_ecosystem_keeps_the_root  |
|`M2-cmake-rank-above-main-modules ` | 1 | 0 | yes | s1618_cmake_project_rank  |
|`M3-no-enclosing-edge ` | 1 | 0 | yes | p16c_a_submodule_claims_its_own_name  |
|`M4-no-search-pairing ` | 1 | 0 | yes | p01_a_search_between_projects_is_an_edge p02_a_conditional_search_records_conditions_only p03_found_only_from_tests_is_test  |
|`M5-expression-emitted-as-absence ` | 2 | 0 | yes | p06_an_unresolved_version_is_kept_as_an_expression s1618_entries_markers  |
|`M6-test-directory-unscoped ` | 2 | 0 | yes | p10_a_test_directory_project_is_test s1618_dir_scope s1618_entries_markers  |
|`M7-reach-ignored ` | 2 | 0 | yes | p12_not_reached_from_the_root_build s1618_entries_markers  |
|`M8-include-vendored-not-merged ` | 1 | 0 | yes | p20_include_vendored_folds_into_the_project  |
|`M9-conditional-search-makes-optional ` | 1 | 0 | yes | p02_a_conditional_search_records_conditions_only  |
|`M10-paired-project-serialised-at-zero ` | 1 | 0 | yes | p_ctl_is_byte_identical_to_main  |
|`M11-collisions-not-dropped ` | 1 | 0 | yes | p16b_an_installed_record_claims_the_name p16c_a_submodule_claims_its_own_name  |
|`M12-vendored-rule-either-signal ` | 1 | 0 | yes | p09_single_signals_are_recorded_without_a_verdict  |
|`M13-cmake-root-not-held-back ` | 1 | 0 | yes | p13_a_cmake_only_tree_has_its_top_project_as_root p20_include_vendored_folds_into_the_project  |

All 13 are caught. None compile with an error, and every mutated file is restored. M8 is caught only because of P-20, added after the first plan showed no case exercised `--include-vendored`.
