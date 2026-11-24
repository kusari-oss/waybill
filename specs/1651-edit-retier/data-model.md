# Data Model: `sbom edit --retier`

## `EditOp::Retier { from: String, to: SbomType }`

- **`from`:** matched exactly against `waybill:sbom-tier`. Any string.
- **`to`:** a `generate::lifecycle_phases::SbomType`, parsed by `SbomType::parse`, the same parser and error as `--sbom-type` (analyze I1).
- **Parsing:** from `--retier FROM=TO` and sequence-stamped in command-line order, like every edit operation.
- **Run-level validation:** no `FROM` maps to two targets.

## The run loop (R2)

`relabelled: BTreeSet<ComponentId>` is local to one `edit::run`. For a `Retier`:
- **`matched`** is the number of components whose tier is `from`;
- for each one not already in `relabelled`, `set_tier(id, to)` runs and the component is added to `relabelled`;
- **`changed`** is the number relabelled.

`set_tier(id, to)`, per adapter:
1. rewrites the component's `waybill:sbom-tier` to `to`;
2. if the component carries no `waybill:retiered-from`, adds it with the *previous* tier. An existing marker keeps its value.

## C194 `operations` entry

```json
{"category": "retier", "matched": 27, "changed": 27, "from": "design", "to": "source"}
```

`from` and `to` are omitted for every other category, so existing records are byte-identical.

## C243 `waybill:retiered-from`

| Field | Value |
|---|---|
| Scope | per component |
| Value | the tier the component had when scanned, e.g. `design` |
| CycloneDX | `properties[] {name: "waybill:retiered-from", value}` |
| SPDX 2.3 | package annotation, `waybill-annotation/v1` envelope |
| SPDX 3 | `Annotation` element on the package, same envelope |
| Protected | yes: in `PROTECTED_ANNOTATIONS` with C21 and C194 |
| Parity | `SymmetricEqual` |

## The document type (R5)

1. `derived_before` is the aggregate of the **input's** component tiers, before any operation, computed by the shared `*_from_tiers` aggregators. The recompute runs only when a `Retier` changed at least one component; the new type is the aggregate of the **output's** tiers, after every operation (analyze U1).
2. If the document's current type equals `derived_before`, it is replaced by the aggregate of the **relabelled** tiers, in:
   - CycloneDX `metadata.lifecycles`;
   - SPDX 2.3 `creationInfo.comment`, the `Observed lifecycle phases: …` clause only;
   - SPDX 3 `software_sbomType` and its comment.
3. Otherwise it is kept, and `EditReport` notes `document_type: kept (does not match the component tiers)`.

## Unchanged

`waybill:graph-completeness` and its reason, components, PURLs, versions, scopes, edges, every other annotation.
