# Data Model: 1280-nix-closure-edges

All state is in-process for one scan. Nothing persists; nothing is cached
between scans.

## Entities

### `DerivationInputs` — the field being read

Per derivation, the `.drv` paths whose outputs the build consumed. Already in
the JSON the reader parses; currently discarded.

| field | shape | note |
|---|---|---|
| `drvs` | map, `.drv` path → `{outputs, dynamicOutputs}` | the whole of what this feature reads. The value is ignored; the **key** is the edge. |
| `srcs` | list of store paths | **not read.** A `src` is a path, not a derivation, so it names no node in this graph. |

Entries naming a `.drv` absent from the closure's own map are dropped (FR-005):
`inputs.drvs` can reference a derivation outside the queried closure.

### `ProjectedGraph` — the result of the projection

| field | shape | note |
|---|---|---|
| `edges` | `BTreeSet<(DrvPath, DrvPath)>` | keyed on `.drv` paths, **not PURLs** — research R1. Sorted, so the emitted order is byte-stable without a later sort. |
| `root` | `Option<DrvPath>` | the derivation nothing consumes. `None` on a closure with a cycle through every node, in which case the anchor is unavailable and no edges are emitted. |
| `residual_orphans` | `BTreeSet<DrvPath>` | emitted components the projection could not reach from the root. Each gets the FR-011 reason. |

Keyed on `.drv` paths for the duration, converted to PURLs once, at the end,
against the final component set. That conversion is the only place a PURL
appears, and it happens after every pass that can rewrite one.

### `StopRule` — the FR-010 predicate, named because it is the feature

A derivation **stops** the walk when it declares a `pname` in either attribute
carrier. It does **not** stop when it declares none.

| derivation | declares `pname` | emitted | walk |
|---|---|---|---|
| a package, emitted | yes | yes | **stop**, emit the edge |
| a package, not emitted | yes | no | **stop**, emit nothing — this is the residual-orphan cause |
| a source fetch, wrapper, hook | no | no | **pass through** to its own inputs |

The middle row is the whole of FR-011's population, and the reason SC-002 is
352 → 5 rather than 352 → 0.

## Invariants

1. **The projection never consults a declared dependency attribute.** Roles
   come from `buildInputs` and friends; edges come from `inputs.drvs`. Asserted
   by SC-005 (the role census is unchanged) rather than assumed.
2. **No self-edge.** A derivation whose projection reaches its own component
   identity — possible when build variants merge — contributes nothing for that
   pair (FR-005).
3. **Edges are between component identities, not derivations.** Several
   derivations collapse onto one `(name, version)`; their edges collapse with
   them and are deduplicated by the `BTreeSet`.
4. **Termination is structural.** The post-order pass marks a derivation before
   descending and treats an already-marked node as a leaf, so a cycle
   terminates with a partial answer rather than recursing (FR-006). nix cannot
   produce one; the guard does not depend on that.
5. **The visited set is per-projection**, created at entry and dropped at
   return. No state survives a scan.

## State transitions

None. The projection is a pure function of the parsed closure and the emitted
component set:

```
(RawClosure, Set<DrvPath> emitted, Map<DrvPath, (name, version)>)
    -> ProjectedGraph
```

Stated as a signature because it is what makes the projection testable without
a scan, and because a function that also needed the component *vector* would
be a function that could rewrite a PURL.

## Sequencing

The one ordering constraint that is not obvious, and the only one whose
violation is silent:

```
closure::resolve            -> RawClosure + roles + read outcome
closure::emit::components   -> Vec<ResolvedComponent>, PURLs are pkg:generic/...
merge_closure_components    -> merged into the manifest-derived set
m1275 closure_identity      -> REWRITES 104 of 381 PURLs to pkg:hackage/...
  ^^^ edges must be built after this line
closure::graph::project     -> ProjectedGraph, keyed on .drv paths
  resolve to PURLs against the FINAL component set
  -> extend `relationships`
```

Research R1 has the measurement. An implementation that builds edges one line
earlier passes every fixture, because no fixture runs the identity pass.
