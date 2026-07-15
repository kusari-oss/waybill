# Contract: m197 Annotation Shape Surface

**Date**: 2026-07-15
**Purpose**: The stable emission surface m197 exposes to SBOM consumers. Any change to these shapes requires a spec update.

## New annotation: `mikebom:declared-as`

**Emission location**: On reconciler-survivor components (any ecosystem, but in practice npm-dominated since npm's alias-declaration syntax is the primary trigger) that consumed one or more design-tier hits declared via an alias.

**Wire shape across all 3 formats**:

- **CycloneDX 1.6**: appears in the component's `properties[]` array:
  ```json
  {
    "name": "mikebom:declared-as",
    "value": "[\"my-preferred-name\",\"legacy-alias\"]"
  }
  ```
  Note: `value` is a JSON-encoded string containing the array (matches the existing envelope convention documented in the m191 reconciler code path).

- **SPDX 2.3**: appears in the package's `annotations[]` array:
  ```json
  {
    "annotationType": "OTHER",
    "annotator": "Tool: mikebom-<version>",
    "annotationDate": "<...>",
    "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:declared-as\",\"value\":[\"my-preferred-name\",\"legacy-alias\"]}"
  }
  ```

- **SPDX 3.0.1**: appears as an `Annotation` element referencing the Package IRI:
  ```json
  {
    "type": "Annotation",
    "annotationType": "other",
    "subject": "https://mikebom.kusari.dev/spdx3/doc-<...>/pkg-<...>",
    "statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:declared-as\",\"value\":[\"my-preferred-name\",\"legacy-alias\"]}"
  }
  ```

**Presence rules**:

- Emitted ONLY when at least one alias was reconciled onto the survivor. Components without alias involvement have NO `mikebom:declared-as` key.
- Array is non-empty when present.
- Array values are lex-sorted for deterministic goldens.

**Consumer contract**:

- Parse as JSON array of strings.
- The array's values are the raw alias names as they appeared in source manifests. Consumers wanting the resolved identity should read `component.name` / `component.purl`.

## Rotated annotations: `mikebom:requirement-ranges` + `mikebom:source-manifests`

**Emission location**: On every reconciler-survivor with at least one design-tier match. Present regardless of single-vs-multi declaration count (uniform per Q1 clarification).

**Wire shape**: JSON array of strings, same envelope conventions as `mikebom:declared-as` above (JSON-encoded arrays inside the format-specific carrier field).

**Presence rules**:

- Emitted whenever the reconciler produces a survivor. If no design-tier hit contributed a range or manifest, the survivor still gets the annotation with an empty-array value — WAIT, correction: per validation rules in data-model.md, arrays MUST be non-empty when the annotation is present. So the annotation is absent when there are zero contributing design-tier hits (an edge case; a survivor with only source-tier contribution).
- Ordering: `mikebom:source-manifests` sorted lex; `mikebom:requirement-ranges` ordered 1:1 with `source-manifests` (Nth range is from Nth manifest).

**Migration from m191 shape** (SBOM consumers):

Pre-m197: `annotation.value` was a scalar string, e.g. `"^11.0"`.
Post-m197: `annotation.value` is a JSON-encoded array, e.g. `"[\"^11.0\"]"`.

A consumer previously reading:
```python
annotation.value  # → "^11.0"
```
should now read:
```python
json.loads(annotation.value)[0]  # → "^11.0"  (for the single-declaration case)
json.loads(annotation.value)     # → ["^11.0", "^11.1.0"]  (multi-declaration case)
```

The old field names `mikebom:requirement-range` and `mikebom:source-manifest` (singular) are removed from m197 output. Consumers hard-coding the singular names produce no matches.

## Rotated annotations: PURL `?epoch=<N>` qualifier emission (US1 + US2)

**Emission location**: Component `purl` field, dpkg + apk readers (US2b rpm already correct per audit).

**Pre-m197 wire shape** (broken per purl-spec): `pkg:deb/debian/foo@1:2.0-r0`
**Post-m197 wire shape** (canonical per purl-spec): `pkg:deb/debian/foo@2.0-r0?epoch=1`

**Presence rules**:

- Epoch qualifier appears ONLY when the source `Version:` field has an epoch prefix (`<digits>:`). Non-epoch versions emit unchanged.
- Explicit `epoch=0` (unusual but valid) IS emitted per Edge Case discussion in spec.

**Consumer contract**:

- Downstream vuln-lookup pipelines that already handle purl-spec `?epoch=` correctly (osv.dev, deps.dev, Grype) work unchanged with m197 output.
- Consumers that were parsing the m197-pre inline-epoch form need updating; the pre-m197 form was purl-spec-non-conformant and shouldn't have been consumed by conformant scanners in the first place.

## Rotated: Versionless PURL canonical form (US3)

**Emission location**: Composer / dart / cocoapods / scala / haskell / erlang reader components with an absent version.

**Pre-m197 wire shape** (broken): `pkg:composer/vendor/pkg@` (trailing `@`)
**Post-m197 wire shape** (canonical per purl-spec): `pkg:composer/vendor/pkg` (no `@`)

**Presence rules**:

- Applies only to components with EMPTY version. Versioned emission unchanged.
- The 5 m191-fixed ecosystems (npm/cargo/maven/gem/pip) are unaffected — m191 already delivered this shape.

## Non-goals (what m197 does NOT change)

- No changes to the Purl newtype's parse / serialize algorithms — the versionless canonical is already correct; per-ecosystem builder helpers just weren't calling the versionless path.
- No changes to file-tier emission.
- No changes to non-reconciler-survivor components' annotations. Only survivors get the E1/E2/E3 shape changes.
- No new `mikebom:*` annotations beyond `mikebom:declared-as`. The rotations of `requirement-range` / `source-manifest` are field-shape changes, not new fields.
