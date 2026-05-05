# Contract — `mikebom:source-identifiers` per-format carrier shapes

This contract specifies how source identifiers are carried in each of mikebom's three output formats. Per Constitution Principle V, **standards-native carriers take precedence**. The `mikebom:source-identifiers` annotation is the documented exception path for user-defined namespaces only.

## C-1 — Built-in identifier carriers (standards-native)

For each emitted built-in identifier, the carrier per format:

### CDX 1.6

`metadata.component.externalReferences[]`:

```json
{
  "metadata": {
    "component": {
      "externalReferences": [
        {
          "type": "vcs",
          "url": "git@github.com:acme/foo.git",
          "comment": "auto-detected from git remote `origin`"
        },
        {
          "type": "distribution",
          "url": "docker.io/acme/foo:v1@sha256:abc...",
          "comment": "auto-detected from resolved image reference"
        },
        {
          "type": "attestation",
          "url": "https://example.org/att/build-42",
          "comment": "manual --with-source"
        }
      ]
    }
  }
}
```

The `type` value is the per-scheme mapping from `identifier-shape.md` C-2.

### SPDX 2.3 — dual carrier

**Primary** (typed, schema-aware-consumer-friendly): main-module `Package.externalRefs[]`:

```json
{
  "packages": [
    {
      "name": "foo",
      "SPDXID": "SPDXRef-Package-foo",
      "externalRefs": [
        {
          "referenceCategory": "PERSISTENT-ID",
          "referenceType": "repo",
          "referenceLocator": "git@github.com:acme/foo.git",
          "comment": "auto-detected from git remote `origin`"
        }
      ]
    }
  ]
}
```

**Note**: SPDX 2.3 spec doesn't enumerate `repo` / `git` / `image` / `attestation` under `PERSISTENT-ID`'s `referenceType` registry. mikebom uses the scheme name as the `referenceType` value verbatim — this is consistent with how SPDX 2.3 implementations tolerate unregistered identifier types under `PERSISTENT-ID` (the category itself is the typed slot; the `referenceType` value is operator-defined for non-registered identifiers per the spec's open-extension posture).

**Redundant fallback** (free-form, well-known field): `creationInfo.creators[]`:

```json
{
  "creationInfo": {
    "creators": [
      "Tool: mikebom-0.1.0-alpha.16",
      "Tool: mikebom-0.1.0-alpha.16 source: repo:git@github.com:acme/foo.git",
      "Tool: mikebom-0.1.0-alpha.16 source: image:docker.io/acme/foo:v1@sha256:abc..."
    ]
  }
}
```

The redundant text-line form is `Tool: <mikebom-version> source: <full-identifier>` — one entry per built-in identifier. Free-form per SPDX 2.3 `creationInfo.creators` semantics. Per Q2 clarification, this dual-carrier path ensures consumers that don't decode `Package.externalRefs[]` still see the identifiers.

### SPDX 3.0.1

`Element.externalIdentifier[]` on the `SpdxDocument` element (perfect fit):

```json
{
  "@context": "https://spdx.org/rdf/3.0.1/spdx-context.jsonld",
  "@graph": [
    {
      "type": "SpdxDocument",
      "spdxId": "https://example.org/spdx/foo-source-doc",
      "externalIdentifier": [
        {
          "type": "ExternalIdentifier",
          "externalIdentifierType": "repo",
          "identifier": "git@github.com:acme/foo.git",
          "comment": "auto-detected from git remote `origin`"
        },
        {
          "type": "ExternalIdentifier",
          "externalIdentifierType": "image",
          "identifier": "docker.io/acme/foo:v1@sha256:abc...",
          "comment": "auto-detected from resolved image reference"
        }
      ]
    }
  ]
}
```

SPDX 3's multi-identifier model handles every built-in scheme + arbitrary user-defined schemes uniformly. Thus user-defined identifiers ALSO emit here on the SPDX 3 side (no envelope-annotation needed for SPDX 3).

## C-2 — User-defined identifier carrier (the `mikebom:*` exception)

Per Constitution Principle V's documented-exception path: user-defined identifier schemes (matching FR-004 regex but NOT in the built-in registry) have no native carrier on CDX or SPDX 2.3. They ride a single document-level `mikebom:source-identifiers` annotation, wrapped in milestone-071's `MikebomAnnotationCommentV1` envelope.

### CDX 1.6

`metadata.properties[]` entry:

```json
{
  "metadata": {
    "properties": [
      {
        "name": "mikebom:source-identifiers",
        "value": "[{\"scheme\":\"acme_corp_id\",\"value\":\"abc123\"},{\"scheme\":\"internal_ticket\",\"value\":\"PROJ-456\"}]"
      }
    ]
  }
}
```

The `value` is a JSON-encoded array (JSON-string-in-string per CDX 1.6 schema constraints). Sorted lex by `(scheme, value)` for determinism per FR-009.

### SPDX 2.3

Document-level `annotations[]` with the `MikebomAnnotationCommentV1` envelope:

```json
{
  "annotations": [
    {
      "annotator": "Tool: mikebom-0.1.0-alpha.16",
      "annotationDate": "2026-05-05T12:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-identifiers\",\"value\":[{\"scheme\":\"acme_corp_id\",\"value\":\"abc123\"}]}"
    }
  ]
}
```

### SPDX 3.0.1

User-defined identifiers emit via the SPDX 3 native `Element.externalIdentifier[]` (C-1 SPDX 3 example) — no separate annotation. This is a structural advantage of SPDX 3's open-typed identifier model.

## C-3 — Catalog row registration

A new `ParityExtractor` row at `mikebom-cli/src/parity/extractors/mod.rs` for `mikebom:source-identifiers`:

- **Row ID**: `C47` (next free C-section row after milestone-072's C46).
- **Directionality**: `SymmetricEqual`. CDX `properties[].value` (string-encoded JSON array) and SPDX 2.3 envelope's `value` (real JSON array) and SPDX 3 native shape MUST canonicalize to the same `BTreeSet<String>` of canonical-JSON entries via the milestone-071 `canonicalize_for_compare` helper.

The cross-format-parity test suite (milestone-071's `holistic_parity.rs`) MUST pass against the new row.

## C-4 — Carrier-position determinism

Per FR-009, the array order in each carrier is deterministic:

- CDX `externalReferences[]`: auto-detected entries first (in detection order — at most one `repo:` from auto-detection today), then manual `--with-source` entries in supply order.
- SPDX 2.3 main-module `Package.externalRefs[]`: same ordering.
- SPDX 2.3 `creationInfo.creators[]`: same ordering.
- SPDX 3 `Element.externalIdentifier[]`: same ordering.
- `mikebom:source-identifiers` envelope's `value` array: sorted lex by `(scheme, value)` (annotations have unordered semantics; lex sort gives a stable serialization).

## C-5 — Stability commitment

- The per-format carrier choices are stable post-073.
- The `mikebom:source-identifiers` envelope shape (JSON array of `{scheme, value, source_label}` objects) is stable for `algo: v1`-style schema. Future fields are skip_serializing_if-gated.
- The C47 catalog row directionality is stable; future user-defined schemes don't change the directionality.
- SPDX 2.3 `Package.externalRefs[].referenceType` accepts the scheme name verbatim — if SPDX 2.3 ever registers a conflicting type name, mikebom adopts the registered name and emits a migration note (similar to the user-defined-scheme migration path).
