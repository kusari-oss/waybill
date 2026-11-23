# T012: mutations (SC-004, research R8)

`mutate.py` is copied from spec 1645's runner. Each mutation is an exact-string replacement, applied after backing up its file with `cp`. Its killing tests run one cargo invocation per filter, and the file is then restored with `cp`. The runner refuses to call a mutation killed if the mutated tree does not compile, or if a filter runs no test (a harness error).

The run covered T010's and T011's wiring in the working tree. Every anchor was checked to occur exactly once in its file before the run.

| ID | Mutation | Result | Killed by |
|---|---|---|---|
| M1 | absorbed methods not applied at S1 | **killed** | `b1_banner_and_pc`, `b2_banner_and_cmake` |
| M2 | a banner match keeps `PackageDatabase` | **killed** | `b3_banner_alone`, `b1_nul_preceded_banners_are_byte_identical_to_main` |
| M3 | a fingerprint match keeps `PackageDatabase` | **killed** | `f1_fingerprint_alone` |
| M4 | absorbed methods not deduplicated by technique | **killed** | `s1646_one_method_per_technique_and_chains_carry` |
| M5 | SPDX `methods` emitted for a single method | **killed** | `r1_record_into_record`, `b3_banner_alone`, `s1646_d1_methods_only_when_two` |
| M6 | the binary-side condition removed | **killed** | `s1646_a_record_into_a_record_absorbs_nothing` |

**6 of 6 killed.** Afterwards, `diff -r` of `waybill-cli/src` against a copy taken before the run showed no difference, and all targets were rebuilt.

Two kills come only from unit tests, and for a reason:
- **M4**: F-2's banner and fingerprint are on one binary, so they are already one entry (milestone 096) and only one absorption happens. The integration tree cannot exercise the duplicate; the chain unit test does.
- **M6**: R-1's two records are both `PackageDatabase`, so with or without the condition nothing differs and nothing is absorbed. The unit test merges records with differing techniques, which is the case the condition exists for.
