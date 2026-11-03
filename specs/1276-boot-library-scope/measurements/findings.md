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

## F6 — a control that cannot move, caught before it was reported (T027)

T027 asks for the 569-of-579 survivor figure by running
`scope-always-present.py` and diffing against T001's baseline. The probe reads
the **committed goldens**, which this feature does not regenerate — so it
reports 579 and 10 before and after, identically, whether the code is right or
wrong.

The near-miss is worth recording because of how it differs from F3. There, I9
read stale goldens and **failed**, loudly, with the exact populations. Here the
check would have **passed**, and "569 of 579 survived" would have gone into a
measurement file as a result when it was an artefact of an unchanged fixture.
A control that cannot move is not a control — the same lesson as
`feedback_a_passing_test_may_exercise_nothing`, arriving through a probe
rather than a test.

What is actually verified of FR-003 at each level, and what is deferred, is in
`after-1276.md`. The corpus figure is now written down as a **prediction for
the refresh diff** rather than a result: 2 targets, scope only, 10 components,
and the two I9 declarations removed.

## F7 — the feature works on its subject project, against a verified baseline (T033)

```
  [baseline 6dd75678]  optional in B but not A (9): [... pretty ...]
  [after]              optional in B but not A (8): [the same 8, no pretty]
```

The two differ by exactly one element and it is the boot library. SC-001 and
SC-003 both met: one component corrected, the eight non-boot controls
untouched.

The baseline was built in a detached worktree at the commit before US1, and
its `scope_propagation.rs` contains **0** occurrences of `compiler_supplied` —
stated because milestone 1275's first attempt at this comparison used
`git stash`, which reverts to HEAD, and HEAD already held the feature being
measured. A baseline that cannot be shown to lack the feature is not a
baseline.

## F8 — US3 lands; two notes on what the count means and one repeated mistake (T020–T025)

C202 `waybill:scope-exemptions` is emitted at document scope in all three
formats with the same value, verified on the real project:

```
CDX  metadata.properties:        {"compiler-supplied":21}
SPDX 2.3 document annotations:   {"compiler-supplied":21}
SPDX 3:                          present
```

