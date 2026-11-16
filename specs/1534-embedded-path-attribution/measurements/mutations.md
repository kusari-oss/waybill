# Mutations (T014, SC-005)

Each mutation was an exact string replacement. After it:
- the feature's tests ran with `--no-fail-fast`: the `embedded_paths` and `installed_lib::resolve` unit tests, and `tests/installed_lib_embedded_paths.rs`;
- the file was restored with `cp`.

`git status` was clean afterwards. Base commit: `0609cca1`.

| # | Mutation | Tests that failed |
|---|---|---|
| M1 | Innermost directory instead of outermost | `the_outermost_directory_wins` |
| M2 | Empty stoplist | `the_stoplist_and_the_toolchain`, `versioned_names_are_the_emitted_components` |
| M3 | Header paths attribute | `a_header_never_attributes`, `us1_binaries_are_attributed_through_their_compiled_sources` |
| M4 | No toolchain exclusion | `the_stoplist_and_the_toolchain`, `us1_binaries_are_attributed_through_their_compiled_sources` |
| M5 | No already-attributed exclusion | `us1_binaries_are_attributed_through_their_compiled_sources` |
| M6 | No shared marking | `us1_binaries_are_attributed_through_their_compiled_sources` |
| M7 | Empty bundled exclusion list | `vendoring_markers_name_bundled_trees`, `us2_bundled_trees_are_nested_versionless_components` |
| M8 | A bundled component versioned from its directory name | `us2_bundled_trees_are_nested_versionless_components` |

All eight are killed.
