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

## Score — 12 of 13, plus one unpredicted consequence that changed the code

Scored against the diff of the committed golden (`origin/main`) and the
refreshed one, on **x86_64-linux**. Refresh run `42146116599`; the earlier run
`42142677053` was discarded because it was generated before the scope fix.

| # | row | predicted | observed | |
|---|---|---|---|---|
| P1 | C184 `edges` key | present, non-zero | **3,459** | ✅ |
| P2 | CDX `dependsOn` | +3,000 to +3,500 | 789 → 3,748, **+2,959** | ❌ **just outside the band** |
| P3 | orphan count | large fall | 352 → **10** | ✅ |
| P4 | components with an orphan reason | non-zero, right roles | **6**, all `build-tooling` / `artifact-input` | ✅ |
| P5 | the wire value | only the new code | only it | ✅ |
| P6 | reason on an `unreferenced` component | 0 | **0** | ✅ |
| P7 | C182 role census | unchanged | **unchanged** | ✅ |
| P8 | component count | unchanged | 406 → 406 | ✅ |
| P9 | `graph-completeness` verdict | still `partial` | still `partial` | ✅ |
| P10 | `BUILD_TOOL_OF` | 103 | **103** | ✅ |
| P11 | `DEPENDS_ON` | large increase | 752 → **3,934** | ✅ |
| P12 | goldens changed | moat only | moat only (+ `image-postgres16`, #1358) | ✅ |
| P13 | invariant I7 | passes | **passes** | ✅ |

**P2 is the miss**, and it is a real one rather than a rounding quibble: I
predicted +3,000 to +3,500 from the darwin figure and got +2,959. The band was
derived by scaling one system's result to another with a different derivation
count and a different attribute population — the same reasoning error this
milestone already made with a probe, committed in a narrower form. The lesson
is not "widen the band"; it is that a cross-system extrapolation does not earn
a numeric band at all, only a direction.

## The unpredicted consequence, which changed the implementation

Not a prediction row, and the most valuable thing the scoring produced.

The first refresh showed **8 components changing CycloneDX `scope`**: seven test
libraries lost `excluded` and `hspec-meta` gained it. Milestone 1269's
transitive dev-scope propagation reads the dependency graph, and `inputs.drvs`
records what the build *consumed* — including test dependencies, which nixpkgs'
Haskell builder passes through `buildInputs`. So seven test libraries stopped
asserting they are absent from the runtime artifact, which for a test library
was the correct claim.

Fixed by excluding these edges from that traversal
(`INFERRED_EDGE_DATA_TYPES` gains `nix-build-graph`), and **SC-005a was added**:
SC-005 required the C182 role census not to move, which it did not, and said
nothing about the other classifier that reads the graph. Verified after the
fix: **0 scope changes** on both systems.

### And the tension that fix created, declared rather than hidden

Excluding the edges from propagation made **invariant I3 fail** for this target:
`hspec-meta` is reachable only through build-graph edges, so propagation
correctly declines to mark it and I3 correctly notices it is unmarked.

**I3 cannot apply propagation's rule**, because it reads the emitted document
and neither CycloneDX `dependencies[]` nor the SPDX relationship arrays carry an
edge's provenance. Unlike I7's `is_explained`, there is no filter to mirror. The
cell is declared in `invariant-declarations.toml` with that reasoning, and
closing it needs either an edge-kind carrier in the output or a decision that
propagation should traverse these edges after all — each its own change.

## One more observed consequence, explained rather than filed away

`DEV_DEPENDENCY_OF` moves **36 → 144** while the dev-scoped component set is
unchanged at 10 and no scope changes. SPDX 2.3's milestone-052 convention types
an edge whose source is dev-scoped as `DEV_DEPENDENCY_OF`, and those same ten
components now have 109 outgoing closure edges where before they had few. The
existing rule applied uniformly to more edges — which is why FR-007a is now
explicit that it governs the C182 role and not lifecycle scope.
