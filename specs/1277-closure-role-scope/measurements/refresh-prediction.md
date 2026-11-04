# What the corpus refresh should change — written before the run is dispatched

Branch `1277-closure-role-scope`, `public-corpus.yml` with
`regen_goldens=true`. Recorded before dispatch, so the diff is checked
against a committed prediction rather than one reconstructed from the diff.

Milestone 1276 is why this file exists. There I predicted a scope change
that did not happen, then explained it with a mechanism that itself measured
false. The split below between **measured** and **inferred** is the point:
a miss in the second group is a correction, a miss in the first means
something moved under the spec.

## Measured — stated flatly

**P1 — only `nix-closure-moat` moves.** It is the one corpus target that
emits closure roles at all: 362 of 387 components. The other 16 emit none
(`measurements/baseline.md`), so they must be **byte-identical**. Any
movement there is a finding, not a detail — the gating of C203 and of the
native markers on `artifact_presence` was chosen precisely so nothing
outside a `--nix-closure` scan can move.

**P2 — 97 components gain a CycloneDX `scope: "excluded"`**: 80
`build-tooling` plus 17 `unreferenced`. The before-census records 0 of them
carrying one today.

**P3 — the same 97 gain C203 `waybill:scope-classification-source`.** Value
`closure-role` for those the propagation did not also classify. The before-census
records 0 C203 values corpus-wide.

**P4 — 97 `BUILD_TOOL_OF` relationships appear in SPDX 2.3, and 97
`usesTool` / `LifecycleScopedRelationship` with `scope: "build"` in SPDX 3.**
The target has exactly one root (1 `DESCRIBES`), so one edge per member.
SPDX 2.3 goes 781 → 878 relationships.

**P5 — the 265 `artifact-input` and `both` members do not change**, and the
10 already-scoped closure members keep their scopes byte-identical. Those 10
are the `hspec`/`QuickCheck` family, scoped by the m1269 propagation through
the cabal graph; FR-002 forbids touching them.

**P6 — invariant I10's declaration becomes stale and must be deleted.**
It is declared at 97 for this target; the refresh should take it to 0. The
harness fails the lane with `I10 / nix-closure-moat: StaleDeclaration` and
names it, which is how m1276's two I9 declarations were retired.

**P7 — I4 (root-edge parity) stays green.** `BUILD_TOOL_OF` is absent from
the harness's `SPDX_INVERSE_DEPENDENCY_TYPES`, so the 97 new edges are not
dependency edges and the subject's direct set is unchanged in both formats.
Asserted locally by
`m1277_the_tool_edges_do_not_disturb_cross_format_dependency_parity`.

**P8 — the nixpkgs component does not move.** It carries `lifecycle=build`
and **no** closure role, so no row of `contracts/role-scope.md` applies to
it. It is the eleventh scoped component on this target and the easiest to
disturb by accident.

## Inferred — held loosely

**P9 — I9 stays green with no new declaration.** The exempt population on
this target is measured at zero: the only `pkg:hackage` component in a
tooling role is `happy@2.1.7`, which is not in the boot set. So no component
should carry `exempt-compiler-supplied`, and C203's value should be
`closure-role` or `closure-role+lifecycle-propagation` throughout — never
the exempt one. **If any component carries `exempt-compiler-supplied`, my
reading of `happy`'s alias-versus-null status is wrong and that is a finding
to run down, not a detail.**

**P10 — some of the 97 carry `closure-role+lifecycle-propagation` rather
than `closure-role`.** The count is unknown. It is the number of
non-shipping-role members the m1269 propagation independently scoped
non-runtime, and the before-census cannot show it because the native marker
did not exist. Expected small: 36 `DEV_DEPENDENCY_OF` edges exist on this
target, but most point at `artifact-input` members.

**P11 — graph completeness and the orphan-reason rows do not move.** Those
passes run on waybill's internal relationship list before emission, and this
feature adds no internal relationship — only a per-component field and
emission-time SPDX edges. So they should not see it at all. Held loosely
because it is a claim about what two passes read, verified by inspection
rather than by running them.

**P12 — the other two Haskell targets are byte-identical**, despite being
Haskell: `haskell-security-advisories` and `haskell-language-server` emit
zero closure roles, so they fall under P1. Listed separately because they
are the two most likely to be assumed affected.

## How the diff gets audited

Per **#1286**, by enumerating the files each target actually wrote — a
directory upload contributes committed bytes for a target that never ran,
and "51 files, 0 differing" once hid a stale target. Per **#1287**, by the
files written rather than any SHA the run reports, since a
`workflow_dispatch` run's `github.sha` is the workflow definition's commit
rather than the dispatched ref's. Per **#1345**, the summary's "Drifted"
heading lists *failed tests* in regen mode, not changed goldens, so it is
not the audit — in milestone 1276 it reported 2 while 3 goldens had moved.
