# Feature Specification: CISA 2026 SBOM Minimum Elements coverage audit

**Feature Branch**: `221-cisa-2026-elements-audit`
**Created**: 2026-07-29
**Status**: Draft
**Input**: User description: "let's look at https://www.cisa.gov/sites/default/files/2026-07/2026_cisa_sbom_minimum_elements_508c.pdf and see if there's anything we need to update in waybill to ensure we support this. I think we already do support all this but I want to double check"

## Clarifications

### Session 2026-07-29

- Q: Signing key sources & format for `SBOM Author Signature` (US2 / FR-007–008) → A: Both, Sigstore keyless default (`--sign` = keyless / Sigstore Bundle; `--sign-key <ref>` = static / JSF; mutually exclusive)
- Q: Signing failure semantics when `--sign` or `--sign-key` was requested but the signature can't be produced (US2) → A: Fail the scan (exit non-zero) with a diagnostic; no silent unsigned fallback
- Q: `--sbom-version` value format (US4 / FR-013–014) → A: Integer only — matches CDX 1.6 `metadata.version` schema (`{"type": "integer", "minimum": 1}`); CISA's semver suggestion is optional ("may"), and waybill's existing UUID `serialNumber` (CDX) + content-addressed `documentNamespace` / `@id` (SPDX 2.3 / SPDX 3, per m010) already satisfy CISA's RFC 9562 alternative for revision identity
- Q: Signing + `--output -` (stdout) — sidecar placement (US2 / FR-008) → A: Reject at CLI parse time — signing requires a durable output path; combining a signing flag with `--output -` produces a diagnostic pointing operators at `--output <file>`. No signature-to-stderr, no surprise disk writes.

## Context

On 2026-07-29, CISA (with 17 co-authoring national cyber agencies) published
"2026 Minimum Elements for a Software Bill of Materials (SBOM)," superseding
the 2021 NTIA baseline. The 2026 revision adds 9 new data-field elements
(SBOM Author Signature, SBOM Data Format Name, SBOM Data Format Version,
SBOM Generation Context, SBOM Tool Name, SBOM Tool Version, SBOM Version,
Component Hash Value, Component Hash Algorithm, Component License),
renames three (Supplier Name → Component Producer, Depth → Coverage,
Known Unknowns → Explicitly Identifying Unknown Information), rewrites
Automation Support as Machine-Processable Data (dropping SWID from
accepted formats), and folds Access Control into Distribution and
Delivery. This specification audits waybill's three emission surfaces
(CycloneDX 1.6, SPDX 2.3, SPDX 3.0.1) against the full 17-field +
6-practice baseline and prescribes closure work for any confirmed gap.

## User Scenarios & Testing

### User Story 1 — Publish a signed CISA 2026 coverage matrix (Priority: P1)

An operator adopting waybill (or a procurement/compliance stakeholder
evaluating it) needs a definitive, evidence-backed statement of which
CISA 2026 minimum elements each of waybill's three emitter outputs
satisfies, so that downstream consumers can point to the matrix when
answering audits, RFP questions, or regulator inquiries (EU CRA,
BSI TR-03183-2, CERT-In, and equivalents).

**Why this priority**: The user's ask ("double check we support this")
is answered by exactly this artifact. Every downstream work item —
signing, generation-context propagation, SBOM-version increment — is
justified by a row in this matrix. Ship the matrix first; scope the
fixes second.

**Independent Test**: A reader can open `docs/cisa-2026-coverage.md`,
find each of the 17 data fields and 6 practices from the CISA
publication, and for each: (a) see a ✅ / ⚠️ / ❌ verdict per emitter,
(b) see the exact file:line where the field is populated (or a
"not-emitted" citation), (c) reproduce the verdict by running a
provided `jq` or `yq` recipe against a fresh `waybill scan` output.

**Acceptance Scenarios**:

1. **Given** the coverage document, **When** a reader looks up
   "SBOM Author Signature", **Then** they see ❌ for all three emitters
   with a citation to the missing envelope-signature field and a
   pointer to User Story 2 as the follow-up.
