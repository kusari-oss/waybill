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
| M10 | two `Optional` declarations merge to no scope | 3 | 0 | yes | d11b, s1613_two_optional_declarations… |
| M11 | pip's cross-project merge first-wins again | 1 | 0 | yes | d11b |
| M12 | R5 skips an explicit `Runtime` | 2 | 0 | yes | d11b, an_explicit_runtime_scope… |
| M13 | dedup keys PyPI on the declared spelling | 3 | 0 | yes | d11c, s1613_pypi_spellings… |
| M14 | dedup normalises every ecosystem's name | 2 | 0 | yes | s1613_pypi_spellings… (its npm half) |

**All 14 are caught** (M1–M9 at `bc13f385`; all 12 re-run at `e4045f0e` after R9, and M13–M14 at `4d73b990` after R10, with the same results for M1–M9 plus d11b where it applies). Every mutation turns at least one named test red. With 0 compile errors each time, these are behavioural failures, not build breaks. `git status --short waybill-cli/src` was empty after the run.
