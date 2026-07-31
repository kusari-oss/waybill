# Feature Specification: Sigstore keyless SBOM signing (completes m221 US2b)

**Feature Branch**: `222-sigstore-keyless-signing`
**Created**: 2026-07-30
**Status**: Draft
**Input**: User description: "us2b"

## Clarifications

### Session 2026-07-30

- Q: Interactive browser OIDC flow — ship in v1 or defer? → A: Ambient + explicit only in v1; interactive returns `SigningError::OidcTokenError` with a diagnostic pointing operators at `SIGSTORE_ID_TOKEN` + `cosign` to fetch a token. Rationale: every practical keyless-signing deployment is either a CI runner (ambient) or an operator with a pre-fetched token (explicit); browser-launch UX + OAuth device flow adds ~200 LOC + hard-to-CI-test path for a niche developer-laptop use case.
- Q: Sigstore staging integration test — real network or mock backend? → A: Real Sigstore staging + isolated CI job (FR-010/FR-012 verbatim). Matches industry convention (cosign, chainguard, actions/attest all test against real sigstage); mock backends drift from real Sigstore over time and can hide regressions. Isolated `lint-and-test-keyless-sbom` job means staging outages don't block unrelated PRs; m221's gate-rerun playbook handles transient flake.
- Q: Success-signing observability — what should waybill log on a successful `--sign`? → A: Log Rekor log-index + Fulcio cert SAN subject + OIDC provider name at INFO on the invocation log; no new SBOM fields. Downstream consumers with the SBOM already have the full Bundle for offline verification; operators/SREs additionally need the same identifiers in their scan log for post-hoc grepping without re-parsing the SBOM. Avoids a new `waybill:sigstore-*` annotation (which would trigger the m071 parity-extractor gate + widen the C-catalog).

## Context

Milestone 221 (feature `221-cisa-2026-elements-audit`, PR #643, merged
as commit `2822ced`) closed 3 of the 4 identified gaps against the
CISA 2026 SBOM Minimum Elements baseline. **US2a** shipped static-key
JSF SBOM signing via `--sign-key <PATH>` for CDX in-document
signatures + DSSE sidecars for SPDX 2.3 / SPDX 3.

**US2b — Sigstore keyless signing — was deferred** and is the sole
remaining implementation gap on the CISA 2026 § SBOM Author Signature
element. The scaffolding is in place:

- CLI plumbing: `--sign` flag surface designed in
  `specs/221-cisa-2026-elements-audit/contracts/cli-flags.md`.
- Type surface: `SigningMode::Keyless { fulcio_url, rekor_url,
  oidc_provider }` variant reserved in
  `waybill-cli/src/sbom/signer.rs`; commented as "US2b" throughout.
- Attestation-signer scaffold: `sign_keyless()` at
  `waybill-cli/src/attestation/signer.rs:170` returns
  `Err(SigningError::KeylessNotImplemented)` — the OIDC-provider
  detector, error-class enum, and function signature are already
  wired to the CLI dispatch layer.
- Test scaffold: `us2b_keyless_bundle_sign_and_verify` in
  `waybill-cli/tests/cisa_2026_signing.rs` marked `#[ignore]`
  pending `WAYBILL_TEST_KEYLESS=1` + Sigstore staging network.

This specification completes the deferred work. The end state:
operators who pass `--sign` produce SBOMs whose signatures verify
via Sigstore's transparency log, without ever handling long-lived key
material — the modern default for CI-native supply-chain signing per
Sigstore adoption across the ecosystem (cosign, syft, chainguard,
GitHub Actions attestations).

## User Scenarios & Testing

### User Story 1 — Emit a Sigstore-keyless-signed SBOM in CI (Priority: P1)

A platform team running waybill inside GitHub Actions (or any OIDC-
capable CI environment) wants signed SBOMs published alongside each
release without provisioning, rotating, or protecting long-lived
signing keys. They pass `--sign` on the `waybill sbom scan`
invocation; waybill obtains an ambient OIDC token, requests a
short-lived Fulcio cert, signs the SBOM, publishes the signature to
Rekor for transparency, and emits a Sigstore Bundle. Downstream
policy consumers verify with `cosign verify-blob --bundle` against
Sigstore's public trust root.

