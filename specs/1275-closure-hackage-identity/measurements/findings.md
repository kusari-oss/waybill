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
