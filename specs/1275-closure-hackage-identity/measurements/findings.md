# Findings: milestone 1275

Appended as they happen, not reconstructed at the end. A probe lie, a
surprise, or a departure from the plan goes here with the evidence that
settled it — research R5's `happy-lib` / `ghc-binary` pattern error is the
model, and it was caught only because one row disagreed between two signals.

---

## F1 — the baseline reproduces exactly, and the branch needed no re-scan (T001)

`size-closure-members.py` against the `001fa50` artifacts returns every figure
`quickstart.md` records: 276 closure-appended generic components, 60 exact
pairs all GHC-built, 26 name collisions none GHC-built, 190 absent of which 14
carry no version. Signals agree 60 of 60.

`git diff --stat 001fa50..HEAD -- '*.rs' '*.toml'` is empty, so the preserved
artifacts are this branch's output. That is the evidence the baseline holds
here — not a re-run, which could only have agreed with an unchanged binary.

**One figure the probe reports that the design docs do not mention**: one
derivation is GHC-built but is *not* an exact pair — `ghc-settings-edit`. It
sits in the `absent` bucket (its name is not in the pinned set at all), so it
is correctly not reclassifiable: the GHC signal alone never mints an identity,
which is exactly the asymmetry FR-005 requires. Recorded because a reader
comparing the probe's output to research R5's table would otherwise find an
extra row unexplained.

## F2 — a second parse costs 1.1 seconds, so the set is handed over, not re-read (T005)

plan.md's Technical Context says "no measurable change… there is nothing for
it to be slow at". **Measured, that is false for the obvious implementation.**

```
T005 parse of 16,634,427 bytes, N=5:
  min=1120ms  median=1121ms  max=1125ms  (entries=19,429)
```

Probe: `m1275_t005_second_parse_cost`, `#[ignore]`d and env-gated, in
`nix/haskell_packages/mod.rs`. Deterministic to within 5 ms across five runs,
so one number is enough to decide on — and the number is 1.1 **seconds**
added to every `--nix-closure` scan of a Haskell project, for a file the
process had already parsed.

**Decision: `enrich` hands its parsed set to the new pass through
`EnrichmentSummary`, so the common path parses once and the pass costs 0 ms.**
A per-scan memo keyed by revision would also work; handing the value over is
less machinery and has no cache to go stale. It needs `PackageSet` to derive
`PartialEq`/`Eq`, which it can — `PackageSetEntry` already does.

Also worth stating because it is the kind of thing a reader assumes: the
probe calls `parse` a sixth time to print the entry count, so the 6.75 s
wall time of the test is six parses, not five of 1.1 s each plus overhead.

## F3 — FR-010's premise does not hold in the case R2 says matters most

FR-010 forbids the pass adding "network access, subprocess calls or filesystem
reads beyond those the scan already performs", on the stated grounds that the
package set "is already fetched, parsed and cached in-process".

**That premise is true only when `enrich` ran.** Research R2 is explicit that
the new pass must not inherit `enrich`'s gates, because a repository whose
only Haskell content arrives through the closure is the scan #1282 is about —
and in exactly that case `enrich` returned `None` before fetching anything, so
nothing is in memory and, on a cold cache, nothing is on disk either. FR-010
and R2 pull in opposite directions and neither notices.

**Resolution, and it is a choice rather than a deduction:** the pass takes the
set `enrich` loaded when there is one; when there is not, it reads the
per-revision **cache only** and never initiates a network fetch, degrading to
`PackageSetUnavailable` on a miss — which is the path FR-009 and Principle XI
already specify. So:

- the common case (any project with declared Haskell dependencies) costs
  nothing and gains full identity;
- a closure-only scan on a warm cache gains full identity;
- a closure-only scan on a cold cache degrades and **says so**, rather than
  silently fetching 16 MB from a pass whose requirement says it will not.

The reading of FR-010 this rests on: its subject is new *surface* — network,
subprocess, new files — not a second read of the same cached file by the same
mechanism. Stated here because the alternative reading forbids the pass from
working at all in R2's case, and that cannot be what FR-010 meant.

