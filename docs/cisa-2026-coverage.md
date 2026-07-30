---
cisa-publication: "2026 Minimum Elements for a Software Bill of Materials (SBOM)"
cisa-publication-date: 2026-07-29
cisa-publication-tlp: TLP:CLEAR
cisa-publication-url: https://www.cisa.gov/sites/default/files/2026-07/2026_cisa_sbom_minimum_elements_508c.pdf
waybill-milestone: 221
last-verified: 2026-07-29
---

# waybill vs CISA 2026 SBOM Minimum Elements — coverage matrix

**Reader path**: see [`specs/221-cisa-2026-elements-audit/quickstart.md`](../specs/221-cisa-2026-elements-audit/quickstart.md).

**Machine-verified**: `waybill-cli/tests/cisa_2026_coverage_matrix.rs` walks
every ✅ verdict below against a fresh scan on every CI run. A regression
that empties a native slot fails the test.

**Vocabulary**:
- **✅** — native field populated by waybill by default.
- **⚠️** — populated via a `waybill:*` annotation (parity-bridging where
  the target format has no native slot). Documented in
  [`docs/reference/sbom-format-mapping.md`](reference/sbom-format-mapping.md).
- **❌** — absent. Every ❌ links to a follow-up user story in
  [`specs/221-cisa-2026-elements-audit/spec.md`](../specs/221-cisa-2026-elements-audit/spec.md).

---

## Data Fields (17)

Rows 1–9 are SBOM Metadata elements (about the SBOM document itself).
Rows 10–17 are Component Data elements (about the target and its
subcomponents).

