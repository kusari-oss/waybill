# Contract: C140 `waybill:project-discovery-mode` doc-scope annotation

**Feature**: 220-project-discovery-scope | **Related**: FR-011

## Surface

New parity-catalog row **C140** — `waybill:project-discovery-mode`. Document-scope annotation. String-valued.

### Emission gate

Emitted iff the scan ran under a non-default mode (`RootOnly` OR `Strict`). Silent when mode = `All` (SC-005 wire-shape byte-identity preserved). Matches m217 C136 + m219 C137-C139 silence-on-default precedent.

### Value

The mode name as rendered by `ProjectDiscoveryMode::Display` (lowercase kebab-case matching CLI wire form):
- `"root-only"` when mode = RootOnly
- `"strict"` when mode = Strict

### Per-format landing slots

| Format    | Landing slot                                             |
|-----------|----------------------------------------------------------|
| CycloneDX | `metadata.properties[]` entry `{"name":"waybill:project-discovery-mode","value":"root-only"}` |
| SPDX 2.3  | Document-level `Annotation` on `SPDXRef-DOCUMENT`, `MikebomAnnotationCommentV1` envelope: `{"schema":"waybill-annotation/v1","field":"waybill:project-discovery-mode","value":"root-only"}` |
| SPDX 3    | `Annotation` element on the SpdxDocument root IRI; same envelope shape |

Identical to the m217 C136 shape (`waybill:go-toolchain-detected`) — same doc-scope precedent.

## Standards-native audit (Constitution Principle V)

**Rejected alternatives**:

1. **CDX `metadata.properties[]` native field**: no native "scan-scope was capped" concept in CDX 1.6.
2. **SPDX 2.3 `creationInfo.creators[]`**: producer-scope (describes the tool that created the SBOM, not a scope decision the operator made). Same rejection reasoning as m217 C136.
3. **SPDX 3 `SpdxDocument.scope`**: no such field in SPDX 3.0.1.
4. **CDX `metadata.lifecycles[]` phase**: describes WHEN in the lifecycle the SBOM was produced (pre-build / build / operations), not WHAT SCOPE the scan used. Orthogonal semantic.

**The genuine signal**: consumers need to distinguish "this SBOM covers everything scannable at this root" from "this SBOM was intentionally scoped via `--project-discovery=root-only`." Without the annotation, a consumer parsing the SBOM has no way to know whether the absent-components are "not present" or "scoped out." That's a real information loss for auditability + Guac ingest + VEX reachability computations.

**KEEP-NO-NATIVE**. Documented in `docs/reference/sbom-format-mapping.md` C140 per the m216/m217/m218/m219 precedent.

## Wire example (multi-format)

### CycloneDX

```json
{
  "metadata": {
    "properties": [
      {"name": "waybill:project-discovery-mode", "value": "root-only"}
    ]
  }
}
```

### SPDX 2.3

```json
{
  "annotations": [
    {
      "annotator": "Tool: waybill-0.1.0-alpha.68",
      "annotationDate": "2026-07-24T00:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:project-discovery-mode\",\"value\":\"root-only\"}"
    }
  ]
}
```

### SPDX 3

```json
{
  "@graph": [
    {
      "type": "Annotation",
      "spdxId": "https://waybill.kusari.dev/spdx3/doc-XXXX/anno-YYYY",
      "creationInfo": "_:creation-info",
      "subject": "https://waybill.kusari.dev/spdx3/doc-XXXX",
      "annotationType": "other",
      "statement": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:project-discovery-mode\",\"value\":\"root-only\"}"
    }
  ]
}
```

## Parity extractor registration

Three new extractor triplets in `waybill-cli/src/parity/extractors/`:
- `c140_cdx` — doc-scope, pattern-matches m217 `c136_cdx` verbatim.
- `c140_spdx23` — doc-scope, pattern-matches m217 `c136_spdx23`.
- `c140_spdx3` — doc-scope, pattern-matches m217 `c136_spdx3`.

New `EXTRACTORS` row registered in `parity/extractors/mod.rs` after C139 (m218's last row):

```rust
ParityExtractor {
    row_id: "C140",
    label: "waybill:project-discovery-mode",
    cdx: c140_cdx, spdx23: c140_spdx23, spdx3: c140_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false
},
```

Nine new use-list entries: `c140_cdx, c140_spdx23, c140_spdx3` in the respective use blocks.

`every_catalog_row_has_an_extractor` bidirectional test asserts registration on both sides — same trip that caught m216/m217/m218/m219 pre-PR gates.

## Consumer-observed invariants

- **Invariant 1**: annotation present in emitted SBOM ⟹ scan ran under non-default mode (RootOnly or Strict).
- **Invariant 2**: annotation absent ⟹ EITHER default All mode was used OR the SBOM predates m220 (both valid interpretations for m215+ consumers).
- **Invariant 3**: 3-format parity — CDX/SPDX 2.3/SPDX 3 outputs from the same scan emit equivalent annotation values.

## Backward compatibility

- Legacy m215-era + alpha.66/67/68 consumers see NO change under default mode. Every existing golden stays byte-identical.
- m220-aware consumers can detect scoping via jq `.metadata.properties[]? | select(.name == "waybill:project-discovery-mode")` (CDX) or `select(comment | fromjson? | .field == "waybill:project-discovery-mode")` (SPDX 2.3).