**Why this priority**: This is the only remaining implementation
gap on CISA 2026's SBOM Author Signature element (row 2 of
`docs/cisa-2026-coverage.md`). Every other CISA 2026 data-field
element is at ✅ or ⚠️-satisfies-per-CISA-text. Closing US2b moves
waybill from "practically compliant with a static-key workaround"
to "unambiguously compliant across every signing path CISA calls
out."

**Independent Test**: An operator runs `waybill sbom scan
./target --sign --output signed.cdx.json` in a GitHub Actions job
with `permissions: id-token: write`. The emitted `signed.cdx.json`
contains a Sigstore Bundle in `metadata.signature`; `cosign
verify-blob --bundle signed.cdx.json --certificate-identity <the
job's OIDC subject> --certificate-oidc-issuer
https://token.actions.githubusercontent.com` returns exit code 0.
Mutation of any byte in the payload flips verify to non-zero.

**Acceptance Scenarios**:

1. **Given** a CI environment with `ACTIONS_ID_TOKEN_REQUEST_URL`
   + `ACTIONS_ID_TOKEN_REQUEST_TOKEN` set (the GitHub Actions ambient
   OIDC surface), **When** the operator runs `waybill sbom scan
   --sign --output <file>`, **Then** waybill acquires an OIDC token
   via the ambient endpoint, requests a Fulcio cert, signs the SBOM
   canonical bytes, uploads the signature+cert to Rekor, and emits
   a Sigstore Bundle at `metadata.signature` in the CDX output (+
   `<output>.sig.bundle.json` sidecar for SPDX 2.3 / SPDX 3).
2. **Given** `SIGSTORE_ID_TOKEN` set in the environment (operator-
   supplied pre-fetched token — the m006 `OidcProvider::Explicit`
   path), **When** the operator runs `--sign` without GitHub
   Actions context, **Then** waybill uses the pre-fetched token
   instead of triggering the ambient endpoint or a browser flow.
3. **Given** neither GitHub Actions env nor `SIGSTORE_ID_TOKEN`
   is set, **When** the operator runs `--sign`, **Then** waybill
   fails-close with `SigningError::OidcTokenError` and a diagnostic
   naming both env-var options and suggesting `cosign` as the
   token-fetching workaround for developer-laptop use cases.
   (Interactive browser flow is deferred to v2 per Clarifications
   Session 2026-07-30 — every practical keyless-signing deployment
   is either CI-native or has a pre-fetched token, and the
   browser-launch UX buys niche benefit for high test complexity.)
4. **Given** a produced Sigstore Bundle from any of the flows above,
   **When** a downstream verifier runs `cosign verify-blob
   --bundle <output> --certificate-identity <expected>
   --certificate-oidc-issuer <expected>` against Sigstore's
   production trust root, **Then** verify returns exit code 0.
5. **Given** a byte in the signed CDX or SPDX payload is mutated
   post-signing, **When** the same `cosign verify-blob` command
   runs, **Then** verify fails deterministically with a non-zero
   exit code and a signature-mismatch diagnostic.
6. **Given** `--sign` is set but the OIDC token acquisition fails
   (endpoint 500, browser flow rejected, ambient env vars absent
   in non-interactive context), **When** `waybill sbom scan` runs,
   **Then** waybill exits non-zero with a diagnostic that names
   the specific `SigningError` variant (`OidcTokenError`,
   `FulcioError`, `RekorError`, `CryptoError`), cleans up any
   partial `--output <path>` file, and MUST NOT emit an unsigned
   SBOM as a silent fallback.
7. **Given** `--sign` is set and Fulcio issues the cert but Rekor
   upload fails (transparency-log endpoint 5xx or timeout), **When**
   `waybill sbom scan` runs, **Then** waybill exits non-zero
   (transparency-log inclusion is a hard requirement for CI-native
   supply-chain policy, per Sigstore convention). Operators who want
   to sign without Rekor for testing MUST use `--sign-key <PEM>` +
   local signing (the m221 US2a path).

