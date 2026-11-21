# T018: mutations (SC-006)

Script: `mutate.py` (this directory), run as `python3 -I -B mutate.py <repo> <outdir> [ids]`. For each mutation it:
- backs up the file with `cp`;
- applies one exact replacement (every anchor checked to occur once);
- runs the killing test;
- restores with `cp`.

A log with `error[E…]` counts as a compile error, never a kill.

The script was run twice:
- **at `a55af43b`, all 22:** 20 killed, M02 survived, M04 failed to compile;
- **M02 and M04 again, after two fixes:** M02's killing test had been the census case, whose platform fixture is a bare `UNIX`, not a comparison, so it now points at the unit test for platform comparisons. M04's replacement made a `match` non-exhaustive.

**Final: 22 of 22 killed.** `git status` showed no source file modified after either run.

**M23 and M24, added after T021.** TheRock's trace found Catch2's C55 moving: a kept-in-scope declaration had tied with the unconditional one and won on file order. The fix ranks an unconditional declaration first (case M-2). M23 removes that rank, M24 swaps the kept and Optional ranks. Both killed; `git status` again showed no source file modified.

**M25, added after T019.** The gate found spec 1614's W-17 failing: a search in a `function()` body had come into scope, because function blocks are not `if` branches and so its chain read as holding. That is spec 1538's sixth shape, which this spec leaves unchanged (spec Background), so `scopes()` now leaves it Optional. M25 removes the guard; W-17 kills it. **25 of 25 killed.**

| Mutation | Result |
|---|---|
| M01 platform atom | KILLED by census_one_event_per_category |
| M02 platform comparison | KILLED by scan_fs::package_db::cmake::conditions::tests::platform_predicates_and_their_negations_are_platform |
| M03 superbuild guard | KILLED by scan_fs::package_db::cmake::conditions::tests::the_superbuild_guard_holds_in_either_order_and_its_positive_form_is_unknown |
| M04 option default | KILLED by t04_option_off_stays_optional |
| M05 missing value is OFF | KILLED by scan_fs::package_db::cmake::conditions::tests::options_take_their_declared_default |
| M06 dependent option | KILLED by scan_fs::package_db::cmake::conditions::tests::a_dependent_option_follows_its_dependencies |
| M07 CACHE BOOL | KILLED by scan_fs::package_db::cmake::conditions::tests::options_take_their_declared_default |
| M08 literal set | KILLED by census_one_event_per_category |
| M09 conflict | KILLED by scan_fs::package_db::cmake::conditions::tests::conflicting_defaults_are_unknown |
| M10 NOT | KILLED by scan_fs::package_db::cmake::conditions::tests::operators_follow_cmake_precedence_and_three_valued_logic |
| M11 AND | KILLED by t05_platform_and_off_option |
| M12 OR | KILLED by scan_fs::package_db::cmake::conditions::tests::operators_follow_cmake_precedence_and_three_valued_logic |
| M13 else | KILLED by t07_else_of_an_on_option |
| M14 elseif history | KILLED by t07e_elseif_after_a_branch_that_holds |
| M15 nesting | KILLED by scan_fs::package_db::cmake::conditions::tests::branches_read_else_and_elseif |
| M16 scope_files | KILLED by t09b_off_option_in_the_parent_directory |
| M17 kept fetch in scope | KILLED by t01_platform_check |
| M18 C232 only with a scope | KILLED by t01_platform_check |
| M19 one_per_purl keeps C232 | KILLED by m1_merged_declarations_keep_the_strongest_scope |
| M20 R9 keys on C232 | KILLED by r1_a_kept_conditional_fetch_still_merges_into_its_submodule |
| M21 searches follow the rule | KILLED by s1_a_package_search_under_a_platform_check_is_in_scope |
| M22 kept record records C236 | KILLED by s3_an_installed_record_searched_under_a_platform_check |
| M23 unconditional outranks kept | KILLED by m2_an_unconditional_declaration_still_survives_a_kept_one |
| M24 kept outranks Optional | KILLED by m1_merged_declarations_keep_the_strongest_scope |
| M25 a function body stays Optional | KILLED by w17_search_in_a_function_body |

## What only the census and unit tests can see

Under FR-004 an unknown chain is in scope. In every combination with the other values, `Platform` and `Unknown` give the same scope. So a component's scope cannot distinguish the platform rule (M01, M02) or the super-build rule (M03) from treating those conditions as unknown. They differ in the census category and evaluation (T006's case) and in the unit tests, which is where those mutations are killed.

The rules that can move a component (option defaults, `AND`, `else`, `elseif`, `scope_files`, the wiring) are each killed by an integration case.
