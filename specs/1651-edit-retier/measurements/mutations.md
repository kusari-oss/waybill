# T017: mutations (SC-006)

`mutate.py` is spec 1650's runner with a new table: `cp` backups, exact single-anchor replacements, one cargo invocation per killing filter, `cp` restore. Afterwards all three sources matched their pre-run SHA-256 (`shasum -c`: OK), and the binary was rebuilt.

```
Ma the run-local set removed (sequential relabelling): KILLED by r2_each_component_is_relabelled_at_most_once
Mb the marker not written (CycloneDX): KILLED by r1_design_becomes_source_and_nothing_else_changes, edit::tests::s1651_set_tier_keeps_the_first_marker_in_every_format
Mc changed miscounted: KILLED by r1_design_becomes_source_and_nothing_else_changes, r2_each_component_is_relabelled_at_most_once
Md the type never recomputed: KILLED by t1_derived_type_is_recomputed_everywhere
Me the type always recomputed (asserted types overwritten): KILLED by t2_asserted_type_is_kept
Mf completeness downgraded by a retier: KILLED by c1_completeness_is_untouched
Mg TO validation removed: KILLED by refusals_name_the_problem_and_write_nothing, cli::edit::tests::s1651_parse_retier
```

**All 7 are killed**, one for each rule SC-006 lists: at-most-once (Ma), the marker (Mb), the counts (Mc), the type recomputed (Md) and kept when asserted (Me), completeness untouched (Mf), and `TO` validation (Mg).
