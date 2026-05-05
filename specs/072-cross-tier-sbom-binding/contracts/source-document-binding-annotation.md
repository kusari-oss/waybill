# Contract — `mikebom:source-document-binding` per-format carrier

This contract specifies how the `SourceDocumentBinding` payload is carried in each of mikebom's three output formats. Per Constitution Principle V, the **standards-native cross-document reference** is emitted alongside; the `mikebom:*` annotation carries only the per-component hash + strength label which has no native equivalent.

## C-1 — Emission triggers

The annotation is emitted on every component carrying `mikebom:sbom-tier: build` or `mikebom:sbom-tier: deployed`. Source-tier (`mikebom:sbom-tier: source`) components do NOT carry it (they ARE the binding target, not the bound entity). This keeps source-tier byte-identity goldens unchanged from alpha.14.

## C-2 — Standards-native cross-document reference (sibling)

Emitted at the **document level** in all three formats, naming the source SBOM document by SHA-256 + optional IRI.

### CDX 1.6

In `metadata.component.externalReferences[]`:

```json
{
  "metadata": {
    "component": {
      "externalReferences": [
        {
          "type": "bom",
          "url": "https://example.org/sbom/foo-source.cdx.json",
          "comment": "source-tier SBOM that produced this build/deployment",
          "hashes": [
            { "alg": "SHA-256", "content": "<sha256-of-source-sbom-canonical-bytes>" }
          ]
        }
      ]
    }
  }
}
```

### SPDX 2.3

Document-level `externalDocumentRefs[]`:

```json
{
  "externalDocumentRefs": [
    {
      "externalDocumentId": "DocumentRef-source-sbom",
      "spdxDocument": "https://example.org/sbom/foo-source.spdx.json",
      "checksum": {
        "algorithm": "SHA256",
        "checksumValue": "<sha256-of-source-sbom-canonical-bytes>"
      }
    }
  ]
}
```

Plus a `BUILT_FROM` relationship at the document level binding the image-tier root component to the source-tier component:

```json
{
  "relationships": [
    {
      "spdxElementId": "SPDXRef-image-root",
      "relatedSpdxElement": "DocumentRef-source-sbom:SPDXRef-source-main-module",
      "relationshipType": "BUILT_FROM"
    }
  ]
}
```

### SPDX 3.0.1

Document-level `import[]` on the `SpdxDocument` element:

```json
{
  "type": "SpdxDocument",
  "spdxId": "https://example.org/spdx/image-doc",
  "import": [
    {
      "type": "ExternalMap",
      "externalSpdxId": "https://example.org/sbom/foo-source.spdx3.json",
      "verifiedUsing": [
        { "type": "Hash", "algorithm": "sha256", "hashValue": "<sha256-of-source-sbom-canonical-bytes>" }
      ]
    }
  ]
}
```

Plus a `Relationship` graph element with `relationshipType: built_from`:

```json
{
  "type": "Relationship",
  "spdxId": "https://example.org/spdx/rel-built-from-1",
  "from": "https://example.org/spdx/image-root",
  "to": ["https://example.org/spdx/source-main-module"],
  "relationshipType": "built_from"
}
```

## C-3 — Per-component `mikebom:source-document-binding` annotation

The per-component hash + strength + reason payload, JSON-encoded per the existing `SourceDocumentBinding` shape from data-model.md. Carrier shape per format:

### CDX 1.6

`components[].properties[]` entry where `name == "mikebom:source-document-binding"` and `value` is the JSON-encoded SourceDocumentBinding (single-line, no whitespace):

```json
{
  "components": [{
    "name": "foo-binary",
    "purl": "pkg:golang/example.com/foo@v1.0.0",
    "properties": [
      {
        "name": "mikebom:source-document-binding",
        "value": "{\"algo\":\"v1\",\"hash\":\"a1b2...\",\"reason\":null,\"source_doc_id\":{\"sha256\":\"e5f6...\"},\"strength\":\"verified\"}"
      }
    ]
  }]
}
```

