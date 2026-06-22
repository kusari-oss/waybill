# Contract — `mikebom:duplicate-purl-divergent` per-component property

The per-component primary signal. Lives on the deduped root component for every detected divergent collision.

## Emission trigger

For every PURL `P` where the cargo reader's dedup observes 2+ Cargo.toml manifests AND `compare_divergence(candidates)` returns a non-`None` `DivergenceRecord`: emit the property on the surviving component identified by `P`.

## CycloneDX 1.6 wire format

Location: `.components[?(.purl == P)].properties[]`

```json
{
  "name": "mikebom:duplicate-purl-divergent",
  "value": "{\"v\":1,\"purl\":\"pkg:cargo/foo@1.2.3\",\"reason\":\"deps-differ\",\"paths\":[\"crates/foo/Cargo.toml\",\"vendor/foo/Cargo.toml\"],\"dep_sets_by_path\":{\"crates/foo/Cargo.toml\":[\"serde\",\"tokio\"],\"vendor/foo/Cargo.toml\":[\"anyhow\",\"serde\",\"tokio\"]}}"
}
```

- `name` MUST be exactly the literal `mikebom:duplicate-purl-divergent`.
- `value` MUST be a JSON-encoded string of the `DivergenceRecord` serialized envelope (per `data-model.md`).
- The property MUST be appended in a deterministic position (after existing properties, before any property added by a later milestone with a name lexically > this one).

## SPDX 2.3 wire format

Location: `.packages[?(.SPDXID == ref_for(P))].annotations[]`

```json
{
  "annotationDate": "<creation-info date, no per-annotation drift>",
  "annotator": "Tool: mikebom-0.1.0-alpha.<N>",
  "annotationType": "OTHER",
  "comment": "{\"mikebom:property\":\"mikebom:duplicate-purl-divergent\",\"value\":<DivergenceRecord as JSON object>}"
}
```

The `comment` field MUST contain the `MikebomAnnotationCommentV1` envelope shape (already used by every other mikebom:* property under SPDX 2.3 per the milestone-071 parity-extractor infrastructure):

```json
{
  "v": 1,
  "mikebom:property": "mikebom:duplicate-purl-divergent",
  "value": {
    "v": 1,
    "purl": "pkg:cargo/foo@1.2.3",
    "reason": "deps-differ",
    "paths": ["crates/foo/Cargo.toml", "vendor/foo/Cargo.toml"],
    "dep_sets_by_path": {
      "crates/foo/Cargo.toml": ["serde", "tokio"],
      "vendor/foo/Cargo.toml": ["anyhow", "serde", "tokio"]
    }
  }
}
```

## SPDX 3.0.1 wire format

Location: `.@graph[?(.spdxId == iri_for(P))].extension[]`

```json
{
  "type": "Extension",
  "mikebom:property": "mikebom:duplicate-purl-divergent",
  "value": { <DivergenceRecord as JSON object> }
}
```

## Absence semantics

When mikebom detects NO divergent collision for `P`, the property MUST NOT appear on the component in any of the three formats. This means:

- A no-collision scan emits identical SBOM bytes (modulo timestamps + serial numbers) to the pre-milestone-134 baseline.
- A scan that detects a `pkg:cargo/foo@1.2.3` collision AND `pkg:cargo/bar@2.0.0` non-collision emits the property only on `foo`, not on `bar`.

## Parity-catalog C-row entry

| Row | Property | Classification | Rationale |
|---|---|---|---|
| C99 | `mikebom:duplicate-purl-divergent` | KEEP-NO-NATIVE | See `research.md` R1 |

The parity extractor at `mikebom-cli/src/parity/extractors/divergent_purl_per_component.rs` reads this property out of all three formats and verifies they agree byte-for-byte after the milestone-071 `canonicalize_for_compare` normalization.
