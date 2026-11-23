# T015: mutations (SC-003, research R7)

`mutate.py` (spec 1647's runner; only the table differs) applies each mutation as exact-string replacements. It runs the listed tests one cargo invocation per filter and restores every edited file with `cp`. A mutation that does not compile counts as an error, never as killed. A filter that runs no test is a harness error.

Run on the wired tree (T012, with T014's unit test):

| Mutation | What it changes | Result |
|---|---|---|
| M1 | `dynamic-linkage` decided `PackageDatabase` | **killed** by `k1_dynamic_linkage_is_binary_analysis`, `k6_deduped_dynamic_linkage_lists_one_method`, `s1648_each_kind_has_its_decided_technique` |
| M2 | `embedded-source-path` decided `PackageDatabase` | **killed** by `k2_embedded_source_path_is_binary_analysis`, `s1648_each_kind_has_its_decided_technique` |
| M3 | `cargo-auditable` decided `BinaryAnalysis` | **killed** by `k3_cargo_auditable_keeps_manifest_analysis`, `s1648_a_record_absorbs_the_newly_binary_kinds` |
| M4 | the deduplicator's gate reverted to spec 1646's two kinds | **killed** by `s1648_a_record_absorbs_the_newly_binary_kinds` |
| M5 | `python-stdlib-collapsed` left without a decision | **killed** by `s1648_every_binary_kind_has_a_technique` |

**5 of 5 killed.**
- **Restoration:** after the run, `diff -r` between a copy of `waybill-cli/src` taken before it and the tree gave 0 lines, and the binary was rebuilt.
- **M4 is killed only by the unit test.** No counted tree and no integration case has a deduplicator merge with either kind (`census.md`, `control.md` K6). The rule is held by the unit test alone.
