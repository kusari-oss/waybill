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

## Mutations (T018)

Each mutation was made from a `cp` backup and the tree rebuilt. The targeted tests then ran, the file was restored from the backup and checked identical, and the tree was rebuilt.

**The shared helper, all eleven files (SC-002).**
- **M1:** SPDX 2.3 drops every `DEPENDS_ON` (row B1). Every file fails, and every failing test fails in `assert_parity`, not in a file's own assertions:

| file | failing tests | via the shared helper |
|---|---|---|
| binary_kind_technique | 8 | 8 |
| boot_library_edges | 7 | 7 |
| closure_resolved_tier | 8 | 8 |
| hpack_include_merge | 20 | 20 |
| library_identity | 10 | 10 |
| merged_binary_evidence | 7 | 7 |
| project_discovery_haskell | 15 | 15 |
| sbom_edit_retier | 10 | 10 |
| version_string_anchored | 24 | 24 |
| version_string_boundary | 19 | 19 |
| version_string_static | 9 | 9 |

- **M2:** SPDX 3 drops `waybill:boot-library-dependencies` (C247), the case spec 1653's M5 could not catch. `boot_library_edges` fails 6 of 7, each with `C247 …: cdx 5, spdx23 5, spdx3 absent`.

**The fixes.**

| mutation | fails |
|---|---|
| `row_verdict` returns `pass` for a broken invariant | 4 `parity::verdict` unit tests |
| `row_verdict` fails every `SymmetricEqual` row (T003) | `holistic_parity`, every fixture: "disagree" |
| R7: the `metadata.component` merge removed (T011) | `s1654_promoted_main_module_keeps_its_component_properties` |
| R8: the lone-component arm restored | 6 of 10 `version_string_static` |
| R4a: the either-endpoint rule restored | 1 cdx extractor unit test; 7 of 8 `closure_resolved_tier` |
| R4b: the late pass-3 call removed (T009) | `edges_into_excluded_members_are_build_dependencies_in_spdx23` |
| R6: `has_complete_record` ignores the closure flag | 8 of 8 `closure_resolved_tier` (E1) |
