# Mutation scorecard — milestone 1277

Produced by `measurements/mutate.py`, which refuses to run on a dirty tree
(its own `git checkout --` restore would otherwise destroy uncommitted work)
and reports `BUILD-ERROR` separately from a caught mutant. A mutant that
does not compile is not evidence a *test* caught it, and conflating the two
is how a scorecard reads nine-of-nine when it is seven plus two compile
failures.

Suites run per mutant: `closure_role_scope`, `public_corpus`, and the
`m1277` unit filter on the bin target.

| mutant | caught | by |
|---|---|---|
| M1 `build-tooling` assigns nothing | yes (13) | the role-mapping and both SPDX tests |
| M2 `artifact-input` is classified | yes (9) | `artifact_input_is_left_alone`, `an_artifact_member_is_not_named_a_build_tool_in_either_format` |
| M3 exemption deleted | yes (9) | `the_exemption_fires_on_the_nulled_member_and_only_on_it` |
| M3b exemption widened to all | yes (11) | the same test, plus `a_non_compiler_supplied_member_in_the_same_role_is_still_classified` |
| M4 `unreferenced` arm removed | yes (7) | `only_the_roles_that_do_not_ship_are_classified`, both SPDX tests |
| **M5 CDX emission moved inside the `include_dev` gate** | **no — equivalent mutant** | see below |
| M6 C203 emitted unconditionally | yes (1) | `the_classification_source_has_three_states_and_absence_is_one` |
| M7 SPDX 2.3 `BUILD_TOOL_OF` omitted | yes (2) | `both_spdx_formats_name_the_same_build_tools` |
| M8 exemption ignored in SPDX 2.3 only | yes (3) | `the_exemption_governs_all_three_formats` |
| M10 classification also carried on `lifecycle_scope` | yes | `the_classification_survives_exclude_scope_build` |

**9 of 10 caught; M5 is equivalent, not a gap.**

## M5's verdict, and why it was the most valuable mutant

M5 moves this feature's CycloneDX emission back inside
`if self.config.include_dev`. It changed **no output** and survived every
suite.

That is not a missing test. `include_dev` is hard-coded `true` at the
`sbom scan` call site (`scan_cmd.rs:5427` and `:3783`) — the
`--include-dev` flag was removed in milestone 052/part-3 and the opt-out
became `--exclude-scope`, applied post-resolution. So on the only code path
that produces closure roles the gate is always open, and no test can
distinguish the two forms. The one other setting of the flag,
`cli/generate.rs:187` (`false`), is on the `sbom generate` path, which never
carries closure roles.

**What it exposed is a mistake in research R1.** That document gave two
measured reasons for not routing the classification through
`LifecycleScope::Build`, and the first was that the CycloneDX emission is
gated on `include_dev`, so a default scan would set no scope. The gate is
real; what feeds it is not what I claimed. I asserted the consequence from
the gate's existence without reading the call site. R1 now records the
reason as **withdrawn**.

The second reason survives and is sufficient: `--exclude-scope build` drops
components by `lifecycle_scope` (`apply_exclude_scope_filter`), so a
classification carried there would be **deletable** — destroying the reason
the `unreferenced` members are emitted at all, since m1035 measured `jq` and
`lua` in that role carrying 6 of one project's 18 CVEs.

**Nothing tested that reason until M5 survived.** The design rested on two
arguments, one of which was false, and the true one had no control. M10 and
`m1277_the_classification_survives_exclude_scope_build` close that: the test
scans with `--exclude-scope build`, asserts the three classified members
survive with their markers, and — as its own inner control — that `nixpkgs`
*is* dropped, so a run in which the flag was silently ignored cannot pass.

## M4 is a substitution, and the original could not be written

The task listed "the role is read from one variant rather than the union".
That is not implementable at this layer: the classifier unions the variants
before the C182 annotation exists, and the pass sees one string. Removing
the `unreferenced` arm exercises the same code for real, and the union is
covered upstream by
`closure::emit::tests::build_variants_merge_into_one_component_and_union_their_roles`.

## Per T034, no survivor was dropped to round the number up

M5 is reported with its verdict rather than deleted, and the count says
9 of 10 rather than 9 of 9. Milestone 1276 discarded two candidates as
invalid and said which; the same standard applies here, and in this case the
"invalid" mutant was the one worth the most.
