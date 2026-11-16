# Mutations (T011, SC-004)

Each mutation was an exact string replacement. After it:
- the feature's tests ran with `--no-fail-fast`: the `cmake_download` unit tests and `tests/cmake_declared_datasets.rs`;
- the file was restored with `cp`.

`git status` was clean afterwards. Base commit: `1985cbe7`.

| # | Mutation | Tests that failed |
|---|---|---|
| M1 | Classify a list-loop download as a dataset without a paired digest list | `in_lists_and_literal_items_resolve_without_digests`, `unequal_lists_are_counted_not_emitted` |
| M2 | Accept `$ENV{…}` in a URL | `the_index_loop_shape_resolves_with_digests`, `unequal_lists_pair_no_digest_and_count`, `unequal_lists_are_counted_not_emitted`, `us1_declared_download_lists_become_datasets` |
| M3 | Identity by URL rather than element (no fallback de-duplication) | `the_index_loop_shape_resolves_with_digests`, `two_files_one_element_and_disagreeing_digests`, `us1_declared_download_lists_become_datasets` |
| M4 | Drop the element's namespace from the PURL | `us1_declared_download_lists_become_datasets` |
| M5 | CycloneDX type `library` instead of `data` | `us2_datasets_are_native_data_kinds` |
| M6 | A version taken from the element | `us1_declared_download_lists_become_datasets` |
| M7 | Keep a digest when declarations disagree | `two_files_one_element_and_disagreeing_digests` |

All seven are killed.

## The SPDX 3 gate is not blind

`us2_datasets_are_native_data_kinds` runs `spdx3-validate` 0.0.5 through the offline wrapper. To check that it enforces the Dataset profile, the same fixture's SPDX 3 output was validated twice:

| Document | Exit | Violations |
|---|---|---|
| As emitted | 0 | 0 |
| With `dataset_datasetType` removed from the `dataset_DatasetPackage` | 1 | 2, naming `Dataset/DatasetType` |
