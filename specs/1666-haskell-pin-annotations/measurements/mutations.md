# Spec 1666 mutations (T012)

Filter: `test(~s1666) | test(~spec1666) | test(~spec1663) | test(~twin_merge) | test(~m926) | test(~m1457) | binary(haskell_pin_annotations)`

| Mutation | Result | Failing tests |
|---|---|---|
| M1 note is a fixed string | killed | s1666_freeze_note_names_the_freeze_file, s1666_stack_lock_note_names_the_lock, s1666_two_lock_files_are_named_sorted |
| M2 no main-module skip | killed | s1666_main_module_carries_no_disagreement |
| M3 never remove C172 on equality | killed | s1666_c174_counts_the_c172_left_after_the_merge, s1666_c172_is_removed_when_the_closure_built_nixpkgs_version |
| M4 never rewrite emitted | killed | s1666_c172_is_rewritten_when_the_closure_built_another_version, the_survivor_keeps_its_identity_and_gains_the_declaration |
| M5 no hackage-freeze alternative | killed | an_unused_freeze_pin_is_an_edgeless_lock_pin, spec1666_an_unreached_freeze_pin_is_edgeless, i7_every_unreachable_component_carries_a_reason |
| M6 no C174 subtraction | killed | s1666_c174_counts_the_c172_left_after_the_merge |

Control, unmutated: Summary [   0.403s] 75 tests run: 75 passed, 5567 skipped; failing: none
