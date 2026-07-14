# Contract: SBOM Emission Shape (m191)

**Date**: 2026-07-14
**Scope**: Byte-level wire shapes for the three formats after m191 reconciliation. Used by integration tests to assert conformance.

## Fixture references

For the shape examples below, assume:

**Fixture A (reconciled pair)** — npm project:
- `package.json` declares `commander: "^11.1.0"`
- `package-lock.json` resolves `commander` to `11.1.0`
- Expected post-m191: exactly ONE `commander` component.

**Fixture B (multi-declaration reconciled)** — npm workspace:
- `packages/foo/package.json` declares `commander: "^11.0"`
- `packages/bar/package.json` declares `commander: "^11.1.0"`
- Root `package-lock.json` resolves both to `commander@11.1.0`
- Expected post-m191: ONE `commander` component with TWO `mikebom:requirement-range` entries, each paired with distinct `mikebom:source-manifest`.

**Fixture C (standalone design-tier — declared but not installed)** — npm project:
- `package.json` declares `optional-dep: "^1.0.0"`
- `package-lock.json` has NO entry for `optional-dep`
- Expected post-m191: ONE standalone design-tier component with versionless PURL.

## Fixture A — CycloneDX 1.6

```json
{
  "type": "library",
  "bom-ref": "pkg:npm/commander@11.1.0",
  "name": "commander",
  "version": "11.1.0",
  "purl": "pkg:npm/commander@11.1.0",
  "properties": [
    { "name": "mikebom:sbom-tier",         "value": "source" },
    { "name": "mikebom:requirement-range", "value": "^11.1.0" },
    { "name": "mikebom:source-manifest",   "value": "package.json" }
  ]
}
```

Assertions:
- Count of `.components[?(@.name=='commander')]` == 1 (down from 2 pre-m191).
- `.purl` and `.bom-ref` unchanged relative to pre-m191 source-tier component (byte-identity of the source-tier ID per FR-008).
- `.properties` includes `mikebom:requirement-range` AND `mikebom:source-manifest` (transferred from the removed design-tier component).
- NO `pkg:npm/commander@` (trailing @) appears anywhere in the emitted CDX.

## Fixture A — SPDX 2.3

```json
{
  "SPDXID": "SPDXRef-Package-commander-...",
  "name": "commander",
  "versionInfo": "11.1.0",
  "externalRefs": [
    { "referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl",
      "referenceLocator": "pkg:npm/commander@11.1.0" }
  ],
  "annotations": [
    { "annotator": "Tool: mikebom",
      "annotationDate": "…",
      "annotationType": "OTHER",
      "comment": "{\"mikebom:requirement-range\":\"^11.1.0\",\"mikebom:source-manifest\":\"package.json\"}" }
  ]
}
```

Assertions:
- Exactly ONE `commander` package. `versionInfo` is the resolved version.
- Annotation carries the transferred design-tier fields per m111 JSON-in-comment envelope convention.

## Fixture A — SPDX 3.0.1

```json
{
  "type": "software_Package",
  "spdxId": "…/pkg/commander-…",
  "name": "commander",
  "software_packageVersion": "11.1.0",
  "software_packageUrl": "pkg:npm/commander@11.1.0"
}
```

Plus separate `Annotation` graph elements carrying the transferred design-tier metadata.

## Fixture B — CycloneDX (multi-declaration)

```json
{
  "type": "library",
  "bom-ref": "pkg:npm/commander@11.1.0",
  "name": "commander",
  "version": "11.1.0",
  "purl": "pkg:npm/commander@11.1.0",
  "properties": [
    { "name": "mikebom:sbom-tier",         "value": "source" },
    { "name": "mikebom:requirement-range", "value": "^11.0" },
    { "name": "mikebom:source-manifest",   "value": "packages/foo/package.json" },
    { "name": "mikebom:requirement-range", "value": "^11.1.0" },
    { "name": "mikebom:source-manifest",   "value": "packages/bar/package.json" }
  ]
}
```