## F4 — the reason's carrier is a new row, C201 `waybill:closure-identity` (T006)

The audit spec.md FR-007a records concluded that no native construct carries
"why a candidate identity was rejected", and that neither C151 nor C170 can be
reused. So C201 is added, with that audit as its justification clause, landing
with its three extractors and its registry entry in the same change.

Three decisions inside it that the spec left open:

1. **A plain closed enum, not a JSON object.** The catalogue has both
   conventions — C182 and C151 are plain strings, C184 and C185 are encoded
   objects. US2's scenario 2 wants the collision recorded; the value
   `version-disagrees` already records it, and keeping the value a closed enum
   is what makes C8's per-component assertion a set comparison rather than a
   parse. The set version behind the collision is **not** carried: it is
   recoverable from the pinned revision and the name.
2. **Emitted on `confirmed` members too**, not only the unreclassified ones
   FR-007 names. FR-007's floor is "every generic member says why"; emitting on
   the confirmed ones as well makes the row answer FR-013's question directly —
   a closure-derived `pkg:hackage` component carries `confirmed`, a
   manifest-derived one carries nothing at all. That is a cleaner tier signal
   than inferring it from `waybill:closure-role`, which is about how nix
   *referenced* a member.
3. **`signals-disagree` is in the enum although it is measured 0 of 60.** A
   variant that cannot occur today is cheaper than discovering it occurred.

**A wrong turn worth recording**: the row was first inserted into `EXTRACTORS`
next to C182, because that is where the other closure rows live and grouping
read naturally. `extractors_table_is_sorted_by_row_id` rejected it —
`C201 >= C183`. The table's order is load-bearing and the test says so
immediately, which is the behaviour you want from a gate; the grouping
instinct was simply wrong.

## F5 — two probes, two populations, and the gate the difference implies (T009)

The Rust probe first reported **763 identities, 106 confirmed** where the
Python probe reported **276 components, 60 exact**. A 2.8× disagreement
between two measurements of the same thing is a defect in one of them, not a
nuance.

It was the Rust probe, and its own comment was the lie: it claimed to group
derivations "exactly as `closure::emit::components` does" while skipping that
function's role filter, which drops every member that is neither
`artifact-input`, `build-tooling` nor `both` (plus `unreferenced`
patch-appliers). `classify::classify` is a pure function of the raw closure
and needs no `nix`, so approximating it bought nothing. Corrected, the probe
calls `classify` and `emit::components` and measures what the tier actually
emits.

**Corrected, against the real closure:**

```
components emitted by the closure tier: 323
    confirmed                  105
    version-disagrees           26
    not-in-package-set         178
    no-version-recorded         14
the two signals agree on 105 of 105 exact-pair rows
exact pair but NOT ghc-built (0): []
ghc-built but NOT an exact pair (3): ghc-settings-edit 0.1.0,
                                     hashable 1.4.7.0, os-string 2.0.10
```

**FR-006 is satisfied against the shipped code, not a re-implementation of
it: 105 of 105, zero disagreements.** The GHC signal rejects all 26
collisions on its own, exactly as research R5 measured.

### Why the two populations differ, and what follows from it

323 is what the closure tier **emits**; 276 is what remains
closure-*appended* after `merge_closure_components` folds 47 of them into
identically-named design-tier components. The spec names that 47. The
Python probe reads the emitted CDX and therefore sees 276; the Rust probe
reads the closure and sees 323. Every bucket is consistent across the two
once that is accounted for: `version-disagrees` is 26 in both (a collision is
a non-Haskell package and never merges with a design-tier Haskell one),
`no-version-recorded` is 14 in both, and the Python probe's
absent-excluding-unknown 176 sits just under the Rust probe's 178.

