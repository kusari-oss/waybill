# Measurements — 1280 (the closure tier's dependency graph)

Every figure in `spec.md` traces to a run recorded here, on a named closure and
a named system. Both systems were measured, because they are not the same
closure: x86_64-linux has 1,749 derivations and aarch64-darwin 1,275, and they
differ in population rather than only in magnitude.

**x86_64-linux is the one the criteria are stated against**, because that is
what the committed corpus goldens pin and therefore what any assertion will be
checked against.

Probe: `input-graph-probe.py`. Input is the output of the same invocation the
reader uses, captured to a file and read from there —

```sh
nix derivation show -r --option allow-import-from-derivation false \
    <repo>#packages.<system>.default > closure.json
```

**Capture it; do not point a probe at a flake and let it evaluate.** On
2026-11-04 running a real `nix` against a committed fixture rewrote that
fixture's `flake.lock` and broke 11 tests in an unrelated file. When the
subject is a corpus clone, copy it first and verify the lock's checksum
afterwards.

## What the closure contributes today

| | x86_64-linux | aarch64-darwin |
|---|---|---|
| derivations | 1,749 | 1,275 |
| emitted closure components | 381 | 388 |
| **dependency edges contributed** | **0** | **0** |
| orphans reported by the document | 352 of 406 | 359 |

The zero is not a measurement artefact: `grep -rn Relationship` across
`waybill-cli/src/scan_fs/package_db/nix/closure/` returns nothing, and C205's
catalogue row says "it emits none today" in those words. Every closure
component is an orphan by construction.

## What `inputs.drvs` contains

| | x86_64-linux | aarch64-darwin |
|---|---|---|
| edges, derivation level | **10,342** | **8,230** |
| derivations explained | 1,748 of 1,749 | 1,274 of 1,275 |
| roots | 1 (`moat-0.1`) | 1 (`moat-0.1`) |
| reachable from that root | all | all |

**8,230 is sbomnix's `dependsOn` count exactly**, which is how we know sbomnix
emits this graph. The declared attributes waybill reads instead explain 533 of
1,275 derivations on darwin — the rest of the graph has been sitting in a field
the reader never opens.

## The projection question (FR-010)

Edges are between *components*, and most derivations are not emitted as one.
Three projections were measured before the question entered the spec, and they
differ by an order of magnitude:

| projection | x86_64-linux edges | closure orphans left |
|---|---|---|
| direct only — drop an edge whose target is not emitted | 3,088 | **381 of 381** |
| full transitive — walk through non-emitted derivations to the next emitted one | 16,187 | **0** |
| through non-package derivations only | not yet measured | — |

**Direct-only is worse than emitting nothing**, and that is the finding that
makes this a question rather than a detail. It emits 3,088 edges and leaves
every component orphaned, because the closure's root — `moat-0.1` — is itself
not an emitted component: nothing references it, so it has no C182 role and
applies no patches, and the existing rule drops it. A graph that looks
populated and is disconnected is harder to diagnose than one that is plainly
absent.

Full transitive connects everything, at 16,187 edges for 381 components — about
42 each, and *more* than sbomnix's 10,342, because projecting through
intermediate nodes turns one path into many edges. It also asserts some edges
that are arguably not dependencies at all: if `A` is built from a tarball that
was fetched using `B`, "A depends on B" is a stretch.

The third option is the one that needs measuring and has not been: project
through derivations that are not packages (fetches, wrappers, setup hooks) and
stop at ones that are. That is where the honest answer probably sits, and
`/speckit.clarify` is where it gets decided.

## Ownership, for the milestone after this one

Not needed for this spec; recorded because it was measured in the same pass and
bears on the Assumptions section.

| | x86_64-linux | aarch64-darwin |
|---|---|---|
| derivations with no `pname` in either carrier | 1,134 | 615 |
| of those, with exactly one named consumer | 522 | 556 |

So the fetch and auxiliary derivations can be attached to the package they were
fetched for, which is what makes emitting them a different proposition from
sbomnix's flat list.

## One conflation to avoid

An earlier draft of the probe printed "``unreferenced`` would fall from 1,315
to 1". That is two different measurements stapled together, and the spec
deliberately makes no such claim.

- C182's `unreferenced` means **no declared dependency attribute names this
  member** — a statement about what nixpkgs says about it.
- `inputs.drvs` answers **did anything consume this** — reachability.

Reading `inputs.drvs` reclassifies nothing. On x86_64-linux, 1,315 derivations
are named by no declared attribute and exactly 1 is consumed by nothing; those
are answers to different questions and the gap between them is not a defect.
SC-005 asserts the role census is *unchanged*, which is the same point stated
as a requirement.

## The projection decision (FR-010), as decided

All four options measured before the question was put, on both systems:

