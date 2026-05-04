# Contract: maven main-module component placement per format

Same per-format placement as cargo (064) / npm (066) / pip (068) / gem (069). Inherits multi-main-module super-root + plural-DESCRIBES from #127. PURL prefix is `pkg:maven/<groupId>/<artifactId>@<version>` per the existing `build_maven_purl` helper (using `/` separators per [PURL spec](https://github.com/package-url/purl-spec); the `:` form from #104's free text was disambiguated to `/` per spec A11).

## CycloneDX 1.6

### Single-module Maven project

```json
{
  "metadata": {
    "component": {
      "bom-ref": "pkg:maven/com.example/my-app@1.2.3",
      "type": "application",
      "name": "my-app",
      "version": "1.2.3",
      "purl": "pkg:maven/com.example/my-app@1.2.3",
      "properties": [
        { "name": "mikebom:component-role", "value": "main-module" },
        { "name": "mikebom:sbom-tier", "value": "source" }
      ]
    }
  }
}
```

### Multi-module reactor (parent + 2 submodules)

```json
{
  "metadata": {
    "component": {
      "bom-ref": "<scan-target>@0.0.0",
      "type": "application",
      "name": "<scan-target>",
      "purl": "pkg:generic/..."
    }
  },
  "components": [
    {
      "bom-ref": "pkg:maven/com.example/parent@1.0.0",
      "type": "application",
      "purl": "pkg:maven/com.example/parent@1.0.0",
      "properties": [{ "name": "mikebom:component-role", "value": "main-module" }, ...]
    },
    {
      "bom-ref": "pkg:maven/com.example/module-a@1.0.0",
      "type": "application",
      "purl": "pkg:maven/com.example/module-a@1.0.0",
      ...
    },
    {
      "bom-ref": "pkg:maven/com.example/module-b@1.0.0",
      "type": "application",
      "purl": "pkg:maven/com.example/module-b@1.0.0",
      ...
    }
  ]
}
```

## SPDX 2.3

```json
{
  "documentDescribes": [
    "SPDXRef-Package-pkg-maven-com-example-module-a-1-0-0",
    "SPDXRef-Package-pkg-maven-com-example-module-b-1-0-0",
    "SPDXRef-Package-pkg-maven-com-example-parent-1-0-0"
  ],
  "packages": [
    {
      "SPDXID": "SPDXRef-Package-pkg-maven-com-example-parent-1-0-0",
      "name": "parent",
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
          "referenceLocator": "pkg:maven/com.example/parent@1.0.0" }
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
      "rootElement": [
        "https://mikebom.kusari.dev/spdx3/doc-.../pkg-module-a",
        "https://mikebom.kusari.dev/spdx3/doc-.../pkg-module-b",
        "https://mikebom.kusari.dev/spdx3/doc-.../pkg-parent"
      ],
      ...
    }
  ]
}
```

## Property substitution behavior

Per FR-012, the following substitution patterns resolve at PURL emission:

| Pattern | Resolved from |
|---------|---------------|
| `${project.groupId}` | self POM's `<groupId>` (or inherited from `<parent>`) |
| `${project.artifactId}` | self POM's `<artifactId>` |
| `${project.version}` | self POM's `<version>` (or inherited from `<parent>`) |
| `${parent.groupId}` | self POM's `<parent>/<groupId>` |
| `${parent.version}` | self POM's `<parent>/<version>` |
| `${revision}` | self or parent POM's `<properties>/<revision>` (Maven flatten plugin convention) |
| Custom keys | self POM's `<properties>` (preferred) or parent POM's `<properties>` (inherited) |

Unresolved properties pass through verbatim with a `tracing::warn!` log.

## POM inheritance behavior

When a child POM's `<groupId>` or `<version>` is absent, the resolution ladder:

1. If `<parent>` block is present in the child POM, use the `<parent>/<groupId>` and `<parent>/<version>` values literally (always complete per Maven's specification — `<parent>` requires a full GAV).
2. If `<parent>` is absent AND either `<groupId>` or `<version>` is missing, the POM is invalid; skip emission silently.

## Multi-module reactor behavior

Parent POM's `<modules>/<module>` elements list relative subdirectories, each containing a child `pom.xml`. The walker discovers these and emits one main-module per resolved child. The parent itself emits a main-module if its own GAV is complete. Bare aggregator parents (only `<modules>`, no own GAV) emit no parent main-module per the spec Edge Cases bullet.

## Same-PURL collision behavior

Same as cargo/npm/pip/gem: first-discovered wins, `tracing::warn!` lists drops. Divergent-PURL detection deferred to #125.

## Cross-format invariants

- PURL byte-identical across all 3 formats.
- C40 supplementary tag in all 3 formats.
- Multi-module reactors produce length-N `documentDescribes` / `rootElement` arrays.

## Does NOT change

- No new property/annotation key.
- No new SPDX `primaryPackagePurpose` enum value.
- No new CDX component `type`.
- No CLI flag changes.
