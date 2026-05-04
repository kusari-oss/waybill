# Contract — parity catalog row + pre-PR gate behavior

This contract captures what every spec author, reviewer, and milestone implementer can rely on after milestone 071 ships.

## C-1 — Catalog completeness

For every `mikebom:*` annotation key that any of mikebom's three emitters (`mikebom sbom scan` → CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 component-level output) emits on a `components[]` / `packages[]` / `@graph[Package]` element, the parity catalog at `mikebom-cli/src/parity/extractors/mod.rs` MUST contain a `ParityExtractor` row whose `label` matches the literal `mikebom:` string and whose three per-format extractor functions return the correct sets for that key.

**Enforcement**: integration test `mikebom-cli/tests/parity_completeness.rs` — fails CI on missing rows with a message naming the key and the source emit-site file.

## C-2 — Directionality invariants

Every catalog row carries one of four `Directionality` values, each with a hard invariant:

| Variant | Hard invariant the test enforces |
|---|---|
| `SymmetricEqual` | After canonicalization (per C-3), `cdx_set == spdx23_set == spdx3_set`. |
| `CdxSubsetOfSpdx` | `cdx_set ⊆ spdx23_set ∧ cdx_set ⊆ spdx3_set`. |
| `PresenceOnly` | `!cdx_set.is_empty() ∧ !spdx23_set.is_empty() ∧ !spdx3_set.is_empty()`. |
| `CdxOnly` | `!cdx_set.is_empty()`. SPDX sides are NOT asserted by this row (they are asserted by a different row named in the rationale comment). |

A row whose actual extracted sets violate its declared invariant is a test failure with a message naming the row, the violation type, and a hint to either fix emission or change the directionality.

## C-3 — Canonicalization rule (default + override)

Default canonicalization for value comparison:

1. Recursively sort all object keys lexicographically.
2. Recursively sort all JSON arrays lexicographically.
3. Normalize whitespace (use `serde_json::to_string`, not `to_string_pretty`).

Per-row override: a `ParityExtractor` row MAY set `order_sensitive: true` to disable step 2 for arrays in that row's value payload. Use `order_sensitive: true` only when the array's insertion order is semantic. None of the six known alpha.13 problem keys (`source-files`, `cpe-candidates`, `deps-dev-match`, `npm-role`, `sbom-tier`, `lifecycle-scope`) need the override; default sort is correct.

`order_sensitive: true` requires a one-line rationale comment naming WHY the order is semantic.

## C-4 — Hard-fail on uncatalogued keys

Per the 2026-05-04 Q1 clarification: when the pre-PR gate observes a `mikebom:*` annotation key emitted by any format on any component-level construct that has no `ParityExtractor` row in the catalog, the gate aborts non-zero with a message of the form:

```text
parity_completeness: uncatalogued mikebom:* key emitted

  key:           mikebom:<name>
  emitted-by:    [cdx, spdx2, spdx3]   (subset present)
  source-file:   mikebom-cli/src/generate/<path>:<line>
  catalog-file:  mikebom-cli/src/parity/extractors/mod.rs

To fix: add a ParityExtractor row in mod.rs with the appropriate Directionality
and per-format extractor fns, and add a rationale entry to
docs/reference/sbom-format-mapping.md if the directionality is not SymmetricEqual.
```

There is no "warn" mode and no environment-variable bypass. Adding a new annotation key without catalog row will fail every PR.

## C-5 — Documentation parity (rationale doc-sync)

For every catalog row whose `directional != SymmetricEqual`, the inline Rust line-comment immediately preceding (or following) the row MUST match this template:

```text
// <Directionality>: <one-line rationale why this asymmetry is correct>. Standards-native: <pointer-to-format-native-construct-or-other-catalog-row>.
```

A separate doc-sync test asserts that for every such row the inline rationale appears verbatim (modulo whitespace normalization) under the "Cross-format annotation parity catalog" section of `docs/reference/sbom-format-mapping.md`. Mismatches fail CI with a diff showing the missing entry and the file where to add it.

The published doc is the operator-visible artifact; the inline comment is the engineer-visible artifact; the doc-sync test ensures they don't drift.

## C-6 — Component identity for cross-format matching

Two annotation observations across formats are "on the same component" iff they share a canonical PURL string. Components without a PURL (rare; primarily binary-class catch-alls) are not parity-checked at the per-component level — the catalog row's extractor returns the union over all components in the document, and the `Directionality` invariant applies to that union.

Implementations MAY introduce per-component-keyed extraction in a follow-up if the union-level granularity proves too coarse. Initial scope: union semantics, matching what the existing 68 catalog rows already do.

## C-7 — Document-level annotations are explicitly out of scope

Per the 2026-05-04 Q3 clarification, this contract applies to component-level emission only. Document-level annotations (CDX `metadata.properties[]`, SPDX 2.3 top-level `annotations[]`, SPDX 3 document-level `Annotation` graph entries) are reported separately by the conformance harness and excluded from the SC-001 measurement and from C-1's discovery scope.

A future milestone may extend this contract to document-level keys with a separate set of catalog rows. Until then, the keys `mikebom:generation-context`, `mikebom:graph-completeness`, `mikebom:graph-completeness-reason`, and the `mikebom:trace-integrity-*` family are out of scope and the pre-PR gate test must explicitly NOT flag their document-level emit sites.

## C-8 — Stability commitment

Once this milestone ships:

- The four `Directionality` variants are stable; adding a new variant requires a constitution-style amendment.
- The `MikebomAnnotationCommentV1` envelope shape is stable; adding fields to the envelope requires a V2 schema and a migration plan.
- The pre-PR gate test name (`parity_completeness`) is stable; renaming requires updating `./scripts/pre-pr.sh` documentation if any.
- The published `docs/reference/sbom-format-mapping.md` "Cross-format annotation parity catalog" section format is stable; consumers are encouraged to parse it for filter rules in their own conformance harnesses.
