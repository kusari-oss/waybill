# Milestone 1273 — mutation results

SC-006: *"Every requirement above is covered by a test that fails when the
behaviour it describes is removed. **Mutation-tested, not assumed** — on this
milestone's own evidence, two of the four defects are cases where the existing
tests passed throughout."*

So a green suite is not evidence here. Recorded per **requirement**, not per
story. A mutant that **SURVIVED** is a missing test or an equivalent mutant,
and which of the two has to be established rather than assumed.

**Method.** Edit the source, run `cargo test -p waybill --test <suite>`,
restore the original, record which named tests failed.

Two harness errors are recorded below because each produced a wrong verdict
before it was caught, and the pattern is the point: *a probe can lie about
the system.*

## US1 — reading hpack manifests (#1278). Suite: `haskell_hpack_us1`

| # | requirement | mutation | verdict | test(s) that failed |
|---|---|---|---|---|
| M1 | FR-001a merge | intersect a repeated name's constraints instead of replacing | **CAUGHT** | `repeated_name_keeps_last_constraint_only` |
| M2 | FR-001a inheritance | apply the top-level list to the library only | **CAUGHT** *(after a new test)* | `toplevel_dependencies_reach_a_stanza_with_no_library` |
| M3 | FR-001a `when:` | treat a default-off `flag()` block as unconditionally built | **CAUGHT** | `when_block_agrees_with_cabal_conditional` |
| M4 | FR-001a `when:` | keep a literal-`false` block's dependencies as optional instead of pruning | **CAUGHT** | `literal_false_when_contributes_nothing` |
| M5 | FR-001a ranges | zero `DeclaredDep.all_ranges` in the lowering | **SURVIVED** — equivalent | — |
| M6 | FR-005 | restore `trivially_complete()` for an empty component set | **CAUGHT** | `unread_manifest_defeats_completeness`, `unparseable_manifest_is_announced` |
| M7 | FR-005 / T028 | let `degraded_ecosystems`' `_ => {}` arm swallow `ManifestUnread` | **SURVIVED** — equivalent for this scan mode | — |
| M8 | FR-017 | restore the no-op guard that omitted `package_yaml_paths` | **CAUGHT** | 11 of 14 |
| M9 | FR-003 | do not record an unreadable manifest | **CAUGHT** | `unread_manifest_defeats_completeness`, `unparseable_manifest_is_announced` |
| M10 | FR-001b | do not record the unmodelled construct | **CAUGHT** *(after fixing a vacuous test)* | `unmodelled_construct_is_announced`, `defaults_performs_no_network_request` |
| M11 | FR-002 | stop suppressing the hpack-generated sibling | **CAUGHT** | `manifest_wins_over_generated_cabal` |
| M12 | FR-001c | do not detect `defaults:` at all | **CAUGHT** | `unmodelled_construct_is_announced`, `defaults_performs_no_network_request` |
| M13 | FR-001b | do not announce `!include` | **CAUGHT** | `unmodelled_construct_is_announced` |

FR-004 and FR-004a are covered by `disagreement_warning_still_fires`, which
asserts both halves — that the warning fires, and that it no longer advises
regenerating. Their mutation rows belong to T061's batch and are recorded
there once US1's PR lands with it.

## US2 — the optional-mark closure (#1281). Suite: `haskell_optional_closure_us2`

| # | requirement | mutation | verdict | test(s) that failed |
|---|---|---|---|---|
| M14 | FR-006 | re-seed the second walk with the marked set still in it (the original defect) | **CAUGHT** | `mark_survives…`, `declined_withdrawal…`, `fanout…` |
| M15 | FR-006 | walk the full declared set, i.e. R3's "any closure membership counts" | **CAUGHT** | same three |
| M16 | FR-009 | withdraw every reviewed mark regardless of reach | **CAUGHT** | same three |
| M17 | FR-007 | record the withdrawal without its justification | **CAUGHT** | `withdrawal_names_its_justification` |
| M18 | FR-008 | emit no reconciliation record, collapsing the three states | **CAUGHT** | `withdrawal_names_its_justification`, `ran_and_withdrew_nothing_differs_from_did_not_run` |

