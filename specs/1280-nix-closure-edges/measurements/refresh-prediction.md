# Refresh prediction — 1280 (#1346)

**Committed before the refresh was dispatched.** Every figure is from a scan by
the actual binary, not a probe — that distinction cost this milestone two wrong
numbers already (`measurements/README.md`, "Two probe figures the
implementation did not match").

The measured scan is **aarch64-darwin**. The goldens pin **x86_64-linux**, a
different closure — 1,749 derivations against 1,275 — so the linux figures
below are stated as *directions and ratios*, not as counts, except where they
come from the committed golden itself.

## Predictions

| # | row | before (committed golden, linux) | predicted after | confidence |
|---|---|---|---|---|
| P1 | C184 gains an `edges` key | absent | present, non-zero | high |
| P2 | CDX `dependsOn` total | 789 | **a large increase** — darwin went 788 → 3,907, so expect roughly +3,000 to +3,500 | medium on the magnitude, high on the direction |
| P3 | orphan count in `graph-completeness-reason` | 352 | **a large fall** — darwin went 359 → 25 | medium on the figure, high on the direction |
| P4 | components carrying `waybill:orphan-reason` | 0 | **non-zero**, all of them closure components with a role of `artifact-input`, `build-tooling` or `both` | high |
| P5 | the new reason's wire value | — | `reachable-only-through-unemitted-package` only | high |
| P6 | components carrying the reason with role `unreferenced` | — | **0** — that role still explains itself | high |
| P7 | C182 role census | 268 / 121 / 45 / 1,315 | **unchanged** | high — asserted by SC-005, and unchanged on darwin |
| P8 | component count | 406 | **unchanged** — this milestone adds no components | high |
| P9 | `graph-completeness` verdict | `partial` | **still `partial`** — the `transitive-edges-unresolvable: hackage` clause is independent | high |
| P10 | SPDX 2.3 `BUILD_TOOL_OF` | 103 | **unchanged at 103** (SC-010) | medium |
| P11 | SPDX 2.3 `DEPENDS_ON` | 752 | **a large increase**, tracking P2 | high |
| P12 | goldens that change | — | **`nix-closure-moat` only**, all three formats | high |
| P13 | invariant I7 for `nix-closure-moat` | `.` (pass) | **`.`** — the 338 pre-refresh offenders are either connected or carry the new reason | medium |

## Least certain, and why

**P2 and P3 on magnitude.** The darwin and linux closures differ in derivation
count and in which attributes they populate, and the projection's output scales
with both. The direction is not in doubt; the figure is, and quoting the darwin
number as the linux prediction is exactly the error this milestone already made
once with a probe.

**P10.** The `BUILD_TOOL_OF` edges come from m1277 and this milestone does not
touch `generate/spdx/relationships.rs` — verified, 0 changes. Medium only
because those edges are emitted per component classified absent from the
artifact, and if the new dependency edges changed any component's scope the
count would move with it. SC-005's unchanged role census is the reason to
expect it holds.

**P13.** The most interesting row. Pre-refresh, I7 fails for this target with
**338** `unreachable-and-unexplained` components — the narrowed rule applied to
a golden whose closure contributes no edges. The refresh should resolve all 338
by connecting most and explaining the rest. If I7 still fails afterwards, the
residue is a population neither the projection reaches nor the new reason
covers, and **that** would be the finding worth having.

## What a miss means

A miss on P7 means `inputs.drvs` leaked into classification, which FR-002
exists to prevent. A miss on P12 means the change is not confined to the
closure tier. A miss on P6 means the guard narrowing was wrong in the direction
that adds a second explanation, which is what milestone 1218's guard exists to
stop. Any of those three is a defect rather than a surprise.

## Score

To be filled in after the refresh, against the diff rather than the summary.
