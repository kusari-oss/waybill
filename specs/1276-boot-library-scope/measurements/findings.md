# Findings: milestone 1276

Appended as they happen, not reconstructed at the end. A probe error, a
surprise, or a departure from the plan goes here with the evidence that
settled it.

---

## F1 — both probes reproduce, and the defect is one component on the subject project (T001)

`slack-web` offline against its pinned revision: 11 components reachable only
through the marked pair, of which exactly **1** is compiler-supplied
(`pretty`). The corpus: **10 of 78** boot-library components carry a
non-runtime scope, in two targets.

So the measured population is small, and that is consistent with the issue's
own framing — #1340 says the value is in settling the semantics, not in the
count. The semantics are settled by the CycloneDX 1.6 `meta:enum` text quoted
in spec.md's Context, which makes this a conformance defect rather than a
preference.

The figure worth keeping in view is not 10 but **569**: the non-runtime scopes
that must survive. This feature's plausible wrong implementation
over-exempts, and that is the number which catches it.

## F2 — R3's coincidence holds where it matters, and the probe that seemed to refute it was measuring the wrong set (T007)

T007 existed because research R3 *argued* the exempt class and C170's
population must coincide — both derive from the same nulled set through
`classify()` — and the plan recorded that an argument is not a measurement.
The measurement was worth running.

First pass, testing the inverse direction (components whose **name** is in the
nulled set but which carry no C170):

```
haskell-aeson                in-nulled-set= 11  with C170=  0  WITHOUT= 11
haskell-language-server      in-nulled-set= 33  with C170= 32  WITHOUT=  1
haskell-security-advisories  in-nulled-set= 27  with C170= 27  WITHOUT=  0
nix-closure-moat             in-nulled-set= 20  with C170= 19  WITHOUT=  1
```

That reads like a refutation. It is not, and the reason matters more than the
figures: **the probe approximated the per-scan boot set with the union of
every nulled name across four cached revisions, which is a set the code never
forms.** The predicate takes the boot set from `EnrichmentSummary`, i.e. from
the scan's own pass against its own pinned revision and its own compiler
series.

Narrowed to the population that could actually be harmed — non-runtime, name
in the set, no C170:

```
haskell-aeson                1  ['directory']
haskell-language-server      0
nix-closure-moat             0
```

**One component, on one target, and it is the FR-005 case.** `haskell-aeson`
carries **zero** C170 components, because the nixpkgs-haskell pass never ran
for it. So its boot set is `None` → empty → nothing is exempt, and `directory`
keeps its scope. The protection is not luck: it is the arm FR-005 specifies
and `m1276_an_empty_boot_set_exempts_nothing` already asserts at the
predicate.

The two single outliers on the other targets — `directory-ospath-streaming`
and `haddock-library` — are names nulled in only one or two of the eight
series (measured during Q1's clarification: 18 of 50 names are not in every
series). Their own targets' revisions and series almost certainly do not null
them, which is why they carry no C170 and would not be exempt.

### What this changes

Nothing in the design, and one thing in the tests. `haskell-aeson` is a
**real corpus target** where a scan with no compiler configuration has
components whose names collide with a nulled set, and exactly one of them is
non-runtime. That is a better C7 fixture than a synthetic one, and T016
should assert against it by name: a scan where the pass did not run exempts
nothing even where names collide.

It also justifies the predicate's second condition. A bare-name match is not
enough — `is_haskell_purl` is required — and the nulled set at the measured
revision contains `binary`, `array`, `unix`, `directory` and `pretty`, every
one a plausible package name in another ecosystem. That is the same
two-condition discipline milestone 1275 needed for closure identity, where a
name match alone would have minted `pkg:hackage/bash@5.3p9` for GNU bash.

## F3 — US1 lands, and two things the plan did not anticipate (T008–T014)

The three-part indivisible change went in as planned: immunity in
`propagate_non_runtime_scope`, I3 amended to exempt the class, and milestone
1275's `t057` inverted. Two gates fired that the plan had not accounted for,
and both were right to.

**The invariant statements have a catalogue, and it is enforced.** Amending
I3's text in the registry failed `statements_and_preconditions_match_the_catalog`
— `docs/reference/sbom-invariants.md` holds the canonical wording and a drift
test compares them. Adding I9 then failed `catalog_and_registry_agree`, which
requires a `### I9` diagnostic section. Both are the gate working: an invariant
whose statement drifts from its documentation is an invariant nobody can act
on. Neither was in the plan's Project Structure, which named only
`invariants/checks/` and `invariants/mod.rs`.

**The new invariant reads committed goldens, which cannot be refreshed
locally.** I3's amendment was safe because it *relaxes* — fewer offenders. I9
*adds* a constraint, and the committed goldens still carry the scopes this
feature removes, so it failed on both affected targets with exactly the
expected populations (8 and 2). The code is correct; the fixtures predate it.

