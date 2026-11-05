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
