# Mutations (T014, SC-007)

`mutate.py` applies each mutation, and `mutate-run.sh` runs `cargo test --bin waybill -- cmake::` and the `cmake_fetch_wrappers` and `cmake_nested_fetches` suites. It then restores the file with `cp`, and checks `git diff --quiet` against `95ff1752`.

**Result: 12 of 12 caught.** Every file was restored clean.

M1 survived the first run. The "no `DL_ARGS`" unit test used the prefix `ARGS`, which the wrapper rule rejects anyway: the synthesized call's `${DL_ARGS_…}` variables do not match an `ARGS` parse's prefix. The test now uses `DL`, where only the prefix check rejects the definition, and it fails under M1 (rerun below).

| id | mutation | failing tests | caught by |
|---|---|---|---|
| M1 | `DL_ARGS` prefix check removed | 1 (after the fix) | `downloadproject_is_recognised_only_with_its_shape` |
| M2 | `PROJ` check removed | 1 | `downloadproject_is_recognised_only_with_its_shape` |
| M3 | template-name check removed | 1 | `downloadproject_is_recognised_only_with_its_shape` |
| M4 | name check made case-sensitive | 1 | `downloadproject_is_recognised_only_with_its_shape` |
| M5 | synthesized call named `<PROJ>-download` | 3 | `d3_a_url_that_does_not_parse_is_named_proj`, `d5_the_last_proj_wins`, `each_of_therocks_keyword_lists_binds_with_its_own_keywords` |
| M6 | forwarded arguments dropped | 14 | `d1_a_github_download_project_call_is_a_component`, `d11_a_shaped_and_an_unshaped_definition_are_ambiguous`, `d13_the_call_sites_condition_alone`, `d14_a_call_in_a_test_directory` and 10 more |
| M7 | template's `BUILD_PROJECT` condition put on the fetch | 2 | `d1_a_github_download_project_call_is_a_component`, `d13_the_call_sites_condition_alone` |
| M8 | keyword lists read literally (FR-002 reverted) | 17 | `d1_a_github_download_project_call_is_a_component`, `d11_a_shaped_and_an_unshaped_definition_are_ambiguous`, `d13_the_call_sites_condition_alone`, `d14_a_call_in_a_test_directory` and 13 more |
| M9 | keyword lists spliced as separate arguments | 4 | `d1_a_github_download_project_call_is_a_component`, `d4_downloadprojects_keywords_are_consumed`, `d8_keywords_come_from_each_copy`, `each_of_therocks_keyword_lists_binds_with_its_own_keywords` |
| M10 | an unresolved list bound as empty | 2 | `d9_an_unresolved_keyword_list_is_not_guessed`, `unresolved_keyword_lists_are_unbound` |
| M11 | a call with no `PROJ` binds anyway | 1 | `a_download_project_call_without_proj_is_unbound` |
| M12 | an unresolvable list fails the shape check (call unread) | 1 | `d9_an_unresolved_keyword_list_is_not_guessed` |
