# T013: mutations (SC-006)

`mutate.py` is spec 1649's runner with a new table. Each mutation backs up the files it edits with `cp`, applies exact replacements (refused unless each anchor occurs once), runs its killing tests one cargo invocation per filter, and restores with `cp`. Afterwards both sources matched their pre-run SHA-256 (`shasum -c`: OK), and the binary was rebuilt.

```
Ma FR-002 removed (the evaluated package keeps design): KILLED by p2_evaluated_package_is_build_whatever_c214_says, r1_the_graph_does_not_hang_off_design, resolve::closure_resolved_tier::tests::s1650_evaluated_package_becomes_build_whatever_c214_says
Mb FR-003 removed (compiler-supplied boot libraries keep design): KILLED by b1_declared_boot_library_is_build, resolve::closure_resolved_tier::tests::s1650_compiler_supplied_boot_library_becomes_build
Mc the design guard removed (any tier is re-tiered): KILLED by resolve::closure_resolved_tier::tests::s1650_other_tiers_are_untouched
Md I11 clause (a) removed: KILLED by corpus_harness_195::invariants::checks::closure_resolved_tier::tests::v1_evaluated_package_design_fails, corpus_harness_195::invariants::checks::closure_resolved_tier::tests::v2_compiler_supplied_design_fails
Me I11 clause (b) removed: KILLED by corpus_harness_195::invariants::checks::closure_resolved_tier::tests::v3_root_hangs_off_design_fails
Mf the evidence check removed (every design component re-tiered): KILLED by p3_unevaluated_repository_package_stays_design, b3_unresolved_component_stays_design, resolve::closure_resolved_tier::tests::s1650_design_without_closure_evidence_is_untouched
```

**All 6 are killed.** Each rule has a test that fails without it:
- FR-002 (Ma) and FR-003 (Mb);
- the `design`-only guard (Mc);
- the evidence requirement (Mf), which is SC-006's "FR-004's unresolved exemption removed";
- I11's two clauses (Md, Me).