The `value` is JSON-encoded with sorted keys (per the milestone-071 canonicalization rule), so the wire-bytes are byte-stable across reruns.

### SPDX 2.3

`Package.annotations[]` entry wrapped in the existing `MikebomAnnotationCommentV1` envelope (`mikebom-cli/src/generate/spdx/annotations.rs:31`):

```json
{
  "packages": [{
    "name": "foo-binary",
    "SPDXID": "SPDXRef-foo-binary",
    "annotations": [{
      "annotator": "Tool: mikebom-0.1.0-alpha.15",
      "annotationDate": "2026-05-05T12:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-document-binding\",\"value\":{\"algo\":\"v1\",\"hash\":\"a1b2...\",\"source_doc_id\":{\"sha256\":\"e5f6...\"},\"strength\":\"verified\"}}"
    }]
  }]
}
```

### SPDX 3.0.1

Same envelope, attached as a graph-element `Annotation` whose `subject` is the Package's IRI and whose `statement` carries the JSON-encoded envelope (same shape as SPDX 2.3 `comment`):

```json
{
  "type": "Annotation",
  "subject": "https://example.org/spdx/foo-binary",
  "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-document-binding\",\"value\":{\"algo\":\"v1\",\"hash\":\"a1b2...\",\"source_doc_id\":{\"sha256\":\"e5f6...\"},\"strength\":\"verified\"}}"
}
```

## C-4 — `Unknown` strength carrier

When `strength == Unknown`, the annotation IS still emitted (per FR-003 — explicit > silent), with `hash: null` and a non-empty `reason`:

```json
{
  "algo": "v1",
  "hash": null,
  "source_doc_id": { "sha256": "<source-sbom-hash-or-empty>" },
  "strength": "unknown",
  "reason": "base-layer-system-package"
}
```

Common `reason` values (the contract is open-ended; mikebom emits these documented values):

| reason | meaning |
|---|---|
| `no-evidence` | Fewer than 2 of (vcs, lockfile, manifest) populated |
| `base-layer-system-package` | Component came from an OS package manager (deb/apk/rpm); no source SBOM expected |
| `sideloaded-binary` | Binary in image with no traceable build path (vendored, sideloaded) |
| `source-not-found-in-bind-target` | `--bind-to-source <path>` was supplied but path didn't contain a matching component |
| `verification-failed` | Hash computed but didn't match the asserted hash in the source SBOM |
| `algo-version-unsupported` | Source SBOM's binding used a future algo version this mikebom can't recompute |

## C-5 — Catalog row registration (cross-format parity)

The new `mikebom:source-document-binding` annotation requires a new `ParityExtractor` row in `mikebom-cli/src/parity/extractors/mod.rs`. Directionality: **`SymmetricEqual`** — the JSON-encoded binding payload must be byte-identical across CDX `properties[].value` (string-encoded JSON) and SPDX 2.3 / 3 envelope's `value` (real JSON object). Canonicalization via the milestone-071 `canonicalize_for_compare` helper handles the string-encoded-JSON-vs-real-JSON-object equivalence.

## C-6 — Deserialization order

Verifiers and consumers MUST tolerate JSON object keys in any order on the wire (the canonical-JSON rule applies to mikebom's emit, not to consumer-side parse). `serde_json::from_str` over the `SourceDocumentBinding` struct handles arbitrary key order automatically.

## C-7 — Stability commitment

Once milestone 072 ships:

- The annotation key `mikebom:source-document-binding` is stable.
- The JSON shape (`{algo, hash, source_doc_id, strength, reason}`) is stable for `algo: "v1"`.
- New optional fields MAY be added in future milestones (`skip_serializing_if`-gated) without bumping the algo version.
- Removed or renamed fields require an algo bump (V1 → V2) with parallel emission.
