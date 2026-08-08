# Contract: NuGet main-module emission shape

**Feature**: 230-nuget-main-module
**Phase**: 1
**Audience**: Downstream SBOM consumers (verify-blob users, VEX analyzers, license auditors) and future waybill contributors extending sibling ecosystems.

waybill's public "interface" for SBOM emitters is the shape of the emitted CycloneDX / SPDX 2.3 / SPDX 3 documents. This contract records the shape a NuGet scan's main-module additions take in each supported format, plus the invariants downstream consumers may rely on.

## CycloneDX 1.6 shape

For each `.csproj` / `.vbproj` / `.fsproj` discovered:

```jsonc
{
  "components": [
    // ... existing package-level NuGet components unchanged ...
    {
      "type": "application",
      "bom-ref": "pkg:nuget/eShop.ServiceDefaults@1.0.0",
      "name": "eShop.ServiceDefaults",
      "version": "1.0.0",
      "purl": "pkg:nuget/eShop.ServiceDefaults@1.0.0",
      "properties": [
        { "name": "waybill:component-role", "value": "main-module" },
        { "name": "waybill:sbom-tier", "value": "source" }
      ],
      "evidence": {
        "identity": [
          {
            "field": "purl",
            "confidence": 1.0,
            "methods": [
              { "technique": "manifest-analysis", "confidence": 1.0,
                "value": "src/eShop.ServiceDefaults/eShop.ServiceDefaults.csproj" }
            ]
          }
        ]
      }
    }
  ],
  "dependencies": [
    {
      "ref": "pkg:nuget/eShop.ServiceDefaults@1.0.0",
      "dependsOn": [
        "pkg:nuget/OpenTelemetry.Exporter.OpenTelemetryProtocol@1.9.0",
        "pkg:nuget/Microsoft.Extensions.Http.Resilience@8.10.0"
      ]
    }
  ]
}
```

Invariants:
- `type: "application"` matches the existing cargo (m064) / gem (m069) / Gemfile (m216) main-module convention. Consumers walking the graph from a root PURL find these via `type: "application"`.
- `bom-ref` and `purl` are identical (existing waybill convention).
- `waybill:component-role: "main-module"` is the primary signal downstream consumers use to identify the component as a project root.

## SPDX 2.3 shape

For each project file:

```jsonc
{
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-<hash>",
      "name": "eShop.ServiceDefaults",
      "versionInfo": "1.0.0",
      "downloadLocation": "NOASSERTION",
      "filesAnalyzed": false,
      "supplier": "NOASSERTION",
      "externalRefs": [
        {
          "referenceCategory": "PACKAGE-MANAGER",
          "referenceType": "purl",
          "referenceLocator": "pkg:nuget/eShop.ServiceDefaults@1.0.0"
        }
      ],
      "annotations": [
        {
          "annotator": "Tool: waybill",
          "annotationType": "OTHER",
          "annotationDate": "2026-08-07T18:00:00Z",
          "comment": "{\"waybill:component-role\":\"main-module\"}"
        }
      ]
    }
  ],
  "relationships": [
    {
      "spdxElementId": "SPDXRef-DOCUMENT",
      "relatedSpdxElement": "SPDXRef-Package-<hash>",
      "relationshipType": "DESCRIBES"
    },
    {
      "spdxElementId": "SPDXRef-Package-<hash>",
      "relatedSpdxElement": "SPDXRef-Package-<opentelemetry-exporter-hash>",
      "relationshipType": "DEPENDS_ON"
    }
  ]
}
```

Invariants:
- The main-module `SPDXID` appears as the `spdxElementId` of `DEPENDS_ON` relationships whose `relatedSpdxElement` is a direct dependency.
- If the main-module is the document's subject (determined by m127 root selection), a `DESCRIBES` relationship links `SPDXRef-DOCUMENT` to the main-module.
- The `waybill:component-role` annotation is carried in the packed JSON annotation-comment envelope (existing waybill SPDX 2.3 convention per parity catalog row C40).

## SPDX 3.0.1 shape

For each project file:

```jsonc
{
  "@graph": [
    {
      "@type": "software_Package",
      "spdxId": "spdx:nuget/eShop.ServiceDefaults@1.0.0",
      "name": "eShop.ServiceDefaults",
      "software_packageVersion": "1.0.0",
      "software_packageUrl": "pkg:nuget/eShop.ServiceDefaults@1.0.0",
      "extension": [
        {
          "@type": "extension_CdxPropertiesExtension",
          "cdxproperties_cdxProperty": [
            { "cdxproperties_name": "waybill:component-role",
              "cdxproperties_value": "main-module" }
          ]
        }
      ]
    },
    {
      "@type": "Relationship",
      "relationshipType": "dependsOn",
      "from": "spdx:nuget/eShop.ServiceDefaults@1.0.0",
      "to": ["spdx:nuget/OpenTelemetry.Exporter.OpenTelemetryProtocol@1.9.0"]
    }
  ]
}
```

Invariants:
- `software_Package` node carrying the main-module PURL, tied to direct-dep `Relationship` nodes via SPDX 3's `from`/`to` predicates.
- The extension-namespaced `waybill:component-role` property carries the same signal as CDX 1.6 and SPDX 2.3.

## Graph-completeness annotation shape (SC-004)

Post-230, the document-scope `waybill:graph-completeness-reason` annotation for a locked NuGet-only scan MUST NOT contain the substring `multi-ecosystem-partial-root: nuget`. The reason may be absent entirely (if no other classifier fires) or may contain other reason codes (e.g., `transitive-edges-unresolvable: npm` when the scan also has an npm subtree).

## Byte-parity boundary (FR-006 / SC-003)

The pre-230 goldens at `specs/audit-nuget-realworld/artifacts/*.waybill.cdx.json` establish the byte-parity baseline for existing package-level components. Post-230:

- **UNCHANGED**: every `components[].purl` present pre-230 remains present with identical `name`, `version`, `properties`, `evidence`, and `type` fields.
- **ADDED**: one new `components[]` entry per discovered project file, matching the CDX shape above.
- **ADDED**: one or more new `dependencies[]` entries whose `ref` matches a new main-module PURL and whose `dependsOn[]` lists reference existing package-level components' PURLs.
- **UNCHANGED**: every pre-230 `dependencies[]` entry remains present with identical `ref` and `dependsOn`.

Diff testing masks noise per memory `feedback_verify_golden_churn_normalized` (content-addressed IDs, serial numbers, timestamps).
