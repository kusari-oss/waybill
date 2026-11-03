# Mutations: milestone 1275

SC-008 / C20 — every requirement covered by a test that fails when the
behaviour is removed, mutation-tested rather than assumed.

Harness: `measurements/mutate.py`, inherited unchanged from milestone 1274.
It refuses to report a verdict it cannot justify: a replacement that does not
change the file is an ERROR, not a SURVIVED, and a compile failure is
`DID-NOT-COMPILE` rather than a caught mutant.

Suite: `cargo test -p waybill --test nix_closure_hackage_identity` (20 tests).

## Final run — 13 mutants, 13 caught, 0 survivors

| # | mutation | requirement | caught by |
|---|---|---|---|
| M1 | exact-pair comparison inverted (`!=` → `==`) | FR-001 | all five US1 tests |
| M2 | license assignment dropped | FR-002 | t012 |
| M3 | GHC conjunct dropped — identity on the package set alone | FR-005 | t021 |
| M4 | version comparison relaxed to a name match | **FR-004** | t020, t023, t029 |
| M5 | `no-version-recorded` collapsed into `not-in-package-set` | FR-007 | t029 |
| M6 | the unavailable path mislabelled | FR-009 | t030 |
| M7 | edge targets bound to the set's version instead of resolved by name | FR-012 | t035 |
| M8 | **the #980 mutant** — edges built before the rename, not after | FR-012, C15 | t035, t038, t038b, t045 |
| M9 | `waybill:closure-role` stripped during the rewrite | FR-013 | t014a |
| M10 | the outcome annotation never emitted | FR-007 | eleven tests |
| M11 | the pass inherits `enrich`'s gate (research R2) | FR-010, R2 | t030b |
| M12 | the dangling-target counter not incremented | FR-012c | t038b |
| M13 | the boot-library check deleted | FR-012c | t038b |

## Three mutants survived first, and each exposed a real gap

A survivor is a finding about the test, not a formality. All three were fixed
by changing the **fixture or the assertions**, never the mutation.

### M4 — the most important test was not testing its requirement

`t020` is the C5/FR-004 negative test: the one that keeps
`pkg:hackage/bash@5.3p9` out of a document. M4 relaxes the version rule to a
name match — precisely what FR-004 forbids — and was caught by `t023`, the
*annotation* test, and **not** by `t020`.

The cause was a fixture property research R5 had already measured: the GHC
signal rejects all 26 real collisions on its own. The fixture's collision was
therefore protected twice, so removing either guard left it protected, and
"matching on name alone is forbidden" was unasserted while looking asserted.

Fixed by adding `waybill-fixture-ghcshell 7.7.7` against a set version of
`0.2.0` — a collision that **is** GHC-built, so the version rule is the only
thing between it and a wrong coordinate.

### M13 — the boot-library check had no negative case

The fixture's only absent declared target was `bootlib`, which *is* a boot
library, so deleting the check that distinguishes one from any other absent
name changed nothing. Fixed by adding `waybill-fixture-absent`: declared,
absent from the closure, not nulled. Without the check, waybill invents a
`pkg:hackage/<name>` component from a string in another package's dependency
list.

### M12 — "reported, not silent" was only intended

The `dangling_non_boot` counter exists so an omitted edge is reported (#1218).
Nothing asserted the report, so deleting the increment was invisible. `t038b`
now asserts the warning reaches stderr.

The first version of that assertion looked for `dangling_non_boot=1` and
never matched: `tracing` wraps field **names** in ANSI escapes, so the field
is not a plain substring. Matching the message text is sufficient, because
M12 leaves the counter at zero and the `> 0` guard then never fires.

## The common shape

M4, M12 and M13 are one lesson in three places: **a guard whose two branches
are never both exercised is not tested, however many tests pass through it.**

And the reason any of them surfaced is that `mutate.py` reports *which* test
caught each mutant. A harness printing a bare CAUGHT would have confirmed all
three gaps instead of exposing them.

## Departure

T024 asked for US2's tests to be watched failing first. They passed on
arrival: `decide` was written with both signals in T008, so T025 and T026 were
already satisfied by the US1 work rather than being separate steps. Teeth were
established by M3, M4 and M10 instead of by a manufactured red run. Recorded
as a departure in `findings.md` F7.
