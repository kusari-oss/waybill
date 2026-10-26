# Contract: `waybill:lifecycle-scope-derivation = transitive` (catalog row **C62**)

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25
**Supersedes**: this file's first version, which specified a *new* row
C199 for a *new* key. See "Why not a new row" below — that was wrong,
and the mistake is recorded rather than quietly corrected because the
way it was caught is the reusable part.

## Semantic

*"This component's non-runtime lifecycle scope was inferred by waybill
from the dependency graph, not read from a declaration."*

Emitted **iff** the propagation pass assigned the component's
`lifecycle_scope`. Absent everywhere else: on a reader-declared
non-runtime component, on a runtime component, and on an unscoped one.

A consumer reads the two fields together:

| `waybill:lifecycle-scope` | derivation | means |
|---|---|---|
| non-runtime | `transitive` | waybill inferred it from the graph (this feature) |
| `test` | `test-only-closure` / `go-mod-why` | waybill inferred it from the Go graph (pre-existing) |
| non-runtime | absent | a reader read it from a manifest or lockfile |
| runtime or absent | absent | not in this marker's domain |

## Vocabulary

**Open** enum, pre-existing. This feature adds exactly one value:

- `transitive`

The row already carried `test-only-closure` and `go-mod-why`, both of
them also "waybill worked this out from the graph". A consumer meeting
an unrecognised value MUST treat the scope as inferred rather than
declared, since the marker's presence is the claim and its value is the
refinement.

## Wire shape

Unchanged from the row's existing shape, in all three formats.

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

Already registered: `ParityExtractor { row_id: "C62", … directional:
Directionality::SymmetricEqual, order_sensitive: false }` with
`c62_cdx` / `c62_spdx23` / `c62_spdx3`. **This feature adds no
extractor and no catalog row** — the value rides the existing three.

`SymmetricEqual` is load-bearing for the new value for the same reason
it was for the old ones: a format that dropped the marker would read as
"a reader declared this scope", which is a false statement rather than
a missing one.

## At most one value per component

Both pre-existing writers (`scan_fs/package_db/mod.rs:785` for
`test-only-closure`, `:1580` for `go-mod-why`) set the scope **and**
the derivation together, and the 1269 pass assigns only where
`lifecycle_scope` is `None`. So the pass can never meet a component
that already carries a derivation, and no precedence rule between
values is needed.

That is an argument, not a guarantee, so a test asserts it:
`a_component_with_an_existing_derivation_is_not_touched` in
`waybill-cli/src/resolve/scope_propagation.rs`.

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

**Verdict**: KEEP-NO-NATIVE in all three formats — the same verdict
C62's own audit reached, re-run rather than inherited. The annotation is
the Principle V finer-information carve-out, and Principle X requires
the distinction be on the wire at all.

## Why not a new row

The first version of this contract specified a new row **C199** for the
key `waybill:lifecycle-scope-derivation`. The key was already live:
**C62** has owned it since the test-closure propagation fix, with an
open enum of exactly this semantic.

The research that produced the C199 plan checked which row *number* was
free — C198 `waybill:source-package` was the highest — and did not
check whether the *key* was taken. Those are different questions and
only one of them was asked.

It was caught by `parity::extractors::tests::extractors_have_unique_labels`,
which failed with `duplicate label(s) in EXTRACTORS:
[("waybill:lifecycle-scope-derivation", ["C62", "C199"])]` on the first
full pre-PR gate run after the extractors landed. Both rows'
extractors, both catalog entries and the `every_catalog_row_has_an_extractor`
check had passed — each is consistent with itself, and the collision is
only visible across rows.

The generalisation is clean rather than a workaround: C62's two existing
values are both graph-derived, its absence already meant "a reader read
it", and its enum was documented as open precisely so new derivation
mechanisms would not churn the schema. What this feature changed in the
row is its prose scope — written as the *test*-scope discriminator for
Go, now the derivation discriminator for any non-runtime scope in any
ecosystem — and nothing on the wire.
