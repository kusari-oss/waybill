# Contract: SBOM emission slots for CISA 2026 elements

**Feature**: 221-cisa-2026-elements-audit
**Applies to**: `waybill-cli/src/generate/cyclonedx/`,
`waybill-cli/src/generate/spdx/` (both 2.3 and 3 paths)

This contract fixes exactly which slot each CISA 2026 element lands in
across the three emitter surfaces. The coverage matrix (`docs/
cisa-2026-coverage.md`) cites this document for its Notes column;
the emitter code cites this document at each populate-site via
`// CISA 2026 § <Element> — see contracts/sbom-emission-contract.md`.

Only elements that need clarification are listed; the majority (14/17
data fields) are already emitted correctly and only need a
citation added to the coverage matrix.

---

## Elements whose emission this feature ADDS or CHANGES

### SBOM Author Signature (new — US2 / FR-007a / FR-007b / FR-008)

| Emitter | Signed with `--sign` (keyless) | Signed with `--sign-key` (static) | Unsigned (default) |
|---------|--------------------------------|-----------------------------------|--------------------|
| CDX 1.6 | `metadata.signature = <Sigstore Bundle protobuf-JSON>` | `metadata.signature = <JSF object>` | slot absent |
| SPDX 2.3 | sidecar file `<output>.sig.bundle.json` | sidecar file `<output>.sig.json` (DSSE) | no sidecar |
| SPDX 3.0.1 | sidecar file `<output>.sig.bundle.json` | sidecar file `<output>.sig.json` (DSSE) | no sidecar |

**Sidecar naming rule**: `<output>.sig.bundle.json` for keyless,
`<output>.sig.json` for static-key DSSE. If the operator supplies
`--output /path/to/scan.spdx.json`, the sidecar is
`/path/to/scan.spdx.json.sig.bundle.json` or
`/path/to/scan.spdx.json.sig.json`.

**Signing input**: The **canonical bytes** of the emitted SBOM
document. CDX canonicalization: RFC 8785 JCS applied to the entire
document *including* the `metadata.signature` slot with `value`
temporarily set to an empty string (JSF standard trick; the signer
overwrites the empty string with the base64 signature bytes after
signing the canonicalized form). SPDX canonicalization: RFC 8785
JCS applied to the full document bytes as written to `--output`,
then wrapped in DSSE PAE using the existing m006 primitives.

**Multiple output formats in one invocation**: If the operator
requests CDX and SPDX in one invocation (`--format cyclonedx-1.6
--format spdx-2.3 --format spdx-3.0.1`), each format is signed
independently — the CDX in-document signature does not cover the
SPDX bytes, and the SPDX sidecar does not cover the CDX bytes.
Each output stands alone from a signature-verification standpoint.

---

### SBOM Generation Context (existing at doc-scope for CDX, new for SPDX — US3 / FR-010 / FR-011 / FR-012)

| Emitter | Slot | Value shape |
|---------|------|-------------|
| CDX 1.6 | `metadata.lifecycles[]` (existing, m047) | Array of `{phase: <cdx-phase>}` — cdx-phase already derived from `ScanArtifacts.generation_context` per m047; no change |
| SPDX 2.3 | `Annotation` on `SPDXRef-DOCUMENT` (**NEW** doc-scope) | `annotationType: "OTHER"`, `annotator: "Tool: waybill-<version>"`, `annotationDate: <RFC3339>`, `comment: "waybill:generation-context=<native>;waybill:cisa-2026-lifecycle=<alias>"` |
| SPDX 3.0.1 | Top-level `Annotation` element with `subject: <SpdxDocument @id>` (**NEW**) | `@type: "Annotation"`, `@id: <content-addressed IRI per m011>`, `annotationType: "other"`, `statement: "waybill:generation-context=<native>;waybill:cisa-2026-lifecycle=<alias>"` |

**One annotation, two signals**: When both this element and SBOM
Version (below) require an annotation, they share a single
annotation element with semicolon-separated key=value pairs, per
R7 element-count optimization:

```text
waybill:generation-context=filesystem-scan;waybill:cisa-2026-lifecycle=after-build;waybill:sbom-version=2
```

If only one signal is present (default: only generation-context),
only that key=value pair is emitted.

---

### SBOM Version (existing but hardcoded for CDX; new caller-supplied path — US4 / FR-013 / FR-014)

| Emitter | Slot | Wire encoding |
|---------|------|---------------|
| CDX 1.6 | `metadata.version` (existing, hardcoded to `1`) | JSON integer. **NEW**: threaded from `SbomVersion::as_u32()` when `--sbom-version` is set; else stays `1` |
| SPDX 2.3 | Same annotation as generation-context above (**NEW**) | Added to `comment` as `;waybill:sbom-version=<N>` |
| SPDX 3.0.1 | Same annotation as generation-context above (**NEW**) | Added to `statement` as `;waybill:sbom-version=<N>` |

**Golden byte-identity** (FR-009): when `--sbom-version` is unset,
CDX golden stays byte-identical (still emits `"version": 1`); SPDX
goldens still require the generation-context annotation regen
noted above but do NOT gain an sbom-version key=value pair (the
absence of the flag suppresses the annotation portion).

---

## Elements this feature does NOT change (coverage matrix cites existing code)