Assertions (per Q1 answer B / FR-004):
- TWO `mikebom:requirement-range` property entries.
- TWO `mikebom:source-manifest` entries.
- Insertion order preserves the range-→-manifest pairing: entry[0] pairs with entry[1], entry[2] with entry[3].
- NO JSON-array-encoded single value.

## Fixture C — CycloneDX (standalone versionless)

```json
{
  "type": "library",
  "bom-ref": "pkg:npm/optional-dep",
  "name": "optional-dep",
  "purl": "pkg:npm/optional-dep",
  "properties": [
    { "name": "mikebom:sbom-tier",         "value": "design" },
    { "name": "mikebom:requirement-range", "value": "^1.0.0" },
    { "name": "mikebom:source-manifest",   "value": "package.json" }
  ]
}
```

Assertions (per US2 / FR-009 / FR-010):
- `.purl` == `"pkg:npm/optional-dep"` (no `@`).
- `.version` field is ABSENT from the JSON entirely — not `""`.
- `.bom-ref` == `"pkg:npm/optional-dep"` (PURL-as-bom-ref per Q3 answer A / FR-013).

## Fixture C — SPDX 2.3

```json
{
  "SPDXID": "SPDXRef-Package-optional-dep-...",
  "name": "optional-dep",
  "versionInfo": "NOASSERTION",
  "externalRefs": [
    { "referenceCategory": "PACKAGE-MANAGER", "referenceType": "purl",
      "referenceLocator": "pkg:npm/optional-dep" }
  ]
}
```

Assertions (per FR-011):
- `versionInfo` == `"NOASSERTION"` (SPDX 2.3 convention for absent version).
- `externalRefs[].referenceLocator` == `"pkg:npm/optional-dep"` (no `@`).

## Fixture C — SPDX 3.0.1

```json
{
  "type": "software_Package",
  "spdxId": "…/pkg/optional-dep-…",
  "name": "optional-dep",
  "software_packageUrl": "pkg:npm/optional-dep"
}
```

Assertions (per FR-012):
- NO `software_packageVersion` property in the graph element (omitted, not empty).
- `software_packageUrl` == `"pkg:npm/optional-dep"`.

## Cross-format PURL parity (FR-015)

For every fixture, the following MUST hold byte-for-byte across the three format outputs:

- CDX `.components[X].purl` == SPDX 2.3 `.packages[X].externalRefs[?(@.referenceType=='purl')].referenceLocator` == SPDX 3 `software_packageUrl`

For Fixture C (versionless): all three equal `"pkg:npm/optional-dep"`.
For Fixture A/B (reconciled): all three equal `"pkg:npm/commander@11.1.0"`.

## Dependency-graph edge rewriting (FR-005)

**Given** a pre-m191 pipe carrying:
```
componentX --[dependsOn]--> designTierCommander (bom-ref: pkg:npm/commander@)
componentX --[dependsOn]--> sourceTierCommander (bom-ref: pkg:npm/commander@11.1.0)
```

**After m191 reconciliation**:
```
componentX --[dependsOn]--> sourceTierCommander (bom-ref: pkg:npm/commander@11.1.0)   # single edge
```

- The edge target that pointed at the removed design-tier component (`pkg:npm/commander@`) MUST be rewritten to point at the surviving source-tier component (`pkg:npm/commander@11.1.0`).
- If componentX already had BOTH edges pre-m191 (rare but possible), the deduped edge collapses to one (no duplicate `dependsOn` entries).
- NO edge dangles pointing at a removed bom-ref.

## Byte-identity regression gate (SC-006)

For every existing golden that has NO design/source pairs AND NO trailing-`@` PURLs:

- CDX `.components`, `.dependencies`, `.metadata` — byte-identical to pre-m191 output.
- SPDX 2.3 `.packages`, `.relationships`, `.creationInfo` — byte-identical.
- SPDX 3 `@graph` — byte-identical.

This is the safety net for the milestone. Any diff on such a golden indicates a regression and MUST be investigated before merge.
