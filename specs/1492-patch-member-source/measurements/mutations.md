# Mutations — spec 1492 (#1502)

Each was applied to `waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`, then restored from a `cp` backup and confirmed byte-identical with `cmp`. Suites: `cargo test --bin waybill nix::closure` and `cargo test --test closure_members`.

```
M1 (T011) the patches rung removed:
  unit: scan_fs::package_db::nix::closure::graph::tests::s1492_the_walk_stops_at_a_listed_patch
  unit: scan_fs::package_db::nix::closure::member::tests::s1380_checks_are_check
  unit: scan_fs::package_db::nix::closure::member::tests::s1380_only_package_members_move_and_only_between_endpoints
  unit: scan_fs::package_db::nix::closure::member::tests::s1380_source_trees_and_patches_are_source
  unit: scan_fs::package_db::nix::closure::member::tests::s1492_a_listed_generated_patch_is_source_not_scaffolding
  unit: scan_fs::package_db::nix::closure::member::tests::s1492_a_listed_patch_is_source_whatever_its_name
  unit: scan_fs::package_db::nix::closure::member::tests::s1492_the_endpoint_helper_agrees_with_classify_given_the_patch_set
  integration: s1380_each_member_carries_the_role_it_is
  integration: s1380_filtering_package_yields_packages_only
  integration: s1380_the_role_and_its_count_agree_across_formats
  integration: s1492_a_listed_local_patch_is_an_edge_target_in_every_format
  integration: s1492_a_listed_local_patch_is_source_in_every_format
  integration: s1492_the_graph_gains_only_the_listed_bare_patch
M2 (T014) the endpoint helper ignores the patch set:
  unit: scan_fs::package_db::nix::closure::graph::tests::s1492_the_walk_stops_at_a_listed_patch
  unit: scan_fs::package_db::nix::closure::member::tests::s1492_the_endpoint_helper_agrees_with_classify_given_the_patch_set
  integration: s1492_a_listed_local_patch_is_an_edge_target_in_every_format
  integration: s1492_the_graph_gains_only_the_listed_bare_patch
restored, identical to backup
```

**M1 (T011, SC-005):** removing the rung fails every listed-patch test in both suites. It also fails two spec 1380 tests (`s1380_checks_are_check`, `s1380_filtering_package_yields_packages_only`), because the rung now carries spec 1380's own `patches` evidence, which this feature moved out of the version-name rung (FR-010).

**M2 (T014, research R2):** ignoring the patch set in the endpoint helper fails only the four graph tests. Every classification test passes. The role and the graph are therefore wired separately, and the patch-set argument is what keeps them in agreement.

Restored: 231 unit and 25 integration tests pass.
