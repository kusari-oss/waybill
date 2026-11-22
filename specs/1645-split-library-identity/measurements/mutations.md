# T016: mutations (SC-004, research R9)

`mutate.py` is copied from spec 1644's runner. Each mutation is an exact-string replacement, applied after backing up its file with `cp`; it runs its killing tests one cargo invocation per filter, then restores the file with `cp`.

The runner refuses to call a mutation killed when the mutated tree does not compile, or when a filter runs no test (a harness error). Run with all of T011–T014's wiring in the working tree.

| ID | Mutation | Result | Killed by |
|---|---|---|---|
| M1 | ncurses has its slug only (`names()`) | **killed** | `m1_name_variant_is_one_component`, `s1645_names_and_slugs_round_trip` |
| M2 | any record name accepted (name check removed) | **killed** | `n2_bundled_library_is_not_merged`, `s1645_each_verdict` |
| M3 | version agreement is a plain string prefix | **killed** | `n3_string_prefix_does_not_agree`, `s1645_versions_agree_at_a_dot` |
| M4 | two qualifying records merge with the first | **killed** | `n4_two_qualifying_records`, `s1645_each_verdict` |
| M5 | the LLVM suffix dropped (`parse_semver_triple` restored) | **killed** | `m2_version_suffix_is_one_component`, `s1645_ncurses_keeps_its_patch_date` |
| M6 | edges to the renamed record not remapped | **killed** | `e1_requires_follows_the_merge`, `s1645_merge_rewrites_the_pair_and_its_edges` |
| M7 | the ncurses patch date dropped (`parse_ncurses_version` restored) | **killed** | `b1_nul_preceded_banners_are_byte_identical_to_main`, `s1526_expat_and_ncurses_real_literals` |
| M8 | the ncurses CPE split removed | **killed** | `m1_name_variant_is_one_component`, `b1_nul_preceded_banners_are_byte_identical_to_main` |

**8 of 8 killed.** Afterwards, `diff -r` of `waybill-cli/src` against a copy taken before the run showed no difference, and the binaries were rebuilt.

M7 does not kill M-1, and that is expected rather than a gap. With the date dropped, the curated `6.6` still agrees with `ncursesw.pc`'s `6.6.20251230` as a dotted prefix (FR-005), so the merge still happens at the record's version. What FR-013 adds is the stated version when no record is present, and B-1, which has no record, guards exactly that.
