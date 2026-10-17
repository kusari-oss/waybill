# Contract — C197 `waybill:version-absence`

Catalogue row **C197** (C196 is the current high-water mark, verified by
reading `docs/reference/sbom-format-mapping.md`). Per-component, closed
enum, `Directionality::SymmetricEqual`, `order_sensitive: false`.

## Semantics

> No version exists to be found for this component; the absence is not a
> resolution failure.

v1 recognises one value, `by-construction`. Future values may extend the
enum without a new C-row, and the row must say so (the C160 precedent).

Emitted **iff** the component is a synthetic Pants resolve anchor —
absent on every package, so emission is byte-identical to pre-feature
output on every component but the seven anchors.

## Per-format carriers

**CycloneDX 1.6** — per-component `properties[]` entry:

```json
{ "name": "waybill:version-absence", "value": "by-construction" }
```

**SPDX 2.3** — per-Package `annotations[].comment`, `MikebomAnnotationCommentV1`:

```json
{ "schema": "waybill-annotation/v1",
  "field": "waybill:version-absence",
  "value": "by-construction" }
```

**SPDX 3.0.1** — per-Package `Annotation.statement`, same envelope shape as
SPDX 2.3.

## Worked example

A Pants Python resolve anchor after this feature, CycloneDX:

```json
{
  "bom-ref": "pkg:generic/python-default?pants-namespace=python",
  "type": "library",
  "name": "python-default",
  "version": "",
  "purl": "pkg:generic/python-default?pants-namespace=python",
  "properties": [
    { "name": "waybill:component-kind",   "value": "lockfile-resolve" },
    { "name": "waybill:pants-resolve",    "value": "python-default" },
    { "name": "waybill:pants-resolve-namespace", "value": "python" },
    { "name": "waybill:version-absence",  "value": "by-construction" }
  ]
}
```

The `version` field stays empty. The marker explains it; it does not
replace it, and nothing invents a version string.

## Relationship to neighbouring rows

| Row | Annotation | Says | Overlap |
|---|---|---|---|
| C151 | `waybill:unresolved-reason` | resolution was attempted and failed | **mutually exclusive** with C197 on one component |
| C160 | `waybill:component-kind` | what kind of thing this is (`lockfile-resolve`) | **co-present** on anchors; different fact |
| C143 | `waybill:pants-resolve` | which resolve(s) this belongs to | co-present; different fact |
| C91 | `waybill:component-tier` = `file` | a file-tier component, which is also inherently versionless | disjoint populations today; a later change could bring both under C197, explicitly out of scope |

## Principle V audit (for the catalogue row's final column)

**KEEP-NO-NATIVE.** No CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 native carrier for
"this component has no version to have". In all three formats the version
field is the only version-bearing property and has no sibling that could
explain an absence. Rejected: (1) CDX
`evidence.identity[field=version].confidence` — grades confidence in a
*concluded* value, so `0` means "unsure what the version is", which is
C151's semantic; adopting it would merge the two populations I8 exists to
separate. (2) SPDX 2.3 `NOASSERTION` — not a schema-permitted `versionInfo`
value, and it states the author's posture rather than a fact about the
component. (3) SPDX 3 `software_SoftwarePurpose` — offers `manifest` and
`bom`, but names what the component *is* rather than that it is
unversioned, and exists in SPDX 3 alone, which is the parity gap this
clause covers. (4) CDX `component.type`, CDX `compositions[].aggregate`,
SPDX 2.3 `primaryPackagePurpose` — audited and rejected by C160 for the
adjacent "stands for a set, not acquirable" semantic; cited, not re-argued.
Full method and schema quotations: `specs/1156-version-absence-marker/research.md` R1.

## Gate

`parity::extractors::tests::every_catalog_row_has_an_extractor` and
`holistic_parity` fail if the row exists without all three extractors, or
an extractor without the row. Both directions are asserted, so the row and
the code land together or not at all.