**The consequence for the pass, which the design docs do not state**: a merged
component keeps the *design-tier* component's identity and PURL, and
`merge_closure_components` copies the closure component's annotations onto it
— so `waybill:closure-role` is present on merged components too and cannot be
the gate. The pass must reclassify only components that are still
`pkg:generic`. A merged component is already `pkg:hackage` and FR-015 forbids
touching it.

`hashable 1.4.7.0` appearing under "ghc-built but not an exact pair" is the
alias gap research R4 predicted, now confirmed by the shipped code rather than
by a Python re-reading of the same file.

## F6 — US1 lands; three mutants caught, and E2's added test is the only thing catching one (T011–T019)

All five US1 tests were watched to fail first, each for the right reason — the
fixture reproduces #1282's defect hermetically:

```
t011  pkg:generic/waybill-fixture-aeson@2.2.4.1  vs  pkg:hackage/...
t012  no licenses array
t013  no hashes array
t014a pkg:generic/...
t014  (initially PASSED — see below)
```

**`t014` passed before the feature existed**, because "exactly one component
for this identity" is true of the single `pkg:generic` component. A test that
passes in the before-state is not yet evidence, so it now also asserts *which*
identity survived: it fails before, and still catches a `pkg:generic` twin
after, which is what US1's second scenario is actually about.

Mutants, after:

```
M1  exact-pair comparison inverted (`!=` → `==`)  CAUGHT by all five
M2  license assignment dropped                    CAUGHT by t012
M9  closure-role stripped during the rewrite      CAUGHT by t014a ONLY
```

**M9 is the one worth noting.** `t014a` is the test `/speckit.analyze` added
for finding E2 — FR-013/C16 had zero task coverage, and the reasoning for
leaving it uncovered was that FR-011 preserves the annotation for free. M9
shows what "for free" is worth: strip the annotation inside the rewrite and
nothing else in the suite notices. The free-by-construction claim was true
and still needed the test, exactly as research R7 argued for FR-008.

### Two implementation facts the design docs did not have

1. **`apply_resolved` could not be reused wholesale.** It sets
   `sbom_tier = "source"`, which would relabel a build-tier closure member and
   destroy the very distinguishability C16 asserts. What *is* reused is its
   two rules: `attach_declared_license` and a newly extracted
   `attach_source_hash` are now `pub(crate)` and shared, so FR-002's "by the
   same rules the existing resolution uses" is one implementation rather than
   two that agree today — the m1274 US2 lesson applied before it could bite.
2. **The gate is `closure-role` AND still-generic**, per F5. Gating on the
   annotation alone would have reclassified the 47 merged components, whose
   `pkg:hackage` identity came from a manifest and which FR-015 forbids
   touching.

Neighbouring suites after US1: `nix_config_closure` 14/14 (C17 holds),
m926 41/41, license_us3 5/5, degradation_detail 6/6, ifd_reason 7/7,
eval_tier 16/16, nixpkgs_declarations 5/5.

## F7 — the most important test in the milestone was not testing what it claimed (T020–T027)

`t020` is the C5/FR-004 negative test, and spec.md, tasks.md and its own
doc comment all call it the test that matters most: it is what keeps
`pkg:hackage/bash@5.3p9` out of a document. **Mutation showed it was not
testing FR-004 at all.**

M4 relaxes the version comparison to a name match — the precise rule FR-004
forbids. It was CAUGHT, but by `t023`, the annotation test, and **not** by
`t020`:

```
M4  version rule relaxed   CAUGHT  t023          <- before
M4  version rule relaxed   CAUGHT  t023, t020    <- after
```

The cause is a property of the fixture that research R5 had already measured
and nobody connected to the test design: **the GHC signal rejects all 26 real
collisions on its own.** The fixture's collision, `shell 5.3p9`, is a
non-Haskell package, so its derivation is not GHC-built either — and with the
version rule removed it was still protected, by the *other* rule. Two
independent guards over one case means neither is tested in isolation, so
"matching on name alone is forbidden" was unasserted while looking asserted.

Fixed by adding `waybill-fixture-ghcshell 7.7.7` against a set version of
`0.2.0`: a collision that **is** GHC-built, so the version rule is the only
thing between it and a wrong coordinate. M4 is now caught by `t020`.

