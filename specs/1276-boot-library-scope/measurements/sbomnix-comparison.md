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
is as close as that comparison can get. 16× waybill's 527. The cause is the
one the spec names: sbomnix treats a store reference as a dependency, waybill
emits only what the package set *declares*.

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
inside it. Measured separately (see
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
point of a package model, and 615 unnameable derivations is an identity
problem this milestone does not address. The bucket that moved is the one
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
genuinely ships. The headline ratios do not move: sbomnix still reports 3×
the components and 10× the edges, because it walks store references while
waybill emits what a package set declares, and this file's original conclusion
is unchanged — **sbomnix sees more of the build, waybill sees more of the
packages.**

What did *not* improve is worth stating too. Edges, licenses and hashes are
identical; 1279 touches classification, not acquisition. The 615 unnameable
derivations — the one bucket that is a genuine identity gap, with 106 named
`source` fetches inside it that sbomnix emits with no identifier at all —
is exactly where it was.

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
