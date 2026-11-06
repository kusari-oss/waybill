# waybill against sbomnix, on one Nix closure

**Date**: 2026-11-03 | **sbomnix**: 1.9.0+g95480c2 | **waybill**: this branch
(milestone 1275 + 1276 US1)
**Subject**: the **`nix-closure-moat`** corpus target at its pin, nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`, 1,290 derivations — the same
closure every measurement in milestones 1275 and 1276 is taken against.

> **Subject corrected 2026-11-04.** This line originally named the
> `waybill-fixture-ifd` flake. It cannot have produced these figures: that
> fixture exists to *require* import-from-derivation, which `--nix-closure`
> refuses, so waybill emits **3** components against it and records
> `waybill:nix-closure-degraded = {"reason":"import-from-derivation-required"}`.
> Re-running the harness against moat reproduces the documented shape
> (sbomnix ~1,274, waybill ~409 today), and m1275's and m1276's own
> measurement files reference `nix-closure-moat` throughout. The findings
> below are unaffected — they are about mechanisms, not about which flake —
> but the *Reproducing* section did not work as written, which is the thing
> a committed probe exists to prevent. See the re-check at the end.
**Harness**: `compare-sbomnix.sh`, committed beside this file.

Milestone 1275's spec carried an inherited figure — "sbomnix emits 8,230 edges
on the same project by treating store references as dependencies" — from
#1282 rather than from an observation. The project's standard is that a number
describing external behaviour must be traceable to a measurement. This is that
measurement, and it answers a question #1340 raised without asking: what does
the other tool do about boot libraries?

## The comparison

| | sbomnix 1.9.0 | waybill |
|---|---|---|
| components | **1,289** | 345 |
| `dependsOn` edges | **8,497** | 527 |
| components with a license | 183 | 106 |
| components with a hash | **0** | **106** |
| components with **no PURL at all** | **107** | 0 |
| PURL types | `pkg:nix` 1,182 · none 107 | `pkg:generic` 217 · `pkg:hackage` 127 · `pkg:github` 1 |
| distinct names | 751 | 339 (234 shared) |

**The edge figure is confirmed at the order #1282 claimed**: 8,497 against the
inherited 8,230, on a different project and a different sbomnix version, which
is as close as that comparison can get. 16× waybill's 527.

> **The cause stated here was wrong, corrected 2026-11-05.** This paragraph
> read: "sbomnix treats a store reference as a dependency, waybill emits only
> what the package set *declares*." The second half is true; the first is not.
> sbomnix is emitting the graph `nix derivation show` records in
> **`inputs.drvs`** — the derivations whose outputs each build actually
> consumed — and on the moat closure that graph has **8,230** edges, equal to
> sbomnix's count to the edge. It is the build DAG, not a loose reading of
> store references, and waybill does not read it at all. See the correction at
> the end of this file; probe at `input-graph-probe.py`.

## The finding that reframes #1340

```
GHC boot libraries, by tool:
  pretty       sbomnix=NO    waybill=yes
  directory    sbomnix=NO    waybill=yes
  ghc-heap     sbomnix=NO    waybill=yes
  array        sbomnix=NO    waybill=yes
  binary       sbomnix=NO    waybill=yes
  deepseq      sbomnix=NO    waybill=yes
