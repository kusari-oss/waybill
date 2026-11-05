# Refresh prediction — 1279 (#1346)

**Committed before the refresh was dispatched.** The point of writing it down
first is that a prediction scored after the fact is not a prediction, and the
misses are the part worth having: milestone 1278's refresh scored 8 of 14, and
the two that missed in the *wrong direction* each exposed something the
measurement had got wrong rather than something nix had.

Every figure below is computed from the **same x86_64-linux closure the
goldens pin** (`nix-closure-moat` at its pin, nixpkgs `a799d3e3`, 1,749
derivations, format version 4), joined against the committed golden's own
values. Nothing here is extrapolated from the darwin closure — that is the
mistake this milestone's spec drafting made twice, and it is wrong in
*membership*, not only magnitude.

## The predictions

| # | row | before (committed golden) | predicted after | confidence |
|---|---|---|---|---|
| P1 | C184 `roles` | 270 / 118 / 42 / 1,319 | **268 / 121 / 45 / 1,315** | high — measured directly |
| P2 | C184 `components-emitted` | 379 | **381** | high |
| P3 | total CDX components | 404 | **406** | high |
| P4 | closure-tagged component roles | 239 / 90 / 38 / 12 | **237 / 92 / 41 / 11** | high |
| P5 | C204 `fields-unconsulted` | 6 fields | **2** — `defaultNativeBuildInputs`, `extraNativeBuildInputs` | high |
| P6 | C204 `declarations-unconsulted` | 230 | **211** | high |
| P7 | C204 `derivations-affected` | 23 | **16** | high |
| P8 | `numactl` role | `unreferenced` | **`artifact-input`** | high |
| P9 | `numactl` CDX `scope` | `excluded` | **absent** (SC-013) | high |
| P10 | `numactl` C203 | `closure-role` | **absent** — m1277 asserts nothing for `artifact-input` | medium |
| P11 | components carrying C203 | 102 | **103** | medium |
| P12 | components with `scope: "excluded"` | 113 | **114** | medium |
| P13 | newly emitted components | — | **2**: `patchelf 0.15.2`, `setup-debug-info-dirs-hook 26.05pre-git`, both `build-tooling` | high |
| P14 | components carrying C205 | 0 (the row is new) | **9** | medium |
| P15 | C202 `scope-exemptions` | `{"compiler-supplied":22}` | **unchanged** | medium |
| P16 | `waybill:graph-completeness` | `partial` | **unchanged** | high |
| P17 | goldens that change | — | **`nix-closure-moat` only**, all three formats | high |

## Which are least certain, and why

**P10, P11, P12 — the m1277 interaction.** These depend on a rule in another
module (`resolve/closure_role_scope.rs`) behaving as read rather than as
measured. The code says `"artifact-input" | "both" => None`, so `numactl`
should lose both its scope and its C203; T020 asserts that file needs no edit
on exactly that basis. But the arithmetic (+2 from the newly emitted
`build-tooling` pair, −1 from `numactl`) assumes no other rule also assigns a
scope to the two new components, and `scope: "excluded"` already stands at 113
against C203's 102 — eleven components get it from somewhere else. If P12
misses, that gap is the first place to look.

**P14 — the marker count.** Medium not because the measurement is weak but
because it is the figure this milestone has already got wrong once: the
contract was drafted saying **3**, which is the count of `unreferenced`
members named *only* by an unclassified attribute — a correct answer to a
different question. The chain is 211 declarations → 16 members → 13 emitted →
9 identities, and each step drops something for its own reason. If the refresh
reports 3, 13 or 16, the shape of the miss will say which step the emitter
actually implements.

**P15.** The 22 compiler-supplied exemptions are a property of the pinned
nixpkgs revision's GHC, which this milestone does not touch. Medium only
because `numactl` leaving the classified population is the kind of change that
can perturb a count computed over it.

## What a miss means

A miss on P1–P7 means the probes and the shipped reader disagree, and the
probes agree with the committed golden on the **before** state exactly — same
6 fields, same 230 declarations, same 23 derivations, same 270/118/42/1,319 —
so a disagreement on **after** is a defect in the implementation, not in the
measurement.

A miss on P13 or P14 is about the emission rule rather than the classifier,
which is the seam this milestone touches least and understands least: the
reach limitation (an `unreferenced` member applying no patch is not emitted at
all) was discovered during implementation, not during planning.