Resolved by declaring the two cells with the cause stated and an expected
post-refresh count of 0. **That is deliberately not what contract C8
forbids**: C8 forbids declaring the *I3* violations, which would have recorded
components the feature corrected as defects awaiting a fix. These record a
golden awaiting regeneration, which is a true statement about the fixture, and
the declarations are the refresh's to delete. If either survives the next
refresh, the immunity has stopped working and the response is to investigate
rather than to raise the count — the declarations say so.

### The harness cannot reach these tests, and that is worth knowing

`mutate.py` runs `cargo test -p waybill --test <suite>`, so it cannot run
**bin-target** unit tests — and `resolve::scope_propagation`'s tests live
there. Run through it, M1 and M2 both reported SURVIVED against
`public_corpus`, which was the harness missing the tests rather than the tests
missing the mutants. Run against `--bin waybill`:

```
M1  immunity removed              CAUGHT  (the C2 and C3 tests)
M2  ecosystem check dropped       CAUGHT  (4 tests, incl. the same-name one)
M3  empty-set guard dropped       SURVIVED  -> invalid mutation, see below
M3b empty set exempts everything  CAUGHT  (the FR-005 test)
```

**M3 was an invalid mutation, not a missing test.** Deleting
`if boot.is_empty() { return HashSet::new(); }` changes nothing observable:
without it the filter still yields nothing, because `boot.contains(..)` is
false for every name. The guard is an optimisation. M3b mutates the semantics
instead — an empty set exempting every Hackage component — and is caught by
`m1276_an_empty_boot_set_exempts_nothing`. This is the same correction
milestone 1274 had to make for its M8: a survivor is a finding about the test
*or* about the mutation, and distinguishing them is the work.

M2 is the one worth keeping in view. It drops `is_haskell_purl`, leaving a
bare-name match — and the nulled set at the measured revision contains
`binary`, `array`, `unix`, `directory` and `pretty`, every one a plausible
package name in another ecosystem. Four tests catch it.

## F4 — sbomnix cannot see the components #1340 is about (comparison)

Full write-up in `sbomnix-comparison.md`; harness in `compare-sbomnix.sh`.
Two results matter here.

**Milestone 1275's inherited edge figure is confirmed.** Its spec carried
"sbomnix emits 8,230 edges on the same project" from #1282 rather than from an
observation. Measured on the same closure: **8,497** against waybill's **527**,
a different sbomnix version and the same order. 16×, and the cause is the one
the spec named — a store reference treated as a dependency versus only what
the package set declares.

**sbomnix emits no component for any GHC boot library.** `pretty`,
`directory`, `ghc-heap`, `array`, `binary`, `deepseq` — none of them, and the
reason is structural rather than an implementation gap: a boot library is not
a separate derivation, the compiler ships it inside its own, so a tool that
walks derivations cannot see one however well it walks.

So the defect #1340 describes **cannot arise** in sbomnix: there is no
component to mis-scope. That is not a point in its favour. The dependency is
real and in the artifact, and waybill can mis-scope these components only
because it emits them at all — which it does by reading declared
dependencies. The 8,497-vs-527 gap and the six-vs-zero gap are the same design
choice measured in two directions: one tool sees more edges, the other sees
more packages.

Also measured, and relevant to milestone 1275's identity work: **107 of
sbomnix's components carry no PURL at all**, and its other 1,182 use
`pkg:nix`, which is not a purl-spec type — the reason waybill's closure
emitter uses `pkg:generic`, and `pkg:hackage` where the pinned set confirms
it. Zero of its components carry a hash; waybill emits 106.

**A measurement error avoided.** The first run used a bare store path and
reported 0 licenses for sbomnix. Its own log said why — "No nixpkgs metadata
source is available for store-path targets… Use a flakeref target instead" —
and with a flakeref it emits 183. Reporting the first number would have been a
false claim about the tool rather than about the invocation. The harness takes
a flakeref and says so in a comment.

## F5 — US2's three tests all passed on arrival, and M4 is why that is still evidence (T015–T019)

A departure from T018, which asked for them to be watched failing first. They
could not be: T006 built the predicate with exactly these properties, and
FR-004a was already satisfied by `candidate_series` before this milestone
began (research R2).

Rather than reverting working code to manufacture a red run, the teeth were
established by mutation:

```
M4  candidate_series unions unconditionally   CAUGHT by the FR-004a test
M3b an empty boot set exempts everything      CAUGHT by the FR-005 test
```

M4 is the one that matters. FR-004a's test asserts a behaviour that already
existed, which is the shape of a test that proves nothing — and M4 shows it
does: remove the narrowing and it fails. The figures are what make it
checkable rather than rhetorical, since the union exempts up to 15 names a
given series does not.