**This is the finding this milestone's mutation budget paid for.** The test
passed, the feature was correct, and the coverage claim was false — and the
only reason it surfaced is that M4's verdict named *which* test caught it.
A mutation harness that printed CAUGHT without the test names would have
confirmed the gap instead of exposing it, which is worth remembering about
`mutate.py`'s output format.

### US2's tests could not be watched to fail (a departure)

T024 asked for T020–T023 to be observed failing first. They passed on
arrival: `decide` was written with both signals in T008, so US2's
implementation tasks (T025 the hard gate, T026 the annotation) were already
satisfied by the US1 work rather than being separate steps. Rather than
reverting the gate to manufacture a red run, the teeth were established by
mutation — M3 (GHC conjunct dropped), M4 (version rule relaxed) and M10
(annotation never emitted), all caught. That is stronger evidence than a
red test against an unimplemented feature, but it is not what the task said,
so it is recorded as a departure rather than a completion.

## F8 — a non-hermetic test, and the R2 case nothing was covering (T028–T034)

`t030` (C10 / FR-009, the unavailable package set) failed on arrival with
`not-in-package-set` where `package-set-unavailable` was expected. **The
verdict was right and the test was wrong**, in a way worth recording because
nothing in the suite would have caught it:

the `project` fixture has a `.cabal`, so `is_enrichable` is satisfied,
`enrich` runs, and `enrich` is **not** offline — it fetched the real 16 MB
`hackage-packages.nix` for the pinned revision over the network and handed
the pass a genuine nixpkgs package set, in which no `waybill-fixture-*` name
appears. So `not-in-package-set` was the correct answer to the question the
test actually asked. Verified rather than assumed: a `RUST_LOG=info` scan with
the cache pointed at a nonexistent directory logs
`version resolution complete … revision=Some("a799d3e3…") degraded=None`, and
`cache_root` honours `WAYBILL_NIXPKGS_CACHE` with no `$HOME` fallback, so the
miss was real and the retrieval was a network call.

Fixed by pointing `t030` at the `closure_only` fixture, which has no `.cabal`:
`enrich` returns before any retrieval, and the pass's own cache-only load
misses and degrades. **Second signal that the fetch is gone: the suite's wall
time fell from 3.29 s to 0.58 s.**

### The gap the fix exposed

Every other test in the suite uses `project`, where `enrich` runs and hands
its set over. So **nothing covered research R2's case** — a repository whose
only Haskell content arrives through the closure, which R2 calls the scan
#1282 is about. A pass gated on `enrich` having run would have skipped
precisely those documents, and the whole suite would still have been green.

`t030b` now covers it: `closure_only` with a **seeded** cache must still
confirm, with a license and a hash, proving the cache-only load does real
work rather than returning an empty set whose output coincidentally matches
the degraded one. M11 — replacing the fallback load with `NoLockfile`, i.e.
making the pass inherit the gate — is CAUGHT by `t030b` and by nothing else.

Mutants: M5 (`no-version-recorded` collapsed into `not-in-package-set`) caught
by t029; M6 (the unavailable path mislabelled) caught by t030; M11 caught by
t030b.

## F9 — the edges, and two more mutants that found untested guards (T035–T045)

Three implementation facts worth recording:

1. **A CycloneDX `dependencies` entry is not an edge.** CDX emits one per
   component, so an entry whose `dependsOn` is empty is a node declaration.
   The first `all_edge_endpoints` helper counted those as endpoints, and
   `t036` (C12, no store-reference edges) failed on `autoconf` — a component
   with no edges at all that simply appears in the graph as itself. The helper
   now skips entries with no targets.
2. **`ResolvedComponent` has no `Default`**, so FR-012c's compiler-supplied
   component is written out in full. That is a good constraint rather than an
   annoyance: a component with every field defaulted is a component nobody
   decided anything about.
