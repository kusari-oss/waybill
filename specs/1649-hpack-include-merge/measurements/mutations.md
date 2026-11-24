# T017: mutations (SC-005)

`mutate.py` is spec 1648's runner with a new table. Each mutation:
- backs up the files it edits with `cp`;
- applies exact replacements, refusing unless each anchor occurs once;
- runs its killing tests, one cargo invocation per filter;
- restores the files with `cp`.

A compile error or a filter matching no test is reported as such, never as a kill. Afterwards both sources matched their pre-run SHA-256 (`shasum -c`: OK), and the binary was rebuilt.

## Results

```
Ma merge precedence reversed (last merge wins): KILLED by m3_first_merge_wins, scan_fs::package_db::haskell::hpack::prep::tests::s1649_fold_first_merge_wins
Mb explicit-over-merge reversed: KILLED by m4_explicit_beats_merge_in_either_order, scan_fs::package_db::haskell::hpack::prep::tests::s1649_fold_explicit_beats_merge_in_either_order
Mc the rename also rewrites a non-<< key: KILLED by scan_fs::package_db::haskell::hpack::prep::tests::s1649_rename_touches_only_line_keys
Mc2 the rename ignores block scalars and quoted strings: KILLED by scan_fs::package_db::haskell::hpack::prep::tests::s1649_rename_touches_only_line_keys
Md includes resolved relative to the manifest: KILLED by i2_nested_include_resolves_relative_to_the_including_file, scan_fs::package_db::haskell::hpack::prep::tests::s1649_splice_nested_include_is_relative_to_the_including_file
Me the root bound removed: KILLED by u2_include_outside_the_root_is_not_read, scan_fs::package_db::haskell::hpack::prep::tests::s1649_bound_outside_root_existing_and_missing
Mf the depth bound removed: KILLED by u4_include_chain_too_deep, scan_fs::package_db::haskell::hpack::prep::tests::s1649_bound_depth
Mg names lowercased again (FR-009): KILLED by l1_names_keep_their_declared_spelling
Mh package:qualifier kept whole (FR-009): KILLED by l2_qualified_names_name_the_package
Mi a single-mapping when: read as nothing (FR-010): KILLED by l3_when_as_one_mapping
Mjk both partial guards removed: the fold drops a tagged merge and an unresolved include is not counted: KILLED by u3_include_cycle_terminates_partial, u4_include_chain_too_deep
Mj only the fold guard removed (expected to survive: the unresolved count still marks it partial): SURVIVED
Mk only the unresolved count removed (expected to survive: strip_tagged still finds the tag): SURVIVED
```

**All 11 mutations in the plan's list, plus the extensions, are killed.** Each FR-009 and FR-010 change has its own (Mg, Mh, Mi), and Mc2 covers the rename's block-scalar and quoted-string skip.

## The two partial guards are redundant by design (Mj, Mk)

An unresolved include marks the manifest `partial` in two independent ways:
- **The tag survives.** The fold leaves a tagged merge in place, and `strip_tagged` finds it.
- **The resolution list is checked.** `read_manifest` counts any unresolved include.

Removing either one alone survives, as predicted before running. Removing both (Mjk) is killed by U3 and U4, the cases whose unresolved include sits under a `<<` key.

Each guard is kept: the fold guard keeps the tag, and so the `strip_tagged` path, meaningful, while the count states FR-004's rule directly rather than relying on a side effect.
