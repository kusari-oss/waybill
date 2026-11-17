# Mutations (T025)

**Method:** `mutate.py` applied each mutation, and `mutate-run.sh` then ran:
- the unit tests: `cmake::`, `cmake_fetch_attach` and `cmake_version`;
- `tests/cmake_nested_fetches.rs`.

Every touched file was restored with `cp`, and `git diff --quiet` confirmed it matched the commit. The harness's `compile_errors` column is not a compile signal: its pattern also matches cargo's `error: test failed` line. Failing tests are the signal.

## Run 1: three survivors, three missing tests

M13, M15 and M16 failed no test. Each is a rule whose test the task list named and which was never written. The tasks had been ticked without the test:

| Mutation | What it breaks | The test it was missing (now added) |
|---|---|---|
| M13 | a required declaration no longer keeps its PURL required (R4) | `a_required_declaration_keeps_its_purl_required` (T018) |
| M15 | first declaration wins applied to `ExternalProject_Add` (R3 P6) | `external_project_never_merges_under_first_declaration_wins` (T021) |
| M16 | an unconditional fetch merges into an existing component (R9) | contract case C-12b (T022) |

Re-applied after the tests were added, each fails exactly its new test.

## Results

| Mutation | What it breaks | Failing tests |
|---|---|---|
| M1 | discovery back to today's file set (FR-001) | 13, including C-1, C-2, C-10, C-11, C-12, C-14 and C-15 |
| M2 | comments not stripped before tokenising | `commented_out_declarations_are_not_read` |
| M3 | `else()` does not mark its branch | `elseif_replaces_and_else_prefixes` |
| M4 | `NOT TARGET` shape removed | C-4, `guards_and_their_else_branches` |
| M5 | `NOT <X>_FOUND` shape removed | C-5, `guards_and_their_else_branches` |
| M6 | `FIND_PACKAGE_ARGS` shape removed | C-3, `find_package_args_unless_never` |
| M7 | `NEVER` no longer cancels `FIND_PACKAGE_ARGS` (P4) | C-3, `find_package_args_unless_never` |
| M8 | `ALWAYS` shape removed | C-6, `always_and_provider_are_fetchcontent_only` |
| M9 | any provider counts, whatever its methods (P5) | C-7, `mode_and_provider_parsing` |
| M10 | function-body shape removed | C-13, `function_body_and_unconditional` |
| M11 | the mode is looked up in the declaring directory only | C-3, C-6 |
| M12 | test scope no longer outranks optional (R7) | C-9 |
| M13 | see run 1 | `a_required_declaration_keeps_its_purl_required` |
| M14 | first declaration wins between siblings | C-10, C-11 |
| M15 | see run 1 | `external_project_never_merges_under_first_declaration_wins` |
| M16 | see run 1 | C-12b |
| M17 | owner edges not added (R6) | C-14 |
| M18 | `unresolved.identity` not counted, which breaks V1 | C-13 |

**All 18 are caught.**
