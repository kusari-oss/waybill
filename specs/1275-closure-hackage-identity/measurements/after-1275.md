# After: what the closure tier emits with milestone 1275

**Task**: T055 (added by `/speckit.analyze`, finding E1) | **Date**: 2026-11-03
**Before**: `001fa50`, built in a detached worktree | **After**: this branch
**Project / revision**: as `baseline-001fa50.md` — a flake depending on `aeson`
and `text`, nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, 1,290
derivations, aarch64-darwin.

Both scans use the **same command line**:

```sh
waybill sbom scan --no-deep-hash --nix-closure --no-deps-dev \
    --no-clearly-defined --path . --format cyclonedx-json --output cyclonedx-json=…
```

**Control**: the baseline binary logs **0** `identity pass` lines, which is the
positive signal that it is genuinely feature-off rather than a build that
silently kept the feature. The first attempt at this comparison got that
wrong — see below.

## The measurement

| | before | after |
|---|---|---|
| components | 342 | **345** (+3) |
| closure members (C182) | 323 | **323** — unchanged |
| closure-appended `pkg:generic` | **276** | **217** (−59) |
| closure `pkg:hackage` | **47** | **106** (+59) |
| …of those, carrying a license | 47 | **106** |
| …of those, carrying a hash | 47 | **106** |
| generic members carrying a reason | **0** | **217** |
| out-edges from closure members | 245 | **523** (+278) |
| dangling edge targets | — | **0** |
| components added | — | `directory`, `ghc-heap`, `pretty` |

## Against the success criteria

| criterion | required | measured |
|---|---|---|
| **SC-001** | 59 of 60 exact pairs carry `pkg:hackage` | **59** ✓ |
| **SC-002** | licensed at no lower a rate than the resolved set's 47 of 65 (72%) | **59 of 59 = 100%** ✓ |
| **SC-003** | zero `pkg:hackage` PURLs whose version the set does not record | **0** ✓ |
| **SC-004** | every remaining generic member carries a reason | **217 of 217** ✓ |
| **SC-005a** | reclassified members carry out-edges, every target present | **+278 edges, 0 dangling** ✓ |
| **SC-006** | same closure, same derivation count, same roles | **323 members; roles identical** ✓ |

Outcome tally from the pass itself, summing to the 276 closure-appended
members exactly:

```
confirmed 59 · version-disagrees 26 · not-in-package-set 177 · no-version-recorded 14
```

## Three numbers that confirm the research rather than merely agreeing with it

1. **+278 out-edges** is exactly the declared-dependency count research R6
   measured independently, and **0** dangle. R6 predicted 265 resolving by
   name and 13 needing a target; the 13 became three distinct components.
2. **The three added components are `directory`, `ghc-heap` and `pretty`** —
   precisely the three distinct dangling targets R6 named, all GHC boot
   libraries, each emitted versionless with `compiler-supplied`.
3. **59, not 60.** The missing one is `hashable 1.4.7.0`, and the mechanism is
   now pinned down rather than inferred: the pinned revision has **no default
   `hashable` attribute at all**, only versioned aliases, so `set.get("hashable")`
   returns nothing and the member is reported `not-in-package-set`. That is
   also why `not-in-package-set` is 177 here against the sizing probe's 176
   absent-with-version. The alias gap R4 predicted costs exactly 1, and this is
   the row it costs.

## A wrong control, caught by its own log line

The first attempt produced the "before" document with `git stash` plus a
rebuild, and reported closure `pkg:hackage` as 106 **both** before and after
while the pass logged `confirmed=59`. Three numbers that cannot all be true.

The cause: `git stash` reverts the worktree to **HEAD**, and HEAD already
contained US1–US3 — the identity pass was committed in `e224e842`. So the
"before" was the feature *minus the Phase 6 edges*, not the feature's absence.
The comparison was measuring Phase 6 alone, which is also why it still showed
+278 edges and +3 components and looked plausible.

It was caught by grepping the baseline's own log for `identity pass` and
finding one. Hence the control stated at the top of this file: a baseline that
cannot be shown to lack the feature is not a baseline. The real comparison
uses a detached worktree at `001fa50`.
