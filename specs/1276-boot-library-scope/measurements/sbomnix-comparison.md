# waybill against sbomnix, on one Nix closure

**Date**: 2026-11-03 | **sbomnix**: 1.9.0+g95480c2 | **waybill**: this branch
(milestone 1275 + 1276 US1)
**Subject**: the `waybill-fixture-ifd` flake, nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`, 1,290 derivations — the same
closure every measurement in milestones 1275 and 1276 is taken against.
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