3. **`apply_resolved` could not be reused** (F6), and neither could the
   existing closure-edge conversion in `scan_cmd.rs` — it filters
   `purl_of` to `pkg:hackage/` components and is fed by `EnrichmentSummary`,
   which the closure pass does not produce. The edge emission is new code that
   reuses `declared_relations`, `is_boot_library` and the same dedup discipline.

### M13 and M12 both survived first, for the same reason

```
M13  boot-library check deleted      SURVIVED  -> CAUGHT by t038b
M12  dangling counter not incremented SURVIVED -> CAUGHT by t038b
```

**M13**: the fixture's only absent declared target was `bootlib`, which *is* a
boot library — so deleting the check that distinguishes a boot library from
any other absent name changed nothing. The guard that stops waybill inventing
a `pkg:hackage/<name>` component from a string in someone else's dependency
list was untested. Added `waybill-fixture-absent`: declared by `aeson`, absent
from the closure, **not** nulled. Now M13 is caught.

**M12**: the `dangling_non_boot` counter exists so the omission is "reported,
not silent" (#1218). Nothing asserted the report, so deleting the increment
was invisible. `t038b` now asserts the warning reaches stderr — and the first
version of that assertion looked for `dangling_non_boot=1`, which never
appears: `tracing` wraps field *names* in ANSI escapes, so the field is not a
plain substring. Matching the message text works and is sufficient, because
M12 leaves the counter at zero and the `> 0` guard then never fires.

Both are the same lesson in two places: **a guard whose two branches are never
both exercised is not tested, however many tests pass through it.** That is
the same shape as F7, where the collision was protected twice over.

