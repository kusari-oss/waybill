# Spec 1662 mutations (T009)

Each mutation was applied to `waybill-cli/src/scan_fs/package_db/haskell.rs`, then the run covered:
- the `spec1662_*` and `cabal_project_packages_tests` unit tests;
- the `hpack_project_membership` integration tests.

Each was reverted with `cp`, checked with `cmp`, and the binary rebuilt afterwards.

| # | Mutation | Tests that fail |
|---|---|---|
| M1 | stand-ins not passed to the matcher | `undeclared_package_yaml_is_not_emitted`, `a_cabal_project_beside_a_stack_file_still_governs`, `a_dropped_manifest_is_absent_from_c200` |
| M2 | stand-in always uses the directory name | `spec1662_every_entry_form_gives_a_package_yaml_the_cabal_verdict` |
| M3 | glob built without `literal_separator(true)` | `spec1662_every_entry_form…`, `spec1662_a_package_yaml_nested_under_a_declared_package_is_not_claimed`, plus #1032's `a_directory_entry_claims_only_its_own_cabal_files` and `a_glob_entry_claims_each_matching_directory` |
| M4 | "nothing declared" counts `.cabal` only | the same three as M1 |
| M5 | dropped manifests kept (and recorded in C200) | M1's three, plus `a_generated_pair_is_one_component_or_none` |
| M6 | an unnamed or unreadable manifest gets no directory-name fallback | `spec1662_an_unnamed_or_unreadable_manifest_stands_in_under_its_directory_name` |
| M7 | every `package.yaml` in a governed tree dropped | the same three as M1 |
| M8 | the "nothing matched, so leave the tree alone" fallback removed | `entries_matching_nothing_leave_the_tree_alone` |
| M9 | `package.yaml` filtered with no `cabal.project` | `a_stack_only_project_is_unaffected`, `ungoverned_package_yamls_are_both_emitted` |
| M8+M10 | M8, plus a missing `packages:` field read as an empty list | `a_project_without_packages_governs_nothing`, `entries_matching_nothing_leave_the_tree_alone` |

`a_project_without_packages_governs_nothing` survives M8 and M10 separately: a project with no `packages:` is protected by two independent guards. It fails only when both are broken, which is what it's there to show.

**Test-first evidence for T006.** The fixture tree was scanned with `main`'s binary (`6ea77ef5`), which emits both `waybill-fixture-hpack-a@0.1.0` and `waybill-fixture-hpack-b@0.1.0`. The branch emits only `…-a`. M1 restores `main`'s behaviour and fails the test. The change was written in the same edit as T004, before the test, and these two runs are the evidence that the test can fail.

**Placement deviation.** T010's cases need a full scan, so they're in `waybill-cli/tests/hpack_project_membership.rs`, not in `haskell.rs` as the task said.

**After the `stack.yaml` union (R7):**

| # | Mutation | Tests that fail |
|---|---|---|
| M11 | `stack.yaml` entries not added | `a_stack_file_beside_a_cabal_project_adds_its_packages`, `the_cabal_reader_takes_the_same_union` |

`a_cabal_project_beside_a_stack_file_still_governs` (in the M1, M4, M5 and M7 rows above) asserted the earlier `cabal.project`-only rule. It was replaced by the two union tests. Its role, a declared package kept and an undeclared one dropped with a stack file present, is now carried by `a_stack_file_beside_a_cabal_project_adds_its_packages` (package `c`).

**Spec 1652 tests changed by this spec.** The first gate run failed `project_discovery_haskell::us3_f3_unlisted_package_recorded` and `us2_strict_ignores_declarations`. Both fixtures (`f3-cabal-project`, `f4-cabal-project-json`) have `tools/e/package.yaml`, which `packages: local-packages/*` doesn't cover.
- **Before:** a `package.yaml` slipped past #1032, so root-only saw `tools/e` and recorded it as a scope drop.
- **Now:** `tools/e` isn't a project member, as a `.cabal` there already wasn't, so it's absent in every mode.

The tests now assert that:
- `us3_f3_unlisted_package_is_not_a_member` (renamed): `tools/e` is absent under `all` and `root-only`, and there's no drop record;
- `us2_strict_ignores_declarations`: strict's record names `[A, B]`.

Both fail under M5. `us3_f2_drop_one` still covers root-only's drop record.
