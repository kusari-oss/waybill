# Mutations (T019, SC-007)

Each mutation was applied by exact string replacement (`mutate.py`, kept in the session scratchpad), and then:
- the feature's tests were run with `--no-fail-fast`: the `test_scope`, `installed_lib` and `v3_packages` unit tests, `tests/installed_test_scope.rs`, and the A13 extractor test;
- the file was restored with `cp`.

`git status` was clean after the run. Base commit: `0c06a355`.

| # | Mutation | Tests that failed |
|---|---|---|
| M1 | Scope when *any* path matches, not every path (`.flatten().max()`) | `every_path_must_match` |
| M2 | Also run the pass *before* scope propagation | `us1_declared_test_payload_is_test_scoped`, `include_test_binaries_reports_them_test_scoped` |
| M3 | Substring directory matching | `test_directories_match_exactly_and_only_when_installed`, `us1_declared_test_payload_is_test_scoped` |
| M4 | Drop the recognition gate (path rules everywhere) | `test_directories_match_exactly_and_only_when_installed`, `include_test_binaries_scopes_what_1524_skips`, `an_unrecognised_source_tree_is_unchanged` |
| M4b | Drop the OS-database recognition signal | `an_extracted_rootfs_with_an_os_database_is_recognised` |
| M5 | Scope OS-owned files | `an_os_owned_file_is_not_scoped`, `an_extracted_rootfs_with_an_os_database_is_recognised` |
| M6 | Treat the interpreter as the location | `an_interpreter_names_its_script`, `us1_declared_test_payload_is_test_scoped` |
| M7 | Re-scope an already-scoped component | `what_is_never_scoped` |
| M8 | Drop the A13 `software_additionalPurpose` fallback | `a13_reads_the_role_of_a_test_scoped_binary`, `us1_declared_test_payload_is_test_scoped` |

## M2 is killed by accounting, not by propagation

M2's failures come from C226's `scoped` counts. The early call scopes the components first, so the real call skips them as already scoped, and the counts come out wrong.

`a_product_a_test_package_requires_stays_unscoped` **passed under M2**, even though its test package has a real `Requires` edge to the product. So on these fixtures, scope propagation never demotes the product, whichever order the passes run in.

*Inference, not measured:* `pkg:generic` has no root seed in these scans, so milestone 1269's propagation skips the ecosystem (its FR-012).

**FR-007 therefore rests on the pass's position**, after propagation (research R2). It is not shown by a mutation that changes a component's scope. The test remains as a guard for the day `pkg:generic` gains a root.