Mutants: M7 (targets bound to the set's version instead of resolved by name)
caught by t035; M8 (the #980 mutant — edges built before the rename rather
than after) caught by t035 and t038; M12 and M13 as above.

T045: CycloneDX and SPDX 2.3 report the same out-edge count for the
reclassified member. #980's failure mode is asymmetric by construction — CDX
keeps a stale edge where SPDX drops the relationship — so agreement is
evidence neither happened.

## F10 — the first "before" document was not a before document (T055)

Full numbers in `after-1275.md`. The finding worth keeping is the control.

The first attempt produced the baseline with `git stash` plus a rebuild, and
reported closure `pkg:hackage` as **106 both before and after** while the pass
logged `confirmed=59`. Three figures that cannot all be true, and the kind of
result it is tempting to explain rather than investigate.

`git stash` reverts the worktree to **HEAD** — and HEAD already contained
US1–US3, because the identity pass was committed in `e224e842` several steps
earlier. So the "before" was the feature *minus Phase 6's edges*. Worse, it
looked plausible: it still showed +278 edges and +3 components, exactly the
Phase 6 delta, so every number in the table was real and the label on the
column was wrong.

Caught by grepping the baseline's own log for `identity pass` and finding one
line. The real baseline is a detached `git worktree` at `001fa50`, and its log
contains **zero** — which is now stated at the top of `after-1275.md` as the
control, because a baseline that cannot be shown to lack the feature is not a
baseline.

Against the true baseline every criterion is met, and three figures confirm
the research rather than merely agreeing with it: **+278** out-edges is exactly
R6's independently measured declared-dependency count with **0** dangling; the
three added components are precisely R6's three distinct boot libraries
(`directory`, `ghc-heap`, `pretty`); and the one exact pair that does **not**
reclassify is `hashable 1.4.7.0`, whose mechanism is now pinned down rather
than inferred — the pinned revision has no default `hashable` attribute at
all, only versioned aliases, so `set.get("hashable")` returns nothing and the
member is reported `not-in-package-set`. That is also why that bucket reads
177 here against the sizing probe's 176 absent-with-version. R4's alias gap
costs exactly 1, and this is the row it costs.

## F11 — the milestone created three I7 offenders, found before the goldens were refreshed

Caught by a post-implementation `/speckit.analyze` pass, not by CI, and worth
recording because the sequencing is the lesson.

FR-012c's components are unreachable from the root **by construction**:
nothing depends on a GHC boot library except the closure members that are
themselves unreachable. The corpus invariant `orphan_explained` (I7) requires
every unreachable component to carry one of a closed set of markers —
`waybill:orphan-reason`, `waybill:unresolved-reason`, `waybill:closure-role`,
`waybill:version-absence = by-construction`, `waybill:component-tier = file`.
These components carried only C170 `compiler-supplied` and `sbom-tier`, and
**C170 was not in that set.**

`invariant-declarations.toml` has I7 rows for `haskell-security-advisories`
(3), `maven-guice` (3) and `python-flask` (13) and **none for
`nix-closure-moat`** — the one target that runs `--nix-closure`. So that
target sits at zero offenders today, and this milestone would have taken it to
three.

Measured, by replicating `is_explained` over the real documents:

```
baseline 001fa50   either way        components=342 unreachable=276 I7=0
1275  without the C170 arm           components=345 unreachable=279 I7=3
1275  with    the C170 arm           components=345 unreachable=279 I7=0
```

The third row is the fix; **the first row is why the fix is not an amnesty.**
The arm moves no cell on the pre-1275 document, so it accounts for exactly
the population this milestone created and masks nothing pre-existing.

C170 is now in both marker sets — `generate/orphan_reason.rs::already_explained`
and the invariant's own `is_explained`, which the former's comment requires to
mirror each other. `compiler-supplied` is a real explanation: the component
exists because a package set declared it, and it has no version because GHC
ships it inside its own derivation.

Two mutants, both caught: **M14** (the invariant's arm removed) by its own unit
test, **M15** (the component stops carrying C170) by `t038` and the new `t056`.

### Why this had to come before the golden refresh

The refresh was one dispatch away. Had it gone first, it would have baked in
three new I7 offenders on `nix-closure-moat`, and the path of least resistance
would then have been to **declare** them in `invariant-declarations.toml` —
recording a defect this milestone created as a known limitation, which is how
a declaration file stops meaning anything. It also falsifies the diff
prediction written before the dispatch, which said 3 files changed and nothing
else; the honest prediction now adds "and `nix-closure-moat` gains no I7
offenders".

`orphan_reason`'s emit-time classifier only ever looks at Go and npm
components, so **zero** `waybill:orphan-reason` annotations in a Nix document
is correct and not a second defect. That was checked before concluding
anything, because 0 of 279 looked exactly like a pass that had not run.

## F12 — the declared edges grow #1340's population, measured and not fixed here

Found while doing #1340's groundwork, which is the only reason it was found at
all: nothing in this milestone's own verification could have.

`scan_cmd.rs` runs this pass at **4789** and milestone 1209/1269's transitive
non-runtime scope propagation at **4805**. So every edge FR-012 adds is an
edge the propagation then traverses — and before FR-012, a non-runtime closure
member had no out-edges, so its scope propagated nowhere.

**Why no test here could have caught it.** The `project` fixture has no
non-runtime component at all, and the measured real project has exactly one
whose count is unchanged before and after (`excluded: 1` both sides). The
suite had **zero** scope assertions. So the path was invisible to every check
this milestone ran, and `nix-closure-moat` — where **10 of 13** non-runtime
components are closure members — would have been the first real exercise of
it, arriving as an unexplained golden diff.

Measured on a fixture built for it (`optional_scope`, a `buildable: False`
stanza whose only dependency is a confirmed closure member):

```
waybill-fixture-aeson    excluded   (reachable only through the disabled stanza)
  -> waybill-fixture-text     excluded   (inherited, via a FR-012 edge)
  -> waybill-fixture-bootlib  excluded   (inherited — and it is a BOOT LIBRARY)
  -> waybill-fixture-absent   excluded   (inherited)
```

The third line is issue #1340 exactly: the compiler ships `bootlib` in every
build whatever the flags say, so `excluded` is a true statement about the
dependency relationship and a false one about whether the artifact contains
the package.

**Not fixed here, deliberately.** #1340 says settling which reading wins
"needs a decision rather than a patch", and names it a milestone-1269 change.
This milestone has no requirement covering lifecycle scope; FR-011's
invariants are membership and roles. Taking the decision inside a milestone
about identity would be scope creep on a semantics question that has two
defensible answers. What was not optional was documenting it, so `t057` pins
the behaviour and names the issue.

### What this means for the golden refresh

The prediction is corrected a second time. Expect on `nix-closure-moat`:

- 3 files changed, 48 byte-identical — unchanged from before;
- **scope changes on existing components**, because its 10 non-runtime closure
  members now have declared out-edges. This is new and was not in either
  earlier prediction;
- **0–2 new components, not 3**: moat's goldens already carry `directory` as a
  versionless `pkg:hackage/directory`, so that edge binds to the existing
  component rather than synthesising one;
- no new I7 declaration (F11).

### #1340's population, measured for its spec

Corpus-wide, from the committed goldens: **10 of 78 boot-library components
carry a non-runtime scope**, in `haskell-security-advisories` (8) and
`nix-closure-moat` (2). The issue's open question — whether an exemption
implies the same for OS packages in a rootfs scan or stdlibs in the file tier —
is answered by mechanism rather than by a bare zero: those classes never carry
a scope at all (142 OS-package and 175 file-tier components, every one
no-scope), so they are not participants in the question. The probe is
`scope-always-present.py`, to be committed into #1340's own measurements
directory.

## F13 — the refresh matched the prediction on every count but one, and my explanation for the exception was also wrong

Run 41709905001, `regen_goldens=true` against this branch. The summary artifact
names both SHAs, which is #1287's fix working: **scanned `def75035`** (this
branch's tip) against a workflow definition from `001fa505`. The job name
confirms the branch input took effect — `Run public corpus
(1275-closure-hackage-identity): success`.

**3 files changed, 48 byte-identical**, enumerated per file rather than
trusting a count (#1286).

| | committed | regenerated |
|---|---|---|
| components | 384 | 387 |
| closure members | 362 | **362 — unchanged** |
| closure `pkg:hackage` | 32 | **134** |
| licensed / hashed | 32 / 32 | **134 / 134** |
| closure out-edges | 139 | **756** |
| components carrying C201 | 0 | **330** |
| non-runtime scopes | 13 | **13 — unchanged** |
| added | — | `exceptions`, `ghc-heap`, `pretty` |

The +102 identities land almost exactly on #1282's own independent figure for a
real library ("103 exact of 316"), which is the strongest corroboration this
milestone has: a number measured by someone else, on a different project,
reproduced by the implementation.

### The prediction that failed, and the explanation that also failed

I predicted **scope changes on this target**, on the strength of findings F12:
this milestone's declared edges feed milestone 1269's propagation, and 10 of
moat's 13 non-runtime components are closure members that now have out-edges.
Measured: non-runtime scopes **unchanged at 13**.

My first explanation was that in a real project a test dependency's
dependencies are usually also runtime-reachable, so m1269's existing immunity
protects them where my synthetic fixture had no runtime path. **Measured, that
is false**: the 10 non-runtime closure members have 50 new out-edges to 23
distinct targets, and **0 of those 23** are reachable from the root by a
runtime-only path. They are not immune, and they were not marked.

So something else prevents the propagation reaching them, and I do not know
what. Candidates not yet tested: m1269's unrooted-ecosystem skip (its
`skipped_unrooted` counter exists for exactly this shape), or an ordering
detail between the identity pass at `scan_cmd.rs:4789` and the propagation at
`4805`. **Recorded as an open question rather than a third guess** — the
project's standard is that a theory gets tested before anything is built on
it, and I have now been wrong twice in a row about this interaction.

**It does not block the refresh.** The goldens are what the code produces, and
every other figure matches. What it blocks is any claim that F12's interaction
is understood; milestone 1276's `t057` pins the behaviour on a fixture where
it demonstrably occurs, which is the right place for it until this is run down.
