# Contract: SBOM Emission Shape (m190)

**Date**: 2026-07-13
**Scope**: The exact wire-shape (JSON key names, value formats) each of the three SBOM formats MUST produce for ipk components after m190. Byte-level contract; used by the integration tests to assert conformance.

## Test fixture reference

For the shape examples below, assume a synthetic ipk with these parsed fields:

| Field | Value |
|---|---|
| `Package:` | `mikebom-fixture-compound` |
| `Version:` | `1:2.0-r0` |
| `Architecture:` | `all` |
| `License:` | `GPL-2.0-only & MIT` |

Expected post-m190 emissions:

## CycloneDX 1.6 (issue #550 fix)

```json
{
  "type": "library",
  "name": "mikebom-fixture-compound",
  "version": "2.0-r0",
  "purl": "pkg:opkg/mikebom-fixture-compound@2.0-r0?arch=all&epoch=1",
  "licenses": [
    {
      "expression": "GPL-2.0-only AND MIT"
    }
  ],
  "evidence": { /* ... existing evidence.occurrences fields, unchanged ... */ }
}
```

**Key shape assertions**:

- `.purl` ends with `&epoch=1` (SC-003).
- `.purl` contains no `@1:2.0-r0` inline-epoch substring.
- `.version == "2.0-r0"` (naked, no `1:` prefix).
- `.licenses[0].expression == "GPL-2.0-only AND MIT"` (SPDX-canonical; SC-001).
- `.licenses[0].expression` contains no raw `&` or `|` operator characters (SC-001).
- Qualifier ordering: `arch` before `epoch` (alphabetical per purl-spec §5.6).

## SPDX 2.3 (parity with existing behavior)

```json
{
  "SPDXID": "SPDXRef-Package-…",
  "name": "mikebom-fixture-compound",
  "versionInfo": "2.0-r0",
  "supplier": "NOASSERTION",
  "downloadLocation": "NOASSERTION",
  "filesAnalyzed": false,
  "licenseDeclared": "GPL-2.0-only AND MIT",
  "licenseConcluded": "NOASSERTION",
  "copyrightText": "NOASSERTION",
  "externalRefs": [
    {
      "referenceCategory": "PACKAGE-MANAGER",
      "referenceType": "purl",
      "referenceLocator": "pkg:opkg/mikebom-fixture-compound@2.0-r0?arch=all&epoch=1"
    }
  ]
}
```

**Key shape assertions**:

- `.licenseDeclared == "GPL-2.0-only AND MIT"` — SPDX-canonical form (post-m190, no longer a `LicenseRef-<hex>` fallback for this input; see research §R1).
- `.versionInfo == "2.0-r0"` (naked).
- `.externalRefs[?(@.referenceType=='purl')].referenceLocator` matches the CDX `.purl` byte-for-byte (FR-013).

## SPDX 3.0.1 (issue #551 fix)

Two elements added to `@graph`:

```json
{
  "type": "software_Package",
  "spdxId": "…/pkg/mikebom-fixture-compound-…",
  "creationInfo": "…",
  "name": "mikebom-fixture-compound",
  "software_packageVersion": "2.0-r0",
  "software_packageUrl": "pkg:opkg/mikebom-fixture-compound@2.0-r0?arch=all&epoch=1"
}
```

```json
{
  "type": "simplelicensing_LicenseExpression",
  "spdxId": "…/license-decl-<hash>",
  "creationInfo": "…",
  "simplelicensing_licenseExpression": "GPL-2.0-only AND MIT"
}
```

And a `Relationship` element linking the two:

```json
{
  "type": "Relationship",
  "spdxId": "…/rel-<hash>",
  "creationInfo": "…",
  "from": "…/pkg/mikebom-fixture-compound-…",
  "relationshipType": "hasDeclaredLicense",
  "to": ["…/license-decl-<hash>"]
}
```

**Key shape assertions**:

- The `software_Package` element for the ipk has a `hasDeclaredLicense` relationship pointing at a `simplelicensing_LicenseExpression` element (SC-002).
- The `LicenseExpression` value is SPDX-canonical (no raw `&`/`|`).
- Empty license input → NO `hasDeclaredLicense` relationship + NO `simplelicensing_LicenseExpression` element for that Package (Q3 answer B for SPDX 3).
- `software_packageUrl` matches the CDX `.purl` byte-for-byte (FR-013).

## Vendor-license shape (US2 acceptance #3)

For a fixture with `License: SomeVendorLicense`, the emissions become:

**CDX 1.6**: `.licenses[0].expression` contains the vendor operand (potentially wrapped as `LicenseRef-<hex>` via the m152 fallback if `try_canonical` cannot recognize the operand):

```json
{"licenses": [{"expression": "LicenseRef-<hex>"}]}
```

**SPDX 2.3**: `.licenseDeclared == "LicenseRef-<hex>"` + `hasExtractedLicensingInfos[]` entry describing the LicenseRef (via existing m153 sweep).

**SPDX 3**: `simplelicensing_LicenseExpression` element with value `LicenseRef-<hex>` + a `simplelicensing_CustomLicense` element defining the LicenseRef (via existing m154 sweep at `v3_licenses.rs::sweep_custom_licenses`).

## Empty-license shape (Q3 answer B)

For a fixture with `License:` missing or empty:

**CDX 1.6**: `.licenses` field omitted OR emitted as `[]` — whichever the current builder does; both are CDX-legal.

**SPDX 2.3**: `.licenseDeclared == "NOASSERTION"` — SPDX-standard sentinel.

**SPDX 3**: NO `simplelicensing_LicenseExpression` element AND NO `hasDeclaredLicense` relationship for that Package.

## Non-epoch shape (byte-identity gate for SC-006)

For a fixture with `Version: 2.0-r0` (no `<digits>:` prefix) AND `License: MIT` (single SPDX-canonical operand):

- CDX `.purl`, SPDX 2.3 `externalRefs[].referenceLocator`, SPDX 3 `software_packageUrl`: all three MUST be byte-identical to the pre-m190 output for the same input.
- CDX `.licenses[0].expression == "MIT"`, SPDX 2.3 `.licenseDeclared == "MIT"`, SPDX 3 `simplelicensing_licenseExpression == "MIT"`: byte-identical to pre-m190.

This is the byte-identity safety net for the milestone — any diff on a no-epoch, single-license golden indicates a regression.

## Cross-format parity gate (FR-013)

For every ipk fixture in the test corpus, the following MUST hold:

- CDX `.components[X].licenses[].expression` normalized via `SpdxExpression::try_canonical` equals SPDX 2.3 `.packages[X].licenseDeclared` normalized via `SpdxExpression::try_canonical` equals SPDX 3 `simplelicensing_licenseExpression` value normalized via `SpdxExpression::try_canonical`.
- CDX `.components[X].purl` equals SPDX 2.3 `.packages[X].externalRefs[?(@.referenceType=='purl')].referenceLocator` equals SPDX 3 `software_Package.software_packageUrl` for the corresponding component (`X` maps by name/version).

The integration test `ipk_license_parity.rs` (US1+US2 combined coverage) is the canonical enforcer.
