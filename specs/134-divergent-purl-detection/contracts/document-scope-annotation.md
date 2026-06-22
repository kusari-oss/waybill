# Contract — `mikebom:purl-collisions-detected` document-scope annotation

The document-scope aggregation surface. Emitted ONCE per SBOM, listing every divergent collision detected in the scan.

## Emission trigger

When 1+ `DivergenceRecord` entries were produced anywhere in the scan, emit the `CollisionsSummary` document-scope annotation. When zero, OMIT entirely (FR-009: no bloat, no spurious signal in no-collision SBOMs).

## CycloneDX 1.6 wire format

Location: `.metadata.properties[]`

```json
{
  "name": "mikebom:purl-collisions-detected",
  "value": "{\"v\":1,\"collisions\":[<DivergenceRecord-1>, <DivergenceRecord-2>]}"
}
```

- `name` MUST be exactly the literal `mikebom:purl-collisions-detected`.
- `value` MUST be a JSON-encoded string of the `CollisionsSummary` envelope.
- Sort order of `collisions[]`: lexically by `record.purl.as_str()` for deterministic byte-identity across runs.

## SPDX 2.3 wire format

Location: `.annotations[]` (document-scope; NOT `.packages[].annotations[]`)

```json
{
  "annotationDate": "<creation-info date>",
  "annotator": "Tool: mikebom-0.1.0-alpha.<N>",
  "annotationType": "OTHER",
  "comment": "{\"v\":1,\"mikebom:property\":\"mikebom:purl-collisions-detected\",\"value\":<CollisionsSummary as JSON object>}"
}
```

## SPDX 3.0.1 wire format

Location: `.@graph[?(.type == \"SpdxDocument\")].extension[]`

```json
{
  "type": "Extension",
  "mikebom:property": "mikebom:purl-collisions-detected",
  "value": { <CollisionsSummary as JSON object> }
}
```

## Per-component vs document-scope payload relationship

The document-scope `collisions[]` array is the union of every `DivergenceRecord` that ALSO appears as a per-component `mikebom:duplicate-purl-divergent` property elsewhere in the SBOM. The two surfaces are strictly redundant by design:

- Per-component surface answers "is THIS component divergent?" via a direct property lookup.
- Document-scope surface answers "what's every collision in the scan?" via a single `jq` query.

A consumer that walks every component and collects every per-component `mikebom:duplicate-purl-divergent` property MUST get the same set of `DivergenceRecord` entries as a consumer that reads the document-scope `collisions[]` directly.

This redundancy is the locked design from `/speckit.clarify` Q1 option C.

## Absence semantics

When no divergent collisions were detected (the common case), the annotation MUST NOT appear in the emitted SBOM in any of the three formats. A no-collision scan's SBOM is byte-identical (modulo timestamps + serial numbers) to the pre-milestone-134 baseline.

## Parity-catalog C-row entry

| Row | Property | Classification | Rationale |
|---|---|---|---|
| C100 | `mikebom:purl-collisions-detected` | KEEP-NO-NATIVE | See `research.md` R1 |

The parity extractor at `mikebom-cli/src/parity/extractors/divergent_purl_document_scope.rs` reads this annotation out of all three formats and verifies they agree byte-for-byte after canonicalization.
