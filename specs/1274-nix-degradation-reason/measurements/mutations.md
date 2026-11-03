# Mutation testing — milestone 1274

**SC-006 / FR-015**: every requirement is covered by a test that fails when
the behaviour is removed, mutation-tested rather than assumed.

Harness: `mutate.py` (carried over from milestone 1273). It refuses to report
a verdict for a mutation it could not apply — an unchanged file is an
**ERROR**, not a SURVIVED — and distinguishes a compile failure from a failing
test. Three mutations in milestone 1273 reported SURVIVED without ever being
applied, and a mutant that survives unapplied looks exactly like a missing
test.

Each verdict names the suites it is relative to, because a run scoped to one
suite can report a survivor the wider suite catches.

## Why these were run before the final gate rather than after

The tests in `nix_ifd_reason.rs` and `nix_degradation_detail.rs` were written
**after** the implementation, not before it, so they were never observed
failing for the right reason. That is a departure from the task plan's
"observe each test fail for the right reason", and it is recorded rather than
glossed: mutation is the only remaining way to establish non-vacuity, so M1,
M2 and M3 were run immediately after the suites first went green rather than
being deferred to Phase 5.

| # | task | requirement | mutation | suites | verdict | caught by |
|---|---|---|---|---|---|---|
| M1 | T030 | FR-007 | `rposition` → `position` in `salient_stderr` — the real pre-change state | `nix_ifd_reason`, `nix_config_closure` | **CAUGHT** | `a_reworded_clause_degrades_to_generic`, `a_refusal_alongside_another_error_is_still_a_refusal` |
| M2 | — | FR-001 | the classifier never fires (always `EvaluationFailed`) | `nix_ifd_reason` | **CAUGHT** | `refusal_is_named_in_the_document`, `a_refusal_alongside_another_error…`, `the_reason_is_logged_too` |
| M3 | T031 | FR-005a | match the bare setting name anywhere in stderr | `nix_ifd_reason` | **CAUGHT** | `an_unrelated_failure_keeps_the_generic_reason` |
| M4 | T032 | FR-005c | classify from the **selected line only** | `nix_ifd_reason`, `nix_degradation_detail` | **CAUGHT** | `a_refusal_alongside_another_error_is_still_a_refusal` |
| M5 | T033 | FR-013 | collapse `ifd-refusal-unverified` into the new wire string | `nix_ifd_reason`, `nix_config_closure` | **CAUGHT** | `other_reasons_keep_their_wire_strings`, `an_unverifiable_ifd_refusal_stops_the_tier…` |
| M6 | T034a | FR-002a | emit the reason with no `detail` key | `nix_degradation_detail`, `nix_ifd_reason` | **CAUGHT** | 7 tests |
| M7 | T034b | FR-003 | demote the degradation log from `info!` to `trace!` | `nix_ifd_reason` | **CAUGHT** | `the_reason_is_logged_too` |
| M8 | T035 | FR-010 | a side effect inside `classify_failure` | `nix_degradation_detail`, `nix_config_closure` | **SURVIVED — mutation misformulated** | — |
| M8b | T035 | FR-010 | a warning on a **successful** evaluation now degrades | `nix_degradation_detail` | **CAUGHT** | `a_flake_that_evaluates_is_untouched` |
| M9 | — | FR-009 | drop the `unwrap_or(0)` fallback — no error line records nothing | `nix_degradation_detail` | **CAUGHT** | `an_unidentifiable_failure_still_records_something` |

Nine caught, one misformulated and replaced. No surviving mutant.

## M8 — the mutation was wrong, not the test

M8 added an early return inside `classify_failure`, intending to make the
classifier change whether a flake evaluates. It survived, and the reason is
structural rather than a coverage gap:

> `classify_failure` is only ever called on the failure path. By construction
> it cannot change whether a flake evaluates.

So the mutation did not express FR-010 at all. FR-010 is guaranteed by *where
the function is called*, not by anything inside it, and a mutation that does
not reach the requirement cannot say anything about the tests for it.

M8b expresses it properly — `if !out.status_success` becomes
`if !out.status_success || out.stderr.contains("warning")`, so a successful
evaluation that printed a warning would degrade. Real nix prints the git-tree
warning on success, so this is a plausible wrong implementation rather than a
contrivance.

**M8b's first run caught the mutation in the wrong place**, and that is the
more useful finding. It failed `t008/t012/t017_real_nix_*` in
`nix_config_closure` and did **not** fail this milestone's own control,
`m1274_us2_a_flake_that_evaluates_is_untouched` — because the control stub
emitted a clean stdout with no warning, which real nix never does. The
control was passing by luck: it would also have passed had FR-010 been
violated, as long as the violation keyed on something the stub did not emit.

Fixed by giving the succeeding stub the git-tree warning real nix prints. M8b
then failed the control directly.

> A control that cannot fail is not a control. The signal here was that a
> mutation was caught by a *neighbouring* suite — which reads as success, and
> is how a weak control survives review.

## What M1 also measured: the planning documents were wrong about the coupling

M1 was described in the plan as "the mutation that matters most… R1 showed
the classifier cannot match the clause under `position()`, so a suite that
stays green here is testing nothing about either story."

It was caught — but by the **detail** tests only. `refusal_is_named_in_the_document`
passed under `position()`.

That is correct behaviour, and it falsifies a claim made in three places:

- research R2: *"the two decisions are not independent… R1 has to land for R2 to be testable at all."*
- `tasks.md`, "Why the rule is in Phase 2 and not in US2".
- `data-model.md` §4: *"Applied to the salient line, which already excludes warnings by construction — the second of FR-005a's two conditions is satisfied by the selection rather than by a separate check."*

All three assume the classifier reads the selected line. **FR-005c forced it
not to.** Because the refusal must win even when another error is printed
after it, the classifier searches every error line for the clause,
independently of where the detail starts — and checks `is_error_line` itself
rather than inheriting that property from the selection. So the classifier
works under `position()` too, and FR-005a's two conditions are both checked
explicitly.

The ordering decision that put the rule in Phase 2 was therefore unnecessary,
though harmless. Recorded because the argument, not just the conclusion, is
what the next milestone will reuse.