---

### Edge Cases

- **Sigstore staging vs production**: `WAYBILL_FULCIO_URL` +
  `WAYBILL_REKOR_URL` env vars must override the default endpoints
  (production Sigstore: `https://fulcio.sigstore.dev` +
  `https://rekor.sigstore.dev`). CI integration tests point at
  staging (`https://fulcio.sigstage.dev` +
  `https://rekor.sigstage.dev`) to avoid polluting production Rekor
  with test entries. This env-var contract is already documented in
  `specs/221-cisa-2026-elements-audit/contracts/cli-flags.md`; US2b
  implementation MUST honor it.
- **Ambient token available but wrong audience**: The GitHub Actions
  OIDC endpoint requires an `audience` parameter. Waybill must use
  `sigstore` as the audience (Sigstore convention) not the default
  GitHub-Actions audience. Wrong audience → Fulcio rejects the token
  → `SigningError::FulcioError`.
- **No ambient / no explicit token available**: Interactive browser
  flow is deferred to v2 per Clarifications Session 2026-07-30. When
  `--sign` is passed and neither GitHub Actions ambient env vars nor
  `SIGSTORE_ID_TOKEN` are set, waybill fails-close immediately with
  `SigningError::OidcTokenError` and a diagnostic naming both
  supported paths. TTY-detection logic is not needed in v1 because
  no code path launches a browser.
- **Long-running scans past OIDC token expiry**: OIDC tokens
  typically expire in 5–15 minutes. A scan that takes longer than
  that between token acquisition and Fulcio cert request must
  succeed if the token was valid at the moment of Fulcio submission
  (Fulcio evaluates at request time). Documented behavior: waybill
  fetches the OIDC token AS LATE AS POSSIBLE in the signing pipeline
  (right before Fulcio submission), NOT at CLI parse time, so
  long scans don't burn the token window on scan work.
- **Rekor inclusion-proof latency**: Rekor sometimes takes 2–5
  seconds to return an inclusion-proof after accepting an entry.
  Waybill must wait up to `WAYBILL_REKOR_TIMEOUT_SECS` (default 30
  seconds) for the inclusion-proof to arrive; timeout MUST surface
  as `SigningError::RekorError` per the fail-close contract.
- **Both `--sign` and `--sign-key` passed**: Already rejected at
  CLI parse per m221 FR-007. No new behavior needed; US2b just
  enables the `--sign` half of the mutual-exclusion pair.

## Requirements

### Functional Requirements

- **FR-001**: waybill MUST complete the scaffolded `sign_keyless()`
  function at `waybill-cli/src/attestation/signer.rs` (currently
  returns `Err(SigningError::KeylessNotImplemented)`) to perform
  the full Sigstore-keyless flow: OIDC token acquisition → Fulcio
  ephemeral cert issuance → sign SBOM canonical bytes → Rekor
  transparency-log inclusion → return signature material to the
  caller for Bundle assembly.
- **FR-002**: waybill MUST assemble a Sigstore Bundle
  (protobuf-JSON encoding at content-type
  `application/vnd.dev.sigstore.bundle+json;version=0.3`) containing
  the Fulcio-issued x509 certificate chain, the DSSE-shaped
  signature bytes, and the Rekor entry (log index + integrated
  time + inclusion proof). The Bundle shape MUST be verifiable by
  `cosign verify-blob --bundle` unmodified.
- **FR-003**: For CycloneDX outputs, waybill MUST populate the
  document-root `metadata.signature` slot with the Sigstore Bundle
  as a JSON object (matches m221 FR-007a).
- **FR-004**: For SPDX 2.3 and SPDX 3 outputs, waybill MUST emit a
  companion Sigstore Bundle sidecar at `<output>.sig.bundle.json`
  (matches m221 FR-008; the `.sig.bundle.json` extension
  distinguishes keyless bundles from the static-key DSSE
  `.sig.json` sidecars).