| projection | edges (linux) | orphans | edges (darwin) | orphans |
|---|---|---|---|---|
| (a) direct only | 3,088 | **381 of 381** | 3,091 | **388 of 388** |
| (a2) direct + root anchor | 3,105 | 7 | 3,107 | 20 |
| (b) transitive through anything unemitted | 16,187 | 0 | 21,237 | 0 |
| **(c) through non-package derivations only** ← chosen | **3,429** | **5** | **3,429** | **16** |

**(a) is disqualified by measurement, not by taste.** It emits 3,088 edges and
leaves every component orphaned, because the closure's root is not itself an
emitted component — so there is no anchor, and a graph that looks populated
and is disconnected is harder to diagnose than one plainly absent.

**(c) returning 3,429 on both systems looked like a harness bug.** It is not:
the distinct source counts differ (378 against 388) and so does the maximum
out-degree (41 against 47), so the walk is reading its input and the totals
genuinely coincide. Checked because identical numbers across cases that should
differ usually mean the probe rather than the system.

### Why the residual orphans are a feature

The 5 on linux and 16 on darwin have **one uniform cause**, measured per
component: each is consumed only by a derivation that declares a `pname` and is
not emitted. On linux they are `diffutils 3.12`, `ed 1.22.5`, `gnumake 4.4.1`,
`gnutar 1.35` and `pyproject-hooks 1.2.0`, consumed by `stdenv-linux`,
`patch 2.8` and `build 1.4.4`.

So the right answer is not to connect them — it is to say why they are not
connected, which FR-011 requires. Closing the remainder means emitting those
intermediate packages, and `stdenv-linux` has no good reason to be absent.
That is the milestone after next, and this measurement is its motivation.

## What else the clarification pass found

Two things that changed the spec and were not the question being asked:

- **The closure is not entirely absent from the relationship arrays.**
  Milestone 1277 emits **103** `BUILD_TOOL_OF` edges in SPDX 2.3, member → root.
  They point the wrong way to make anything reachable from the root, and
  CycloneDX carries none, so the "zero edges" claim is specifically about
  *dependency* edges. SC-010 now pins the 103 so this feature cannot disturb
  them.
- **The closure root is already a component, spelled by a different reader.**
  `moat-0.1` corresponds to `pkg:hackage/moat@0.1`, which the manifest reader
  emits and the document root already depends on. The closure tier would spell
  it `pkg:generic/moat@0.1`. FR-004 therefore anchors on the document root
  rather than on an identity match, because matching would cross a PURL-type
  boundary and a wrong match silently reparents the whole graph.

## First end-to-end measurement — aarch64-darwin, 2026-11-05

The implementation, not a probe. Same project copy, same stub-free real `nix`,
`--nix-closure --no-deps-dev --no-clearly-defined`.

| | sbomnix | pre-1280 | **post-1280** |
|---|---|---|---|
| components | 1,274 | 413 | 413 |
| **edges** | 8,230 | 788 | **3,907** |
| orphans reported | — | **359** | **25** |
| closure components with an incoming edge | — | 107 of 388 | **369 of 388** |
| dangling edge endpoints | — | 0 | **0** |
| C182 role census | — | 235/93/43/17 | **235/93/43/17** |
| licensed / hashed / no-PURL | 301 / 0 / 106 | 137 / 136 / 0 | unchanged |

- **`visits = 1275` for 1,275 derivations.** FR-012's linearity holds on the
  real closure, not only on fixtures: the memo is hit on every revisit.
- **`unanchored_root_edges = 0`.** The closure root resolved to a component,
  confirming research R5 — `moat-0.1` is already `pkg:hackage/moat@0.1` from
  the Haskell manifest reader, which the document root already depends on. The
  anchor works here with no emitter-side work.
- **SC-005 holds exactly**: the role census is byte-identical, so `inputs.drvs`
  did not leak into classification.
- **`graph-completeness` stays `partial`**, as the corrected SC-003 expects —
  the `transitive-edges-unresolvable: hackage` clause is independent of this
  work. The criterion was reworded during analysis precisely so it would not
  fail here.

### Two probe figures the implementation did not match

| | probe predicted | implementation |
|---|---|---|
| contributed edges (darwin) | 3,429 | **3,450** |
| residual orphans (darwin) | 16 | **25** |

Same cause for both: the probe's identity rule was `pname` alone, while waybill
falls back to the derivation's `name`, so it emits a slightly larger set with a
slightly different orphan residue. The probe reproduced the **pre**-change
state exactly, which is what made it useful for choosing the projection — but
it is not the implementation, and quoting its post-change figure as a target
was a mistake. **SC-002's linux figure of 5 needs re-measuring against the
binary before it is scored**, not before.
