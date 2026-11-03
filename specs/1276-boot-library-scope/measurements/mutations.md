# Mutations: milestone 1276

SC-007 / C14 — every requirement covered by a test that fails when the
behaviour is removed, mutation-tested rather than assumed.

## Final run — 6 mutants, 6 caught, 0 survivors

| # | mutation | requirement | caught by |
|---|---|---|---|
| M1 | the exempt class removed from the immune set | FR-001, FR-002 | the C2, C3 and three further predicate tests |
| M2 | `is_haskell_purl` dropped, leaving a bare-name match | FR-001, FR-003 | four tests, incl. the same-name-other-ecosystem one |
| M3b | an empty boot set exempts every Hackage component | FR-005 | `m1276_an_empty_boot_set_exempts_nothing` |
| M4 | `candidate_series` unions unconditionally | FR-004, FR-004a | `m1276_candidate_series_narrows_to_what_the_flake_names` |
| M5 | I3's compiler-supplied exemption deleted | FR-002a | `m1276_a_compiler_supplied_component_is_exempt` |
| M6 | the C202 record emitted only when the count is nonzero | FR-007 | t020 and t021 |

## Two mutants needed their mutation corrected, not their test

### M3 was a no-op

Deleting `if boot.is_empty() { return HashSet::new(); }` changes nothing
observable: without it the filter still yields nothing, because
`boot.contains(..)` is false for every name. The guard is an optimisation, so
the mutation could not fail. **M3b** mutates the semantics instead — an empty
set exempting every Hackage component — and is caught.

Same correction milestone 1274 made for its M8. A survivor is a finding about
the test *or* about the mutation, and telling them apart is the work.

### M5 survived against the corpus for a structural reason

Deleting I3's exemption should make I3 demand markers for the 10 components
this feature unmarks. Against `public_corpus` it **SURVIVED** — and not
because the exemption was untested, but because **the committed goldens still
carry the `excluded` markers the exemption exists to stop requiring.** With
the markers present, I3 passes either way. It will only distinguish them after
the goldens are refreshed.

Closed with a synthetic document in the checker's own test module, plus its
control: the same document with the C170 marker removed must **fail**, or the
exemption test would pass for a checker that exempts everything. M5 is now
caught by the first of that pair.

This is the third time in two milestones that the invariant layer's reliance
on committed fixtures has shaped the work — the I9 declarations (F3), T027's
unmovable control (F6), and this. The pattern is worth naming: **a check that
reads a committed golden cannot detect a change that the golden predates.**

## A harness limitation worth recording

`mutate.py` runs `cargo test -p waybill --test <suite>`, so it cannot reach
**bin-target** unit tests — and `resolve::scope_propagation`'s live there. Run
through it, M1 and M2 both reported SURVIVED against `public_corpus`, which was
the harness missing the tests rather than the tests missing the mutants. The
bin-target mutants in the table above were run with an inline equivalent.
Extending the harness to take a `--bin` target is the obvious fix and is not
done here.
