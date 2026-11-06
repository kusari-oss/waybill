# Findings — 1280 (#1346)

What this milestone corrected about its own reasoning. Three of the five
changed the code or the spec; two changed only a number.

## F1 — Research R1 was wrong, and the codebase already knew

R1 was written as "the finding the plan is built around": edges built before
the milestone-1275 identity pass would dangle for 27% of the graph, because
that pass rewrites 104 of 381 component PURLs.

They would not. `closure_identity.rs:662` calls
`apply_renames(&summary.renames, relationships)` — endpoints move in the same
step as identities — and the comment above it states the reasoning R1 presented
as a discovery, citing #980. `haskell_packages/mod.rs:530` makes it a
requirement on callers: *"The caller MUST apply these; `apply_renames` does
it."*

Caught by noticing `reclassify` takes `&mut relationships` while wiring T025 —
which a finding *about* dangling relationships should have asked about first.
One grep at Phase 0 would have done it.

The placement survived on a weaker and sufficient ground: the identity→PURL
join needs the final component set. **T026 survived intact**, because it
asserts a property — every closure edge's endpoints resolve to a present
component — rather than pinning a call site. A test written to pin T025's
placement would have encoded the wrong reason and still passed.

## F2 — FR-011 could never fire, and the fix is a narrowing

Milestone 1218's classifier treats `waybill:closure-role` as a sufficient
explanation for any orphan, and invariant I7's `is_explained` mirrors it, so
every closure component was skipped before reaching any arm.

The convention is right for one role and wrong for three. Measured: of 25
unreachable closure components on darwin, 14 `build-tooling` and 6
`artifact-input` carry a role asserting that something depends on them while
nothing in the graph reaches them. C182 is half of that contradiction, not its
explanation. The remaining 5 are `unreferenced` and stay explained by their
role.

Both guards narrowed to `unreferenced` together. The frozen
`explained_pre_1156` copy deliberately **not** touched: it is a drift detector,
and changing it would defeat its purpose.

## F3 — The edges reclassified eight components' scope, and that needed a fix

The first refresh showed 8 components changing CycloneDX `scope`: seven test
libraries lost `excluded` and `hspec-meta` gained it. Milestone 1269's
transitive dev-scope propagation reads the dependency graph, and `inputs.drvs`
records what the build *consumed* — including test dependencies, which
nixpkgs' Haskell builder passes through `buildInputs`, the reason C182's own
docs say `artifact-input` does not mean "ships".

Fixed by excluding these edges from that traversal; **SC-005a added**, because
SC-005 required only the *role* census not to move and said nothing about the
other classifier that reads the same graph.

**And the fix created a tension that is declared rather than hidden.**
Invariant I3 then failed: `hspec-meta` is reachable only through build-graph
edges, so propagation rightly declines to mark it and I3 rightly notices it is
unmarked. Unlike I7, **there is no filter to mirror** — I3 reads the emitted
document, and no format carries an edge's provenance. Declared in
`invariant-declarations.toml` with that reasoning.

## F4 — Two probe figures were quoted as targets

The probe predicted 3,429 contributed edges and 16 residual orphans on darwin;
the implementation measures 3,450 and 25. The probe keyed identity on `pname`
alone while the reader falls back to the derivation's `name`.

It reproduced the **pre**-change state exactly, which is what made it useful
for choosing the projection among four. Quoting its **post**-change figure as a
target was the error, and SC-002 carried it into the spec before being
corrected.

## F5 — The P2 miss is the same error in a narrower form

The refresh prediction gave CDX edges as +3,000 to +3,500; the observed figure
is +2,959. The band was derived by scaling the darwin result to a system with a
different derivation count and a different attribute population.

The lesson is not "widen the band". A cross-system extrapolation does not earn
a numeric band at all — only a direction. Every other magnitude in that
prediction was either taken from the committed golden or expressed as a
direction, and all of those hit.

## F6 — A test helper reported two dangling edges the product does not have

T058 failed with two dangling endpoints under `--tier source-only`. A direct
scan of the same fixture with the same flags has none, and the real project has
none out of 3,901 edges.

The helper was wrong: `doc_root_ref` took `dependencies[0].ref` — whichever
entry sorts first, usually a component — instead of the subject. CycloneDX
names the subject by **bom-ref** in `dependencies[].ref` and by **PURL** in
`metadata.component.purl`, so the known-good set needs both.

Worth recording because the failure looked exactly like a product defect in a
feature that emits edges, and the thing that distinguished them was running the
same scan by hand. T026 had been passing with the same partly-wrong set.
