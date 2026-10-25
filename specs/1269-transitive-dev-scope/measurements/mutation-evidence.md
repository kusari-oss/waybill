# Mutation evidence (T014, SC-003)

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25
**Subject**: `waybill-cli/src/resolve/scope_propagation.rs`
**Command**: `cargo test -p waybill --bin waybill resolve::scope_propagation`

SC-003 asks for a positive control that is *demonstrated* rather than
merely present: a test that passes whether or not the guard it names
exists proves nothing, and a pass that over-propagates satisfies every
count-based check in US1 while silently demoting dependencies that
ship. Each guard was therefore deleted in turn and the suite re-run.

Baseline, unmutated: **14 passed; 0 failed**.

| # | Mutation | Guard removed | Result | Test that caught it |
|---|---|---|---|---|
| M1 | `if immune.contains(purl) { continue }` deleted from Phase B | FR-002 absolute immunity | **1 failed** / 13 passed | `a_runtime_reachable_component_is_never_demoted` |
| M2 | `if scope_by_purl.contains_key(purl) { continue }` deleted | FR-003 strictly additive | **3 failed** / 11 passed | `a_reader_assigned_scope_is_never_changed`, plus `an_already_correct_document_is_untouched` and `a_dev_only_transitive_is_marked` on the assignment count |
| M3 | Phase A traverses *into* non-runtime components | the immunity set's boundary | **5 failed** / 9 passed | `a_dev_only_transitive_is_marked`, `a_cycle_terminates_and_every_member_is_marked`, `an_unscoped_inbound_path_does_not_block_marking`, `the_strongest_inbound_scope_wins_in_either_order`, `a_typed_dependency_edge_is_still_traversed` |
| M4 | `if !ecosystems_with_root.contains(..) { continue }` deleted | FR-012 decline where immunity is unestablishable | **1 failed** / 13 passed | `an_ecosystem_without_a_root_is_left_alone` |

Restored after each mutation; final re-verification: **14 passed; 0
failed**.

## Reading these results

**M1 is the one SC-003 names.** Without the immunity check, `shared` —
a component the root depends on directly *and* which dev tooling also
depends on — is demoted to `development`. That is the under-reporting
failure: it is invisible in orphan counts, invisible in the I3
violation counts US1 is measured by, and worse than the bug being
fixed, because a consumer filtering to runtime components loses
something that genuinely ships. The control fails on it, by name.

**M3's blast radius is the interesting one.** Letting Phase A walk
through a non-runtime component makes the immunity set swallow the
entire dev closure, so nothing is ever marked — the feature becomes a
silent no-op that passes `a_runtime_reachable_component_is_never_demoted`
(the direction it over-protects) while failing every marking test. The
two guards fail in opposite directions, which is why both are tested
rather than inferring one from the other.

**M2's collateral is not noise.** Dropping the never-overwrite guard
makes the pass re-stamp components a reader already classified, so the
assignment count rises and `a_dev_only_transitive_is_marked` fails on
`assigned_total() == 1`. A count assertion catching a correctness
mutation is the count doing its job.

## What this does not establish

That the pass fixes the measured corpus populations — that is T024 and
the corpus lane (SC-001, SC-002), against real documents rather than
fixtures. These mutations establish only that the fixtures can fail,
which is the precondition for their passing to mean anything.