2. **Given** the coverage document, **When** a reader looks up
   "Component Hash Value", **Then** they see ✅ across CDX / SPDX 2.3 /
   SPDX 3, with citations to `components[].hashes[].content`,
   `packages[].checksums[].checksumValue`, and the SPDX 3 hash object,
   plus a reproducible `jq` recipe against a scan output.
3. **Given** the coverage document, **When** a reader looks up a
   Practices & Processes element (e.g., "Frequency"), **Then** they
   see an explanation that this is an organizational practice (not a
   payload field) plus which waybill behaviors (deterministic
   regeneration per invocation, fresh serialNumber/documentNamespace)
   allow an operator to satisfy the practice.

---

### User Story 2 — Emit a native SBOM Author Signature (Priority: P2)

A procurement organization requires an SBOM whose contents can be
cryptographically verified as unmodified since author generation, per
the new CISA 2026 "SBOM Author Signature" element (Definition:
"A digital signature attributable to the SBOM author").

**Why this priority**: The only element with confirmed ❌ across all
three of waybill's emitters. CDX 1.6 has a native `signature` slot
(JSON Signature Format, JSF); SPDX has no envelope-signature field
but the CISA text explicitly recognizes external signing infrastructure
(NIST DSS, ISO/IEC 14888-4:2024, ENISA Agreed Cryptographic Mechanisms)
as satisfactory. waybill already ships attestation-signing plumbing
via the milestone-006 sbomit-suite pipeline, so the primitives exist.

**Independent Test**: An operator runs `waybill scan --sign` (Sigstore
keyless — the documented default) OR `waybill scan --sign-key <ref>`
(static key material) and receives a CDX document with a populated
`signature` object at document root and a companion detached DSSE /
Sigstore-Bundle envelope for the SPDX outputs. A verifier can validate
the signature offline (for static keys) or via Sigstore's transparency
log (for keyless) given the public key/cert, without contacting
waybill itself.

**Acceptance Scenarios**:

1. **Given** the operator passes `--sign` with a valid OIDC identity
   provider available, **When** `waybill scan` runs, **Then** the CDX
   output contains a `signature` object holding a Sigstore Bundle
   whose Rekor entry and Fulcio cert both verify with `cosign
   verify-blob --bundle <bundle>`.
2. **Given** the operator passes `--sign-key <PEM-or-KMS-ref>`,
   **When** `waybill scan` runs, **Then** the CDX output contains a
   `signature` object conforming to the CycloneDX 1.6 JSF schema and
   verifying with the corresponding public key.
3. **Given** signing enabled by either flag, **When** a byte in the
   CDX or SPDX payload is mutated post-signing, **Then** verification
   fails deterministically.
4. **Given** the operator passes both `--sign` and `--sign-key`,
   **When** `waybill scan` runs, **Then** the CLI rejects the
   combination at parse time with a diagnostic that names the
   mutually-exclusive flags.
5. **Given** signing disabled (the default — neither flag set),
   **When** the operator runs `waybill scan`, **Then** existing CDX
   and SPDX outputs are byte-identical to today (no regression in
   unsigned goldens).

---

### User Story 3 — Surface SBOM Generation Context at document scope in SPDX 2.3 and SPDX 3 (Priority: P3)

An SBOM consumer inspecting a `waybill`-generated SPDX 2.3 or SPDX 3
document needs to tell at document scope whether the components were
observed via a build-time eBPF trace (highest fidelity), a filesystem
scan, or a container-image scan — without needing to open the
companion attestation envelope. CycloneDX already surfaces this via
`metadata.lifecycles[]` (milestone 047).

**Why this priority**: Semantic parity gap — the information exists
in-memory during emission (`ScanArtifacts.generation_context`) and
is already threaded into the CDX metadata block, but the SPDX 2.3
and SPDX 3 documents only carry it via component-level
`waybill:generation-context` annotations, not at document scope. The
CISA element is defined at document scope. Downstream tools that ingest
SPDX and index by document-level provenance therefore miss the signal.

