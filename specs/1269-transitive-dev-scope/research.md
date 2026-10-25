# Phase 0 Research: transitive propagation of non-runtime lifecycle scope

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25

Every number below was produced by reading the tree or the committed
goldens during this phase. Nothing here is extrapolated, and the two
findings that contradict the spec's own assumptions are called out as
such.

---

## R1 — Where the pass runs

**Decision**: `waybill-cli/src/cli/scan_cmd.rs`, immediately after the
**second-pass** milestone-191 reconciler (`:4022`) and **before** the
`--exclude-scope` filter (`:4034`), `apply_tier_filter` (`:4067`) and
format-builder dispatch.

**Rationale**: three separate properties have to hold at the insertion
point, and only that one position gives all three.

1. The component set must be final. `scan_fs/mod.rs:1283` — where the
   issue pointed — runs before `deduplicate()` and before both
   reconciler passes, so components it sees can still be folded away.
   The second reconciler pass at `scan_cmd.rs:4022` is the last mutation
   of the set before serialization.
2. It must run before `--exclude-scope`. That filter drops components
   whose `lifecycle_scope` is in the operator's exclude list. Today an
   operator asking to exclude `development` gets the declared members
   dropped and their transitive closure kept — the same defect, surfaced
   through a flag. Propagating first makes `--exclude-scope development`
   drop the closure, which is what the flag already promises.
3. It must run once, before all three builders, or the formats can
   disagree. Each builder reads the same `components` / `relationships`
   slices, so one pass upstream is what makes FR-009 and FR-010 hold by
   construction rather than by three parallel implementations.

**Alternatives rejected**: inside each format builder (three
implementations, three chances to disagree); at
`scan_fs/mod.rs:1283` (component set not final — the spec's own
starting assumption, which is why the issue's root-cause pointer is a
diagnosis and not a location).

---

## R2 — FR-002a is satisfied by placement, not by filtering

**Finding (measured)**: every anchoring call site is **inside a format
builder**, downstream of R1's insertion point.

| anchoring mechanism | call site |
|---|---|
| `anchor_resolve_components` (m868) | `generate/cyclonedx/builder.rs:885`, `generate/spdx/document.rs:684`, `generate/spdx/v3_document.rs:655` |
| `anchor_retained_mainmod_edges` (m860) | same three sites (wrapped) |
| m158 workspace-peer-linkage | `generate/graph_completeness/mod.rs:573` |
| "primary depends on every component nothing else depends on" | `build_dependencies`, in the CycloneDX builder |

**Decision**: no anchoring edge exists in the graph the pass reads, so
FR-002a needs no provenance filtering. It is enforced instead by a
**guard assertion**: the pass asserts its input carries no relationship
whose provenance source is one of the three synthetic anchoring sources
(`milestone-868-resolve-ownership`,
`milestone-860-retained-main-module`,
`milestone-158-workspace-peer-linkage`). A future change that moves
anchoring upstream then fails loudly instead of silently granting
immunity to the population the feature exists to mark.

**Alternatives rejected**:

- *A provenance denylist consulted at traversal time.* This is the
  `matches!(x, A|B|C)` failure mode — a synthetic edge class added later
  and not added to the list silently confers immunity, and the symptom
  is an under-marked SBOM, which looks exactly like a correct one.
- *A new `EdgeDerivation { Declared, Inferred }` field on
  `Relationship`.* Type-driven and robust, and unnecessary: placement
  already decides it. It would touch `waybill-common` and every edge
  construction site to buy what an assertion buys for free. Revisit only
  if anchoring ever moves upstream.

---

## R3 — The one inferred edge class the pass does see

**Finding**: milestone-218 cross-ecosystem bridges are created in
`scan_fs/mod.rs:1186` and `:1206`, upstream of R1, and are
discriminated by `provenance.data_type`:

```
"cross-ecosystem-bridge"             // EdgeEmission::Resolved
"cross-ecosystem-bridge-ambiguous"   // EdgeEmission::Ambiguous
```

They are matched by **name across ecosystems**, behind
`--experimental-cross-ecosystem-edges` /
`WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES` (off by default).

**Decision**: FR-002b excludes both `data_type` values from both
traversals. `provenance.source` is *not* the discriminator — it holds
the manifest path (`entry.source_path`), the same as a declared edge.

---

## R4 — Edge re-typing does not require moving anything

**Finding**: `apply_lifecycle_scope_to_edges`
(`scan_fs/mod.rs:1904`) rewrites only relationships still typed
`DependsOn` and skips any already-typed edge — its own comment calls
that "an invariant defense against future reader changes". A second
call after propagation therefore types exactly the newly-marked
targets' edges and is otherwise a no-op.

**Decision**: re-run it at the R1 site rather than move it. The
function becomes `pub(crate)`; its four unit tests stay where they are:

- `optional_dispatch_rewrites_depends_on_to_optional_depends_on` (`:2663`)
- `not_needed_fallthrough_rewrites_depends_on_to_test_depends_on` (`:2680`)
- `precedence_optional_wins_over_not_needed` (`:2700`)
- `runtime_target_with_not_needed_stays_depends_on` (`:2721`)

**This supersedes the spec's closing assumption**, which anticipated
moving the call and migrating those tests. Idempotence makes that
unnecessary — and a second call is strictly safer than a relocated one,
because the first call's behaviour is unchanged.