```

**sbomnix emits no component for any of them**, and the reason is structural
rather than a gap in its implementation: a boot library is not a separate
derivation. The compiler ships it *inside* its own derivation, so a tool that
walks derivations cannot see one, however thoroughly it walks.

Two consequences worth stating plainly:

1. **The defect #1340 describes cannot arise in sbomnix.** There is no
   component to mis-scope. That is not a point in sbomnix's favour: the
   dependency is real and present in the artifact, and a consumer asking "what
   is in this build" gets a less complete answer. waybill can mis-scope these
   components *because it emits them at all*, which it does by reading the
   package set's declared dependencies rather than only store references.
2. **It is the same mechanism in both milestones.** Milestone 1275's FR-012
   edges and these boot-library components both come from
   `runtime_relations` — what the revision *declares* — and that is exactly
   what a store-reference walk cannot recover. The 8,497-vs-527 gap and the
   six-vs-zero gap are the same design choice measured in two directions: one
   tool sees more edges, the other sees more packages.

## Where each tool is stronger, measured rather than asserted

- **sbomnix: completeness of the derivation graph.** 1,289 components against
  345. waybill filters by role (`emits_as_component`) and merges build
  variants by `(pname, version)`, which is deliberate and documented, but it
  does mean 944 derivations appear in no waybill component.
- **waybill: identity and integrity.** Zero of sbomnix's components carry a
  hash, and **107 carry no PURL at all** — a component with no identifier is
  one a consumer cannot match against anything. Its 1,182 `pkg:nix` PURLs use
  a type that is **not in the purl spec**, which is precisely why waybill's
  closure emitter uses `pkg:generic` and, since milestone 1275,
  `pkg:hackage` where the pinned set confirms the identity.
- **Licenses: not comparable as a ratio.** 183 against 106 over different
  populations — sbomnix has nixpkgs metadata for all 1,182 of its `pkg:nix`
  components, waybill has the package set's declaration for its 127
  `pkg:hackage` ones.

## A measurement error avoided, and worth recording

The first run used a bare store path and reported **0 licenses** for sbomnix.
That would have been a false claim about the tool: its own log said
`No nixpkgs metadata source is available for store-path targets. Skipping
nixpkgs metadata. Use a flakeref target instead.` With a flakeref it emits
183. The harness takes a flakeref and says why in a comment, because the
limitation was the invocation's and reporting it as the tool's would have been
the kind of error this file exists to avoid.

The 0 hashes figure survived that correction and is a real difference.

## Reproducing

```sh
SBOMNIX=<sbomnix binary> PROJ=<flake dir> WAYBILL=./target/debug/waybill \
  bash specs/1276-boot-library-scope/measurements/compare-sbomnix.sh
```

sbomnix 1.9.0 is in the local nix store from a prior milestone's work; it is
not on `PATH`. `--buildtime` and the flakeref form are both load-bearing.

## Where the 944-component difference actually comes from

Decomposed rather than attributed to "waybill filters more". The closure holds
**1,290** derivations; sbomnix emits **1,289** components and waybill **345**.

### 1. 800 derivations that nothing in the closure references — 107 of them named `source`

A derivation no other derivation reaches through `buildInputs`,
`propagatedBuildInputs`, `nativeBuildInputs` or `propagatedNativeBuildInputs`.
What they are, by name:

```
  107  source                            <- fetchurl / fetchgit source derivations
    9  gnu-config
    7  apple-sdk-14.4
    7  expand-response-params
    6  clang-wrapper
    5  sdk-hook.sh
    5  xcode-project-check-hook
    5  CoreSymbolication
```

These are source fetches, setup hooks and compiler wrappers — the machinery of
*how* a build runs, not packages that end up identifiable in an artifact. The
107 `source` derivations are the clearest case, and sbomnix's own output
confirms it from the other side: its most repeated component names are
`source` ×107, `gnu-config` ×9, `apple-sdk` ×8,
`cctools-binutils-darwin-wrapper` ×7, `die-hook` ×7. **All 107 `source`
components are among its 107 carrying no PURL at all** — a component named
`source` with no identifier is one no vulnerability feed or license policy can
match.

waybill drops these by role, with one deliberate exception: it keeps the **47**
unreferenced derivations that apply a patch. Milestone 1035 measured why — `jq`
and `lua` are unreferenced in one project's closure and carried 6 of its 18
CVEs, so dropping them would have taken the patches with them.

### 2. 183 derivations collapsed as build variants of 146 identities

Among the referenced derivations, 287 distinct `(pname, version)` pairs are
produced by 470 derivations. The worst cases:

```
  die-hook 26.05pre-git                              7 derivations
  perl 5.42.0                                        6 derivations
  update-autotools-gnu-config-scripts-hook 26.05…    6 derivations
