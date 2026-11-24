# Failing rows in the eleven test files' own parity-check calls, at main 614e3c2

Same temporary instrument as `golden-failing-rows-before.md`, each file run with `-- --test-threads=1` (parallel runs corrupted 70 of 109 appended lines).

| file | failing-row records | rows |
|---|---|---|
| binary_kind_technique | 3 | B1 3 |
| closure_resolved_tier | 34 | B1 7, B2 7, C1 4, C182 4, C203 4, E1 8 |
| hpack_include_merge | 19 | C1 19 |
| library_identity | 0 | — |
| merged_binary_evidence | 0 | — |
| project_discovery_haskell | 4 | C1 3, C4 1 |
| sbom_edit_retier | 0 | — |
| version_string_anchored | 15 | B1 15 |
| version_string_boundary | 10 | B1 10 |
| version_string_static | 6 | B1 6 |
| boot_library_edges | 18 | B1 6, B2 6, E1 6 |

Total records: 109; by row: {'B1': 47, 'B2': 13, 'C1': 26, 'C182': 4, 'C203': 4, 'C4': 1, 'E1': 14}.
