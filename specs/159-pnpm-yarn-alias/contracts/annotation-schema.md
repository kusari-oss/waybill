# Contract: `mikebom:pnpm-alias` / `mikebom:yarn-alias` annotations

**Milestone 159** • Wire-format contract for the two new component-scope annotations.

## CycloneDX 1.6

Location: `.components[].properties[]`

```json
{
  "components": [
    {
      "type": "library",
      "bom-ref": "pkg:npm/%40slorber/react-helmet-async@1.3.0",
      "name": "@slorber/react-helmet-async",
      "version": "1.3.0",
      "purl": "pkg:npm/%40slorber/react-helmet-async@1.3.0",
      "properties": [
        {
          "name": "mikebom:pnpm-alias",
          "value": "react-helmet-async"
        }
      ]
    }
  ]
}
```

- The property is emitted ONCE per unique local-name that reached the component (FR-012).
- The `value` is a bare string, byte-identical to the local-name as it appeared in the lockfile (FR-007). No URL-encoding, no case normalization, no envelope JSON.
- Absent from components NOT reached via an alias (FR-008 — regression-guard against false positives).

## SPDX 2.3

Location: per-package `annotations[]` (attached to the `SPDXRef-Package-*` for the alias-resolved component).

```json
{
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-npm-...",
      "name": "@slorber/react-helmet-async",
      "versionInfo": "1.3.0",
      "externalRefs": [{"referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl", "referenceLocator": "pkg:npm/%40slorber/react-helmet-async@1.3.0"}],
      "annotations": [
        {
          "annotator": "Tool: mikebom-<version>",
          "annotationDate": "<timestamp>",
          "annotationType": "OTHER",
          "comment": "mikebom:pnpm-alias=react-helmet-async"
        }
      ]
    }
  ]
}
```

Same presence rules as CDX. The `comment` field carries `<name>=<value>` — matches milestone-127/134/158 established SPDX 2.3 shape.

## SPDX 3.0.1

Location: `Annotation` element in the `@graph` array with `subject = <package IRI>` for the alias-resolved component.

```json
{
  "@graph": [
    {
      "type": "Annotation",
      "spdxId": "urn:mikebom:...",
      "annotationType": "other",
      "subject": "urn:mikebom:...:package-slorber-react-helmet-async-1.3.0",
      "statement": "mikebom:pnpm-alias=react-helmet-async",
      "creationInfo": { "@id": "..." }
    }
  ]
}
```

Same presence rules. Matches milestone-158 SPDX 3 shape.

## Multi-alias case (FR-012)

When the same resolved component is reached via TWO DIFFERENT local-names (rare monorepo case), the annotation is emitted TWICE with the same `name` field but different `value`s:

```json
"properties": [
  {"name": "mikebom:pnpm-alias", "value": "react-helmet-async"},
  {"name": "mikebom:pnpm-alias", "value": "helmet-async-shim"}
]
```

CDX 1.6 explicitly allows duplicate `name` fields in `properties[]`. SPDX 2.3 + SPDX 3 both allow multiple Annotation elements attached to the same subject.

## Value grammar

- `mikebom:pnpm-alias.value` ∈ `{<any-npm-package-name>}` — must be a valid npm dep-name (may be scoped, may contain hyphens, etc.). Byte-identical to lockfile source.
- `mikebom:yarn-alias.value` — same grammar as above, MAY include the scope prefix `@scope/name`.

## Parity (SC-010)

Two rows registered in the milestone-071 parity catalog:

- Row C106: `mikebom:pnpm-alias`
- Row C107: `mikebom:yarn-alias`

Both use `Directionality::SymmetricEqual` and `order_sensitive: false`. Enforcement: `cargo +stable test -p mikebom parity_symmetric` (part of `cargo test --workspace`).

## Format-mapping doc entries

Two new rows added to `docs/reference/sbom-format-mapping.md` (mirrors milestone-158's C104/C105 addition):

- C106 row: describes the `mikebom:pnpm-alias` semantic, CDX/SPDX/SPDX 3 emissions, and the KEEP-NO-NATIVE justification (no standards-native package-alias property exists).
- C107 row: same shape for `mikebom:yarn-alias`.
