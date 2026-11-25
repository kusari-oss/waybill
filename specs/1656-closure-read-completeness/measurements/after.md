# After: spec 1656

## Controls (T001, T005, T006, T010)

Before-state: `controls-before.md`. After the change, all pass:

| test | file | fixture | asserts |
|---|---|---|---|
| `m1720_a_declaration_gap_is_a_completeness_reason` | `closure_structured_attrs.rs` | `CLOSURE_JSON` (declaration gap and 6 of 6 contents unread) | C104 `partial`; C105 contains `closure-declarations-unconsulted: 1 declaration(s) in 1 derivation(s)` (the read row's own counts), then `closure-contents-unread: 6 of 6 member(s)`; SPDX 2.3 and SPDX 3 carry the identical C105; parity rows C104 and C105 `pass` |
| `m1720_native_claims_and_orphan_reasons_are_unchanged` | `closure_structured_attrs.rs` | same | no `complete` composition or SPDX 3 relationship; orphan reasons equal `controls-before.md`'s 5 |
| `m1720_unread_contents_are_a_completeness_reason` | `boot_library_edges.rs` | stub, plain project | no read row; `closure-contents-unread: 4 of 4 member(s)`; no declarations code; full `assert_parity` |
| `m1720_a_fully_read_closure_names_neither_gap` | `closure_binary_contents.rs` | `populated_store()` plus both of `waybill-fixture-lib`'s outputs (`out`, `doc`) | absent/partial/failed all 0; no read row; C105 exactly `transitive-edges-unresolvable: cargo, golang, hackage` (its other reason, unchanged) |

- **The negative control** passed before the change too, as it should: it fails only if a closure-read code fires when neither gap exists.
  - A first attempt gave the library one output file. That made it `partial`, because the library declares `out` and `doc`, and that fixture error was caught by the test's own CONTROL assertion.
- **The declaration-gap fixture's parity:** it fails A7, B1 and B2 identically before the change. That was measured by stashing `waybill-cli/src` and running parity-check on the same fixture: `fails=["A7","B1","B2"]`. Those gaps are filed as #1700 and #1701. The test therefore asserts the rows this feature writes (C104 and C105 `pass`) through a new shared helper, `common::parity::assert_rows_pass`, which treats `not-checked` as a failure.

## Unit tests (T004, T009)

There are 7 new unit tests, all passing:
- the wire text, including the carrier-only form;
- neither gap;
- each gap alone;
- order after `project-discovery-scope`;
- empty `unknown` stays `unknown`;
- predicate and codes agree on five combinations;
- `the_closure_read_codes_degrade_no_ecosystem`.

## Mutations (T008)

Each was applied by an exact-string replacement and reverted with `cp`, verified with `cmp`. Each run is checked to have produced all 5 `test result` lines.

| mutation | tests failing |
|---|---|
| M1: CycloneDX builder passes `None` | 11. The stub scans fail C105 parity in `scan_with`'s `assert_parity` (10 in `boot_library_edges`), plus `m1720_a_declaration_gap…` |
| M2: SPDX 3 passes `None` | 11, the same set as M1 |
| M3: `closure_read_gaps` drops `contents` | `m1720_unread_contents…`, `boot_library_edges_are_complete`, `m1720_a_declaration_gap…`, and milestone 1378's `m1378_unread_members_withhold_the_completeness_claim_in_both_formats`, because the shared predicate also feeds the root claim |
| M4: `apply_closure_read` appends nothing | 4 unit tests (`each_gap_alone…`, `both_gaps_follow…`, `an_empty_unknown_set…`, `the_predicate_and_the_codes_agree`) and 3 integration tests |

**Harness fault, caught and corrected.** The first mutation run passed the integration test list as one zsh variable. zsh does not split words, so cargo rejected it with `unexpected argument`, and no integration test ran in any of the four. The mutations appeared to survive. Running M2 by hand showed that it fails the declaration-gap test, which exposed the harness. The run above uses a bash array, and checks the count of `test result` lines so that a run which executes nothing cannot look like a pass.

## Corpus goldens (T013)

Pending the dispatch. Research R5's expectations are scored here afterwards.
