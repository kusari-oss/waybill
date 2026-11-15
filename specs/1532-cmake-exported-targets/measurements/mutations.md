# Mutations (spec 1532, task T014)

**Method.** Each mutation was applied, and the `installed_lib_targets`,
`installed_lib_us1` and `installed_lib_us3` targets were run under bash with
`--no-fail-fast`, reading the `---- <name> stdout ----` blocks. The file was
then restored with `cp` from a copy saved before the mutation, and the
restore was confirmed with `cmp`. After restoring, the working tree held no
unintended change.

| # | Mutation | Failing tests | Failure |
|---|---|---|---|
| M1 | The raise count is ignored (`raise * 0`) | `us1_exported_files_are_attributed_to_their_package`, `us2_nothing_is_attributed_without_certainty` | `opt/sdk/lib/libops.a` is not in P's evidence, which holds only its version file. |
| M2 | Any `${VAR}/` prefix is resolved, not only `${_IMPORT_PREFIX}/` | `us2_…` | The C225 object reads `not-shipped: 2, unresolved-location: 2`, where it should read 0 and 4. |
| M3 | The same-name rule ignores the install prefix | `the_same_name_rule_stays_within_its_prefix` | `opt/b/lib/libwaybill-fixture-x.a` is attributed to the component from `opt/a`. |
| M4b | Both existence checks removed (root-relative resolution, then `is_file`) | `us1_…` | The missing `libnot-shipped.so` is attributed. |
| M5 | The OS-ownership skip removed | `us2_…` | `owned-by-os-package: 0, read: 12`, where it should read 1 and 11. |
| M6 | Hash-cache seeding disabled | `us1_…` | The log reads `hash cache: seeded=0 reused=0` (FR-005a, analysis U1). |

## Mutations that did not test what they claimed

- **M4, first form.** It removed only the root-relative resolution's
  "missing" branch and fell back to the joined path. The following
  `is_file()` check still counted the file as not shipped, so the mutation
  was equivalent and every test passed. M4b removes both checks and is
  caught.
- **M3, first run.** It survived: no test had a same-name component in
  another prefix. `the_same_name_rule_stays_within_its_prefix` was added,
  and M3 now fails it.
- **M7: spelling an attributed binary by the binary component's own path,
  removed.** This mapping was added for analysis U2: through a directory
  symlink one file could have two spellings, which would miss the
  hash-cache key. Removing it failed no test, including a fixture built for
  exactly that case (`opt/rocm` → `rocm-7.0`). The binary scan reports a
  file under the same root-relative spelling that `resolve_in_root`
  produces, so the mapping changed nothing. The code was removed, and the
  fixture stays as a test,
  `an_attributed_binary_keeps_the_binary_components_spelling`, pinning that
  the two spellings agree.
