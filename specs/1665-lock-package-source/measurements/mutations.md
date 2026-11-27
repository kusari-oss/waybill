# Mutations (FR-009, SC-004, T010)

Script: `measurements/mutate.py`. Each mutation makes one or more exact replacements in `haskell.rs`, then runs `binary(lock_package_source)` plus the two parser tests under nextest. The file is then restored from a backup, and `filecmp` confirms the restore. The baseline before the run was 4 of 4 passing.

| ID | Mutation | Result | Killed by |
|---|---|---|---|
| M1 | stack-lock components lose their source | killed | `every_pin_names_exactly_the_files_that_pinned_it` |
| M2 | freeze components lose their source | killed | the same test |
| M3 | Phase F reverts to first-wins (no merge) | killed | the same test (`reached` lists only `d`) |
| M4 | merge keyed by name instead of PURL | killed | the same test (`split@3.0.0` gains `b`) |
| M6 | merge only within the stack-lock loop | killed | the same test (`reached` loses `a` and `b`) |

**M5, retired as equivalent.** The first draft sorted each merged component's extra source paths. Removing the sort changed no result, even after `e/cabal.project.freeze` was added so that the paths arrive out of order: freeze files are read before lock files, so `e` arrives ahead of `a`. Emission already sorts every component's source files (spec 1542). The output check shows it: `d`, the primary path, appears between `b` and `e`. So the sort was redundant and was removed, and the `e/` case stays in the fixture as the test of that ordering.

**Not mutated: `edgeless-lock-pin`.** That classification is spec 1663's code and this spec does not touch it. `an_unreached_pin_is_still_an_edgeless_lock_pin` guards it against regression.
