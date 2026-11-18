# T016 mutations

Run with `measurements/mutate-run.sh` (from the repository root). Each mutation was applied by exact string replacement (`mutate.py`, which refuses an anchor that does not occur exactly once). Then:
- the integration file and the related unit tests ran;
- the file was restored with `cp` from `git show HEAD:<file>`, and checked with `cmp`.

| Mutation | What it breaks | Failed test binaries | Compile errors | Restored | Tests turned red |
|---|---|---|---|---|---|
| M1 | pip does not descend into listed directories | 1 | 0 | yes | d01_to_d05, d08, d11 |
| M2 | case-sensitive directory match | 2 | 0 | yes | d01_to_d05 (D-5 `.CI`), the_glob_matches_listed_names_only |
| M3 | `.venv` added to the listed set | 2 | 0 | yes | d06_d07_d09, d12a, listed_names_match…, the_glob_matches… |
| M4 | no ancestor rule in the classifier | 2 | 0 | yes | d01_to_d05 (D-2 `.ci/metrics`), s1613_the_ancestor_rule… |
| M5 | selector not widened | 2 | 0 | yes | d08, s1613_read_requirements_files…, s1613_the_selector… |
| M6 | `tox-requirements.txt` accepted | 2 | 0 | yes | d06_d07_d09 (D-9), s1613_read_requirements_files…, s1613_the_selector… |
| M7 | the R5 pass removed | 2 | 0 | yes | d10, only_under_listed_directories… |
| M8 | R5 "every file" made "any file" | 2 | 0 | yes | d11, mixed_locations…, a_runtime_component_loses… |
| M9 | R5 overwrites an existing scope | 1 | 0 | yes | mixed_locations_scoped_components… |

**All 9 are caught.** Every mutation turns at least one named test red. With 0 compile errors each time, these are behavioural failures, not build breaks. `git status --short waybill-cli/src` was empty after the run.
