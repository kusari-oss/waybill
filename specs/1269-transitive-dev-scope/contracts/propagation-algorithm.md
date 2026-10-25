# Contract: the propagation algorithm

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25

The pass is a pure function of the component slice, the relationship
slice and the root seed set. Same inputs, same output, no I/O.

```
propagate_non_runtime_scope(
    components:    &mut [ResolvedComponent],
    relationships: &[Relationship],
    seeds:         &HashSet<String>,       // per-ecosystem root seed set
) -> PropagationSummary
```

## Preconditions

| # | Precondition | If violated |
|---|---|---|
| P1 | The component set is final: post-dedup, post-enrichment, post-both m191 reconciler passes | A marked component may be folded away afterwards, and its marker with it |
| P2 | No relationship carries an anchoring provenance source (`milestone-868-resolve-ownership`, `milestone-860-retained-main-module`, `milestone-158-workspace-peer-linkage`) | **Assert and fail.** Anchoring edges would immunize the orphaned non-runtime closure — the population the feature exists to mark — while every reachable-population check still passed |
| P3 | `seeds` is the same set the emitters' completeness reporting uses | Immunity would be computed from a different root than the document declares |

## Algorithm

```
# Edge set shared by both phases (FR-002b)
E = { r in relationships
      : r.relationship_type is a dependency relation
      and r.provenance.data_type not in
          { "cross-ecosystem-bridge", "cross-ecosystem-bridge-ambiguous" } }

# Phase A — immunity (FR-002)
immune = BFS over E from seeds,
         never traversing INTO a component whose lifecycle_scope
         is Some(s) with s != Runtime

# Phase B — propagation (FR-001, FR-004, FR-005)
assigned = {}
for scope in [Optional, Build, Test, Development]:        # strongest first
    seeds_s = { c : c.lifecycle_scope == Some(scope) }
    for c in BFS over E from seeds_s:
        if c in immune:            continue               # FR-002
        if c.lifecycle_scope != None: continue            # FR-003
        if c not in assigned:      assigned[c] = scope    # FR-004 by order

# Apply
for (c, scope) in assigned:
    c.lifecycle_scope = Some(scope)
    c.extra_annotations["waybill:lifecycle-scope-derivation"] = "transitive"
```

Strongest-first iteration is what implements FR-004: a component
reached by both an `Optional` and a `Development` seed is claimed by
the `Optional` pass, which runs first, and the later pass skips it.
Equivalent to a max over inbound scopes, and cheaper to read.

Phase B's BFS does not stop at unscoped components — they are
transparent (FR-001, clarified 2026-10-25). It stops only at the
immunity set.

## Postconditions

| # | Postcondition | Requirement |
|---|---|---|
| Q1 | Every component in `immune` has the `lifecycle_scope` it entered with | FR-002 |
| Q2 | No component whose `lifecycle_scope` was `Some(_)` on entry changed | FR-003 |
| Q3 | Every component in `assigned` carries the C199 marker; no other component does | FR-007, FR-008 |
| Q4 | A component reachable over `E` from a declared non-runtime component and absent from `immune` has a non-runtime scope | FR-001, FR-005 |
| Q5 | `assigned` is empty ⇒ no component and no annotation changed | FR-013 |
| Q6 | The result is independent of the order of `components` and `relationships` | determinism |

## Termination

Each BFS marks a component visited once; `E` is finite. A cycle among
non-runtime components terminates and every member is assigned, because
visitation is per-component and not per-path (FR-011).

## Degenerate inputs

| Input | Behaviour |
|---|---|
| `seeds` empty for an ecosystem | That ecosystem's components cannot be immunized, so the pass MUST NOT assign into it (FR-012). Implementation: restrict Phase B's assignment to components whose ecosystem has at least one seed. |
| No component carries a non-runtime scope | `assigned` empty, no-op, no log |
| No relationships | `assigned` empty, no-op |
| Single component | no-op |

## Re-typing

The caller re-runs `apply_lifecycle_scope_to_edges` immediately after
the pass. It rewrites only still-`DependsOn` edges, so the call types
exactly the newly-marked targets' edges and is a no-op on everything
the first call already typed (research R4). This is what makes FR-010
hold in all three formats without per-format work.

## Summary record

```
PropagationSummary {
    immune: usize,
    assigned_by_scope: BTreeMap<LifecycleScope, usize>,
    seeded_from: usize,     // declared non-runtime components used as seeds
}
```

Emitted as one INFO line when `assigned_by_scope` is non-empty; silent
otherwise (research R9). Not part of any SBOM.
