# Contract: pip main-module component placement per format

Same per-format placement as cargo (064) and npm (066). Inherits multi-main-module super-root + plural-DESCRIBES from #127. Only differences from cargo/npm: PURL prefix is `pkg:pypi/...` and the `name` field undergoes PEP 503 normalization before encoding into the PURL.

## CycloneDX 1.6

### Single pip main-module (single-project scan)

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:pypi/my-pkg@1.0.0",
      "type": "application",
      "name": "my-pkg",
      "version": "1.0.0",
      "purl": "pkg:pypi/my-pkg@1.0.0",
      "properties": [
        { "name": "mikebom:component-role", "value": "main-module" },
        { "name": "mikebom:sbom-tier", "value": "source" }
      ]
    }
  }
}
```

### Name normalization

Manifest declares `name = "My_Package.Name"`:

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:pypi/my-package-name@1.0.0",
      "type": "application",
      "name": "My_Package.Name",  // verbatim display value
      "version": "1.0.0",
      "purl": "pkg:pypi/my-package-name@1.0.0"  // PEP 503-normalized
    }
  }
}
```

### Editable install (FR-011)

After Phase-A emission and venv-merge:

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:pypi/my-pkg@1.0.0",
      "type": "application",
      "name": "my-pkg",
      "version": "1.0.0",
      "purl": "pkg:pypi/my-pkg@1.0.0",
      "properties": [
        { "name": "mikebom:component-role", "value": "main-module" },
        { "name": "mikebom:sbom-tier", "value": "deployed" }   // venv wins
      ],
      "hashes": [...]   // from venv .dist-info METADATA
    }
  }
}
```

## SPDX 2.3

```json
{
  "documentDescribes": ["SPDXRef-Package-pkg-pypi-my-pkg-1-0-0"],
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-pkg-pypi-my-pkg-1-0-0",
      "name": "my-pkg",
      "versionInfo": "1.0.0",
      "primaryPackagePurpose": "APPLICATION",
      "annotations": [
        { "annotator": "Tool: mikebom-...",
          "annotationType": "OTHER",
          "comment": "{\"mikebom:component-role\": \"main-module\"}" }
      ],
      "externalRefs": [
        { "referenceCategory": "PACKAGE-MANAGER",
          "referenceType": "purl",
          "referenceLocator": "pkg:pypi/my-pkg@1.0.0" }
      ]
    }
  ]
}
```

## SPDX 3.0.1

```json
{
  "@graph": [
    {
      "type": "SpdxDocument",
      "rootElement": ["https://mikebom.kusari.dev/spdx3/doc-.../pkg-..."],
      ...
    },
    {
      "type": "software_Package",
      "name": "my-pkg",
      "software_packageVersion": "1.0.0",
      "software_primaryPurpose": "application",
      "software_packageUrl": "pkg:pypi/my-pkg@1.0.0",
      ...
    }
  ]
}
```

## Same-PURL collision behavior

Same as cargo / npm: first-discovered wins, `tracing::warn!` lists drops, divergent-PURL detection deferred to #125.

## Cross-format invariants

- PURL byte-identical across CDX / SPDX 2.3 / SPDX 3.
- PEP 503 normalization applied consistently.
- C40 supplementary tag in all 3 formats.

## Does NOT change

- No new property/annotation key.
- No new SPDX `primaryPackagePurpose` enum value.
- No new CDX component `type`.
- No CLI flag changes.