**Independent Test**: Run `waybill scan --format=spdx-2.3` on a
filesystem target; the emitted document contains a document-scope
annotation or comment stating "filesystem-scan" (or the CISA
equivalent "after build"). Same test for `--format=spdx-3`.

**Acceptance Scenarios**:

1. **Given** a filesystem scan, **When** SPDX 2.3 is emitted, **Then**
   the document carries a document-scope annotation whose value is
   the current `GenerationContext` variant (`filesystem-scan`,
   `container-image-scan`, `build-time-trace`) plus a normalized
   CISA-vocabulary alias (`before-build` / `build` / `after-build`).
2. **Given** a filesystem scan, **When** SPDX 3 is emitted, **Then**
   the `SpdxDocument`'s `CreationInfo` carries an `Annotation` with
   the same value and the CISA alias.
3. **Given** a build-time trace, **When** SPDX 2.3 or SPDX 3 is
   emitted, **Then** the document-scope value is `build-time-trace` /
   `build`.

---

### User Story 4 — Carry a caller-supplied SBOM document version (Priority: P3)

A consumer that receives multiple waybill-generated SBOMs for the
same target (e.g., an operator regenerates after adding metadata,
after a corpus refresh, after a waybill upgrade) needs a monotonic
"SBOM Version" integer they can order by to identify the latest
revision. CDX currently hardcodes `metadata.version: 1`; the SPDX
outputs don't carry the value at all today.

Note that waybill's existing content-addressed identity infrastructure
already satisfies the CISA element's RFC 9562 alternative pathway:
the CDX `serialNumber` (a UUID per invocation) and the SPDX 2.3
`documentNamespace` / SPDX 3 `@id` (both content-addressed per
milestone 010) uniquely identify every revision. This user story
covers the integer-counter pathway that consumers indexing by CDX
`metadata.version` (or an equivalent SPDX annotation) expect.

**Why this priority**: The CISA 2026 SBOM Version element (page 9)
says: "SBOM authors should update the version for an SBOM for a
given component-name/component-version pair when editing data about
the target component." waybill has no mechanism to carry a
caller-supplied SBOM version through the emitters. The gap is real
but low-blast-radius (unique-identity path already covers it for
identity purposes). Cost of fix is low (one CLI flag + wiring to
three emit sites).

**Independent Test**: Operator runs `waybill scan --sbom-version 2`
and inspects the CDX (`metadata.version = 2`), SPDX 2.3 (document-
scope annotation or `documentComment` carrying `"2"`), and SPDX 3
(`CreationInfo` annotation carrying `"2"`) outputs — all three carry
the value. Absent the flag, waybill emits `1` as today (no
regression in unsigned goldens).

**Acceptance Scenarios**:

1. **Given** the operator omits `--sbom-version`, **When** `waybill
   scan` runs, **Then** the CDX `metadata.version` remains `1` and
   the SPDX outputs remain byte-identical to today's goldens.
2. **Given** the operator passes `--sbom-version 2`, **When**
   `waybill scan` runs, **Then** the value `2` appears in the CDX
   `metadata.version` slot as an integer and in the SPDX 2.3 / SPDX 3
   annotation strings as `"2"`.
3. **Given** the operator passes a non-integer value (`2.0`, `v2`,
   `latest`, empty string, embedded newline / tab / NUL, or any
   value < 1), **When** `waybill scan` runs, **Then** the flag is
   rejected at CLI parse time with a diagnostic pointing to CISA
   2026 § SBOM Version and CDX 1.6 `metadata.version` schema
   (`{"type": "integer", "minimum": 1}`).

---

### Edge Cases

- What happens when a Component has no producer AND no version AND no
  hash (fully anonymous binary)? Per CISA § Component Producer / §
  Component Version / § Component Hash: emit an explicit "unknown"
  marker. Today, SPDX 2.3 uses `NOASSERTION` for version and
  producer — verified at `waybill-cli/src/generate/spdx/packages.rs`
  (lines 285 and 641). CDX 1.6 omits the fields entirely, which the
  CISA § Explicitly Identifying Unknown Information practice says is
  ambiguous (unknown vs. withheld). Coverage matrix must flag this
  asymmetry.
