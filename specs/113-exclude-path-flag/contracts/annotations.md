# Contract — Transparency annotation

**Feature**: 113-exclude-path-flag

When `ExclusionSet::is_empty() == false` at SBOM emission time, the emitted document MUST carry an envelope-level annotation listing the active entries. This is a Constitution Principle X (Transparency) requirement, not a feature option.

## Payload

```text
mikebom:exclude-path = "<entry1>,<entry2>,…,<entryN>"
```

- Entries are emitted in the order they were supplied (CLI flags in argv order, then env-var entries in env-string order).
- Literal entries are normalized to forward-slash form.
- Pattern entries are emitted verbatim.
- No deduplication at emission (the `ExclusionSet` constructor already deduplicates; double appearance would be a parse bug).

## Per-format emission

### CDX 1.6

Added to `metadata.properties[]`:

```text
{
  "name": "mikebom:exclude-path",
  "value": "tests/fixtures,**/testdata"
}
```

Code site: `mikebom-cli/src/generate/cyclonedx/metadata.rs` — extends the existing properties-building loop.

### SPDX 2.3

Added to `creationInfo.annotations[]`:

```text
{
  "annotationType": "OTHER",
  "annotator": "Tool: mikebom-<version>",
  "annotationDate": "<creationInfo.created>",
  "comment": "mikebom:exclude-path=tests/fixtures,**/testdata"
}
```

Code site: `mikebom-cli/src/generate/spdx/annotations.rs` — extends the document-level annotation emitter.

### SPDX 3

Added as an `Annotation` element on the `SpdxDocument`:

```text
{
  "@type": "Annotation",
  "annotationType": "other",
  "subject": "<SpdxDocument SPDXID>",
  "statement": "mikebom:exclude-path=tests/fixtures,**/testdata",
  "creationInfo": "<creationInfo blank-node ref>"
}
```

Code site: `mikebom-cli/src/generate/spdx/v3_annotations.rs` — extends the document-level annotation emitter.

## Standards-native audit (Principle V bullet 5)

| Format | Native field for "operator excluded paths at scan time" | Verdict |
|---|---|---|
| CDX 1.6 | None. `metadata.lifecycles` expresses build phase; `metadata.tools.services` expresses scan tooling but not its parameters. | Justifies the `mikebom:exclude-path` property. |
| SPDX 2.3 | None. `creationInfo.creatorComment` is a free-form opaque field, but a structured annotation with the `mikebom:` prefix is more interoperable. | Justifies the annotation. |
| SPDX 3 | None. `Annotation` is the appropriate structured-extension surface. | Justifies the annotation. |

This audit MUST be cited in `docs/reference/sbom-format-mapping.md` per Principle V bullet 5; the implementation task includes adding the row.

## Parity catalog

The annotation gets three new parity-catalog rows (one per format) tracking that all three emitters write the same payload for the same scan invocation. Pattern: same as milestone-072's cross-tier binding rows.

## What this contract DOES NOT cover

- Per-component annotations on suppressed neighbors (not emitted — there's no neighbor to annotate).
- Per-walker logging of matches (covered in research R6).
- Backwards-compatibility migration (none needed — feature is additive and behind a default-off flag).