These 14 elements are already correctly emitted. The feature only
adds the citation to `docs/cisa-2026-coverage.md`. Each row's
"Notes" column may reference this document for context but no
emitter code change is required.

| # | Element | Existing slot |
|---|---------|---------------|
| 1 | SBOM Author | CDX: `metadata.authors[]`; SPDX 2.3: `creationInfo.creators[]`; SPDX 3: `CreationInfo.createdBy` |
| 3 | SBOM Data Format Name | CDX: `bomFormat: "CycloneDX"`; SPDX 2.3: implicit in `spdxVersion` prefix; SPDX 3: implicit in `@context` |
| 4 | SBOM Data Format Version | CDX: `specVersion: "1.6"`; SPDX 2.3: `spdxVersion: "SPDX-2.3"`; SPDX 3: `CreationInfo.specVersion` |
| 6 | SBOM Timestamp | CDX: `metadata.timestamp`; SPDX 2.3: `creationInfo.created`; SPDX 3: `CreationInfo.created` |
| 7 | SBOM Tool Name | CDX: `metadata.tools.components[].name`; SPDX 2.3: `creationInfo.creators[] contains "Tool: waybill-<version>"`; SPDX 3: `Tool.name` |
| 8 | SBOM Tool Version | Same as Tool Name across all three; version is part of the tool identity string |
| 10 | Component Producer | CDX: `components[].supplier.name`; SPDX 2.3: `packages[].supplier` (uses `NOASSERTION` when unknown per CISA § Explicitly Identifying Unknown Information); SPDX 3: `Package.originatedBy` |
| 11 | Component Dependency Relationship | CDX: `dependencies[]`; SPDX 2.3: `relationships[]` with `DEPENDS_ON` / `BUILD_DEPENDENCY_OF` / `DEV_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` / `OPTIONAL_DEPENDENCY_OF`; SPDX 3: `Relationship` element |
| 12 | Component Hash Value | CDX: `components[].hashes[].content`; SPDX 2.3: `packages[].checksums[].checksumValue`; SPDX 3: hash object on `Package.verifiedUsing` |
| 13 | Component Hash Algorithm | Same slots as Hash Value; algorithm enum uses IANA Hash Function Textual Names |
| 14 | Component Identifiers | CDX: `components[].purl` + `components[].externalReferences[]` (cpe, swhid, gitoid); SPDX 2.3: `packages[].externalRefs[]`; SPDX 3: `Package.externalIdentifier[]` |
| 15 | Component License | CDX: `components[].licenses[]`; SPDX 2.3: `packages[].licenseConcluded` + `licenseDeclared`; SPDX 3: `simplelicensing_LicenseExpression` element |
| 16 | Component Name | CDX: `components[].name`; SPDX 2.3: `packages[].name`; SPDX 3: `Package.name` — multiple entries allowed via multiple component elements |
| 17 | Component Version | CDX: `components[].version` (omitted when unknown); SPDX 2.3: `packages[].versionInfo` (`NOASSERTION` when unknown); SPDX 3: `Package.software_version` |

Row 2 (Author Signature), row 5 (Generation Context), row 9 (SBOM
Version) are the three that this feature adds/changes; the other
14 are audit-only.

---

## Native-fields-first audit (Principle V bullet 5)

Per Waybill Constitution Principle V, any new `waybill:*` annotation
requires an audit of native fields first. This feature introduces
three annotations:

1. **`waybill:generation-context`** (existing per m047; already
   documented in `docs/reference/sbom-format-mapping.md`) — no new
   audit needed.
2. **`waybill:cisa-2026-lifecycle`** (NEW per FR-012):
   - CDX audit: `metadata.lifecycles[].phase` is native, but it
     uses CDX-specific vocabulary (`design`, `pre-build`, `build`,
     `post-build`, `operations`) not CISA vocabulary (`before-build`,
     `build`, `after-build`). The annotation is a parity-bridging
     alias per Principle V bullet 5 (a `waybill:*` property is
     permitted "to carry finer-grained information the standard
     does not express, or to bridge a parity gap when one format
     has the native field but another doesn't"). The bridging role
     here is between CDX vocab and CISA vocab, plus the
     SPDX-doesn't-have-doc-scope-lifecycle gap.
   - SPDX 2.3 audit: no native doc-scope lifecycle field. Parity
     gap → annotation permitted.
   - SPDX 3 audit: `LifecycleScopeType` exists on relationships
     (m052) but not on `SpdxDocument`. Parity gap at document
     scope → annotation permitted.
   - **Documentation deliverable**: add row to
     `docs/reference/sbom-format-mapping.md` naming the parity
     gaps and citing this feature per Principle V's docs
     requirement.
3. **`waybill:sbom-version`** (NEW per FR-013 for SPDX only):
   - CDX audit: `metadata.version` is native. No annotation
     emitted for CDX.
   - SPDX 2.3 audit: no native SBOM-document-version field. The
     closest candidate `Package.versionInfo` is component-version,
     not SBOM-version; `documentNamespace` is content-addressed
     identity, not a monotonic counter. Parity gap → annotation
     permitted.
   - SPDX 3 audit: same as SPDX 2.3.
   - **Documentation deliverable**: add row to
     `docs/reference/sbom-format-mapping.md`.

Both new annotations get one row each in the sbom-format-mapping
doc. Reviewers can reject the feature if the rows are missing at
merge time.
