# Contract: `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` annotations

**Milestone 158** • Wire-format contract for the two new document-scope annotations.

## CycloneDX 1.6

Location: `.metadata.properties[]`

```json
{
  "metadata": {
    "properties": [
      {
        "name": "mikebom:graph-completeness",
        "value": "complete"
      },
      {
        "name": "mikebom:graph-completeness-reason",
        "value": "orphaned-components-detected: 3"
      }
    ]
  }
}
```

- The `mikebom:graph-completeness` entry MUST appear exactly ONCE per emitted SBOM (FR-003).
- The `mikebom:graph-completeness-reason` entry appears exactly ONCE **when `mikebom:graph-completeness.value != "complete"`** and MUST NOT appear otherwise (FR-004 + FR-005).

## SPDX 2.3

Location: document-level `annotations[]` (attached to `SPDXRef-DOCUMENT`).

```json
{
  "SPDXID": "SPDXRef-DOCUMENT",
  "annotations": [
    {
      "annotator": "Tool: mikebom-<version>",
      "annotationDate": "<timestamp>",
      "annotationType": "OTHER",
      "comment": "mikebom:graph-completeness=complete"
    },
    {
      "annotator": "Tool: mikebom-<version>",
      "annotationDate": "<timestamp>",
      "annotationType": "OTHER",
      "comment": "mikebom:graph-completeness-reason=orphaned-components-detected: 3"
    }
  ]
}
```

Same presence rules as CDX. The `comment` field carries `<name>=<value>` — matches milestone-127's `mikebom:root-selection-heuristic` shape.

## SPDX 3.0.1

Location: a document-scope `Annotation` element in the `@graph` array with `subject = "SPDXRef-DOCUMENT"`.

```json
{
  "@graph": [
    {
      "type": "Annotation",
      "spdxId": "urn:mikebom:...",
      "annotationType": "other",
      "subject": "SPDXRef-DOCUMENT",
      "statement": "mikebom:graph-completeness=complete",
      "creationInfo": { "@id": "..." }
    },
    {
      "type": "Annotation",
      "spdxId": "urn:mikebom:...",
      "annotationType": "other",
      "subject": "SPDXRef-DOCUMENT",
      "statement": "mikebom:graph-completeness-reason=orphaned-components-detected: 3",
      "creationInfo": { "@id": "..." }
    }
  ]
}
```

Same presence rules. The `statement` field carries `<name>=<value>` — matches milestone-127 SPDX 3 shape.

## Value grammar

Per data-model.md's `GraphCompletenessValue::as_str()` + `ReasonCode::to_reason_string()`:

- `mikebom:graph-completeness.value` ∈ `{"complete", "partial", "unknown"}`.
- `mikebom:graph-completeness-reason.value` matches this BNF:

```text
reason_string   ::= reason_entry ("; " reason_entry)*
reason_entry    ::= code ": " detail
code            ::= documented reason code (see graph-completeness-vocabulary.md)
detail          ::= human-readable string; MUST NOT contain the `;` character
```

## Parity (SC-010)

The two annotations register in the milestone-071 parity catalog:

- Row C70: `mikebom:graph-completeness`
- Row C71: `mikebom:graph-completeness-reason`

Both rows use `Directionality::SymmetricEqual` and `order_sensitive: false`. Enforcement: `cargo +stable test -p mikebom parity_symmetric` (part of `cargo test --workspace`).
