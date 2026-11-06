# Phase 0 research: components for the closure members waybill does not name

Every finding below was read out of the tree or measured against the pinned
`nix-closure-moat` closure. Three of them contradicted the spec as clarified,
and the spec was corrected before this plan was written rather than after.

## R1 — the emitter and the graph deliberately share one predicate

`emit::emitted_drv_keys` carries this comment:

> Derived from `emits_as_component` rather than from a second copy of the
> rule: the projection must emit an edge to exactly the nodes the emitter
> emits, and two predicates that have to agree is the drift this project
> keeps paying for.

This feature breaks that identity on purpose. After FR-001 the emitted set is
every derivation; after FR-013 the edge-endpoint set is only the `pname`-bearing
members plus `source` and `package`. Emission becomes a **superset** of edge
participation.

**Decision**: keep one classification and derive both sets from it, rather than
writing a second predicate. `emits_as_component` becomes "is a closure member"
and a new `is_edge_endpoint` is defined *in terms of the role*, so the two can
be asserted to stand in a subset relation by a test. The comment's warning is
about two independently-maintained rules, not about having two sets.

**Alternative rejected**: leaving `emitted_drv_keys` as the edge set and adding
a parallel emitter path. That is precisely the second copy the comment warns
about.

## R2 — the projection already takes its endpoint set as an argument

`graph::project(closure, emitted)` takes the endpoint set as a parameter, and
`graph::stops(drv)` is `declared_pname().is_some()`. So the projection needs no
structural change: it needs `stops` widened to the new roles and the same set
passed as `emitted`.

**Decision**: widen `stops` to `pname-bearing | source | package`, and pass the
matching set. One rule, consulted twice, as today.

## R3 — FR-013 as clarified was self-contradictory, and measurement settled it

The clarification claimed both that sources and packages become edge endpoints
**and** that the measured graph shape is preserved. Those cannot both hold:
stopping earlier replaces a wide transitive fan-out with a direct edge.

Measured on the pinned closure with a derivation-level proxy of the projection:

| stopping rule | edges |
| --- | --- |
| `pname` only (today) | 12,013 |
| `pname` \| `source` \| `package` | **7,755** |

−35%. The proxy counts derivation-level edges and so does not reproduce the
component-level figure in the golden (3,459); what transfers is the direction
and the order of magnitude, not the number.

**Decision**: the graph changes, and the spec now says so. SC-011 no longer
asserts the count is unchanged; it requires the new count to be measured and
every edge to resolve. FR-018 requires the document to carry the figure,
because a graph that silently changes size is indistinguishable from one that
broke.

## R4 — the C182 census counts derivations, not components

The committed golden for the pinned target records:

```json
{"derivations": 1749, "components-emitted": 381, "edges": 3459,
 "roles": {"artifact-input": 268, "both": 45,
           "build-tooling": 121, "unreferenced": 1315}}
```

The role counts total **1,749** — the whole closure. `classify()` assigns a
role by walking attribute fields, which reference `pname`-less derivations too,
so the census already covers the population this feature emits. 1,315
`unreferenced` is mostly them.

This falsified the premise of the clarification question about the census,
which assumed it described only named members.

**Decision**: leave the census untouched. It cannot change by emitting more
components, because it does not count components. FR-012 was rewritten to say
so, and SC-013 now exists to catch a *reclassification* side effect rather than
to require work.

**Also corrected**: the baseline figures carried into this work from earlier
sessions were wrong. The golden says 381 components and 3,459 edges; "3,748
edges" had been quoted repeatedly from memory.

## R5 — the orphan-reason mechanism is an enum plus an annotation allowlist

`generate/orphan_reason.rs` holds a reason enum (milestone 1280 added
`ReachableOnlyThroughUnemittedPackage`) and an `already_explained` chain that
treats certain annotations as sufficient explanation — among them
`waybill:closure-role == "unreferenced"`, narrowed to exactly that value by
milestone 1280.

**Decision**: add one reason variant for the FR-014 population and let the new
four-role annotation explain it, rather than widening the `unreferenced` arm.
Widening it would re-create the defect milestone 1280 fixed: a role treated as
a blanket explanation, so a genuine orphan hides behind it.

**Scale note**: 513 components will carry this reason on the pinned closure.
That is a population, not an exception, so the reason string has to read as a
statement of design rather than an apology for a gap.

## R6 — identifier construction

`waybill_common::types::purl::encode_purl_segment` normalises `+` only; it is
not a general percent-encoder. Measured: every one of the 1,145 names matches
`^[A-Za-z0-9._+-]+$`, so no name requires escaping beyond what that helper
already does, and none is empty.

**Decision**: build the name segment as `<name>-<hash-prefix>`, where the hash
is the content hash where one exists and the derivation hash otherwise
(FR-003 / FR-004). Keep the unmodified name on the component's `name` field
(FR-016).

**Open, for the tasks rather than here**: the hash-prefix length. It must be
long enough that truncation introduces no collision the full hash does not
have. The probe already computes identity over full hashes; the task that
chooses a prefix length must re-run it over the prefix and assert zero
collisions rather than assume a conventional length is safe.

## R7 — scale

The feature roughly quadruples the emitted component count for this tier: 381
today, ~1,479 after (381 + 1,098). All of it is in-process transformation of a
closure already parsed into memory — no new I/O, no new subprocess, no network.

**Decision**: no performance requirement is specified. The work is proportional
to a closure already held in memory, and the one superlinear risk — the naive
projection milestone 1280 avoided — is already solved by its memoised
post-order pass, which this feature does not change.