| # | Element (CISA 2026) | Category | Change (vs 2021) | CDX 1.6 | SPDX 2.3 | SPDX 3.0.1 | Notes |
|---|---------------------|----------|-------------------|---------|----------|-----------|-------|
| 1 | SBOM Author | Metadata | Major Update | ✅ `metadata.authors[]` at `waybill-cli/src/generate/cyclonedx/metadata.rs:798` | ✅ `creationInfo.creators[]` at `waybill-cli/src/generate/spdx/document.rs:806` | ✅ `CreationInfo.createdBy` at `waybill-cli/src/generate/spdx/v3_document.rs:229` | m080 wires `--creator` / `--annotator` from `waybill trace run`; standalone `waybill sbom scan` uses waybill as the sole author. Distinct from Component Producer (element 10). |
| 2 | SBOM Author Signature | Metadata | New | ⚠️ opt-in `--sign-key <PATH>` — populates `metadata.signature` with a JSF (JSON Signature Format, draft-cyberphone-jsf-00) object at `waybill-cli/src/sbom/signer.rs::sign_cdx_document_in_place` (m221 US2a). Sigstore keyless (`--sign`) pending US2b. Absent by default per FR-009. | ⚠️ opt-in `--sign-key <PATH>` — emits DSSE envelope sidecar at `<output>.sig.json` (SPDX 2.3 has no native in-document envelope-signature slot) via `waybill-cli/src/sbom/signer.rs::sign_spdx_bytes_to_dsse` (m221 US2a). Sigstore keyless pending US2b. Absent by default. | ⚠️ opt-in `--sign-key <PATH>` — same DSSE sidecar shape as SPDX 2.3 (SPDX 3 also lacks a native in-document envelope-signature slot). Sigstore keyless pending US2b. Absent by default. | Feature 221 US2a landed static-key signing (PEM path). Sigstore keyless (`--sign`) is the R6 risk item, pending US2b — completing the m006 scaffolded `sign_keyless()` with real Fulcio + Rekor + Sigstore Bundle assembly. Test coverage: 6/6 in `waybill-cli/tests/cisa_2026_signing.rs` (US2b test `#[ignore]`d pending WAYBILL_TEST_KEYLESS=1 + OIDC + Sigstore staging). CLI rejects `--sign-key + --output -` at parse per FR-008a; fails-close on any signing error per FR-009a (cleans up partial output). Verification: `jq .metadata.signature signed.cdx.json` + JSF-verifier tool (any RFC 7515 aware) for CDX; `sigstore-rs` `CosignVerificationKey::from_pem(...)?.verify_signature(...)` for DSSE. |
| 3 | SBOM Data Format Name | Metadata | New | ✅ `bomFormat: "CycloneDX"` at `waybill-cli/src/generate/cyclonedx/builder.rs:813` | ⚠️ implicit in `spdxVersion: "SPDX-2.3"` at `waybill-cli/src/generate/spdx/document.rs:152` (format name and version are combined in one slot per SPDX 2.3 § 6.1) | ⚠️ implicit in top-level `@context` at `waybill-cli/src/generate/spdx/v3_document.rs:863` (SPDX 3 uses JSON-LD, format name is the `@context` URL) | SPDX doesn't split format name from format version the way CDX does; the ⚠️ reflects that the CISA element is technically satisfied but the value has to be inferred from a compound slot. |
| 4 | SBOM Data Format Version | Metadata | New | ✅ `specVersion: "1.6"` at `waybill-cli/src/generate/cyclonedx/builder.rs:814` | ✅ `spdxVersion: "SPDX-2.3"` at `waybill-cli/src/generate/spdx/document.rs:152` | ✅ `CreationInfo.specVersion: "3.0.1"` at `waybill-cli/src/generate/spdx/v3_document.rs:227` | All three formats emit a version literal at document scope. |
| 5 | SBOM Generation Context | Metadata | New | ✅ native `metadata.lifecycles[]` at `waybill-cli/src/generate/cyclonedx/metadata.rs:1099` (m047 aggregates `ScanArtifacts.generation_context` into CDX-native phases). ✅ courtesy alias `metadata.properties[waybill:cisa-2026-lifecycle]` at `metadata.rs` (m221 US3 / FR-012). | ⚠️ doc-scope `Annotation` on `SPDXRef-DOCUMENT` at `waybill-cli/src/generate/spdx/annotations.rs::annotate_document` — carries both `waybill:generation-context` (waybill-native variant) and `waybill:cisa-2026-lifecycle` (CISA-vocab alias) per m221 US3 / FR-010 + FR-012. | ⚠️ top-level `Annotation` element with `subject: <SpdxDocument @id>` at `waybill-cli/src/generate/spdx/v3_annotations.rs::push_document_fields` — same two-annotation shape as SPDX 2.3 per m221 US3 / FR-011 + FR-012. Validates cleanly against SPDX 3.0.1 schema + SHACL via `spdx3-validate==0.0.5`. | CISA "before-build"/"build"/"after-build" vocab satisfied per CISA page 9 ("more specific identifiers can satisfy this element"). Mapping table lives in `waybill_common::attestation::metadata::GenerationContext::as_cisa_2026_lifecycle`: `build-time-trace → build`; `filesystem-scan → after-build`; `container-image-scan → after-build`. Parity extractor row `C141` at `waybill-cli/src/parity/extractors/mod.rs`. |
| 6 | SBOM Timestamp | Metadata | Minor Update | ✅ `metadata.timestamp` (RFC 3339, deterministic via `OutputConfig.created`) at `waybill-cli/src/generate/cyclonedx/metadata.rs:799` | ✅ `creationInfo.created` at `waybill-cli/src/generate/spdx/document.rs:826` | ✅ `CreationInfo.created` at `waybill-cli/src/generate/spdx/v3_document.rs:228` | RFC 9557 tolerates RFC 3339 (RFC 9557 § 3.2 extension). |
| 7 | SBOM Tool Name | Metadata | New | ✅ `metadata.tools.components[].name = "waybill"` at `waybill-cli/src/generate/cyclonedx/metadata.rs:825` | ✅ `creationInfo.creators[]` contains `"Tool: waybill-<version>"` at `waybill-cli/src/generate/spdx/document.rs:806` | ✅ `Tool.name` referenced via `CreationInfo.createdBy` at `waybill-cli/src/generate/spdx/v3_document.rs:229` | Native across all three. |
| 8 | SBOM Tool Version | Metadata | New | ✅ same slot as Tool Name (`metadata.tools.components[].version`) | ✅ version embedded in the `Tool: waybill-<version>` string (same slot as Tool Name) | ✅ version on the `Tool` element (same slot as Tool Name) | Waybill emits the workspace version at build time via `env!("CARGO_PKG_VERSION")`. |
| 9 | SBOM Version | Metadata | New | ✅ native `metadata.version` at `waybill-cli/src/generate/cyclonedx/builder.rs:826` — defaults to `1` (byte-identical to pre-m221) when `--sbom-version` unset; carries the operator-supplied integer when set. ⚠️ parity annotation `metadata.properties[waybill:sbom-version]` emitted only when `--sbom-version` is set (m221 US4 / FR-013). | ⚠️ doc-scope `Annotation` on `SPDXRef-DOCUMENT` carrying `waybill:sbom-version=<N>` at `waybill-cli/src/generate/spdx/annotations.rs::annotate_document` — emitted only when `--sbom-version` is set (m221 US4 / FR-013). SPDX 2.3 has no native SBOM-document-version field; the annotation is the primary carrier. | ⚠️ top-level `Annotation` element with `subject: <SpdxDocument @id>` carrying `waybill:sbom-version` at `waybill-cli/src/generate/spdx/v3_annotations.rs::push_document_fields` — same emission gate as SPDX 2.3. | CISA 2026 § SBOM Version blesses RFC 9562 UUIDs as an alternative pathway; waybill's existing `serialNumber` UUID (CDX at `builder.rs:815`) + content-addressed `documentNamespace` (SPDX 2.3 per m010) + `@id` (SPDX 3 per m010) already satisfy the identity pathway. The `--sbom-version` integer covers the monotonic-counter pathway consumers who key on `metadata.version` expect. Parity extractor row `C142` at `waybill-cli/src/parity/extractors/mod.rs`. Value type: positive integer (`{"type": "integer", "minimum": 1}`); non-integer values and values < 1 rejected at CLI parse per FR-014. |
| 10 | Component Producer | Component | Major Update | ✅ `components[].supplier.name` populated from `ResolvedComponent.supplier` | ✅ `packages[].supplier` at `waybill-cli/src/generate/spdx/packages.rs:186` (uses `NOASSERTION` sentinel per CISA § Explicitly Identifying Unknown Information when unknown; verified at `packages.rs:285` and `641`) | ✅ `Package.suppliedBy` (IRI reference to an `Organization` element in the `@graph`, deduplicated per `v3_agents.rs:68`) | Renamed from 2021 "Supplier Name" — waybill absorbed the rename at m080. Both SPDX 2.3 and SPDX 3 model supplier + originator as separate slots (`supplier` / `originator` on SPDX 2.3 packages, `suppliedBy` / `originatedBy` on SPDX 3 packages); waybill populates the *supplier* slot with the entity that distributed the artifact (which CISA's Component Producer definition covers). |
| 11 | Component Dependency Relationship | Component | Minor Update | ✅ `dependencies[]` array with `ref` + `dependsOn[]` in `waybill-cli/src/generate/cyclonedx/dependencies.rs` | ✅ `relationships[]` with `DEPENDS_ON` (plus m052-native `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` and m179-native `OPTIONAL_DEPENDENCY_OF` per Section B row B2 of `docs/reference/sbom-format-mapping.md`) | ✅ `Relationship` element with `relationshipType: "dependsOn"` plus m052-native `LifecycleScopeType` parameter | Waybill's dep-graph semantics exceed CISA's baseline (dev/build/test scope carried natively per Principle V). |
| 12 | Component Hash Value | Component | New | ✅ `components[].hashes[].content` at `waybill-cli/src/generate/cyclonedx/builder.rs:1048` (when `include_hashes` per `--no-hashes`) | ✅ `packages[].checksums[].checksumValue` at `waybill-cli/src/generate/spdx/packages.rs:192` | ✅ `Package.verifiedUsing[]` Hash object per Section A row A6 of `sbom-format-mapping.md` | Content hash of the executable/package artifact; hex-encoded per CISA. |
| 13 | Component Hash Algorithm | Component | New | ✅ `components[].hashes[].alg` (IANA Hash Function Textual Names per CDX 1.6 § component.hashes enum) at `builder.rs:1048` | ✅ `packages[].checksums[].algorithm` enum at `packages.rs:192` (SPDX 2.3 supports `SHA1`/`SHA224`/`SHA256`/`SHA384`/`SHA512`/`MD5`/etc.) | ✅ `Hash.algorithm` on the verifiedUsing element (lowercase-no-hyphen `sha256` per SPDX 3.0.1 `prop_Hash_algorithm` enum) | All three formats' algorithm identifiers map to IANA-registered names per CISA's requirement. |
| 14 | Component Identifiers | Component | Major Update | ✅ `components[].purl` (always present when derivable) + `components[].cpe` at `builder.rs:1152` + `components[].externalReferences[]` for SWHID / OmniBOR at `builder.rs:1142` | ✅ `packages[].externalRefs[]` with `referenceCategory: "PACKAGE-MANAGER"` (PURL), `"SECURITY"` (CPE), `"OTHER"` (SWHID/OmniBOR) | ✅ `Package.software_packageUrl` + `Package.externalIdentifier[]` for PURL/CPE23/SWHID/OmniBOR per Section A row A1 of `sbom-format-mapping.md` | CISA says "at least one common software identifier"; waybill emits PURL always plus CPE/SWHID/OmniBOR when resolvable. |
| 15 | Component License | Component | New | ✅ `components[].licenses[]` at `builder.rs:1123` (SPDX identifiers/expressions; `waybill:*` fallback annotation for non-canonicalizable per m146 dedupe) | ✅ `packages[].licenseConcluded` + `packages[].licenseDeclared` at `packages.rs:193-195` (canonical SPDX expression / `LicenseRef-<hash>` per m153) | ✅ `simplelicensing_LicenseExpression` element referenced via `Relationship` (`hasDeclaredLicense` / `hasConcludedLicense`) per Section A row A7 of `sbom-format-mapping.md` | All three formats emit SPDX identifiers per CISA's SPDX preference (page 12). |
| 16 | Component Name | Component | Minor Update | ✅ `components[].name` (required by CDX 1.6 schema) | ✅ `packages[].name` at `packages.rs` (required by SPDX 2.3 schema) | ✅ `Package.name` (required by SPDX 3.0.1 schema) | CISA "must allow multiple entries" satisfied via the multiplicity of the `components[]` / `packages[]` array — one entry per distinct name. |
| 17 | Component Version | Component | Major Update | ⚠️ `components[].version` populated when known, omitted when unknown (CDX has no `NOASSERTION` convention — asymmetry with SPDX flagged per Edge Case #1 in `spec.md`) | ✅ `packages[].versionInfo` at `packages.rs:181` — uses `NOASSERTION` when unknown per `packages.rs:641` (satisfies CISA § Explicitly Identifying Unknown Information) | ✅ `Package.software_packageVersion` at `waybill-cli/src/generate/spdx/v3_packages.rs` (with `NOASSERTION`-equivalent omission) | m191 reconciler ensures the version field carries an explicit "unknown" marker on the SPDX side; CDX-side omission is documented and left to consumers to interpret. |

---

## Practices & Processes (6)

Per CISA 2026 § SBOM Minimum Elements page 7, Practices & Processes
"outline principles that guide SBOM operations across the software
lifecycle." They describe how an **organization** engages with SBOM
data — not payload fields inside the SBOM itself. Consumers auditing
this element look for evidence in operator workflows, tooling contracts,
and delivery pipelines, not in a jq-extractable slot.

### Accommodation of Updates to SBOM Data (Major Update)

**CISA text**: > "Organizations should accommodate updates to SBOM data,
including corrections. SBOM authors should correct errors promptly.
Organizations may consider errors, whether stemming from SBOM author
practices or selection of inadequate tools, in organizational risk
management decisions." (page 13)

**Classification**: **Organizational practice** — not a payload
element. Waybill's role is to enable the operator to satisfy the
practice.

**How waybill enables the operator to satisfy this**:
- Every `waybill sbom scan` invocation regenerates the SBOM from
  scratch — no cached state, no stale document. A correction to the
  target (updated lockfile, patched binary, added metadata) reflects
  in the very next scan.
- Deterministic output (RFC 8785 canonical JSON, milestone-010
  content-addressed identifiers) means "re-run to correct" produces
  a byte-comparable diff so operators can prove what changed.
- `--sbom-version <N>` (US4 pending) lets operators tag the corrected
  revision so consumers can order-by-revision.

### Coverage (Major Update — was "Depth")

**CISA text**: > "An SBOM should include information for all components
that make up the target software, including transitive dependencies.
There is no minimum depth. ... SBOMs should provide a comprehensive
listing of the components to facilitate recipients' risk-based
decisions." (page 13)

**Classification**: **Organizational practice** — describes the
recipient's ability to make risk-based decisions, not a payload slot.

**How waybill enables the operator to satisfy this**:
- Every `waybill sbom scan` emits a document-scope
  `waybill:graph-completeness` annotation (milestone 158) tracking
  horizontal breadth (per-ecosystem component enumeration) and
  vertical depth (transitive-dep resolution ladder per m055 + m160).
- Ecosystem-completeness per m158 assigns each ecosystem a
  compact 5-tier state (Complete / TransitivePartial / DirectOnly /
  Manifest / None) so consumers can filter their risk decisions.
- Fail-close on any completeness regression per Constitution
  Principle III (no silent gap-filling).

### Distribution and Delivery (Minor Update — absorbed "Access Control")

**CISA text**: > "SBOMs should be available promptly to those who
need them. Access controls may limit the sharing of SBOM data with
unauthorized parties but should not prevent information sharing
between authorized parties or restrict organizations from integrating
SBOM data into trusted security tools. There are multiple ways of
sharing SBOM data. For example, an SBOM can accompany installation.
Alternatively, an SBOM can be accessible through a version-specific
URL, an application programming interface (API) to a database, or a
public repository. Any such software service or offering should
operate in accordance with the provider's security policy." (page 13)

**Classification**: **Organizational practice** — describes what
happens *after* the SBOM is emitted; entirely outside waybill's
runtime scope.

**How waybill enables the operator to satisfy this**:
- Waybill emits to `--output <path>` (default `waybill.cdx.json`) or
  `--output -` (stdout) — the operator wires that into their delivery
  pipeline of choice (artifact registry, OCI Referrers per m186, S3
  bucket, HTTP endpoint).
- OCI Referrers integration per m186 (`--sbom-source referrer|either`)
  lets the operator pull a pre-existing SBOM instead of scanning —
  supporting "distribution via OCI registry" natively.

### Explicitly Identifying Unknown Information (Major Update — was "Known Unknowns")

**CISA text**: > "If information required for any of the data fields
is not provided, the SBOM author should explicitly state whether the
information is unknown to the SBOM author or whether the SBOM author
is withholding the information from the SBOM." (page 13)

**Classification**: **Organizational practice**, but has native
payload manifestations (see below).

**How waybill enables the operator to satisfy this**:
- SPDX 2.3 emitter uses `NOASSERTION` sentinels for
  `packages[].versionInfo` (`packages.rs:641`) and
  `packages[].supplier` (`packages.rs:285`) when the value is unknown
  to waybill — satisfies the "explicitly state unknown" clause
  natively per SPDX 2.3 convention.
- CDX 1.6 omits unknown fields (no sentinel convention exists); the
  ambiguity between "unknown" and "withheld" for CDX outputs is
  documented in `spec.md` Edge Case #1 and left for consumers to
  interpret from the omission.
- Waybill does NOT withhold information in the default output —
  any absent field means "waybill did not know," never "waybill
  knew but hid." This posture is documented here as the operator's
  guarantee.

### Frequency (Minor Update)

**CISA text**: > "Each software version or update should have an
associated SBOM. When a component producer issues a new build or
release, they (or the SBOM author) should also generate a new SBOM to
reflect the changes. This includes software builds that integrate
updated components or dependencies. When a component producer (or
SBOM author) discovers new details about the underlying components or
corrects an error in the existing SBOM data, they (or the SBOM
author) should issue a revised SBOM." (page 14)

**Classification**: **Organizational practice** — describes the
cadence with which the operator invokes the SBOM generator.

**How waybill enables the operator to satisfy this**:
- Deterministic regeneration per invocation with a fresh
  `serialNumber` (CDX) and content-addressed `documentNamespace` /
  `@id` (SPDX per m010) — every scan produces a distinctly-identified
  document even for the same target.
- No caching of prior SBOMs means the operator's CI can safely
  invoke `waybill sbom scan` on every commit, every release tag,
  every published container image — the operator chooses the
  cadence.
- Fast: typical mid-sized project SBOM emits in <5 seconds per m094
  perf goldens, so per-commit invocation is feasible.

### Machine-Processable Data (Major Update — was "Automation Support")

**CISA text**: > "Automation support is critical for managing
software component data at scale, particularly across organizational
boundaries. SBOM implementations should be compatible with each
other to support automation due to the volume of data, diverse use
cases, and variety of tools involved with SBOMs. The two data
formats currently widely used by software ecosystem stakeholders to
generate and consume SBOMs are SPDX and CycloneDX. These data
formats are a product of open, international processes and are both
machine-processable and human-readable." (page 14)

**Classification**: **Organizational practice** on the operator side
(choose a widely-supported format); native payload guarantee on the
waybill side (waybill emits only in machine-processable formats).

**How waybill enables the operator to satisfy this**:
- Waybill emits CycloneDX 1.6, SPDX 2.3, and SPDX 3.0.1 — the three
  formats CISA 2026 names. Selectable via `--format <name>`;
  multi-format in one invocation via repeated `--format` flags.
- All three outputs are valid JSON (SPDX 3 is JSON-LD).
- SPDX 3 conformance validated in CI via
  `spdx3-validate==0.0.5` (milestone 078) — gated by
  `WAYBILL_REQUIRE_SPDX3_VALIDATOR=1`.
- **2026 change advisory (SWID removed)** <!-- fr-016-swid-advisory -->:
  Per CISA 2026 Appendix B § Automation Support: "Remove Software
  Identification (SWID) Tags from list of data formats. ... SWID tags
  are not a widely used SBOM data format for which multiple tools
  exist." Waybill has never emitted SWID and has no plan to; this is
  a no-change advisory row acknowledging the CISA vocabulary update.

---

## Appendix A — Reproducible verification recipes

Every ✅ cell in the matrix above cites a slot; this appendix gives
the exact `jq` recipe to extract the value from a fresh scan. Every
recipe MUST return a non-empty JSON value when run against a live
scan output; the integration test at
`waybill-cli/tests/cisa_2026_coverage_matrix.rs` asserts this on
every CI run.

**Setup** (run once):

```bash
target_dir=~/.cache/waybill/fixtures/*/transitive_parity/cargo
waybill sbom scan \
  --path "$target_dir" \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json \
  --output cyclonedx-json=/tmp/scan.cdx.json \
  --output spdx-2.3-json=/tmp/scan.spdx.json \
  --output spdx-3-json=/tmp/scan.spdx3.json
```

Recipes below assume `/tmp/scan.cdx.json` /
`/tmp/scan.spdx.json` / `/tmp/scan.spdx3.json` from the setup.

**Element: SBOM Author** (row 1)
- CDX: `jq -r '.metadata.authors[].name' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.creationInfo.creators[]' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][] | select(.type=="CreationInfo") | .createdBy[]' /tmp/scan.spdx3.json | head -1`

**Element: SBOM Data Format Name** (row 3)
- CDX: `jq -r '.bomFormat' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.spdxVersion' /tmp/scan.spdx.json`
- SPDX 3: `jq -r '.["@context"]' /tmp/scan.spdx3.json`

**Element: SBOM Data Format Version** (row 4)
- CDX: `jq -r '.specVersion' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.spdxVersion' /tmp/scan.spdx.json`
- SPDX 3: `jq -r '.["@graph"][] | select(.type=="CreationInfo") | .specVersion' /tmp/scan.spdx3.json`

**Element: SBOM Generation Context** (row 5, all three formats post-m221 US3)
- CDX: `jq -r '.metadata.lifecycles[].phase' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.annotations[]?.comment | select(contains("waybill:cisa-2026-lifecycle"))' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="Annotation") | .statement | select(contains("waybill:cisa-2026-lifecycle"))' /tmp/scan.spdx3.json | head -1`

**Element: SBOM Timestamp** (row 6)
- CDX: `jq -r '.metadata.timestamp' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.creationInfo.created' /tmp/scan.spdx.json`
- SPDX 3: `jq -r '.["@graph"][] | select(.type=="CreationInfo") | .created' /tmp/scan.spdx3.json`

**Element: SBOM Tool Name** (row 7)
- CDX: `jq -r '.metadata.tools.components[].name' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.creationInfo.creators[] | select(startswith("Tool:"))' /tmp/scan.spdx.json`
- SPDX 3: `jq -r '.["@graph"][] | select(.type=="Tool") | .name' /tmp/scan.spdx3.json`

**Element: SBOM Tool Version** (row 8)
- CDX: `jq -r '.metadata.tools.components[].version' /tmp/scan.cdx.json`
- SPDX 2.3: `jq -r '.creationInfo.creators[] | select(startswith("Tool:"))' /tmp/scan.spdx.json` (version is embedded in the creator string)
- SPDX 3: `jq -r '.["@graph"][] | select(.type=="Tool") | .name' /tmp/scan.spdx3.json` (version embedded in tool name)

**Element: SBOM Version** (row 9)
- CDX (native): `jq '.version' /tmp/scan.cdx.json` (returns 1 by default; the operator-supplied integer when `--sbom-version <N>` is passed)
- SPDX 2.3 (annotation, only when `--sbom-version` is set): `jq -r '.annotations[]?.comment | select(contains("waybill:sbom-version"))' /tmp/scan.spdx.json | head -1`
- SPDX 3 (annotation, only when `--sbom-version` is set): `jq -r '.["@graph"][]? | select(.type=="Annotation") | .statement | select(contains("waybill:sbom-version"))' /tmp/scan.spdx3.json | head -1`

**Element: Component Producer** (row 10)
- CDX: `jq -r '.components[]?.supplier?.name // empty' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.supplier // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .suppliedBy // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component Dependency Relationship** (row 11)
- CDX: `jq '.dependencies[]?.dependsOn // empty | length' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.relationships[]?.relationshipType' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="Relationship") | .relationshipType' /tmp/scan.spdx3.json | head -1`

**Element: Component Hash Value** (row 12)
- CDX: `jq -r '.components[]?.hashes[]?.content // empty' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.checksums[]?.checksumValue // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .verifiedUsing[]?.hashValue // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component Hash Algorithm** (row 13)
- CDX: `jq -r '.components[]?.hashes[]?.alg // empty' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.checksums[]?.algorithm // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .verifiedUsing[]?.algorithm // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component Identifiers** (row 14)
- CDX: `jq -r '.components[]?.purl // empty' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.externalRefs[]?.referenceLocator // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .software_packageUrl // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component License** (row 15)
- CDX: `jq -r '.components[]?.licenses[]? | (.license.id // .license.name // .expression // empty)' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.licenseDeclared // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="simplelicensing_LicenseExpression") | .simplelicensing_licenseExpression // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component Name** (row 16)
- CDX: `jq -r '.components[]?.name // empty' /tmp/scan.cdx.json | head -1`
- SPDX 2.3: `jq -r '.packages[]?.name // empty' /tmp/scan.spdx.json | head -1`
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .name // empty' /tmp/scan.spdx3.json | head -1`

**Element: Component Version** (row 17)
- CDX: `jq -r '.components[]?.version // empty' /tmp/scan.cdx.json | head -1` (may be empty for unknown-version components per Edge Case #1)
- SPDX 2.3: `jq -r '.packages[]?.versionInfo // empty' /tmp/scan.spdx.json | head -1` (returns `NOASSERTION` when unknown)
- SPDX 3: `jq -r '.["@graph"][]? | select(.type=="software_Package") | .software_packageVersion // empty' /tmp/scan.spdx3.json | head -1`

---

## Regeneration process

When a subsequent CISA publication or a subsequent waybill milestone
changes any cell:

1. Update the affected row's cell (verdict / slot / file:line).
2. Update `last-verified` in the front-matter to the current date.
3. Run `cargo +stable test --workspace --test cisa_2026_coverage_matrix`
   locally to confirm every recipe still resolves to a non-empty
   value.
4. If a CISA element itself was added/removed, bump the section
   header count (`(17)` → `(18)`) and add/remove the matrix row.
5. If a waybill emitter surface moved (line-number churn), the test
   `--nocapture` output will name the failing cell — update the
   citation.
