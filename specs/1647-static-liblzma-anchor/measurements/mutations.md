# T013: mutations (SC-004, research R7)

`mutate.py` (spec 1646's runner; only the table differs) applies each mutation as an exact-string replacement. It runs the listed tests one cargo invocation per filter and restores every edited file with `cp`. A mutation that does not compile counts as an error, never as killed. A filter that runs no test is a harness error.

Run on the wired tree (T011, plus T012's test):

```
python3 -I -B specs/1647-static-liblzma-anchor/measurements/mutate.py $PWD <scratch>/mut-1647
```

| Mutation | What it changes | Result |
|---|---|---|
| M1 | the `.symtab` read keeps undefined entries | **killed** by `s3_undefined_symtab_symbol_never_anchors`, `importer_undefined_in_both_tables_never_anchors` |
| M2 | the `.symtab` anchors are dropped | **killed** by `s1_…`, `s2_…`, `s5_shared_candidate_versions_neither` |
| M3 | the `.strtab` gate's needle never matches | **killed** by `s1_…`, `s2_…` (the gate is on the path, not dead) |
| M4 | an anchor in both tables counts as `.symtab`-only | **killed** by `s6_both_tables_as_main` and unit `s1647_rule_a_absent_when_dynsym_already_anchors` |
| M5 | a `.dynsym` anchor is labelled `symtab` | **killed** by `defined_anchor_symbol_anchors_elf_and_macho` (spec 1644) and `s6_both_tables_as_main` |

**5 of 5 killed.**
- **Restoration:** after the run, `diff -r` between a copy of `waybill-cli/src` taken before it and the tree gave 0 lines, and the binary was rebuilt.
- **M5 is added beyond research R7's M1–M4.** It checks that the census's table label has a test behind it.
- **R7's planned "M5"** (the magic accepted with two candidates) applied only if FR-004 were adopted. It was rejected, so that mutation does not exist.
