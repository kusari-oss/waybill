# T015: mutations (SC-003, research R9)

`mutate.py` (in this directory) ran each mutation at `b94a5a19`. For each, it backs up every file the mutation edits with `cp`, applies exact-string replacements, runs each killing filter as its own `cargo test -p waybill` invocation, and restores the files with `cp`.

The runner refuses to call a mutation killed when:
- it does not compile (reported as a compile error);
- a filter ran no test (reported as a harness error).

M8 and M9 are additions to R9's seven. M8 covers the amended boundary's text test. M9 covers the census amendment itself.

| ID | Mutation | Result | Killed by |
|---|---|---|---|
| M1 | bzip2 versioned without its anchor | **killed** | `n1_bzip2_without_anchor`, `s1644_no_anchor_no_verdict` |
| M2 | GMP versioned without its anchor | **killed** | `n2_gmp_without_anchor`, `s1644_no_anchor_no_verdict` |
| M3 | an imported symbol anchors liblzma (definedness removed on ELF and Mach-O, `scan.rs`) | **killed** | `n3_liblzma_imported_symbol`, `n9_liblzma_macho_imported_symbol`, `imported_or_unlisted_symbol_anchors_nothing` |
| M4 | the first of two candidates taken | **killed** | `n4_gmp_two_candidates_is_ambiguous`, `s1644_two_candidates_are_ambiguous` |
| M5 | a shared candidate versions both libraries (FR-005a removed) | **killed** | `n5_shared_candidate_versions_neither`, `s1644_a_shared_candidate_versions_neither` |
| M6 | a GMP candidate need not be the whole string | **killed** | `s1644_candidate_boundaries` |
| M7 | the xz grammar accepts a commit suffix | **killed** | `n7_liblzma_snapshot_suffix`, `s1644_anchor_with_no_candidate` |
| M8 | a space counts as a binary boundary | **killed** | `s1644_candidate_after_binary_data` |
| M9 | the census amendment reverted: only NUL may precede a candidate | **killed** | `g1_gmp_after_binary_data`, `g1_census_gmp_after_binary_data`, `s1644_candidate_after_binary_data` |

**9 of 9 killed.** After the run, `git status --short waybill-cli` listed 0 changed files.

M6 and M8 are killed by unit tests only. No integration fixture exercises a version with trailing text, or one preceded by a space and then a binary byte. Both rules are byte-level and covered directly by the unit tests.