M14 and M15 are the two forms `research.md` R3 rejected, and both are caught
by **three** tests rather than one — including the fan-out test, which is the
one a shallow fix would slip past. R3's third rejected form (a parent-local
`reached_from` test) is not expressible as a small edit here because the
re-seeded walk replaced the parent-local structure entirely; the fan-out test
is what would catch it, and M14/M15 exercise that test.

### A vacuous fixture, caught before the fix existed

T032 **passed before the implementation did**, which is the signal that
matters. The fixture had no `flake.nix`, so `candidate_series` returned every
`KNOWN_GHC_SERIES`, and an offline cache miss on any one of them degrades the
whole pass (#975). The log said it plainly —
`degrading, compiler configuration unavailable ... reason="offline"` — and
with the pass degraded no withdrawal ran, so the marks survived for the wrong
reason and the test reported green over a defect that was fully present.

Fixed by giving the fixtures a `flake.nix` naming one compiler series and
seeding `configuration-common.nix` alongside the per-series file. After that,
5 of 7 tests failed as they should have from the start.

The general shape: **a test that passes before the code exists is not a
passing test.** This is the third time in this milestone that a green result
turned out to be measuring nothing.

## US3 — licenses from the cached package set (#1280)

Suites: `haskell_nixpkgs_license_us3` (integration) and
`package_set.rs::license_tests` + `mod.rs::license_attach_tests` (unit, over
inline literals — no cache).

| # | requirement | mutation | verdict | test(s) that failed |
|---|---|---|---|---|
| M19 | FR-010 | never attach the license | **CAUGHT** | 3 integration tests |
| M20 | FR-011a | publish `"unknown"` as a license | **CAUGHT** | `unknown_yields_no_license`, 2 unit |
| M21 | FR-011 | drop an attribute absent from the mapping table | **CAUGHT** | `uncanonicalisable…`, 2 unit |
| M22 | FR-010 | map `bsd3` to the wrong SPDX id | **CAUGHT** | 2 integration, 2 unit |
| M23 | precedence | remove the "do not overwrite an existing license" guard | **CAUGHT** *(after a new test)* | `an_existing_license_is_not_overwritten` |

### M23 — a guard that was dead, and what to do about that

M23 survived, and the investigation is the useful part. No component reaching
`apply_resolved` can have a pre-existing license today: `build_main_module`
is the only reader path that sets one, and the main module carries a version,
so `is_enrichable` excludes it. The guard was therefore **untested defensive
code** — unreachable by any scan input.

Deleting it was the other option. It stays because the precedence invariant
is one a near-future caller needs — milestone 1010's manifest-declared
licenses are exactly such a caller — and because the alternative to deleting
untested code is *testing* it. `attach_declared_license` is now tested
directly, where the case is reachable without inventing a scan input that
does not exist. Re-run after that, M23 is caught.

Contrast with **M5**, where the surviving mutant's field was actively
overwritten downstream: that assignment was deleted, because it implied the
field was load-bearing when nothing read it. A dead *guard* and a dead
*assignment* get opposite treatment, and the difference is whether the code
states an invariant someone will need or merely looks like it does something.

### A process deviation, recorded

US3's implementation (T050–T053) was written **before** its integration tests
(T043–T049), inverting the order every phase of `tasks.md` specifies. The
guarantee TDD ordering buys — that a test can actually fail — was therefore
not established by construction here, and the mutation table above is the
only thing that establishes it. That is why M23's survival mattered: with the
tests written first it would have been visible immediately as a test that
never ran red.

---

## The two mutants that did real work

### M2 — a test that asserted the rule without isolating it

`toplevel_dependencies_reach_every_stanza_kind` asserts that each inherited
name is present and that scopes are right. Its fixture has a library, so
under "inherit into the library only" **every name was still emitted through
the library** and every scope assertion still held. The rule was asserted and
not isolated.

The fix is a second fixture, `inheritance_tests_only/`: no library, no
executable, one test suite, and its only dependency declared solely by the
top-level list. A library-only reader emits nothing there.

### M10 — a test satisfied by its own fixture's name

`unmodelled_construct_is_announced` asserted
`doc_text(&doc).contains("defaults")` and `...contains("include")`. The
fixture directories are **named** `defaults/` and `include/`, so the manifest
path inside C200's `manifests-read` satisfied the substring with nothing
announced at all. The assertion tested the fixture's name.

The fix parses the C200 value and asserts on `unmodelled-construct`
structurally. Three assertions in the suite were converted the same way, and
the helper carries the reason so the next person does not reintroduce it.

This is the m1272 lesson repeating: a passing test may exercise nothing, and
the only thing that distinguishes "asserted" from "asserted vacuously" is
removing the behaviour and watching.

## The two survivors that are not missing tests

Both were established by measurement, not by inspection.

### M5 — equivalent: the field is recomputed downstream

`haskell.rs`'s cross-stanza fold recomputes `all_ranges` from every
constraint collected for a name before emission, so whatever the lowering
sets is overwritten. No input can observe the difference.

Acted on rather than filed: the lowering now sets `Vec::new()` with a comment
saying why. A populated-looking value implied the field was load-bearing on
the hpack path when it is not — the kind of thing that misleads the next
reader into preserving it.

### M7 — equivalent in scan mode, and measured so

`degraded_ecosystems` is consulted only where a per-ecosystem `complete`
aggregate would be emitted, which requires `complete_ecosystems` to be
non-empty — a rootfs scan with dpkg/apk read. A source-tree Haskell scan
never reaches it.

Measured rather than reasoned: the mutant and the fix were each run over the
`malformed`, `defaults` and `include` fixtures and the emitted
`compositions[]` compared. **Identical in all three.** So T028's wiring is
correct and currently unobservable here, and an assertion added to this suite
would pass under the mutant too — it would be a vacuous test, which is what
this exercise exists to avoid.

T028 stays. A reason code that is correct but unobservable in one scan mode is
not the same defect as one that is wrong, and the arm is what stops the
annotation and the native field disagreeing the first time a rootfs scan
carries an unread Haskell manifest.

## A finding this exercise surfaced, outside US1's scope

For the `defaults` and `include` fixtures the emitted document carries

```json
{"aggregate": "complete", "dependencies": ["pkg:hackage/waybill-fixture-defaults@0.1.0"]}
```

while C200 reports the same manifest as **partial**. That `complete` is a
dependency-claim (milestone 1069's `dependency_claims`), asserting the
component's dependency relationships are completely known — for a manifest
whose dependency set waybill has just said it cannot fully read.

It is **not** a regression from this milestone: `dependency_claims` predates
it and does not consult the hpack outcome. It is also not any FR in US1's set,
so it is recorded here rather than fixed under this story — expanding scope
mid-implementation is how a milestone stops being reviewable. Worth a
follow-up issue: the native field and C200 disagree, and Principle X is about
exactly that.

## Harness errors recorded

**1. `error: test failed` read as a compile failure.** The first classifier
greped `^error` and reported M3, M4 and M11 as `DID-NOT-COMPILE`. That string
is cargo reporting a *failing test* — i.e. a **caught** mutant — so three
successes were recorded as harness breakage. Fixed by matching `^error\[` and
`^error: could not compile` specifically. Three cases reading identically
when they should differ was, again, the signature.

**2. A mutation applied to the wrong file.** M12's first run edited
`haskell.rs` when the code lives in `haskell/hpack.rs`, mutating nothing and
reporting `SURVIVED`. Re-run against the right file: `CAUGHT`. A mutant that
survives because it was never applied looks exactly like a missing test,
which is why every `SURVIVED` row above had to be explained rather than
accepted.
