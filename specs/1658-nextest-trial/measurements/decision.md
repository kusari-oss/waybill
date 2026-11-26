
## #1716 check (T019)

The slowest test per lane in CI run 45728714707, under nextest:

| lane | slowest | next |
|---|---|---|
| Linux | `sbom_edit_filter::i1454…` 115.2 s | `spdx3_conformance::every_existing_golden_passes_validator` 32.1 s |
| macOS | `holistic_parity::parity_synthetic_container_image` 28.6 s | `sbom_edit_filter::i1454…` 26.5 s |
| Windows | `holistic_parity::parity_synthetic_container_image` 48.9 s | `cisa_2026_coverage_matrix::test_native_verdicts_have_non_empty_values` 43.6 s |

#1716's self-scan tests (`binary_build_provenance`, `binary_id_enrich`, and the others that scan the debug binary) are not the floor on any lane; the slowest of them is 15.4 s on macOS. **#1716 stays filed.**

The Linux floor is `i1454` at 115 s. That is 3.5× its local time under nextest (about 50 s) and 4.3× macOS CI's 26.5 s, after #1711's fix. Recorded as a follow-up to #1711, not fixed here.
