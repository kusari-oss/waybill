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

## Score

To be filled in after the refresh, against the diff rather than against the
summary — including any miss and what it corrected.