A miss on P17 is the serious one. Any movement outside `nix-closure-moat` means
the widening leaked past the two constants in `closure/classify.rs`, and SC-008
fails regardless of how well everything else scored.

## Score — 16 of 17

Scored against the diff, not the run summary. Refresh run `42094294917`
(regen) on this branch; control run `42095328076` (compare) on `main`.

| # | row | predicted | observed | |
|---|---|---|---|---|
| P1 | C184 `roles` | 268 / 121 / 45 / 1,315 | 268 / 121 / 45 / 1,315 | ✅ |
| P2 | `components-emitted` | 381 | 381 | ✅ |
| P3 | total components | 406 | 406 | ✅ |
| P4 | component roles | 237 / 92 / 41 / 11 | 237 / 92 / 41 / 11 | ✅ |
| P5 | C204 fields | 2, the two stdenv attributes | exactly those two | ✅ |
| P6 | declarations unread | 211 | 211 | ✅ |
| P7 | derivations affected | 16 | 16 | ✅ |
| P8 | `numactl` role | `artifact-input` | `artifact-input` | ✅ |
| P9 | `numactl` scope | absent | absent | ✅ |
| P10 | `numactl` C203 | absent | absent | ✅ |
| P11 | components with C203 | 103 | 103 | ✅ |
| P12 | `scope: "excluded"` | 114 | 114 | ✅ |
| P13 | newly emitted | `patchelf`, `setup-debug-info-dirs-hook`, both `build-tooling` | both, both `build-tooling` | ✅ |
| P14 | components with C205 | 9 | 9, the nine named | ✅ |
| P15 | C202 exemptions | unchanged | `{"compiler-supplied":22}` | ✅ |
| P16 | `graph-completeness` | unchanged | `partial` | ✅ |
| P17 | goldens that change | `nix-closure-moat` only | **also `image-postgres16`** | ❌ |

All three medium-confidence rows about the m1277 interaction (P10–P12) hit
exactly, which settles T020's claim with evidence rather than by reading the
code: `numactl` lost its scope **and** its C203 without that file being
touched. P14 hit at 9 — the figure this milestone had already got wrong once
— and the nine components are the nine `remainder-census.py` named.

`patchelf` arrives carrying **both** a `build-tooling` role and the C205
marker, which is the gating row the fixture tests as T044. It is worth noting
that it appears in the real closure too: that row exists because every other
marker test uses a member whose only reference is unclassified, so a rule that
emitted the marker only for `unreferenced` members would have passed them all.

### One consequence not predicted

`waybill:graph-completeness-reason` moved `350 component(s) not reachable from
root` → `352`. That is exactly the two newly emitted components, both of which
are orphans. Coherent, and recorded as unpredicted rather than folded in as
though it had been foreseen — the prediction listed the doc-scope rows it
expected to move and this was not among them.

### The miss — P17, and it is not this milestone's

`image-postgres16` changed in all three formats. The diff is narrow enough to
read directly: 263 components before and after, **none added or removed**, and
the only difference is `waybill:layer-digest` on 13 components, all moving
`sha256:3ce18cb3…` → `sha256:38b0c486…` together. Nothing in this milestone
touches OCI layer attribution.

**Established by control, not by inference.** A compare-mode run of the
corpus on `main` — no code from this branch — fails on exactly one target:

```
test corpus_image_postgres16 ... FAILED
test result: FAILED. 131 passed; 1 failed
```

with the same two digests. The drift is pre-existing on `main`, so P17's miss
is environmental and the goldens for that target are **not** refreshed here;
bundling an unrelated pre-existing drift into this PR would make neither
change reviewable. Filed separately.

It is a real defect and a surprising one: the target is pinned by **image
digest**, so its layer digests ought to be immutable. The likely mechanism is
the runner's Docker re-serializing layers when the image is saved, which would
make a digest-pinned corpus target reproducible only against a fixed Docker
version — the same class as the ubuntu-image `/bin/ls` finding of 2026-11-05.

### Enumeration, by file rather than by name

51 regenerated documents compared against the committed tree: **exactly 3
differ**, all `image-postgres16`. Counting files rather than target names is
deliberate — a directory upload contributes bytes for targets that never
wrote, which is how #1286 shipped with one target's goldens stale.
