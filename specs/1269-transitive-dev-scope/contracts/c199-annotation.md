# Contract: C199 `waybill:lifecycle-scope-derivation`

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25

## Semantic

*"This component's non-runtime lifecycle scope was inferred by waybill
from the dependency graph, not read from a declaration."*

Emitted **iff** the propagation pass assigned the component's
`lifecycle_scope`. Absent everywhere else: on a reader-declared
non-runtime component, on a runtime component, and on an unscoped one.

A consumer reads the two fields together:

| `lifecycle-scope` | C199 | means |
|---|---|---|
| non-runtime | `transitive` | waybill inferred it from the graph |
| non-runtime | absent | a reader read it from a manifest or lockfile |
| runtime or absent | absent | not in this marker's domain |

## Vocabulary

Closed enum. v1 recognises exactly one value:

- `transitive`

Future values may extend the enum without a new C-row (the C160 / C197
precedent). A consumer encountering an unrecognised value MUST treat
the scope as inferred rather than declared, since the marker's presence
is the claim and its value is the refinement.

## Wire shape

**CycloneDX 1.6** — per-component `properties[]`:

```json
{ "name": "waybill:lifecycle-scope-derivation", "value": "transitive" }
```

**SPDX 2.3** — per-Package `annotations[].comment`, the existing
`MikebomAnnotationCommentV1` envelope:

```json
{
  "schema": "waybill-annotation/v1",
  "field": "waybill:lifecycle-scope-derivation",
  "value": "transitive"
}
```

**SPDX 3.0.1** — per-Package `Annotation.statement`, same envelope.

## Parity

`Directionality::SymmetricEqual`, `order_sensitive: false`. The
extractor registers in `waybill-cli/src/parity/extractors/mod.rs`
alongside the catalog row in the same change; a row without an
extractor fails `every_catalog_row_has_an_extractor` and
`holistic_parity`.

## Standards-native audit (Principle V)

Performed against `waybill-cli/tests/fixtures/schemas/`; quotations and
method in research R6. Summary:

| format | closest native construct | why it does not carry this |
|---|---|---|
| CDX 1.6 | `evidence.identity[].field` + `confidence.score` | `field`'s enum is `group,name,version,purl,cpe,omniborId,swhid,swid,hash` — `scope` is not among them, so identity evidence cannot describe a scope claim |
| CDX 1.6 | `component.scope` | carries the classification (`required`/`optional`/`excluded`); no sibling expresses how it was reached |
| SPDX 2.3 | `DEV_/TEST_/BUILD_/OPTIONAL_DEPENDENCY_OF` | carry the scope; a relationship has only `comment` beside it, no provenance attribute |
| SPDX 3.0.1 | `LifecycleScopedRelationship.scope` | carries the scope |
| SPDX 3.0.1 | `RelationshipCompleteness` | grades whether a relationship set is complete, not how a scope was derived — adopting it would assert something else |

**Verdict**: KEEP-NO-NATIVE in all three formats. The annotation is the
Principle V finer-information carve-out, and Principle X requires the
distinction be on the wire at all.
