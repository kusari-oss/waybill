# Quickstart — Sigstore keyless SBOM signing (US2b)

**Feature**: 222-sigstore-keyless-signing
**Audience**: platform teams running waybill in CI, developers with
a `cosign login` habit, procurement/compliance stakeholders
verifying signed SBOMs.

Two tasks. Do them in order.

---

## 1. Sign an SBOM keylessly in GitHub Actions

The dominant use case. Add `permissions: id-token: write` to the job
that runs waybill and pass `--sign`:

```yaml
jobs:
  build-and-sign-sbom:
    runs-on: ubuntu-latest
    permissions:
      id-token: write   # unlocks GitHub Actions ambient OIDC
      contents: read
    steps:
      - uses: actions/checkout@<sha>
      - name: Generate signed SBOM
        run: |
          waybill sbom scan \
            --path ./my-project \
            --format cyclonedx-json \
            --output signed.cdx.json \
            --sign
      - uses: actions/upload-artifact@<sha>
        with:
          name: signed-sbom
          path: signed.cdx.json
```

What happens under the hood (per
`contracts/keyless-signing-flow.md`):

1. waybill scans the target normally.
2. Just before serialization, it hits the ambient OIDC endpoint
   (`ACTIONS_ID_TOKEN_REQUEST_URL` + `ACTIONS_ID_TOKEN_REQUEST_TOKEN`
   with `audience=sigstore`) to fetch a JWT.
3. Posts the JWT + an ephemeral P-256 pubkey to Fulcio; receives
   a short-lived (~10 min) x509 cert.
4. Signs the SBOM canonical bytes with the ephemeral private key.
5. Uploads {cert, signature} to Rekor as a `hashedrekord` entry;
   waits up to 30s for the inclusion-proof.
6. Assembles a Sigstore Bundle and embeds it in
   `metadata.signature` (CDX) or a `<output>.sig.bundle.json`
   sidecar (SPDX).
7. Logs three fields at INFO for post-hoc audit:
   `rekor_log_index`, `fulcio_cert_subject`, `oidc_provider`.

**Failure surface** (per m221 FR-009a — inherited):
- OIDC endpoint unreachable → `SigningError::OidcTokenError`, exit
  non-zero, partial output unlinked.
- Fulcio down → `SigningError::FulcioError`, ditto.
- Rekor timeout (30s default, tune via `WAYBILL_REKOR_TIMEOUT_SECS`)
  → `SigningError::RekorError`, ditto.
- Never silently emits an unsigned SBOM.

## 2. Verify a keyless-signed SBOM

Use `cosign` (any recent version — bundle spec v0.3 is stable):

```bash
cosign verify-blob \
  --bundle signed.cdx.json \
  --certificate-identity 'https://github.com/<org>/<repo>/.github/workflows/<name>.yml@refs/heads/main' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

The `certificate-identity` matches the OIDC subject the token
carried (visible in your waybill INFO log as `fulcio_cert_subject`).
`certificate-oidc-issuer` for GitHub Actions is always
`https://token.actions.githubusercontent.com`.

## Alternative: sign with a pre-fetched token (outside GitHub Actions)

If you're on a laptop or a CI system without GitHub Actions'
ambient OIDC (e.g. self-hosted Jenkins), fetch a token with
`cosign login` first, then export + pass to waybill:

```bash
# One-time (or refresh every 15 minutes)
export SIGSTORE_ID_TOKEN=$(cosign login --identity-token)

waybill sbom scan \
  --path ./my-project \
  --format cyclonedx-json \
  --output signed.cdx.json \
  --sign
```

waybill's `OidcProvider::detect()` picks up `SIGSTORE_ID_TOKEN`
and uses it directly — no network call to fetch a token, just the
Fulcio + Rekor round-trips.

**Note**: the interactive browser flow (`waybill --sign` alone
without either env-var setup) is **deferred to a follow-up
milestone**. If you run `--sign` with neither ambient nor explicit
env, you get:

```text
Error: SigningError::OidcTokenError:
  no OIDC token available; set SIGSTORE_ID_TOKEN (e.g. via
  `cosign login`) or run inside GitHub Actions with
  `id-token: write`. Interactive browser flow is deferred to
  a follow-up milestone.
```

## SPDX multi-format sign

Same shape as m221 US2a — waybill emits sidecars for SPDX
outputs (SPDX has no in-document signature slot):

```bash
waybill sbom scan \
  --path ./my-project \
  --format cyclonedx-json --output signed.cdx.json \
  --format spdx-2.3-json --output signed.spdx.json \
  --format spdx-3-json   --output signed.spdx3.json \
  --sign
# Produces:
#   signed.cdx.json                    (Bundle in metadata.signature)
#   signed.spdx.json                    (unchanged SPDX 2.3)
#   signed.spdx.json.sig.bundle.json    (Bundle sidecar for SPDX 2.3)
#   signed.spdx3.json                   (unchanged SPDX 3)
#   signed.spdx3.json.sig.bundle.json   (Bundle sidecar for SPDX 3)
```

Verify each individually with `cosign verify-blob --bundle
<sidecar>` — for SPDX, the sidecar contains the Bundle; the SPDX
document itself is the signed payload.

## What this feature does NOT change

- Default path (no `--sign`, no `--sign-key`) is byte-identical
  to today's goldens (FR-015 + m221 FR-009 both enforced).
- `--sign-key <PEM>` (m221 US2a) path is unchanged. Operators
  with static-key workflows keep working exactly as they did.
- eBPF layer untouched.
- **Zero new Cargo dependencies at any lockfile layer.** Sigstore
  CTFE public keys are vendored as `&'static [u8]` DER SPKI at
  `waybill-cli/vendor/sigstore/ctfe_{prod,stage}.der` — the
  `sigstore-trust-root-*` feature is NOT enabled (audit at Phase 0
  R1 showed it transitively pulls `aws-lc-rs`, violating
  Constitution Principle I). See `docs/sigstore-trust-keys.md` for
  the vendoring recipe + rotation policy (~1x/year, ~30 min per
  rotation).

## Post-shipping audit trail

Every successful `--sign` invocation surfaces three grep-able
fields at INFO level in waybill's own log:

```text
INFO waybill::attestation::signer: SBOM signed via Sigstore keyless
  rekor_log_index=12345678
  fulcio_cert_subject=https://github.com/kusari-oss/waybill/.github/workflows/release.yml@refs/tags/v0.1.0
  oidc_provider=github-actions-ambient
```

SREs auditing signature provenance can:
- Look up the Rekor entry directly:
  `rekor-cli get --log-index 12345678`
- Confirm the signing identity matches expectations without
  re-parsing the SBOM Bundle.
- Verify which OIDC provider path fired (audit trail for CI
  workflow drift).

No new SBOM annotations for these fields (Q3 decision) — the log is
the audit surface, the Bundle-in-SBOM carries the same info in
machine-verifiable form.