---

## R5 — Both populations coexist in the same documents (measured)

Computed from the committed CycloneDX goldens: runtime-only reachability
from `metadata.component`, not traversing into a component whose
`scope` is `excluded`.

| target | components | marked | root out-edges (of which marked) | runtime-reachable | all-reachable | I3 violations | unreachable |
|---|---|---|---|---|---|---|---|
| `python-flask` | 108 | 22 | 4 (0) | 15 | 86 | **51** | 23 |
| `rust-ripgrep` | 68 | 3 | 10 (0) | 57 | 62 | **2** | 7 |
| `haskell-language-server` | 399 | 19 | 6 (0) | 324 | 365 | **26** | 35 |
| `npm-express` | 45 | 16 | — | 29 | 45 | **0** | 0 |

Three things follow:

1. The reproduced I3 counts match the declarations exactly (51 / 2 / 26),
   so the declared counts are current and the check is reproducible
   outside the lane.
2. **Zero** of each root's out-edges point at a marked component. That
   is the measured form of "anchoring is not currently immunizing the
   dev closure", which makes FR-002a a regression guard rather than a
   fix for a present miss — worth stating, because the spec argues
   FR-002a from a hazard rather than from an observation.
3. `npm-express` is a non-vacuous pass: 16 marked, 29 runtime-reachable,
   and the marked set is exactly the complement of the runtime-reachable
   set. A real control cell, not `NotApplicable`.

---

## R6 — Standards-native audit for the derivation marker

Per Principle V and FR-017, performed against the vendored schemas in
`waybill-cli/tests/fixtures/schemas/`, quoting them rather than memory.

**CycloneDX 1.6** (`cyclonedx-1.6.json`)

- `component.scope`: `"enum": ["required", "optional", "excluded"]`. The
  `meta:enum` text for `excluded` reads *"Components that are excluded
  provide the ability to document component usage for test and other
  non-runtime purposes."* It carries the classification; it has no
  sibling expressing how the classification was reached.
- `evidence.identity[].field`:
  `"enum": ["group","name","version","purl","cpe","omniborId","swhid","swid","hash"]`
  — **`scope` is absent**, so identity evidence cannot describe a scope
  claim at all, and `identity[].confidence.score` ("*The confidence of
  the claim meeting the requirement*") grades identity, not scope.
- → no native carrier. `properties[]` is the Principle V finer-info
  carve-out.

**SPDX 2.3** (`spdx-2.3.json`)

- 45 `relationshipType` values, including `DEV_DEPENDENCY_OF`,
  `TEST_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`,
  `OPTIONAL_DEPENDENCY_OF`. They express the scope itself; a
  relationship object carries only `comment` alongside, with no
  provenance attribute. `Package` has no scope-provenance field.
- → no native carrier. The existing `MikebomAnnotationCommentV1`
  envelope in `annotations[]` is the vehicle.

**SPDX 3.0.1** (`spdx-3.0.1.json`)

- `LifecycleScopedRelationship` with `prop_LifecycleScopedRelationship_scope`
  and the `LifecycleScopeType` vocabulary carry the scope — this is what
  `generate/spdx/v3_relationships.rs` already emits as `rel["scope"]`.
- `RelationshipCompleteness` exists but grades whether a *relationship
  set* is complete, not how a scope was derived. Adopting it would
  assert something different and weaker.
- → no native carrier. `Annotation.statement`, same envelope.

**Conclusion**: KEEP-NO-NATIVE in all three formats. One new catalog
row, `Directionality::SymmetricEqual`.

---

## R7 — Catalog row number

**Finding**: the highest row in `docs/reference/sbom-format-mapping.md`
is **C198** (`waybill:source-package`); C195–C197 are
`waybill:go-mod-why`, `waybill:go-mod-why-reason`,
`waybill:version-absence`.

**Decision**: **C199**, `waybill:lifecycle-scope-derivation`. Its
extractor lands in the same change — a row without one fails
`parity::extractors::tests::every_catalog_row_has_an_extractor` and
`holistic_parity` by design.

---

## R8 — Expressing the lattice

**Decision**: an explicit `fn strength(&self) -> u8` on
`LifecycleScope` with an **exhaustive match**, ordered
`Runtime(4) > Optional(3) > Build(2) > Test(1) > Development(0)`.

**Rationale**: a new variant added later fails to compile, which is the
Principle IV guarantee. `#[derive(Ord)]` would bind the ordering to
declaration order — invisible, and silently wrong the first time a
variant is inserted in the middle. A `matches!`-style predicate is the
documented failure mode of this codebase (`#1246`, and the enum
allowlist that survived ~2 years in `#892`).

---

## R9 — Observability

**Decision**: one INFO line when the pass marked anything, carrying
per-scope counts and the size of the immunity set; silent when it marked
nothing. Matches the convention every comparable pass in `scan_fs`
follows, and the "marked nothing" case is most documents.

---

## R10 — Scale

**Finding**: the largest corpus target is `haskell-language-server` at
399 components. The pass is one BFS for immunity plus at most four
seeded BFS passes, each `O(V+E)`.

**Decision**: no performance criterion. A spec'd number here would be
noise against a 399-node graph, and this project's rule is that a
number in a success criterion must describe something measured.
