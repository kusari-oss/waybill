# Phase 0: Research — CISA 2026 SBOM Minimum Elements coverage audit

**Feature**: 221-cisa-2026-elements-audit
**Date**: 2026-07-29
**Status**: Complete

This document resolves the technical unknowns identified in the plan's
Technical Context and the two commitments made in the Constitution
Check. Each item follows the Decision / Rationale / Alternatives shape.

---

## R1 — Verify sigstore 0.11 + enabled features remain C-clean (Principle I)

**Decision**: Reuse the existing `sigstore = "0.11"` dependency at
`waybill-cli/Cargo.toml:141` with the exact feature set already
enabled: `default-features = false, features = ["cosign-rustls-tls",
"fulcio-rustls-tls", "rekor-rustls-tls", "bundle"]`. No feature
additions. No version bump.

**Rationale**: Milestone 089 already vetted this configuration
against Principle I: `cargo tree -p waybill --target
x86_64-unknown-linux-gnu -e normal` produces zero hits on
`openssl-sys`, `libz-sys`, `aws-lc-rs`, `native-tls`. The Waybill
constitution's Principle I audit note at
`docs/architecture/signing.md:70–77` documents this verification
posture. Adding the new SBOM-level signing surface for this feature
does not toggle any additional sigstore features (`bundle` already
present for keyless Bundle emission per US2 acceptance 1), so the
audit result is unchanged. A CI job will re-run the `cargo tree`
grep as part of the tasks list per FR-009 (byte-identity of unsigned
goldens implies no new transitive deps in the unsigned path).

**Alternatives considered**:
- **Bump to sigstore 0.13+** — rejected: forces `aws-lc-rs` via `cert`
  feature, violates Principle I (per m089 audit note).
- **Wrap our own signing on top of `ring` or `p256`** — rejected:
  reinvents cert-chain building + JSF encoding, more code + more
  attack surface, no upside over reusing sigstore-rs which is already
  a workspace dep.

<!-- verified: 2026-07-29 — cargo tree -p waybill -e normal produced 968 dep lines; grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|native-tls' returned zero hits. sigstore="0.11" with features ["cosign-rustls-tls","fulcio-rustls-tls","rekor-rustls-tls","bundle"] confirmed at waybill-cli/Cargo.toml:161. Principle I audit passes. -->

---

## R2 — Sigstore Bundle vs. JSF in the CDX `signature` slot (FR-007a / FR-007b)

**Decision**: Emit format-dependent payloads into CDX's document-root
`signature` slot:
- `--sign` (keyless) → **Sigstore Bundle** protobuf-JSON encoding
  (the `bundle` sigstore-rs feature already emits this shape for
  `cosign sign-blob --bundle`). Bundle wraps: Fulcio-issued cert,
  DSSE-shaped signature, Rekor entry.
- `--sign-key` (static) → **JSF (JSON Signature Format,
  draft-cyberphone-jsf-00)** conforming to CycloneDX 1.6's
  `signature` object schema. JSF ships as an untagged JSON object
  with `algorithm`, `publicKey`, `value` fields.

**Rationale**: CDX 1.6's `signature` schema
(`https://cyclonedx.org/schema/bom-1.6.schema.json#/definitions/signature`)
is loosely typed — it accepts any JSON object. JSF is the CDX-native
happy path (matches `cyclonedx-cli sign` output), and JSF is what
downstream tools like `cdxgen`'s verifier expect. Sigstore Bundle
strictly speaking is not JSF, but CDX permits it in the slot; the
tradeoff is that verifier tooling for keyless mode is
Sigstore-native (`cosign verify-blob --bundle`) rather than CDX-
native. This is acceptable because keyless signing is inherently
Sigstore-integrated; anyone opting into `--sign` is opting into the
Sigstore verifier stack. This split matches the split we're forced
into for SPDX anyway (no in-doc slot, sidecar always required), so
the mental model is uniform: keyless → Sigstore ecosystem, static
key → format-native ecosystem.

