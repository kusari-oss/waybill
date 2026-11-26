# Spec 1663 mutations (T012)

Each mutation was applied to a clean copy and run against:
- the `spec1663_*`, `parse_stack*`, `build_snapshot*`, `extract_resolver*` and `orphan_reason` unit tests;
- the `stack_snapshot_identity` integration test.

Each was restored with `cp` from a backup, checked with `cmp`, and the binary rebuilt. The clean run: 49 of 49 pass.

| # | Mutation | Tests that fail |
|---|---|---|
| M1′ | the whole lock rejected when `original` is a string (the pre-fix behaviour) | `parse_stack_lock_reads_a_string_original`; IT |
| M2 | the `lts` path not named | `spec1663_a_stackage_url_is_named_as_its_resolver`, `spec1663_which_field_names_the_snapshot`; IT |
| M3 | nightly not zero-padded | `spec1663_a_stackage_url_is_named_as_its_resolver`; IT |
| M4 | another URL becomes `unknown` | `spec1663_any_other_url_keeps_the_url`; IT |
| M5 | the digits-only check removed | `spec1663_any_other_url_keeps_the_url` |
| M6 | a duplicate snapshot dropped, not merged | IT |
| M7 | the snapshot orphan rule removed | `spec1663_a_source_tier_snapshot_with_a_lock_file_is_a_toolchain_pin` |
| M8 | `source_path` left empty | `spec1663_a_url_snapshot_purl_keeps_its_url`; IT |

"IT" is `every_snapshot_is_named_sourced_and_explained`. M1 (changing the variant to `Resolver(u64)`) didn't compile and was replaced by M1′.

**Test-first evidence.**
- `parse_stack_lock_reads_a_string_original` failed on the unchanged code with the production error, `invalid type: string "lts-24.35", expected struct StackSnapshotOriginal`.
- Scanned by `main`'s binary, the IT fixture gives three `unknown` snapshots, rejects (d)'s lock, and has no `waybill-fixture-dep-d`.

**Deviation.** In the IT fixture, the synthetic document root depends on every snapshot, so none is unreachable there. The IT therefore asserts FR-005 as written, "reachable or `build-toolchain`". The orphan rule itself is shown by its unit test, and end to end on hs-opentelemetry (T013).

**Added at implementation (T017, T018):**

| # | Mutation | Tests that fail |
|---|---|---|
| M9 | a lock's pins confined to its own directory | `a_root_lock_governs_listed_subdirectory_packages` |
| M10 | the `edgeless-lock-pin` rule removed | `spec1663_an_unreached_stack_lock_pin_is_edgeless`, `a_root_lock_governs_listed_subdirectory_packages` |