- **FR-005**: The OIDC-token acquisition path MUST support two
  provider variants in v1 (via the existing
  `waybill-cli/src/attestation/signer.rs::OidcProvider::detect()`):
  GitHub Actions ambient (`ACTIONS_ID_TOKEN_REQUEST_URL` +
  `ACTIONS_ID_TOKEN_REQUEST_TOKEN`) and explicit (`SIGSTORE_ID_TOKEN`
  env var). Interactive browser flow (`OidcProvider::Interactive`)
  is deferred to v2 per Clarifications Session 2026-07-30 —
  `sign_keyless()` MUST return `SigningError::OidcTokenError` with a
  diagnostic pointing operators at `SIGSTORE_ID_TOKEN` + `cosign` for
  the token fetch when `OidcProvider::detect()` returns
  `Interactive`.
- **FR-006**: Fulcio endpoint MUST default to
  `https://fulcio.sigstore.dev` and be overridable via
  `WAYBILL_FULCIO_URL`. Rekor endpoint MUST default to
  `https://rekor.sigstore.dev` and be overridable via
  `WAYBILL_REKOR_URL`. This matches the m221 `--sign` contract
  and enables staging integration testing.
- **FR-007**: Rekor transparency-log inclusion MUST be a hard
  requirement — if Rekor is unreachable or the inclusion-proof
  wait times out (default 30s, overridable via
  `WAYBILL_REKOR_TIMEOUT_SECS`), signing MUST fail with
  `SigningError::RekorError` and the caller MUST fail-close per
  m221 FR-009a (non-zero exit + unlink partial output).
- **FR-008**: OIDC token fetch MUST be deferred to as late in the
  signing pipeline as possible — the token is fetched immediately
  before Fulcio submission, NOT at CLI parse time — so long scans
  don't burn the token's validity window on scan work before
  reaching the signing step.
- **FR-009**: When `OidcProvider::detect()` returns `Interactive`
  in v1 (i.e., no ambient GitHub Actions env, no
  `SIGSTORE_ID_TOKEN`), signing MUST fail-close with
  `SigningError::OidcTokenError` naming "no OIDC token available;
  set SIGSTORE_ID_TOKEN (e.g. via `cosign login`) or run inside
  GitHub Actions with `id-token: write`. Interactive browser flow
  is deferred to a follow-up milestone." Partial output MUST be
  unlinked per m221 FR-009a. No TTY detection is required in v1
  since no code path launches a browser.
- **FR-010**: An integration test at
  `waybill-cli/tests/cisa_2026_signing.rs::us2b_keyless_bundle_sign_and_verify`
  MUST run the full keyless flow end-to-end against Sigstore
  staging (`https://fulcio.sigstage.dev` +
  `https://rekor.sigstage.dev`) and verify the resulting Bundle
  with `sigstore-rs`'s `CosignVerificationKey` primitives (or
  shell out to `cosign verify-blob --bundle` if the binary is
  available on `$PATH`).
- **FR-011**: The integration test MUST be gated behind
  `WAYBILL_TEST_KEYLESS=1` — absent this env var, the test emits
  an `INFO: us2b_keyless_bundle_sign_and_verify skipped
  (WAYBILL_TEST_KEYLESS unset)` diagnostic and returns green. This
  matches the m221 scaffold's `#[ignore]` posture and lets the
  general `cargo test --workspace` remain hermetic + fast.
- **FR-012**: A dedicated CI job `lint-and-test-keyless-sbom` in
  `.github/workflows/ci.yml` MUST run the integration test with
  `permissions: id-token: write, contents: read`, env
  `WAYBILL_TEST_KEYLESS=1 WAYBILL_FULCIO_URL=https://fulcio.sigstage.dev
  WAYBILL_REKOR_URL=https://rekor.sigstage.dev`. The job is
  independent from the primary `lint-and-test` job — a Sigstore-
  staging outage MUST NOT block PRs that don't touch signing code
  (existing green `--sign-key` path continues to work).