**Alternatives considered**:
- **JSF for both** — rejected: JSF has no defined way to embed a
  Rekor entry or a Fulcio-issued short-lived cert; keyless verifiers
  need both. Emitting JSF for keyless would strand consumers who
  need to verify against a transparency log.
- **Sigstore Bundle for both** — rejected: static-key operators
  (BSI, air-gapped deployments, PKCS#11 hardware tokens) don't want
  a Sigstore-shaped envelope; they want a JSF that any RFC 7515-aware
  tool can verify.
- **Always sidecar, never in-document** — rejected: fails CISA's
  intent that SBOM Author Signature is "attributable to the SBOM
  author" as a first-class element. CDX gives us the slot; not using
  it forfeits the compliance win.

**Documentation deliverable**: `docs/reference/sbom-format-mapping.md`
gets a new row noting the split emission path (Sigstore Bundle for
keyless, JSF for static) — this is Principle V's audit-trail
requirement even though the slot itself is native (the choice of
payload within the slot is the parity decision).

---

## R3 — SPDX 2.3 document-scope annotation mechanism (FR-010)

**Decision**: Emit generation-context as an `Annotation` on
`SPDXRef-DOCUMENT` with:
- `annotationType: "OTHER"`
- `annotator: "Tool: waybill-<version>"`
- `annotationDate: <ScanArtifacts.created>` (RFC 3339)
- `comment: "waybill:generation-context=<variant>;
  waybill:cisa-2026-lifecycle=<alias>"` (structured key-value pairs,
  matches existing waybill annotation conventions per m080 + m111 +
  m145)

Use `Annotation`, not `documentComment`, because:
1. `documentComment` is free-form prose and consumers can't reliably
   parse structured data out of it.
2. `Annotation` is explicitly designed for machine-readable metadata
   per SPDX 2.3 § 8.5.
3. Waybill already emits document-scope annotations for
   `waybill:file-inventory-mode` (m133) and `waybill:sbom-type`
   (m081); this reuses the pattern.

**Rationale**: SPDX 2.3 has no native document-level lifecycle field.
The m052 `LifecycleScopeType` in SPDX 3 is per-relationship, not
per-document. Per Principle V bullet 5, when no native field exists,
a `waybill:*` annotation is permitted; this row goes into
`docs/reference/sbom-format-mapping.md`. The structured
key-semicolon-value pattern parses cleanly with `jq -r
'.annotations[] | select(.annotator | test("waybill")) | .comment'`
followed by a shell split — the recipe goes into US1's coverage doc.

**Alternatives considered**:
- **New `spdxDoc.lifecycleContext` property** — rejected: SPDX 2.3
  schema is closed; unknown top-level keys break the sbomqs
  validator + LF SPDX tools.
- **Emit as `packages[SPDXRef-Root].primaryPackagePurpose`** —
  rejected: `primaryPackagePurpose` is a component-level classifier
  (APPLICATION / LIBRARY / etc.), semantically wrong for lifecycle
  context.
- **Emit via `creationInfo.comment`** — rejected: same free-form
  prose problem as `documentComment`; harder for tools to key on.

---

## R4 — SPDX 3 document-scope CreationInfo Annotation + spdx3-validate 0.0.5 tolerance (FR-011)

**Decision**: Emit generation-context as a top-level SPDX 3
`Annotation` element with:
- `@type: "Annotation"`
- `@id: <content-addressed IRI per m011>`
- `annotationType: "other"` (SPDX 3 uses lowercase `other`, distinct
  from SPDX 2.3's uppercase `OTHER`)
- `subject: <SpdxDocument @id>` (the document-scope binding)
- `creationInfo: <ref to shared CreationInfo>` (matches existing
  emission)
- `statement: "waybill:generation-context=<variant>;
  waybill:cisa-2026-lifecycle=<alias>"`

**Rationale**: The m078 `spdx3-validate==0.0.5` pinned validator
accepts any well-formed `Annotation` element whose `subject` and
`creationInfo` IRIs resolve within the same @graph. The
`annotationType` enum in SPDX 3.0.1 includes `other`
(https://spdx.github.io/spdx-spec/v3.0.1/annexes/annotationtype/),
so we are within the enum. A quick spike (write a doc with the
proposed Annotation shape, run through `.venv/spdx3-validate/bin/
spdx3-validate --input <path>`) will confirm this in the Phase 2
tasks; if the validator surfaces a warning about `other` (deprecated
in a future SPDX 3.1?), fall back to `review` per the enum's
review/document/other trio.

**Rationale for content-addressed IRI**: Matches milestone 011's
IRI-generation policy — `@id` is a sha256 of the annotation's
canonical bytes, so re-running the same scan emits a byte-identical
annotation and doesn't churn goldens. Uses the existing
`v3_id_type_map.rs` machinery.

<!-- validator-result: 2026-07-29 ok — `.venv/spdx3-validate/bin/spdx3-validate --json <regenerated-cargo.spdx3.json>` exits 0 after adding the `waybill:cisa-2026-lifecycle` doc-scope Annotation. Schema + SHACL checks both pass. No fallback to `annotationType: "review"` required. -->


**Alternatives considered**:
- **Emit as `SpdxDocument.comment`** — rejected: `comment` is
  free-form; same parsing problem as SPDX 2.3.
- **Add to `CreationInfo.comment`** — rejected: `CreationInfo` is
  shared across many elements; document-scope semantics are lost.
- **Skip the CISA-alias emission (FR-012), only emit the waybill
  variant** — rejected: US3 acceptance 1 requires both.

---

## R5 — waybill `GenerationContext` → CISA lifecycle vocabulary mapping (FR-012)

**Decision**: Fixed mapping table, encoded as a `fn as_cisa_2026_lifecycle(&self) -> &'static str`
on the existing `GenerationContext` enum in
`waybill-common/src/attestation/metadata.rs`:

| `GenerationContext` variant (wire) | CISA 2026 vocabulary alias |
|-----------------------------------|-----------------------------|
| `build-time-trace`                | `build`                     |
| `filesystem-scan`                 | `after-build`               |
| `container-image-scan`            | `after-build`               |

**Rationale**:
- CISA § SBOM Generation Context (page 9): "General software
  lifecycle references such as 'before build,' 'build,' and 'after
  build,' as well as more specific identifiers, can satisfy this
  element. For example, an SBOM generated from source code could be
  identified as 'before build,' and binary analysis tools can
  generate an SBOM 'after build.'"
- Waybill's `build-time-trace` observes syscalls during compilation
  → CISA's `build`.
- Waybill's `filesystem-scan` and `container-image-scan` both scan
  post-build artifacts (source tree with lockfiles, or a built
  container image) → CISA's `after-build`.
- Waybill has no `before-build` variant today because that would
  require source-only mode with no build execution — a `sbom design`
  mode that would map to `before-build` is out of scope for this
  feature but reserved for future work.

**Alternatives considered**:
- **Map `filesystem-scan` → `before-build`** — rejected: filesystem
  scans typically hit post-build artifacts (installed packages,
  built binaries in `target/`). Calling that "before build" is
  misleading.
- **Emit both variants (waybill-native and CISA-vocab) separately,
  no mapping** — rejected: forces consumers to keep two
  vocabularies in mind; the whole point of the alias is to give
  consumers who key on CISA vocab a direct answer.

---

## R6 — Complete the milestone-006 keyless-signing flow (FR-007a)

**Decision**: Finish the `sign_keyless()` implementation in
`waybill-cli/src/attestation/signer.rs` (currently scaffolded per
line ~170 comment: "v1: keyless flow requires live network calls
to Fulcio + Rekor. Return a structured error rather than crashing")
using sigstore-rs 0.11's `SigstoreClient`:

1. Resolve OIDC token via existing `OidcProvider::detect()` at
   `signer.rs` (GitHubActions ambient token, Explicit token file,
   or Interactive browser flow).
2. POST to Fulcio `/api/v2/signingCert` with the OIDC token +
   ephemeral P-256 pubkey → receive short-lived cert.
3. Sign SBOM canonical bytes with the corresponding P-256 privkey
   using `SigStoreSigner`.
4. POST cert + signature to Rekor `/api/v1/log/entries` (type:
   `hashedrekord`) → receive inclusion proof.
5. Assemble Sigstore Bundle (protobuf-JSON): `mediaType:
   application/vnd.dev.sigstore.bundle+json;version=0.3`,
   `verificationMaterial.x509CertificateChain.certificates`,
   `messageSignature.messageDigest.algorithm: SHA2_256`,
   `messageSignature.messageDigest.digest`, `messageSignature.
   signature`, `verificationMaterial.tlogEntries`.
6. Return `SbomSignatureEnvelope::Keyless(SigstoreBundle)`.

**Rationale**: sigstore-rs 0.11 exposes all these primitives via
`sigstore::fulcio::FulcioClient`, `sigstore::rekor::apis::
EntriesApi`, `sigstore::bundle::Bundle`. The `bundle` feature
already enabled at Cargo.toml:141 gives us the Bundle
serializer. The FR-009a fail-close contract means any of steps 1–5
returning an error surfaces as `SigningError::{FulcioError,
RekorError, OidcError, ...}` with the CLI exit code 1 + partial
output cleanup.

**Complexity note**: This is the highest-risk item in the plan.
The scaffolded state means we need to write ~150 LOC of Fulcio +
Rekor integration plus the bundle assembly. `sigstore-rs` 0.11's
API surface here is stable but under-documented; the reference
implementation is `cosign sign-blob --bundle`, source at
`sigstore/cosign@main/cmd/cosign/cli/sign/sign_blob.go`, which we
port piecewise. Estimate: 2 days of implementation + 1 day of
integration testing against Sigstore's staging environment.

**Alternatives considered**:
- **Shell out to `cosign sign-blob`** — rejected: violates
  Principle I (introduces Go binary as build/runtime dep) and
  Principle IV (no typed error propagation across process
  boundary).
- **Ship US2 with static-key only, defer keyless** — considered
  but rejected: the CISA text explicitly recognizes Sigstore as
  best-practice signing infrastructure; shipping without keyless
  makes US2 a partial win. If the Fulcio/Rekor implementation
  slips, the plan-phase task decomposition can split into
  US2a (static) + US2b (keyless) — but the spec commits to both.

---

## R7 — SBOM Version emission points across the three emitters (FR-013)

**Decision**:

| Emitter | Slot | Wire encoding |
|---------|------|---------------|
| CDX 1.6 | `metadata.version` | JSON integer (schema-typed `{"type": "integer", "minimum": 1}`) |
| SPDX 2.3 | `Annotation` on `SPDXRef-DOCUMENT` | `annotationType: "OTHER"`, `comment: "waybill:sbom-version=<N>"` |
| SPDX 3 | Top-level `Annotation` element with `subject: <SpdxDocument @id>` | `annotationType: "other"`, `statement: "waybill:sbom-version=<N>"` |

**Rationale**:
- CDX's `metadata.version` slot is exactly what CISA describes:
  "Identifier designated by the SBOM author to specify a change
  in the SBOM document." Directly native. No alias needed.
- SPDX 2.3 has no native SBOM-version field. The `Annotation`
  route is the only spec-compliant path per Principle V (bullet 5
  audit: `Package.versionInfo` is component-version, not
  SBOM-version; `documentNamespace` is content-addressed identity,
  not a monotonic counter). Reuses the same annotation pattern as
  R3.
- SPDX 3 same as SPDX 2.3 — reuse the R4 annotation shape.

**Rationale for reusing the same annotation container for both
generation-context (R3/R4) and sbom-version**: reduces the number
of Annotation elements per document (SPDX 3's spdx3-validate is
sensitive to per-run element count for perf reasons); combining
into one annotation with two semicolon-separated key=value pairs
keeps the element count flat when both features are used.

**Alternatives considered**:
- **Emit sbom-version as `documentComment`** — rejected: same
  free-form-prose issue as R3.
- **Coerce non-integer values by parsing semver → major integer** —
  rejected in `/speckit.clarify` Q3. Integer only per CDX schema.

---

## R8 — Test infrastructure for US2 (static + keyless integration tests)

**Decision**:
- **Static-key tests**: generate an ephemeral P-256 keypair per
  test using `sigstore::crypto::signing_key::SigStoreKeyPair::
  new(SigningScheme::ECDSA_P256_SHA256_ASN1)`, sign an SBOM,
  verify with the matching pubkey. Runs unprivileged in
  standard CI (Principle VII satisfied).
- **Keyless tests**: gated behind `WAYBILL_TEST_KEYLESS=1` env
  var. In CI (GitHub Actions), pair with `id-token: write`
  permission and use the ambient OIDC token. Locally, tests skip
  with `INFO: keyless tests skipped; set WAYBILL_TEST_KEYLESS=1`.
  Point at Sigstore staging (`https://fulcio.sigstage.dev`,
  `https://rekor.sigstage.dev`) to avoid polluting the production
  Rekor log with test entries.

**Rationale**: Matches the m006 test pattern verbatim
(`waybill-cli/src/attestation/signer.rs:359+`). No new fixtures
required for static-key path; keyless path needs a CI-only
GitHub Actions job (call it `lint-and-test-keyless-sbom`) with
`id-token: write` and network access to sigstage.

**Alternatives considered**:
- **Mock Fulcio + Rekor with wiremock** — rejected initially: the
  bundle-assembly path exercises real x509 cert parsing and Rekor
  inclusion-proof crypto; mocking would need to build a whole
  test CA. Real Sigstore staging is simpler and matches
  production behavior. Reserved as fallback if sigstage
  rate-limits become an issue.

---

## R9 — Constitution amendment path (Principle V "CISA 2025" → "CISA 2026")

**Decision**: Out-of-scope for this branch. File a follow-up
constitution PR after this milestone lands:

- Change: `Principle V` line reference "CISA 2025 Minimum Elements"
  → "CISA 2026 Minimum Elements"
- Semver bump: 2.0.0 → 2.1.0 (MINOR: expanded normative content —
  2026 is a strict superset of 2025)
- SYNC IMPACT REPORT note: "Motivating case: milestone 221
  (this feature). Constitution updated to reflect the current
  target compliance baseline; no principle removed or reinterpreted."

**Rationale**: A constitution edit inside a feature branch conflates
compliance-tool evolution with the tool's own build; better to keep
the constitution amendment as a separate reviewable diff so future
maintainers can find it via `git log .specify/memory/`. The
implementation in this branch already targets 2026 per spec — the
constitution catches up after the fact.

---

## Summary of resolved unknowns

| Plan Technical Context item | Status | Resolved by |
|-----------------------------|--------|-------------|
| sigstore 0.11 C-cleanliness for new signing surface | ✅ | R1 (reuse m089 audit) |
| CDX `signature` slot payload format | ✅ | R2 (Bundle for keyless, JSF for static) |
| SPDX 2.3 doc-scope annotation mechanism | ✅ | R3 (Annotation on SPDXRef-DOCUMENT) |
| SPDX 3 doc-scope annotation mechanism + validator tolerance | ✅ | R4 (Annotation @type at top level) |
| waybill → CISA lifecycle vocab mapping | ✅ | R5 (3-row mapping table) |
| Completing scaffolded keyless flow | ⚠️ high-risk | R6 (Fulcio + Rekor + Bundle implementation) |
| SBOM Version emission points | ✅ | R7 (native CDX, annotation for SPDX) |
| Test infra for static + keyless | ✅ | R8 (ephemeral keys + sigstage) |
| Constitution 2025→2026 update | 📤 deferred | R9 (out-of-scope; follow-up PR) |

All NEEDS CLARIFICATION items in the plan are resolved. R6 carries
implementation risk documented for the Phase 2 tasks generator.
