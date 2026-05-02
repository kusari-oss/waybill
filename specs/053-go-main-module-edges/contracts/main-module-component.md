# Contract: Main-module component placement per output format

This contract defines the exact byte-shape commitment milestone 053 makes for each of mikebom's three SBOM output formats. Reviewers verify these via golden diffs; tasks reference this document for "what does the output look like."

## CycloneDX 1.6 (`--format cyclonedx-json`)

### Placement: `metadata.component`

The main-module appears as `metadata.component`, NOT as a sibling in the top-level `components[]` array.

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9",
      "type": "application",
      "name": "github.com/argoproj/argo-workflows",
      "version": "v3.3.9",
      "purl": "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9",
      "cpe": "cpe:2.3:a:mikebom:github_com_argoproj_argo_workflows:v3.3.9:*:*:*:*:*:*:*",
      "properties": [
        { "name": "mikebom:component-role", "value": "main-module" },
        { "name": "mikebom:sbom-tier", "value": "source" }
      ]
    }
  },
  "components": [
    /* DOES NOT contain the main-module entry — it lives in metadata.component */
    /* Continues to contain every go.sum-derived dep + the maven manifest entries etc. */
  ]
}
```

### Edges: `dependencies[]`

```json
{
  "dependencies": [
    {
      "ref": "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9",
      "dependsOn": [
        "pkg:golang/cloud.google.com/go@v0.100.2",
        "pkg:golang/github.com/golang/protobuf@v1.5.2",
        "pkg:golang/github.com/sirupsen/logrus@v1.8.1"
        /* ... one entry per direct require, target dangling-dropped if not in components */
      ]
    },
    /* Other entries unchanged: existing go.sum transitive edges (when GOMODCACHE populated) */
  ]
}
```

The `ref` of the new dependency record matches the `metadata.component.bom-ref`. CycloneDX 1.6 §5.4 explicitly permits `dependencies[].ref` to point at `metadata.component`.

### Validation

- `metadata.component.type == "application"` ✓
- `metadata.component.purl` is a valid PURL per the existing PURL spec validator ✓
- The main-module's `bom-ref` does NOT appear in `components[]` ✓ (verified by `tests/scan_go.rs::main_module_emitted_in_metadata_only`)
- Every direct require from the project's `go.mod` (post-`replace`/`exclude`) appears as a `dependsOn` target whose target is also present as a component ✓ (dangling targets silently dropped)

## SPDX 2.3 (`--format spdx-2.3-json`)

### Placement: `packages[]` with `primaryPackagePurpose`

SPDX 2.3 does not have a separate metadata-component slot. The main-module is a regular `packages[]` entry, distinguished by:

```json
{
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-XXXXXXXXXXXXXXXX",
      "name": "github.com/argoproj/argo-workflows",
      "versionInfo": "v3.3.9",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false,
      "licenseDeclared": "NOASSERTION",
      "licenseConcluded": "NOASSERTION",
      "primaryPackagePurpose": "APPLICATION",
      "externalRefs": [
        {
          "referenceCategory": "PACKAGE-MANAGER",
          "referenceType": "purl",
          "referenceLocator": "pkg:golang/github.com/argoproj/argo-workflows@v3.3.9"
        },
        /* CPE entry per existing convention */
      ],
      "annotations": [
        {
          "annotator": "Tool: mikebom-0.1.0-alpha.X",
          "annotationDate": "...",
          "annotationType": "OTHER",
          "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:component-role\",\"value\":\"main-module\"}"
        }
      ]
    },
    /* Other packages: every go.sum entry + maven entries unchanged */
  ]
}
```

### Document subject

```json
{
  "SPDXID": "SPDXRef-DOCUMENT",
  "name": "...",
  "documentDescribes": ["SPDXRef-Package-XXXXXXXXXXXXXXXX"],
  "relationships": [
    {
      "spdxElementId": "SPDXRef-DOCUMENT",
      "relatedSpdxElement": "SPDXRef-Package-XXXXXXXXXXXXXXXX",
      "relationshipType": "DESCRIBES"
    },
    {
      "spdxElementId": "SPDXRef-Package-XXXXXXXXXXXXXXXX",
      "relatedSpdxElement": "SPDXRef-Package-CLOUDGOOGLECOMGO",
      "relationshipType": "DEPENDS_ON"
    },
    /* ... one DEPENDS_ON per direct require */
  ]
}
```

### Validation

- `packages[].primaryPackagePurpose == "APPLICATION"` for exactly the main-module entry; all other packages either omit the field OR (in future milestones) carry their own ecosystem-appropriate purpose ✓
- `documentDescribes[]` contains the main-module's SPDXID ✓
- The `SPDXRef-DOCUMENT DESCRIBES <main-module>` relationship is present in `relationships[]` ✓
- The `mikebom:component-role: main-module` annotation is on the main-module package per the existing C40 wiring ✓

## SPDX 3.0.1 (`--format spdx-3-json`, opt-in / experimental per Constitution Principle V)

### Placement

The main-module appears as a regular Element. The document's primary subject (via the existing v3 root-component construction at `generate/spdx/v3_relationships.rs:137`) targets the main-module element.

### Native role/purpose field

SPDX 3.0.1 defines `software_primaryPurpose` on Software Package / SoftwareArtifact elements (verified at `mikebom-cli/tests/fixtures/schemas/spdx-3.0.1.json` — `$defs/prop_software_SoftwareArtifact_software_primaryPurpose`). Set `software_primaryPurpose: "application"` on the main-module element. The enum value `"application"` (lowercase, no namespace prefix) is the canonical SPDX 3.0.1 form per the schema's enum definition. No cross-check punt remains — the field is in the schema mikebom already validates against.

### Document `DESCRIBES` (or v3 equivalent)

The existing v3 relationship-builder emits a relationship from the SBOM document to its root package. Continue using this for the main-module — no structural change to v3_relationships.rs's emission, just ensure the root-package selection picks the main-module when present (same algorithm as SPDX 2.3, since the root-selection helper is shared).

### Validation

- `softwarePurpose: "application"` (or v3-version-appropriate field) on the main-module Element when the field is present in the SPDX 3.0.1 schema ✓
- Document-level `DESCRIBES` (or v3 equivalent) targets the main-module ✓
- `mikebom:component-role: main-module` annotation present per existing v3 C40 wiring ✓

## Cross-format invariants

These hold across all three formats:

1. **PURL identity**: the main-module's PURL is byte-identical across CDX, SPDX 2.3, SPDX 3 outputs from the same scan. `pkg:golang/<module-path>@<version>`, with `<version>` resolved once per scan via the FR-001 ladder.
2. **C40 supplementary tag**: every format carries `mikebom:component-role: main-module` on the main-module via its format-appropriate construct (CDX `properties`, SPDX 2.3 annotation envelope, SPDX 3 native field per existing C40 wiring).
3. **Direct-require edge count**: identical N edges from main-module to direct-require targets across all three formats (modulo dangling-target dedup, which applies identically in all three).
4. **Cross-host byte identity**: when the test fixture has no `.git` directory, every byte of the main-module-related output (its PURL, name, version, edges, properties, document-describes pointer) is identical across linux/macos hosts because step 3 of the version ladder fires deterministically.

## Schema validation

Existing tests:
- `tests/spdx3_schema_validation.rs` — extends to cover the new `softwarePurpose` field (if implemented for SPDX 3) on the new Go fixture.
- Existing `cdx_regression_*.rs` — extends to cover the new `metadata.component.purl` (now `pkg:golang/...` for Go scans) on the new Go fixture.
- Existing `spdx_annotation_fidelity.rs` — continues to verify C40 annotation parity across all 9 ecosystem fixtures.

## Out of contract

- LICENSE detection on the main-module (issue #103) — main-module emits with empty `licenses` regardless of whether LICENSE exists at the workspace root.
- npm / cargo / maven / pip / gem main-modules (issue #104) — only Go gets a main-module in milestone 053.
- Polyglot doc-root nested-application restructure (Trivy-style) — milestone 053 keeps the existing super-root with multi-DESCRIBES; nested-application is future work tied to #104.