```

waybill merges these to one component per identity and unions their roles.
sbomnix emits each derivation, which is why `die-hook` appears seven times in
its document.

### 3. ~21 referenced derivations with no `pname`

No name to build an identity from, so no component.

### So the difference is a modelling choice, stated plainly

**sbomnix models the build graph**: one component per derivation, sources and
hooks and per-variant duplicates included. **waybill models the package set**:
one component per identifiable package, with the build machinery dropped and
variants merged.

Neither is wrong for its own question. For "reproduce or audit this build",
sbomnix's 1,289 is the more faithful record and waybill's role filter discards
information. For "what software is in this artifact, and can I match it
against a feed", 944 of those components are machinery — and 107 of them carry
no identifier at all, which is the form in which the difference stops being a
matter of taste.

The honest summary of both halves of this comparison: **sbomnix sees more of
the build, waybill sees more of the packages.** The boot libraries are the
sharpest instance — six packages genuinely in the artifact that a
derivation-walk cannot reach, and 944 derivations that a package model
deliberately does not keep.


---

# Re-check, 2026-11-04 (after milestones 1277 and 1278)

Same harness, same sbomnix 1.9.0+g95480c2, same moat pin, same host. Run
because milestone 1278 (#1348) taught the closure reader to read
`structuredAttrs`, which moves two of the three buckets this file decomposed.

## The comparison now

| | sbomnix | waybill | (2026-11-03) |
|---|---|---|---|
| components | **1,274** | **409** | 1,289 / 345 |
| `dependsOn` edges | **8,230** | **788** | 8,497 / 527 |
| with a license | 301 | 137 | 183 / 106 |
| with a hash | **0** | **136** | 0 / 106 |
| components with **no PURL at all** | **106** | **0** | 107 / 0 |
| distinct names | 819 | 403 (**305 shared**) | 751 / 339 (234 shared) |
| GHC boot libraries | **0 of 6** | **6 of 6** | unchanged |

**The gap is 865, from 944.**

## What of that is milestone 1278's, measured rather than inferred

The 2026-11-03 figures and today's differ by more than one milestone: m1276's
back half and all of m1277 landed between them, **and the closure itself
shrank from 1,290 derivations to 1,275 with nixpkgs pinned**, so a nix release
moved the ground too. Attributing the whole delta to 1278 would be exactly the
derived-arithmetic-as-measurement error this directory exists to avoid.

So it was measured with one variable: the same closure, the same flags, the
same host, only the binary differing — `67f1cb75` (the m1277 merge) against
`c3b4ce50` (the m1278 merge).

| | pre-1278 | post-1278 | 1278's share |
|---|---|---|---|
| components | 373 | 409 | **+36** |
| edges | 779 | 788 | **+9** |
| licensed | 135 | 137 | +2 |
| hashed | 134 | 136 | +2 |
| `pkg:hackage` | 158 | 160 | +2 |

The remaining 345 → 373 spans three causes that are not separated here and are
not claimed.

## The 865, decomposed against the 1,275-derivation closure

| bucket | count | what it is |
|---|---|---|
| no `pname` in **either** attribute carrier | **615** | waybill can name nothing, so emits nothing. sbomnix names them from the derivation — and this is where its **106 `source` components with no PURL at all** live |
| build variants merged | **248** | 660 named derivations collapse to 412 distinct `(pname, version)` |
| unreferenced, named, applying no patch | **~103** | dropped by role. 41 of the 144 unreferenced-and-named are *kept* because they patch |
| *less* components only waybill has | **−98** | the 6 boot libraries, and package-set members a derivation-walk cannot reach |

## What is actually left, as opposed to deliberate

Two of the three buckets should not close. Merging variants is the point of a
package model; dropping build machinery is what keeps a document matchable
against a vulnerability feed. Both are documented choices, and this file's
original conclusion stands: **sbomnix sees more of the build, waybill sees
more of the packages.**

The bucket that *is* an identity gap rather than a modelling choice is the
**615 unnameable derivations**, and specifically the **106 named `source`**
inside it.

> **"Unnameable" is wrong, corrected 2026-11-05.** All 615 carry a `name`;
> what they lack is a `pname`. More to the point, the blocker was never
> identity: `inputs.drvs` names a consumer for every one of them, and **556 of
> the 615 have exactly one named consumer**, so each can be attached to the
> package it was fetched for. Identity is available too — 106 carry
> `owner`/`repo`/`rev`, 425 carry a url and an `outputHash`, 84 have only a
> name and their owning derivation. See the correction at the end. Measured separately (see
`specs/1278-closure-structured-attrs/measurements/source-identity-probe.py`):
104 of 106 carry `owner`, `repo` and `rev` and key to
`pkg:github/<owner>/<repo>@<ref>`; the content-hash fallback is needed for
none of them; and the 16 colliding identities are all the same bytes fetched
twice, confirmed by output hash. sbomnix emits all 106 **with no identifier at
all**, which is the form in which this stops being a matter of taste.

So the honest answer to "what is left": one item, and it is identity rather
than coverage.

## One number this re-check cannot explain

sbomnix's edge count reads **8,230** today — exactly the figure #1282
asserted, and which this file replaced with a measured 8,497 while noting it
was "confirmed at the order #1282 claimed". The inherited number now
reproduces exactly. No explanation is offered; the possibility worth stating
is that #1282's figure came from this same closure after all, in which case it
was never the unsourced estimate it appeared to be.

---

# Re-check, 2026-11-05 (after milestone 1279)

Same harness, same sbomnix `1.9.0+g95480c2` already in the store, same moat
pin, same host, same flags. Run because milestone 1279 (#1346) taught the
closure classifier to read four more nixpkgs dependency attributes, and the
bucket it should move is the one this file identified as a real gap rather
than a modelling choice.

**Measured with one variable.** Two waybill binaries built from
`48a5115e` (main's tip before 1279) and `4d43eae9` (the 1279 merge), run
against the same project copy with the same sbomnix output. The 2026-11-04
figures could not isolate this milestone — m1277 and m1278 both landed since,
and the closure itself has moved once under a pinned nixpkgs.

**The control holds**: the pre-1279 arm reproduces 2026-11-04's re-check
exactly — sbomnix 1,274 / 8,230 edges / 301 licensed / 0 hashed / 106 with no
PURL / 819 names, waybill 409 / 788 / 137 / 136 / 0 / 403, **305 shared**. So
the deltas below are milestone 1279's and not the host's.

## The comparison now

| | sbomnix | pre-1279 | **post-1279** | 1279's share |
|---|---|---|---|---|
| components | 1,274 | 409 | **413** | **+4** |
| `dependsOn` edges | 8,230 | 788 | 788 | 0 |
| with a license | 301 | 137 | 137 | 0 |
| with a hash | 0 | 136 | 136 | 0 |
| with **no PURL at all** | 106 | 0 | 0 | 0 |
| distinct names | 819 | 403 | **407** | +4 |
| **shared with sbomnix** | — | 305 | **309** | **+4** |
| **sbomnix-only (the gap)** | — | 514 | **510** | **−4** |
| waybill-only | — | 98 | 98 | 0 |
| GHC boot libraries | 0 of 6 | 6 of 6 | 6 of 6 | unchanged |

**The gap is 861, from 865.** Small, and it is the *right four*: every one of
the four components waybill newly emits is a name sbomnix already had —
waybill-only stays at 98 while shared rises by exactly 4, so the closure is
convergence and not two tools inventing different components.

## The four, each attributed to the attribute that reaches it

| component | before → after | reached through |
|---|---|---|
| `libcxx 21.1.6+apple-sdk-26.4` | `unreferenced` → `artifact-input` | `depsTargetTargetPropagated` |
| `bootstrap-stage0-libcxx` | `unreferenced` → `artifact-input` | `depsTargetTargetPropagated` |
| `bootstrap-stage0-compiler-rt` | `unreferenced` → `artifact-input` | `depsTargetTargetPropagated` |
| `nss-cacert 3.123` | `unreferenced` → `build-tooling` | `propagatedNativeBuildInputs` |

Eight derivations change role in all; the other four (`compiler-rt`,
`compiler-rt-libc`, `xcbuild`, `find-xml-catalogs-hook`) were already emitted
on the strength of a patch or another reference, so their role improves
without changing the component count.

**`libcxx` is the case #1346 was filed about, in its clearest form.** It is
the C++ standard library; it genuinely ships inside the artifact. Before this
milestone waybill called it `unreferenced` — and on this closure that meant it
was **not emitted at all**, because an `unreferenced` member applying no patch
becomes no component. A consumer asking "what is in this build" was not told
about the C++ runtime it links against. sbomnix had it all along
(`pkg:nix/libcxx@21.1.6%2Bapple-sdk-26.4`).

That is a different kind of improvement from the other three. `nss-cacert`
moving to `build-tooling` adds a component and then marks it
`scope: "excluded"` — useful, but a claim about the build. `libcxx` arriving as
`artifact-input` with no scope is a missing *artifact* dependency appearing.

## The buckets, recomputed on the same closure

| bucket | pre | post | |
|---|---|---|---|
| no `pname` in either carrier | 615 | 615 | unchanged — 1279 adds no names |
| build variants merged | 248 | 248 | unchanged |
| `unreferenced` **and** named | 144 | 139 | −5 |
| …of which kept because they patch | 41 | 38 | |
| …of which dropped by role | **103** | **101** | **−2** |
| *less* components only waybill has | −98 | −98 | unchanged |

Two of the three buckets are untouched and should be: merging variants is the
point of a package model, and the 615 fetch derivations are a problem this
milestone does not address. (Called "an identity problem" when first written;
it is an *ownership* problem, and `inputs.drvs` solves it — see the correction
at the end of this file.) The bucket that moved is the one
this file called the role filter.

**The probe's −2 and the documents' +4 are different measurements, and the
documents are the authoritative one.** This decomposition counts identities by
`pname` alone; waybill falls back to the derivation's `name` when `pname` is
absent, so its emitted set is larger than the probe's and moves differently.
The probe is kept because it reproduces 2026-11-04's buckets exactly, which is
what makes the comparison legible — not because its delta is the answer.

## So: better results?

Yes, on this closure, and narrowly. 4 components, all of them real, all of
them already in the other tool's output, one of them a runtime library that
genuinely ships. The headline ratios do not move: sbomnix still reports 3× the
components and 10× the edges.

> **The reason given here was wrong, corrected 2026-11-05.** This read
> "because it walks store references while waybill emits what a package set
> declares", and concluded that the file's original framing —
> *sbomnix sees more of the build, waybill sees more of the packages* — was
> unchanged. The first clause is false and the framing flatters us: sbomnix
> emits the `inputs.drvs` graph, which is what the build actually consumed,
> and waybill emits a manifest's opinion of it. See the correction at the end.

What did *not* improve is worth stating too. Edges, licenses and hashes are
identical; 1279 touches classification, not acquisition. The 615 fetch
derivations are exactly where they were — and the correction below explains why
that bucket is much cheaper to close than this file previously claimed.

## This system is not the one the goldens pin

Every figure above is **aarch64-darwin**, 1,275 derivations, matching the
earlier runs in this file. The corpus goldens pin **x86_64-linux**, 1,749
derivations, where the same milestone moves 7 derivations rather than 8 and
emits 2 new components rather than 4.

The two closures also populate *different attributes*, which this re-check
confirms in live output rather than in a probe: after the widening, darwin's
`fields-unconsulted` is `["defaultBuildInputs", "defaultNativeBuildInputs"]`
while linux's is `["defaultNativeBuildInputs", "extraNativeBuildInputs"]`. The
C205 marker lands on **3** components here and on **9** there. A figure from
one system quoted against the other is wrong in membership, not in magnitude —
the error milestone 1279's own spec made twice before measuring.

## Reproducing

```sh
# sbomnix 1.9.0+g95480c2, already in the store from the 2026-11-03 run
S=/nix/store/cbh4k59d3sa3w9axcwdbk6jzkg99ybvk-python3.14-sbomnix-1.9.0+g95480c2/bin/sbomnix