- What happens when the operator signs a scan whose target directory
  is mutated between the scan and the signing? Documented behavior:
  waybill signs the emitted-document bytes, not the target tree; a
  post-scan target mutation is out of scope.
- What happens when signing fails mid-scan (Fulcio/Rekor unreachable,
  OIDC provider offline, KMS auth expired, PKCS#11 device disconnect)?
  Per FR-009a: waybill exits non-zero, cleans up any partial output
  file, and prints a diagnostic that names the failure class. No
  silent unsigned fallback. Rationale: matches cosign / gpg / notary
  conventions; downstream policy enforcement (Sigstore admission
  controllers, Kyverno) would otherwise only catch the missing
  signature much later.
- What happens when the operator combines `--sign` (or `--sign-key`)
  with `--output -` (stdout)? Per FR-008a: waybill rejects the
  invocation at CLI parse time with a diagnostic. There is no
  signature-to-stderr fallback, no surprise disk writes, and no
  attempt to multiplex signature bytes into stdout.
- What happens when SPDX 3 conformance validator (m078) rejects the
  document after adding the new document-scope Annotation for
  Generation Context? Gate the m078 CI job on the annotation being
  semantically valid `Annotation` per SPDX 3.0.1; block the merge if
  `spdx3-validate==0.0.5` regresses.
- What happens when a downstream consumer expects the CISA element
  vocabulary ("before build" / "build" / "after build") but waybill
  emits its more-specific vocabulary (`filesystem-scan` etc.)? Per
  CISA page 9: "General software lifecycle references such as
  'before build,' 'build,' and 'after build,' as well as more
  specific identifiers, can satisfy this element." Waybill satisfies
  the element as-is; the alias emission in US3 acceptance scenario 1
  is a courtesy, not a compliance requirement.

## Requirements

### Functional Requirements

- **FR-001**: The coverage matrix MUST enumerate every one of the
  17 data-field elements (9 SBOM Metadata + 8 Component Data) and 6
  practices/processes elements listed in the CISA 2026 publication.
- **FR-002**: For each data-field element, the matrix MUST record
  waybill's coverage status (✅ native / ⚠️ annotation-only or
  format-implicit / ❌ absent) separately for CycloneDX 1.6,
  SPDX 2.3, and SPDX 3.0.1.
- **FR-003**: For each ✅ verdict, the matrix MUST cite the emitter
  source location (`file:line`) that populates the field.
- **FR-004**: For each ⚠️ or ❌ verdict, the matrix MUST record the
  reason (annotation used in lieu of native field / format has no
  native slot / not emitted) and link to the follow-up user story or
  document a rationale for accepting the gap.
- **FR-005**: For each Practice or Process element, the matrix MUST
  explain that CISA classifies it as an organizational practice
  (not a payload field) and identify the waybill behavior(s) that
  enable an operator to satisfy the practice.
- **FR-006**: The matrix MUST be reproducible: for every ✅ verdict,
  a `jq`/`yq` recipe applied to a fresh `waybill scan` output MUST
  extract a non-empty value at the cited path.
- **FR-007**: waybill MUST accept two mutually-exclusive opt-in
  signing flags: `--sign` (Sigstore keyless — the documented default
  path, invokes OIDC + Fulcio + Rekor) and `--sign-key <PATH>` (static
  key material — filesystem path to a PEM-encoded ECDSA P-256,
  Ed25519, or RSA private key). When both are passed, the CLI MUST
  reject the combination at parse time. Cloud-KMS URI and PKCS#11
  references are out-of-scope for this feature (deferred to a
  follow-up milestone per plan.md § Follow-ups).
- **FR-007a**: When `--sign` is set, the CycloneDX emitter MUST
  populate its document-root `signature` object with a Sigstore
  Bundle (protobuf-JSON encoding of the bundle format used by
  `cosign sign-blob --bundle`).
- **FR-007b**: When `--sign-key` is set, the CycloneDX emitter MUST
  populate its document-root `signature` object conforming to the
  CycloneDX 1.6 JSF (JSON Signature Format) schema, signed with the
  referenced key material.