- **FR-013**: Coverage matrix row 2 (SBOM Author Signature) in
  `docs/cisa-2026-coverage.md` MUST be updated after US2b lands
  to reflect the change from "opt-in `--sign-key` only, `--sign`
  pending US2b" to "opt-in `--sign` (Sigstore keyless) OR
  `--sign-key <PATH>` (static)". Coverage-matrix integration
  test at `waybill-cli/tests/cisa_2026_coverage_matrix.rs` MUST
  continue to pass after the row 2 wording update.
- **FR-014**: When `--sign` produces a signed CDX output, the
  Sigstore Bundle at `metadata.signature` MUST include the leaf
  Fulcio cert plus at least the intermediate chain segments needed
  for verifier trust-root lookup (typically leaf +
  Fulcio-intermediate; the Sigstore root itself is expected to be
  in the verifier's pre-configured trust root, matching
  cosign / sigstore-rs `Verifier::production()` convention). This
  ensures downstream verifiers can walk from the embedded leaf to a
  known Sigstore trust anchor without out-of-band chain material.
- **FR-015**: When `--sign` is unset, all three emitters MUST
  produce byte-identical output to today's goldens (matches m221
  FR-009 no-regression contract). US2b MUST NOT touch the default
  emission path.
- **FR-016**: On successful `--sign`, waybill MUST log at INFO
  level three fields on the invocation log (via the existing
  `tracing::info!` machinery): the Rekor log-index of the entry
  (transparency-log lookup key), the Fulcio-issued cert's SAN
  subject (who signed), and the OIDC provider variant name
  (`github-actions-ambient` or `explicit-env`). These identifiers
  duplicate what's already inside the emitted Sigstore Bundle;
  the INFO log surfaces them for operator post-hoc grepping
  without requiring a Bundle re-parse. No new SBOM fields; no
  `waybill:sigstore-*` annotation (which would trigger the m071
  parity-extractor gate).

### Key Entities

- **Sigstore Bundle**: The v0.3 protobuf-JSON envelope containing
  Fulcio cert chain, DSSE-shaped signature bytes, and Rekor
  transparency-log entry. Shape defined by
  `https://raw.githubusercontent.com/sigstore/protobuf-specs/main/protos/sigstore_bundle.proto`;
  serialized to JSON via sigstore-rs 0.11's `bundle` feature.
- **OIDC Provider**: Enum variant capturing how waybill obtains
  an OIDC token — GitHub Actions ambient endpoint, explicit
  `SIGSTORE_ID_TOKEN` env var, or interactive browser flow.
  Reused from m006 `waybill-cli/src/attestation/signer.rs::OidcProvider`.
- **Fulcio Cert Response**: Short-lived (~10 minute validity) x509
  certificate issued by Fulcio in response to a valid OIDC token
  + ephemeral P-256 public key. Contains SAN extensions naming
  the OIDC subject + issuer so downstream verifiers can enforce
  identity constraints.