# Copy the corpus clone first. Evaluating a flake in place can re-lock it:
# on 2026-11-04 that rewrote a committed `flake.lock` and broke 11 tests in
# an unrelated file. Verify the copy afterwards.
cp -R ~/.cache/waybill/corpus/53983fe1021353b3/d069055*/repo moat-proj
md5 -q ~/.cache/waybill/corpus/53983fe1021353b3/d069055*/repo/flake.lock moat-proj/flake.lock

SBOMNIX=$S PROJ=$PWD/moat-proj WAYBILL=./target/debug/waybill \
  ./specs/1276-boot-library-scope/measurements/compare-sbomnix.sh
```

---

# Correction, 2026-11-05: the edge deficit is a field we do not read

Everything above explained waybill's edge deficit as a difference of
philosophy — sbomnix walks store references, waybill emits what a package set
declares, so the gap is a modelling choice and largely deliberate. **That is
wrong, and it was wrong in the direction that flattered us.**

`nix derivation show` gives every derivation an **`inputs.drvs`** map: the
derivations whose outputs this build consumed. It is present on all 1,275
derivations of this closure. waybill never reads it — the closure reader
parses `buildInputs`-style attributes, which is the *declared* view, and
`inputs.drvs` is what the build actually used.

Probe: `input-graph-probe.py`, committed beside this file.

## Measured

| | declared attributes (what waybill reads) | `inputs.drvs` (what nix records) |
|---|---|---|
| edges, derivation level | 3,893 | **8,230** |
| edges, collapsed to component identities | 2,926 | **6,479** |
| derivations explained | 533 of 1,275 | **1,274 of 1,275** |
| roots | many | **1** (`moat-0.1`) |
| reachable from the roots | — | **1,275 of 1,275** |

**8,230 is sbomnix's figure exactly.** That is what settles the question:
sbomnix emits the `inputs.drvs` graph at derivation granularity. It is not
treating store references loosely; it is reading the build DAG, and the
inherited #1282 figure of 8,230 was right all along — the 2026-11-03 re-check
that "confirmed it at the order" was measuring a different flake.

Two consequences that have nothing to do with edge counts:

- **`unreferenced` would fall from 742 to 1.** The role's residual population
  is almost entirely derivations that `inputs.drvs` explains and the declared
  attributes do not. Milestone 1279 moved 8 of them.
- **The 615 fetch derivations get an owner.** 556 have exactly one named
  consumer, 30 several, 29 none — so a source tarball can be emitted *as the
  source of* the package it built, rather than as an orphan or not at all.

Their identity was never the blocker either:

| | count |
|---|---|
| `owner` + `repo` + `rev` → `pkg:github/<owner>/<repo>@<rev>` | 106 |
| url + `outputHash` → `pkg:generic?download_url=…&checksum=…` | 425 |
| only a name and an owning derivation | 84 |

sbomnix emits all 615 with **no PURL at all**, so this is a place waybill
could be ahead rather than catching up.

## What this file should have said

Not *sbomnix sees more of the build, waybill sees more of the packages*. That
framing made a missing read sound like a considered position. Accurately:

> sbomnix emits the build graph nix records. waybill emits a manifest's
> opinion of that graph, and is missing the graph itself. Where waybill is
> genuinely ahead is **identity** — PURLs on every component, hashes, Hackage
> resolution, the GHC boot libraries no derivation walk can see — and that
> advantage is independent of the graph, so there is no trade to defend.

## Scale of the fix

Reading `inputs.drvs` is one field. On this closure it would take waybill from
413 components and 788 edges to roughly **825 components and ~6,500 edges**,
against sbomnix's 1,274 and 8,230 — the residual being build variants merged
on `(pname, version)`, which is deliberate and should be annotated rather than
undone.

**The 825 is the honest figure, not 1,274.** Three separate collapses sit
between the derivation count and what waybill would emit, and conflating any
of them with the others is how this file got its original conclusion wrong:
1,275 derivations → 825 distinct identities (variant merging) → emitted
components. Quoting sbomnix's derivation-granular 1,274 as the target would
mean giving up variant merging, which is a feature.

## What was wrong, and the shape of the error

Three claims in this file were wrong, all in the same direction:

1. "sbomnix treats a store reference as a dependency" — it reads the build DAG.
2. "615 **unnameable** derivations" — all 615 carry a `name`; what they lacked
   was an owner, which `inputs.drvs` supplies for every one of them.
3. "the one bucket that is a genuine identity gap" — identity was available
   for 531 of the 615 without any new work.

Each made a gap look like a decision. The common cause is that the comparison
measured **outputs** — two documents, counted and differenced — and never
measured the **input** both tools were reading. One probe over the closure JSON
answered in minutes what three re-checks of the SBOMs could not, because the
answer was never in the SBOMs.

---

# Re-check, 2026-11-06 (after milestones 1280 and 1282)

Same harness, same sbomnix `1.9.0+g95480c2` from the store, same moat pin,
same aarch64-darwin host, same flags. waybill at `bf5fb15` — main's tip after
#1364 merged.

Run because the 2026-11-05 correction at the end of this file made a
**falsifiable prediction**, and both halves of it have now shipped: milestone
1280 reads `inputs.drvs` for edges, and milestone 1282 emits the `pname`-less
members. The prediction was

> On this closure it would take waybill from 413 components and 788 edges to
> roughly **825 components and ~6,500 edges**.

## The comparison now

| | sbomnix | post-1279 | **post-1282** | predicted |
|---|---|---|---|---|
| components | 1,274 | 413 | **942** | ~825 |
| `dependsOn` edges | 8,230 | 788 | **5,131** | ~6,479 |
| with a license | 301 | 137 | **225** | — |
| with a hash | **0** | 136 | **136** | — |
| with **no PURL at all** | **106** | 0 | **0** | — |
| distinct names | 819 | 407 | **810** | — |
| **shared with sbomnix** | — | 309 | **712** | — |
| **sbomnix-only, raw names** | — | 510 | **107** | — |
| **sbomnix-only, normalised** | — | — | **32** | — |
| waybill-only | — | 98 | **98** | — |
| GHC boot libraries | 0 of 6 | 6 of 6 | **6 of 6** | — |

waybill's own C184 record for the same run: 1,275 derivations, 917 components
emitted by the tier, 5,658 edges, `member-roles` `{source: 455, package: 12,
scaffolding: 7, fragment: 65}`. The tier's 5,658 and the document's 5,131 are
different scopes and the difference is not explained here.

**The name gap closed from 510 to 107 while waybill-only held at exactly 98**,
so this is convergence on one closure rather than two tools inventing
different components. The 107 is a naming artifact — normalised, the true
residual is **32**, and the gap that matters is identity rather than
coverage. See below.

## Both predictions were wrong, in opposite directions

Components overshot by 117, edges undershot by ~1,350. Recorded because a
prediction that is quietly dropped when it misses is worse than no prediction.

The component overshoot has a measured cause: the ~825 came from "1,275
derivations → 825 distinct identities (variant merging)", which assumed the
new population merges on `(pname, version)` the way the named one does. It
does not — milestone 1282 gives each member a content- or recipe-hash
identity, so 681 `pname`-less derivations yield 603 distinct identities rather
than collapsing toward the variant-merged figure.

The edge undershoot is **not** explained here. FR-013 stops the walk at
sources and packages and passes *through* scaffolding and fragments, which
removes transitive edges a full `inputs.drvs` collapse would keep, and that is
the obvious candidate — but it has not been measured, and this file's history
is that the obvious candidate was wrong three times running.

## The residual gap is 32, not 107 — and it is not the important one

The 107 is a **name-convention artifact**, caught only by testing it. nix names
a Python derivation `python3.13-urllib3`; waybill names the component by its
`pname`, `urllib3`. Comparing raw name sets counts every such package as
missing.

Normalising nix's language prefixes (`python3.N-`, `perl5.X.Y-`, …) before
comparing:

| | names |
|---|---|
| sbomnix-only, raw | 107 |
| sbomnix-only, normalised | **32** |

**74 of the 79 `python3.13-*` names are already in the document** under their
bare `pname`. What remains is 32, and it is mostly build machinery — the six
`bootstrap-stage*-stdenv-darwin` stages, `stdenv-darwin`, `pkg-config-role-hook`,
`pkgs-config-setup-hook`, `cctools-binutils-darwin`, `llvm-binutils`,
`macOS-SDK`, `cups-headers` — with a short tail of real tools: `gawk 5.4.0`,
`gzip 1.14`, `gnused 4.9`, `less 692`, `patch 2.8`, `pkgconf`, `mailcap`,
`libstemmer`, `jailbreak-cabal`, `libSystem`.

All 32 are `pname`-bearing and `unreferenced` — milestone 1282's documented
exclusion, unchanged.

## The gap that matters is identity, not coverage

The packages a vulnerability feed would care about are **already in the
document** and still cannot be matched, because they carry no ecosystem:

| package | version | PURL emitted |
|---|---|---|
| `urllib3` | 2.6.3 | `pkg:generic/urllib3@2.6.3` |
| `requests` | 2.33.1 | `pkg:generic/requests@2.33.1` |
| `lxml` | 6.0.2 | `pkg:generic/lxml@6.0.2` |
| `jinja2` | 3.1.6 | `pkg:generic/jinja2@3.1.6` |
| `certifi` | 2026.01.04 | `pkg:generic/certifi@2026.01.04` |
| `openssl` | 3.6.2 | `pkg:generic/openssl@3.6.2` |
| `curl` | 8.20.0 | `pkg:generic/curl@8.20.0` |
| `zlib` | 1.3.2 | `pkg:generic/zlib@1.3.2` |

**781 of 942 components carry `pkg:generic`.** OSV and the other feeds key on
an ecosystem-qualified PURL; `pkg:generic` has none, so every one of these is
invisible to a vulnerability query despite being present, correctly named and
correctly versioned.

sbomnix is not ahead here — its 1,168 `pkg:nix` PURLs use a type the purl spec
does not define, and its remaining 106 carry no PURL at all. Neither document
answers "is this build affected by CVE-X". That makes it open ground rather
than a deficit.

**This reorders the work.** Closing the 32-name coverage gap adds `gzip` and
`gawk` as two more unmatchable `pkg:generic` components. Giving the 781 their
real ecosystem types makes the packages already present answerable to a feed.
The second is worth more, and the two are independent.

## Milestone 1282 emits every member it claims to, checked exactly

| role | distinct identities in the closure | emitted | delta |
|---|---|---|---|
| `source` | 455 | 455 | **0** |
| `package` | 12 | 12 | **0** |
| `scaffolding` | 7 | 7 | **0** |
| `fragment` | 65 | 65 | **0** |
| **total** | **539** | **539** | **0** |

615 derivations of the 1,275 declare no `pname` in **either** carrier; they
reduce to 539 distinct identities under content- and recipe-hash merging, and
all 539 are in the document. FR-001 holds on this closure with nothing left
over.

## Four probe errors, recorded because each produced a figure that looked right

1. **"The sbomnix-only names are the ones it cannot identify."** Plausible —
   sbomnix has 106 components with no PURL and the gap was 107. Measured: **0
   of 107**. Its 106 unidentified components share a single name, `source`,
   which waybill emits. The near-equal counts were a coincidence.
2. **"60 of the 72 `package`-role members are missing."** The probe assumed
   every `pname`-less derivation becomes a member-role-bearing component.
   Most do not: `pname()` falls back to the derivation's own name, so
   `libxo-1.7.5` is emitted as `libxo` version `1.7.5` with no member role at
   all.
3. **"Ten derivations are a genuine gap in milestone 1282."** The sharpest of
   the three, because it named a defect in code that had merged an hour
   earlier, and it was wrong. `cups-headers`, `macOS-SDK`,
   `python3.13-pystemmer` and `shell_cmds` all declare a `pname` — in the
   **top-level `structuredAttrs` carrier**, which milestone 1278 taught
   `attr_str` to read and which the probe never looked at. The probe read
   `env.pname` and `env.__json` only. 66 of the closure's 1,275 derivations
   carry their `pname` there, and counting them as nameless is what produced
   both the 681 (against the true 615) and the fictitious gap.

4. **"The gap is 107 names."** Taken from the harness's own `names:` line and
   carried into a decomposition, an issue draft and a priority call before
   being tested. nix names a Python derivation `python3.13-urllib3`; waybill
   names it `urllib3`. **74 of the 79 `python3.13-*` names were already in the
   document.** The true residual is 32.

**All four errors have one shape**: the probe modelled what waybill reads or
emits instead of reading it. That is the same failure the 2026-11-05 correction
identified — it measured the two documents and never measured the input both
tools were given — and it recurred three times in one afternoon against a
file that opens by warning about it. The check that settled each was querying
the emitted document or the closure itself.

## Where each tool stands now

**waybill's identity advantage is unchanged and is the durable one**: every
component carries a PURL against sbomnix's 106 with none, 136 carry a hash
against 0, and the six GHC boot libraries no derivation walk can reach are
still 6-of-6 against 0-of-6.

**sbomnix still reports more of the build**: 1,274 components to 942 and 8,230
edges to 5,131. The ratio was 3× and 10× at the last re-check; it is now
**1.35× and 1.6×**.

## Reproducing

```sh
S=/nix/store/cbh4k59d3sa3w9axcwdbk6jzkg99ybvk-python3.14-sbomnix-1.9.0+g95480c2/bin/sbomnix
cp -R ~/.cache/waybill/corpus/53983fe1021353b3/d069055*/repo moat-proj
md5 -q ~/.cache/waybill/corpus/53983fe1021353b3/d069055*/repo/flake.lock moat-proj/flake.lock
SBOMNIX=$S PROJ=$PWD/moat-proj WAYBILL=./target/debug/waybill \
  bash specs/1276-boot-library-scope/measurements/compare-sbomnix.sh
```

`flake.lock` was verified byte-identical on both sides after this run.
