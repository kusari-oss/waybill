# Mutations (T021, SC-007)

`mutate.py` applies each mutation and `mutate-run.sh` runs the guarding tests:
- `cargo test -p waybill --bin waybill -- cmake::`;
- `cargo test --test cmake_fetch_wrappers --test cmake_nested_fetches`.

It then restores every touched file with `cp`, and checks `git diff --quiet` against the commit (`acf55abe`).

**Result: 30 of 30 caught.** Every file was restored clean.

M30 survived the first run, then was caught. Spec 1538's `fetch_entry` expands the same-file variables again after binding, so no test exercised the expansion *before* binding. It matters where a variable decides how the binder parses: a keyword held in a variable is a keyword in CMake. W-26 (`set(_kw TOUCH)` then `wf_parse(x ${_kw} t URL u)`) was added, and fails under M30. The rerun of M30 alone is recorded below.

| id | mutation | failing tests | caught by |
|---|---|---|---|
| M1 | `${ARGN}` shape not detected | 1 | `each_forwarding_shape_is_detected` |
| M2 | `${ARGV}` shape not detected | 1 | `each_forwarding_shape_is_detected` |
| M3 | `${<P>_UNPARSED_ARGUMENTS}` shape not detected | 1 | `each_forwarding_shape_is_detected` |
| M4 | `${<P>_<KW>}` shape not detected | 3 | `a_one_value_keyword_keeps_its_last_value`, `each_forwarding_shape_is_detected`, `w3_parsed_variables_reach_fetchcontent` |
| M5 | parameter shape not detected | 6 | `a_conditional_set_is_unknown`, `a_list_command_makes_a_variable_unknown`, `each_forwarding_shape_is_detected`, `positional_arguments`, `w10_definitions_must_agree`, `w21_positional_arguments` |
| M6 | R2 removed: a body with its own identity becomes a wrapper | 3 | `c13b_a_parameterised_name_with_a_github_repository_is_emitted`, `bodies_that_are_not_wrappers`, `w19_a_body_with_its_own_identity_is_not_a_wrapper` |
| M7 | a multi-value keyword takes one word (W1 lost) | 3 | `a_multi_value_keyword_swallows_a_following_url`, `a_wrapper_calling_a_wrapper_needs_no_identity_check`, `w11_a_multi_value_keyword_swallows_the_url` |
| M8 | a one-value keyword keeps its first value (W4 lost) | 1 | `a_one_value_keyword_keeps_its_last_value` |
| M9 | a `NOT DEFINED` default overrides the caller (W8 lost) | 2 | `a_default_applies_only_when_unset`, `w13_a_body_default_applies_when_the_call_omits_it` |
| M10 | the last `URL` read instead of the first (W2/W3 lost) | 1 | `w12_the_bodys_url_comes_first` |
| M11 | the first definition wins instead of agreement (R5) | 5 | `a_cycle_through_a_second_definition_terminates_unresolved`, `agreeing_definitions_bind_and_disagreeing_ones_do_not`, `same_named_definitions_keep_distinct_ordinals`, `w10_definitions_must_agree`, `w9_a_cycle_terminates_unresolved` |
| M12 | recursion guard removed (depth cap still bounds it) | 1 | `a_cycle_through_a_second_definition_terminates_unresolved` |
| M13 | depth cap removed | 1 | `recursion_and_depth_are_unresolved` |
| M14 | FR-005 name not tied to the URL's version | 2 | `archive_names_follow_the_parsed_version`, `w4_five_calls_five_components` |
| M15 | FR-005 naming applied to direct calls | 2 | `commented_out_declarations_are_not_read`, `w1_an_argn_wrapper_call_is_a_component` |
| M16 | template bodies read as declarations (FR-008) | 9 | `w1_an_argn_wrapper_call_is_a_component`, `w10_definitions_must_agree`, `w11_a_multi_value_keyword_swallows_the_url`, `w4_five_calls_five_components`, `w5_a_wrapper_never_called_emits_and_counts_nothing`, `w6_an_in_tree_wrapper_call_is_unresolved`, `w7_a_loop_variable_is_not_guessed`, `w8_a_wrapper_chain_is_followed`, `w9_a_cycle_terminates_unresolved` |
| M17 | a wrapper call inside a wrapper body read as a call site | 2 | `w8_a_wrapper_chain_is_followed`, `w9_a_cycle_terminates_unresolved` |
| M18 | C234 union in one-per-PURL dropped | 1 | `w4c_a_direct_and_a_wrapped_declaration_of_one_purl_keep_the_chain` |
| M19 | definitions keyed case-sensitively | 1 | `w22_a_call_in_another_case_is_the_same_call` |
| M20 | the wrapper's body taken as a function body for the shape | 13 | `w1_an_argn_wrapper_call_is_a_component`, `w10_definitions_must_agree`, `w12_the_bodys_url_comes_first`, `w13_a_body_default_applies_when_the_call_omits_it`, `w18_names_that_do_not_parse_keep_the_bound_name`, `w2_therocks_shape_is_bound_and_named_from_the_archive`, `w21_positional_arguments`, `w22_a_call_in_another_case_is_the_same_call`, `w24_a_wrapped_call_in_a_submodule_is_its_dependency`, `w25_a_call_site_variable_with_a_literal_set_is_expanded`, `w3_parsed_variables_reach_fetchcontent`, `w4_five_calls_five_components`, `w8_a_wrapper_chain_is_followed` |
| M21 | conditions inside wrapper bodies dropped | 1 | `w14_call_site_and_body_conditions` |
| M22 | `ambiguous-calls` not counted | 2 | `w10_definitions_must_agree`, `w9_a_cycle_terminates_unresolved` |
| M23 | `calls` not counted | 8 | `w10_definitions_must_agree`, `w11_a_multi_value_keyword_swallows_the_url`, `w2_therocks_shape_is_bound_and_named_from_the_archive`, `w4_five_calls_five_components`, `w6_an_in_tree_wrapper_call_is_unresolved`, `w7_a_loop_variable_is_not_guessed`, `w8_a_wrapper_chain_is_followed`, `w9_a_cycle_terminates_unresolved` |
| M24 | an unresolved wrapper call not counted in `unresolved.identity` (V1) | 2 | `w10_definitions_must_agree`, `w9_a_cycle_terminates_unresolved` |
| M25 | C234 not emitted | 10 | `w1_an_argn_wrapper_call_is_a_component`, `w17_a_call_inside_a_plain_function`, `w2_therocks_shape_is_bound_and_named_from_the_archive`, `w21_positional_arguments`, `w22_a_call_in_another_case_is_the_same_call`, `w24_a_wrapped_call_in_a_submodule_is_its_dependency`, `w3_parsed_variables_reach_fetchcontent`, `w4b_two_calls_of_one_url_are_one_component_with_one_chain`, `w4c_a_direct_and_a_wrapped_declaration_of_one_purl_keep_the_chain`, `w8_a_wrapper_chain_is_followed` |
| M26 | `wrappers` never present | 9 | `wire_is_absent_when_nothing_was_read_and_v1_holds_otherwise`, `w10_definitions_must_agree`, `w11_a_multi_value_keyword_swallows_the_url`, `w2_therocks_shape_is_bound_and_named_from_the_archive`, `w4_five_calls_five_components`, `w6_an_in_tree_wrapper_call_is_unresolved`, `w7_a_loop_variable_is_not_guessed`, `w8_a_wrapper_chain_is_followed`, `w9_a_cycle_terminates_unresolved` |
| M27 | `wrappers` always present (I-7) | 3 | `i7_no_wrapper_no_wrappers_key`, `wire_is_absent_when_nothing_was_read_and_v1_holds_otherwise`, `w19_a_body_with_its_own_identity_is_not_a_wrapper` |
| M28 | a variable another command changes keeps a stale value | 1 | `a_list_command_makes_a_variable_unknown` |
| M29 | a conditional `set()` taken as unconditional | 1 | `a_conditional_set_is_unknown` |
| M30 | call-site variables not expanded before binding | 1 (after W-26) | `w26_a_keyword_in_a_call_site_variable_is_a_keyword` |