- **Rekor Log Entry**: The tuple returned by Rekor's
  `/api/v1/log/entries` endpoint on successful upload: log index
  (monotonically increasing), integrated timestamp, inclusion
  proof (Merkle-tree witnesses to the log's signed tree head).
  Enables offline verification of "this signature existed at
  Rekor time T without needing to trust Rekor at verify time."

## Success Criteria

### Measurable Outcomes

- **SC-001**: `cargo test -p waybill --test cisa_2026_signing
  us2b_keyless_bundle_sign_and_verify` returns exit 0 when
  `WAYBILL_TEST_KEYLESS=1` is set AND Sigstore staging endpoints
  are reachable AND an OIDC provider is available (GitHub Actions
  or `SIGSTORE_ID_TOKEN`).
- **SC-002**: The dedicated CI job `lint-and-test-keyless-sbom`
  on the feature branch passes at least 3 consecutive PR runs
  before merge — establishes a baseline flake rate against
  Sigstore staging.
- **SC-003**: After merge, `docs/cisa-2026-coverage.md` row 2
  (SBOM Author Signature) shows ⚠️ for all three emitters WITH
  BOTH `--sign` AND `--sign-key` cited as satisfying paths. Prior
  state cited only `--sign-key` with US2b as the follow-up.
- **SC-004**: An operator following the quickstart at
  `specs/221-cisa-2026-elements-audit/quickstart.md` Option A
  ("Sigstore keyless (recommended for CI / GitHub Actions)")
  produces a bundle that verifies via `cosign verify-blob --bundle`
  in under 30 seconds end-to-end.
- **SC-005**: `cargo +stable test --workspace --no-fail-fast`
  passes without setting `WAYBILL_TEST_KEYLESS=1` — the general
  test suite stays hermetic (no Sigstore network dep on the hot
  path).
- **SC-006**: `cargo +stable clippy --workspace --all-targets --
  -D warnings` reports zero errors + zero warnings — no dead-code
  warnings from the previously-scaffolded types, no unused-import
  warnings from the newly-wired sigstore-rs bundle module.
- **SC-007**: Default-path byte-identity preserved — running the
  full `cdx_regression` + `spdx_regression` + `spdx3_regression`
  suites without setting `--sign` produces zero golden churn.
- **SC-008**: On successful `--sign`, the invocation log contains
  exactly three INFO-level structured events (or one INFO event
  with three named fields) carrying `rekor_log_index`,
  `fulcio_cert_subject`, and `oidc_provider` — verifiable by
  grepping the CI job output OR by the integration test
  capturing waybill's stdout/stderr and asserting the fields
  are present with non-empty values.

## Assumptions

- **sigstore-rs 0.11 is sufficient**: The workspace already carries
  `sigstore = "0.11"` with features `cosign-rustls-tls`,
  `fulcio-rustls-tls`, `rekor-rustls-tls`, `bundle` (per m089 audit
  at `waybill-cli/Cargo.toml`). No version bump, no new features
  toggled, no new Cargo dependencies.
- **Sigstore staging is reachable from GitHub Actions runners**:
  Both `fulcio.sigstage.dev` and `rekor.sigstage.dev` are public
  HTTPS endpoints; GitHub-hosted runners can reach them without
  network configuration. If future policy changes block this, the
  integration test's CI-only gate lets the general test suite
  remain green while a fix is drafted.
- **The CDX 1.6 `metadata.signature` slot accepts a Sigstore
  Bundle JSON object**: The CDX schema is loosely typed at this
  slot (per m221 research §R2); no schema validator we test
  against rejects the shape. Verified during m221 US2a design.
- **OIDC audience `sigstore` is the correct GitHub Actions
  audience for Fulcio**: Sigstore convention across cosign,
  chainguard's tooling, and the actions/attest-build-provenance
  ecosystem. No project-specific audience needed.
- **Rekor entries can be created by test runs against Sigstore
  staging without rate-limit concerns**: Sigstore staging is
  deliberately provisioned for CI test loads. If a specific
  ecosystem-wide test wave triggers rate-limit issues,
  `WAYBILL_REKOR_URL` can point to a private Rekor for isolation
  (not shipped this milestone).
- **Constitution Principle I (Pure Rust, Zero C) is preserved**:
  Adding the Fulcio + Rekor calls uses sigstore-rs 0.11's existing
  `rustls-tls` HTTPS clients (no new native TLS dep). Re-verify
  via `cargo tree` in Phase 0 research (mirrors m221 R1 pattern).
- **US2b does not require re-signing existing SBOMs**: Operators
  who already published `--sign-key`-signed SBOMs keep those
  signatures valid. US2b adds a new signing path; it does not
  invalidate or migrate the old one.
- **The scaffold's existing type surface is close-to-final**: The
  m006-era `SigningIdentity::Keyless { fulcio_url, rekor_url,
  oidc_provider, transparency_log }` variant + the
  `SigningError::{OidcTokenError, FulcioError, RekorError,
  CryptoError}` variants are already used by the CLI dispatch
  layer. US2b fills the function body; it does not restructure
  the interface. If a specific variant proves inadequate during
  Phase 1 design, the enum extends additively (no breaking
  change).