The plumbing cost is worth recording for the next doc-scope row: **23
references across 12 files** — `ScanArtifacts`, the CDX metadata inputs and
builder, three emitters, five `None` construction sites, the catalogue row and
three parity extractors. The struct comment at `metadata.rs:236` warns that
the list is positional and a mid-list insertion can silently re-bind an
adjacent `Option<&str>` with no type error (#844), so the field is appended at
the end like its two predecessors.

### The count is the immune set, not the corrections

**21**, where the corpus-wide mis-scoped population is 10 and this project's
is 1. That is the honest reading of FR-007 — "how many components the rule
exempted" — and 21 components were exempted, i.e. made immune. Only those that
would otherwise have inherited a scope were *corrected*.

Worth stating because the two numbers answer different questions. 21 is stable
across runs and makes the rule's presence visible, which is FR-007's purpose.
It is **not** a count of components whose scope changed, and a reader who took
it for one would overstate the feature's effect by 20. If a corrections count
is wanted later, it is a second key in the same object — which is why the row
is a JSON object rather than a bare integer.

### I repeated 1275's F8 exactly

`t020` passed on arrival in **3.42 s**, logging
`revision=Some(a799d3e3…) degraded=None`. The `project` fixture has a
`.cabal`, so `enrich` runs, and `enrich` is not offline — pointed at an empty
cache it fetched the real 16 MB `hackage-packages.nix` over the network. That
is the identical trap milestone 1275 recorded as its own finding F8, in the
same suite family, and I walked into it one milestone later.

Fixed by seeding a package set and a nulled compiler configuration into a
temporary cache, which also makes the count nonzero rather than incidental.
**The suite now runs in 0.16 s** — a 21× drop, and the same independent signal
1275 used to confirm the fetch was gone.

The lesson that did not transfer the first time, written more plainly: in this
codebase, pointing `WAYBILL_NIXPKGS_CACHE` at an empty directory does not make
a test offline. It makes it *fetch*. Offline requires either a seeded cache or
a fixture with no `.cabal`.

Mutant: M6 (emit the record only when the count is nonzero) is CAUGHT by both
tests.

## F9 — T028 found a design flaw in US3, not just a diff (T028)

The cargo and pip fixtures came back **4 differing lines** against the
pre-1276 baseline, where C16 asked for byte identity. The lines are entirely
the new C202 property:

```
> "name": "waybill:scope-exemptions",
> "value": "{\"compiler-supplied\":0}"
```

**No component's scope changed**, so FR-008 holds. Two separate things
followed.

### The record conflated the two facts it exists to separate

US3 scenario 2: *"'the rule exempted nothing' and 'the rule did not run' are
different facts."* A bare count does not carry that. A **Cargo** scan recorded
`{"compiler-supplied":0}` — and so would a Haskell scan whose compiler
configuration was read and nulls nothing. In the first the rule had no class
to apply; in the second it applied one and found nothing. Identical on the
wire, which is precisely the defect the requirement was written against, and I
had shipped it one scenario after quoting the scenario.

Fixed with a second key: `{"compiler-supplied":N,"exempt-class":"known"|"unavailable"}`.
Three states now distinguishable — absent (the propagation did not run),
`unavailable` (ran, no class), `known` (ran with a class). Both tests assert
their own half, so neither can drift back.

It also removes a smaller oddity: a Python project's document said
`{"compiler-supplied":0}`, which reads as "the Haskell exemption found nothing
here" when the truth is "no compiler configuration was ever read".

### C16 asked for something FR-007 forbids

C16 required a non-Haskell emission to be byte-identical. FR-007 requires a
document-scope record wherever the propagation ran, and it runs for every
ecosystem — so the two cannot both hold. C16 is now qualified with the
exception, and the measured size of it: four lines, one property. The
requirement is not weakened, because a record whose *absence* means "this rule
did not run" has to be present wherever it did.

This is the second contract in two milestones whose planning-time wording
turned out stricter than the implementation could be (milestone 1275's FR-012c
was the first, and also mine). Both were found by running the check rather
than by reading the contract.

## F10 — what the golden refresh must show, and what it must remove (T035)

This feature changes the goldens of **two** corpus targets, and the expected
diff is narrow enough to be checked line by line rather than skimmed:

| target | expected change |
|---|---|
| `haskell-security-advisories` | 8 components lose `excluded` |
| `nix-closure-moat` | 2 components lose `excluded` |
| every target, all 17 | gains one document-scope property, C202 `waybill:scope-exemptions` |

Nothing else. No component gains or loses, no PURL changes, no edge moves. The
C202 addition is measured at **four lines** on the `cargo` and `pip` fixtures
and is the exception C16 now states explicitly.

**The refresh must also delete two declarations.** `invariant-declarations.toml`
carries I9 entries for both affected targets, recording that their goldens
predate this feature. They are the refresh's to remove, and each says so. If
either survives a refresh, the immunity has stopped working and the response is
to investigate rather than to raise the count — not to leave a declaration
standing, which is how a declarations file stops meaning anything.

**Order matters.** Milestone 1275, on which this branch is stacked, needs its
own refresh first: its prediction is 3 files changed and 48 byte-identical,
plus scope changes on `nix-closure-moat` from its declared edges feeding the
propagation (that milestone's F12). Refreshing this feature's goldens before
1275's would blend two milestones' diffs on the one target they share, and
`nix-closure-moat` is where both land.

## F11 — the gate caught a proportionality error US3 had shipped (T036, first run)

**The gate failed**: `GATE_EXIT=101`, 35 failing tests across 5 suites, every
one a `*_byte_identity` golden comparison — `cargo`, `pip`, `npm`, `golang`,
`maven`, `gem`, `deb`, `rpm`, `cmake`, `bazel`, `apk`. The diff was always the
same one property:

```
+ "statement": "{… \"field\":\"waybill:scope-exemptions\",
                  \"value\":\"{\\\"compiler-supplied\\\":0,
                              \\\"exempt-class\\\":\\\"unavailable\\\"}\"}"
```

The first instinct is to regenerate 35 goldens. The right question is whether
the emission deserves them, and it did not: **a Haskell-specific record was
being written onto every document in every ecosystem**, where it can only ever
say zero. Measured: none of the 12 golden ecosystems has a `flake.lock`, so
none of them can ever have an exempt class.

**Fixed by gating on applicability rather than on having run.** The record is
emitted iff a pinned compiler configuration was read. Three states, carried by
presence and value together:

```
absent        the rule was not applicable — no compiler configuration
{"…":0}       applicable, exempted nothing
{"…":N}       applicable, exempted N
```

That drops the `exempt-class` key added in F9 — absence now says what the key
said, and says it without repeating itself on every document in the
repository. **35 goldens untouched.**

### The requirement needed qualifying, and the test needed a real fixture

FR-007 said "including zero" without saying *of what population*, and US3
scenario 2's "a scan that exempts none" silently covered two different
scans. Both are now explicit, with a third acceptance scenario for the
inapplicable state.

`t021` then failed for a better reason than before: the fixture seeding a
configuration that nulls a name nothing has wrote an **empty**
`hackage-packages.nix`, and `enrich` returns early on an empty package set —
before the boot set is computed — so the rule was inapplicable rather than
applicable-and-empty, which is the opposite of what the fixture is for. One
package-set entry fixes it. Worth recording because the fixture looked right
and tested the wrong state.

### Two notes on process

F9 and F11 are the same finding found twice at different scales. T028 measured
4 differing lines on two fixtures and I treated the *conflation* as the
problem; the gate measured 35 failures across eleven ecosystems and made the
*proportionality* the problem. The second framing subsumes the first, and the
cheaper check found the smaller half.

And the earlier "gate green" I reported after US1 was real, but the run I
started after US3 I never read a verdict for — I read its log mid-flight, saw
8 `ok` lines, and said it was still running. It was; it then failed, and the
re-run overwrote it. No false claim was made, but the margin was one sentence,
and the lesson is the project's own: read the verdict line, never the progress.

## F12 — I9 is a live check, and M5's survival was structural (T030, T031)

**T030 — I9 can fail.** Inverting its offender filter surfaced **32**
violations on `haskell-language-server` with a named example. An invariant
never seen to fail is not evidence, and SC-007a asked for this because I9 is
expected green from the moment the fix lands: the direct-mark population it
covers is measured at zero.

**T031 — 6 mutants, 6 caught, 0 survivors**, after two mutations had to be
corrected rather than two tests written. M3 was a no-op (deleting an early
return equivalent to the general path); M3b replaces it. And **M5 survived
against the corpus for a structural reason**: deleting I3's exemption should
make it demand markers for the 10 components this feature unmarks, but the
committed goldens still carry those markers, so I3 passes either way until the
refresh. Closed with a synthetic document in the checker's own test module
plus a control — the same document without the C170 marker must fail, or the
exemption test would pass for a checker that exempts everything.

**That is the third time in two milestones** that the invariant layer's
reliance on committed fixtures has shaped the work: the I9 declarations (F3),
T027's unmovable control (F6), and M5. The pattern deserves a name, because it
has now cost three separate pieces of reasoning: **a check that reads a
committed golden cannot detect a change the golden predates.** It cuts both
ways — F3's case failed loudly and was easy to see, F6's and M5's passed
quietly and were not.
