# After: spec 1654

## The eleven test files' own scans (T014, before US2)

Each file was run with a temporary, uncommitted hook that copied every scan's three documents out. The file was restored from a `cp` backup and checked identical. Then `waybill sbom parity-check --format json` ran on each kept scan with the spec 1654 binary.

| file | scans | failing rows before (`test-failing-rows-before.md`) | after |
|---|---|---|---|
| binary_kind_technique | 10 | B1 3 | 0 |
| closure_resolved_tier | 8 | B1 7, B2 7, C1 4, C182 4, C203 4, E1 8 | 0 |
| hpack_include_merge | 20 | C1 19 | 0 |
| project_discovery_haskell | 26 | C1 3, C4 1 | 0 |
| version_string_anchored | 28 | B1 15 | 0 |
| version_string_boundary | 24 | B1 10 | 0 |
| version_string_static | 9 | B1 6 | 0 |
| boot_library_edges | 8 | B1 6, B2 6, E1 6 | 0 |
| library_identity, merged_binary_evidence, sbom_edit_retier | — | 0 | measured again under the shared helper (T017) |

The stub closure fixture (`tests/fixtures/nix_closure_boot_versions`, `--nix-closure`) is scanned by `closure_resolved_tier` and `boot_library_edges`. All 16 of their kept scans report `summary.parity_gaps = 0` (SC-003).

Causes fixed: R4 (B1/B2), R6 as revised (E1), R7 (C1, C4, C182, C203) and R8 (the binary files' B1).

## Corpus goldens

Measured after CI regeneration (T022).