- **FR-008**: When either signing flag is set, waybill MUST also
  emit a detached signature envelope alongside each SPDX 2.3 and
  SPDX 3 output at a stable filename (`<output>.sig.bundle.json`
  for keyless, `<output>.sig.json` for static-key DSSE), because
  neither SPDX version has a native in-document envelope-signature
  slot.
- **FR-008a**: When either signing flag is set AND `--output` is
  `-` (stdout), waybill MUST reject the invocation at CLI parse
  time with a diagnostic that names both flags and directs
  operators to supply `--output <file>`. Rationale: signing
  without a durable output path defeats the signature's purpose
  (nothing to hand a verifier), and multiplexing signature bytes
  into stdout has no standard framing.
- **FR-009**: When neither signing flag is set, all three emitters
  MUST produce byte-identical output to today's goldens (no
  regression).
- **FR-009a**: When signing was requested (via `--sign` or
  `--sign-key`) but a signature cannot be produced (Fulcio/Rekor
  unreachable, OIDC flow rejected, key material unreadable, KMS
  auth failure, PKCS#11 device error, algorithm rejected, etc.),
  waybill MUST exit non-zero with a diagnostic that names the
  underlying failure class, and MUST NOT emit any SBOM or sidecar
  file to a caller-supplied `--output <path>`. Partially-written
  files MUST be cleaned up (unlink on failure). Silent unsigned
  fallback is prohibited.
- **FR-010**: The SPDX 2.3 emitter MUST carry the current
  `ScanArtifacts.generation_context` value at document scope as an
  `Annotation` on `SPDXRef-DOCUMENT` per SPDX 2.3 § 8.5.
  (`documentComment` was considered and rejected during clarify —
  see research §R3 — because free-form prose is not reliably
  machine-parseable by downstream consumers.)
- **FR-011**: The SPDX 3 emitter MUST carry the current
  `ScanArtifacts.generation_context` value at document scope as a
  top-level `Annotation` element with `subject: <SpdxDocument @id>`,
  `annotationType: "other"`, and `@id` computed content-addressed
  per milestone 010. (An embedded `CreationInfo` annotation was
  considered and rejected during plan phase — see research §R4 —
  because `CreationInfo` is shared across many elements and
  document-scope semantics would be lost.)
- **FR-012**: The document-scope Generation Context values from FR-010
  and FR-011 MUST also emit a `waybill:cisa-2026-lifecycle` alias
  (`before-build` / `build` / `after-build`) mapped from the waybill
  variant, to give consumers who key on CISA vocabulary a direct
  lookup.
- **FR-013**: waybill MUST accept an opt-in CLI flag `--sbom-version
  <N>` where `N` is a positive integer, matching CDX 1.6's
  `metadata.version` schema (`{"type": "integer", "minimum": 1}`).
  The value carries into all three emitters: CDX `metadata.version`
  as an integer, SPDX 2.3 as a `waybill:sbom-version=<N>` key
  appended to the FR-010 doc-scope Annotation `comment`, SPDX 3 as
  the same key appended to the FR-011 top-level Annotation
  `statement`. When unset, CDX `metadata.version` remains `1`
  (today's default); the SPDX Annotation containers still emit
  generation-context per FR-010/011, but the `waybill:sbom-version`
  key=value pair is suppressed.
- **FR-014**: `--sbom-version` MUST reject non-integer values
  (`2.0`, `v2`, `latest`, empty string, embedded whitespace or
  control characters) and values less than `1`, at CLI parse time.
  CISA 2026 § SBOM Version allows semver at the *author's*
  discretion, but CDX 1.6 schema forbids it in this slot; the
  integer-only constraint is the intersection.
- **FR-015**: The coverage document MUST be committed at
  `docs/cisa-2026-coverage.md` and MUST include a header row citing
  the CISA publication (title, publication date, TLP:CLEAR
  designation).
- **FR-016**: The coverage document MUST call out that the 2026
  publication drops SWID from Machine-Processable Data — waybill
  emits neither SWID nor was there ever a plan to; this is a
  no-change advisory row, not a code-path change.
- **FR-017**: An integration test MUST assert, against a scan of the
  milestone-090 fixture repo, that every ✅ verdict in the matrix
  passes its `jq`/`yq` recipe. A regression that empties a native
  field MUST fail the test.

### Key Entities

- **CISA 2026 Element**: One of 17 data-field elements or 6
  practice/process elements from the CISA publication. Attributes:
  category (Metadata / Component / Practice), definition text,
  change-class-vs-2021 (New / Major Update / Minor Update /
  Removed / Unchanged).
- **Emitter Coverage Verdict**: A tuple `(element, emitter,
  status, evidence)` where emitter ∈ {CDX 1.6, SPDX 2.3, SPDX 3.0.1},
  status ∈ {✅, ⚠️, ❌}, and evidence is either a source citation
  (`file:line`) or a "not emitted" note plus follow-up user-story
  link.
- **CISA Practice Mapping**: For each Practice element, the
  waybill behaviors that let an operator satisfy the practice
  (e.g., "Frequency" is satisfied by waybill regenerating on every
  invocation with a fresh serialNumber; "Distribution and Delivery"
  is satisfied by waybill emitting to `--output <path>` or stdout
  and leaving delivery to the operator's pipeline).

## Success Criteria

### Measurable Outcomes

- **SC-001**: 100% of the 17 CISA 2026 data-field elements have a
  documented coverage verdict per emitter — 51 cells filled in the
  matrix (17 × 3), each with either a source citation or a follow-up
  link.
- **SC-002**: 100% of the 6 CISA 2026 practice/process elements have
  a documented waybill-satisfies-this-by explanation.
- **SC-003**: A downstream evaluator can determine, in under 5
  minutes of reading, which CISA elements waybill satisfies today
  and which require the follow-up work described in User Stories 2–4.
- **SC-004**: After US2 lands, running the CycloneDX signature
  verification CLI (Sigstore `cosign verify-blob` or equivalent JSF
  tool) against a signed waybill CDX output succeeds with exit code
  0, and mutation of any byte in the CDX payload causes exit code
  non-zero.
- **SC-005**: After US3 lands, `jq -r '.annotations[] |
  select(.annotator | test("waybill"))'` (SPDX 2.3) and the SPDX 3
  equivalent path each return a Generation Context value at
  document scope on every fresh scan.
- **SC-006**: After US4 lands, `waybill scan --sbom-version 2.0`
  produces outputs where the value 2.0 is discoverable at a
  documented path in all three formats.
- **SC-007**: All existing golden tests continue to pass unmodified
  when the new features are opt-in and disabled by default. Regen
  is required only for the FR-010/FR-011 document-scope Generation
  Context annotations, and the regen is limited to the golden
  test files enumerated in memory
  `feedback_release_bump_regen_all_golden_tests`.

## Assumptions

- CycloneDX 1.6 remains the target CDX version; upgrading to a
  newer CDX version is out of scope for this feature.
- SPDX 3.0.1 remains the target SPDX 3 version; upgrading is out of
  scope.
- Signing infrastructure will reuse the milestone-006 sbomit-suite
  key-management surface (Sigstore keyless preferred; static
  key material accepted). No new crate dependencies at plan time —
  verified during Phase 0 research.
- The coverage document is a living reference — it must be updated
  when a subsequent CISA publication or a subsequent waybill
  milestone changes any cell in the matrix. That maintenance
  policy lives in the doc's header.
- Practices & Processes are out of scope for code changes: they
  describe how the *operator* uses waybill, not what waybill
  emits. The matrix documents this framing explicitly to prevent
  future reviewers from filing "gap" tickets against practice
  elements.
- The 2026 CISA drop of SWID from Machine-Processable Data has zero
  code impact — waybill never emitted SWID. Document as a no-change
  row and move on.
- Elements that CISA marks "may include" but not "must include"
  (e.g., OmniBOR / SWHID in Component Identifiers, proprietary
  license conditions in Component License) are opt-in per operator
  policy and do not block a ✅ verdict for the baseline element.
