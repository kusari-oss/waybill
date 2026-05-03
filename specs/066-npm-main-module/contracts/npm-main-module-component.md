# Contract: npm main-module component placement per format

This contract specifies the per-format placement of npm main-module component(s) in the SBOM output. It parallels milestone 064's `cargo-main-module-component.md` and inherits the same multi-main-module super-root + plural-DESCRIBES infrastructure shipped in #127. Only the PURL prefix (`pkg:npm/...`) and scoped-name encoding differ.

## CycloneDX 1.6

### Single npm main-module (single-package scan)

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:npm/express@4.18.2",
      "type": "application",
      "name": "express",
      "version": "4.18.2",
      "purl": "pkg:npm/express@4.18.2",
      "properties": [
        { "name": "mikebom:component-role", "value": "main-module" },
        { "name": "mikebom:sbom-tier", "value": "source" }
      ]
    }
  },
  "components": [
    /* npm main-module is NOT here — exclusively in metadata.component */
    { "bom-ref": "pkg:npm/accepts@...", ... },
    ...
  ]
}
```

**Key invariants** (same as cargo / Go):
- `metadata.component.type` MUST be `"application"`.
- `metadata.component.bom-ref` MUST equal the npm main-module's PURL.
- The same PURL MUST NOT appear in `components[]`.
- C40 supplementary tag in `metadata.component.properties[]`.

### Scoped main-module

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:npm/%40kusari/foo@1.0.0",
      "type": "application",
      "name": "@kusari/foo",
      "version": "1.0.0",
      "purl": "pkg:npm/%40kusari/foo@1.0.0",
      ...
    }
  }
}
```

**Key invariants**:
- `name` field is the verbatim manifest name (`@kusari/foo`, including the `@` sigil).
- `purl` field URL-encodes the `@` to `%40` per PURL spec.
- `bom-ref` matches `purl`.

### Multiple npm main-modules (workspace scan)

When the scan contains an npm 7+ workspace with N member packages, `metadata.component` becomes the synthetic super-root and each main-module appears as a regular `components[]` entry — identical to cargo workspace handling per milestone 064 + #127.

```json
{
  "metadata": {
    "component": {
      "bom-ref": "<scan-target-name>@0.0.0",
      "type": "application",
      "name": "<scan-target-name>",
      "purl": "pkg:generic/...@0.0.0"
    }
  },
  "components": [
    {
      "bom-ref": "pkg:npm/a@0.5.0",
      "type": "application",
      "name": "a",
      "purl": "pkg:npm/a@0.5.0",
      "properties": [{ "name": "mikebom:component-role", "value": "main-module" }, ...]
    },
    {
      "bom-ref": "pkg:npm/b@0.5.0",
      "type": "application",
      ...
    }
  ],
  "dependencies": [
    {
      "ref": "<scan-target-name>@0.0.0",
      "dependsOn": ["pkg:npm/a@0.5.0", "pkg:npm/b@0.5.0"]
    },
    {
      "ref": "pkg:npm/b@0.5.0",
      "dependsOn": ["pkg:npm/a@0.5.0"]
    }
  ]
}
```

## SPDX 2.3

### Single or multiple npm main-modules

```json
{
  "spdxVersion": "SPDX-2.3",
  "documentDescribes": [
    "SPDXRef-Package-pkg-npm-a-0-5-0",
    "SPDXRef-Package-pkg-npm-b-0-5-0"
  ],
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-pkg-npm-a-0-5-0",
      "name": "a",
      "versionInfo": "0.5.0",
      "primaryPackagePurpose": "APPLICATION",
      "annotations": [
        {
          "annotator": "Tool: mikebom-...",
          "annotationType": "OTHER",
          "comment": "{\"mikebom:component-role\": \"main-module\"}"
        }
      ],
      "externalRefs": [
        { "referenceCategory": "PACKAGE-MANAGER",
          "referenceType": "purl",
          "referenceLocator": "pkg:npm/a@0.5.0" }
      ]
    }
  ],
  "relationships": [
    { "spdxElementId": "SPDXRef-DOCUMENT",
      "relatedSpdxElement": "SPDXRef-Package-pkg-npm-a-0-5-0",
      "relationshipType": "DESCRIBES" },
    { "spdxElementId": "SPDXRef-DOCUMENT",
      "relatedSpdxElement": "SPDXRef-Package-pkg-npm-b-0-5-0",
      "relationshipType": "DESCRIBES" }
  ]
}
```

**Key invariants**:
- Every npm main-module package has `primaryPackagePurpose: "APPLICATION"`.
- `documentDescribes[]` lists every npm main-module's SPDXID, sorted by SPDXID for cross-host determinism.
- One `SPDXRef-DOCUMENT DESCRIBES <main-module>` relationship per main-module.
- C40 annotation envelope on each main-module package.

## SPDX 3.0.1

### Single or multiple npm main-modules

```json
{
  "@graph": [
    {
      "type": "SpdxDocument",
      "rootElement": [
        "https://mikebom.kusari.dev/spdx3/doc-.../pkg-...a",
        "https://mikebom.kusari.dev/spdx3/doc-.../pkg-...b"
      ],
      ...
    },
    {
      "type": "software_Package",
      "spdxId": "https://mikebom.kusari.dev/spdx3/doc-.../pkg-...a",
      "name": "a",
      "software_packageVersion": "0.5.0",
      "software_primaryPurpose": "application",
      ...
    },
    {
      "type": "Relationship",
      "relationshipType": "describes",
      "from": "https://mikebom.kusari.dev/spdx3/doc-...",
      "to": ["https://mikebom.kusari.dev/spdx3/doc-.../pkg-...a"]
    }
  ]
}
```

**Key invariants**:
- Every npm main-module element has `software_primaryPurpose: "application"`.
- `rootElement[]` is plural; lists every npm main-module IRI sorted alphabetically.
- One `describes` Relationship per main-module IRI (per #127).

## Same-PURL collision behavior

Same as cargo (milestone 064 spec Q1):
1. Exactly one main-module emitted per PURL (deterministic first-discovered-wins).
2. First-discovered crate's outgoing direct-dep set is retained.
3. Single consolidated `tracing::warn!` lists all dropped duplicates.
4. SBOM bytes do not encode the dedup occurrence.
5. Divergent-PURL detection deferred to issue #125 (covers npm too).

## Cross-format invariants

- PURL emitted in CDX `metadata.component.purl`, SPDX `externalRefs[*].referenceLocator`, and SPDX 3 element identity MUST be byte-identical for the same scan.
- Scope encoding (`%40`) is consistent across all three formats.
- C40 role tag (`mikebom:component-role: main-module`) present in all three formats.
- Cross-format consistency tested by parity-extractor C40 path in `tests/holistic_parity.rs`.

## Does NOT change

- No new property/annotation key.
- No new SPDX `primaryPackagePurpose` enum value.
- No new CDX component `type` value.
- No new relationship type.
- No CLI flag changes.
