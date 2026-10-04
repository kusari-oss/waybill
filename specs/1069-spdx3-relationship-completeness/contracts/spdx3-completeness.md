# Contract: SPDX 3 dependency relationships and `completeness`

## Shape

One relationship per `(from, type, scope)`:
- `type` is `Relationship`, or `LifecycleScopedRelationship` with `scope` ∈ {`development`, `build`, `test`};
- `relationshipType` is `dependsOn`;
- `to` is the sorted, deduplicated set of target IRIs.

`spdxId` = `<doc>/rel-` + hash(`from|dependsOn|<scope or "">|<targets joined by ",">`), 16-character prefix.

## `completeness`

Taken from `dependency_claims`, the predicate CycloneDX `compositions[]` uses:

| `from` component | CycloneDX | SPDX 3 relationship(s) from it |
|---|---|---|
| in `complete` (ecosystem resolved) | `aggregate: complete`, `dependencies` | `completeness: complete` |
| in `unknown` (ecosystem not resolved) | `aggregate: unknown` | `completeness: incomplete` |
| in `unknown`, no outgoing dependency | `aggregate: unknown` | one added `dependsOn → [NoAssertionElement]`, `completeness: noAssertion` |
| the scan root, when trace integrity is clean | separate `aggregate: complete` record | `completeness: complete` |
| none of the above (ecosystem not enumerated completely) | no claim | no `completeness`; no added relationship |
| in `complete`, no outgoing dependency | `aggregate: complete` | nothing added |

`NoAssertionElement` is the bare JSON-LD term. It expands to `https://spdx.org/rdf/3.0.1/terms/Core/NoAssertionElement`.

## Unchanged

- CycloneDX and SPDX 2.3 output.
- Non-dependency SPDX 3 relationships: `contains`, `describes`, licence and agent relationships.
- `waybill:graph-completeness`, `waybill:graph-completeness-reason` and `waybill:orphan-reason`, in all three formats.

## Catalogue

`docs/reference/sbom-format-mapping.md`: the dependency-edge row's SPDX 3 column describes the grouping and `completeness`, and names CycloneDX `compositions[]` as its counterpart (FR-007). No new `waybill:` field, so no new catalogue row.
