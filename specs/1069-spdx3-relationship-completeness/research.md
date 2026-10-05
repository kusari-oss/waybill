# Research: SPDX 3 native dependency completeness

`G/` = `waybill-cli/src/generate/`. External behaviour is measured; see `measurements/README.md`.

## R1 — The validator accepts every shape this feature emits

**Measured** (`measurements/validator.txt`): the pinned `spdx3-validate` 0.0.5, on a real cobra document, passes:
- grouped `to` lists;
- each of `complete`, `incomplete` and `noAssertion`;
- `dependsOn → NoAssertionElement`.

It rejects `completeness: bogus`, so the enum is really checked.

**Decision**: no validator change, and no new dev tool. FR-006 is satisfied by the existing conformance gate (milestone 078) plus a probe re-run at implementation.

## R2 — `NoAssertionElement` spelling

**Measured** (SPDX 3.0.1 JSON-LD context): `to` is `@type: @vocab`, and the context maps the term `NoAssertionElement` to `https://spdx.org/rdf/3.0.1/terms/Core/NoAssertionElement`.

**Decision**: emit the bare term `NoAssertionElement`. It expands to the SPDX individual, matching how the document already uses vocabulary terms.

## R3 — What an absent `completeness` means

**Found** (model text): the model defines the three values and **no default**.

**Decision**: absence claims nothing, which is RDF's open-world reading. That is all FR-001d and FR-001c rely on. The spec's assumption is reworded to say it is inferred, not stated.

## R4 — The predicate, shared with CycloneDX (FR-001a, FR-003)

**Found**:
- `G/cyclonedx/compositions.rs::build_compositions` decides the claims inline from four inputs:
  - `complete_ecosystems`;
  - the component's `purl.ecosystem()`;
  - the BFS `reachable_set` from `compute_graph_completeness`;
  - `degraded_ecosystems`, derived in `G/cyclonedx/builder.rs` (~975–1003) from the reason codes `TransitiveEdgesUnresolvable` and `GoTransitiveCoverageDegraded`.
- The claim is all-or-nothing per ecosystem.
- The root gets a separate `complete` record when `target_aggregate(integrity) == "incomplete_first_party_only"` and components are non-empty.

**Decision**: extract `pub(crate) fn dependency_claims(...) -> DependencyClaims { complete, unknown, root_complete }` into `G/cyclonedx/compositions.rs` (or a sibling module), together with the degraded-ecosystem derivation.
- `build_compositions` calls it, and its bytes stay identical (FR-005, pinned by goldens).
- The SPDX 3 emitter calls the same function. The two formats therefore cannot disagree about which component is in which set.

**Alternative rejected**: re-deriving the predicate in the SPDX 3 emitter. Two copies of an all-or-nothing rule drift, and the failure is a silent cross-format contradiction, which is the #871 bug class.

## R5 — Sequencing (the issue's deferral reason)

**Found**: in `G/spdx/v3_document.rs`:
- `compute_graph_completeness` (~864) needs only `scan.components` and `m194_classifier_relationships`, which are built before relationships are emitted (~665);
- dependency relationships are built at ~673.

**Decision**: compute the completeness result before the relationships are built, and use it in both places. The issue's "reorder the emitter" is a move of one call, with no change to its inputs.

## R6 — Where grouping happens

**Found**: SPDX 3 `dependsOn` relationships come from four producers:
- `build_dependency_relationships` (`G/spdx/v3_relationships.rs:62`);
- the issue-#236 synthetic-root fallback (`G/spdx/v3_document.rs` ~690–790);
- the #1009 supplement anchor (~800–815);
- the root fallback in `v3_relationships.rs` (~129–210).

Each builds one relationship per edge with a content-hash IRI over `from|type|to`. None carries a per-edge `comment` or other per-edge data, so grouping loses nothing.

**Decision**: one post-pass, `group_dependency_relationships(all_relationships, claims)`, runs over the final relationship list before sorting:
- **Grouping key:** it groups `dependsOn` relationships by `(from, type, scope)`, where `type` is `Relationship` or `LifecycleScopedRelationship` and `scope` is present only on the latter.
- **Targets:** the union of targets, sorted and deduplicated.
- **IRI:** `rel-` plus the hash of `from|dependsOn|scope|<sorted targets joined>`. Deterministic, and distinct from any single-edge IRI.
- **Completeness:** from R4, on the `from` component:
  - `complete` if `from` ∈ `complete`, or `from` is the root and `root_complete`;
  - `incomplete` if `from` ∈ `unknown`;
  - otherwise absent.
- **Unknown leaves:** every component in `unknown` with no outgoing `dependsOn` gets `Relationship{from, dependsOn, to: [NoAssertionElement], completeness: noAssertion}`, with a content-hash IRI over `from|dependsOn|NoAssertionElement` (FR-001b).

One pass over the final list covers all four producers. Fixing each producer would leave the next producer to repeat the mistake.

**Alternative rejected**: grouping inside each producer. Four sites, and a fifth producer added later would emit per-edge relationships again without any test noticing.

## R7 — Readers of SPDX 3 relationships

**Found**:
- The edge-parity extractors (`parity/extractors/spdx3.rs` ~300–380) already iterate `to` as an array, and map each target through `purl_by_iri`. `NoAssertionElement` has no PURL, so it is skipped and creates no phantom edge.
- Corpus and integration tests that read `e["to"]` iterate the array.

**Decision**: no extractor change is expected. The gate confirms it. Any test that indexes `to[0]` or counts relationships as edges is updated to count targets.

## R8 — Byte-identity and goldens

- **CycloneDX and SPDX 2.3:** byte-identical by construction. R4's extraction is pinned by the CycloneDX goldens.
- **SPDX 3 goldens:** change for every scan with dependency relationships, both in-repo and corpus (all 17 corpus targets).
- **Procedure:**
  - in-repo goldens are regenerated with the repository's golden-update path;
  - corpus goldens use two regen runs, `diff -r`, and `xtask corpus-diff`.
- **The only acceptable diff:**
  - dependency relationships regrouped;
  - `completeness` added;
  - `NoAssertionElement` relationships added for unknown leaves;
  - the document IRI / namespace hash, if it covers element ids.
- **Annotations:** `waybill:graph-completeness*` and `waybill:orphan-reason` values are unchanged (FR-004, SC-005).
