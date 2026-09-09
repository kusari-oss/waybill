# Quickstart: keyless signing identities

**Feature**: 779-signing-identity-shapes

## Signing in GitHub Actions (the new path)

No token fetch, no helper action, no secret:

```yaml
permissions:
  id-token: write     # required — without it there is no ambient token
  contents: read

steps:
  - uses: actions/checkout@<sha>
  - run: |
      waybill sbom scan --path . \
        --format cyclonedx-json --output signed.cdx.json \
        --sign
```

waybill detects the ambient credential, exchanges it for a short-lived
certificate, and writes `signed.cdx.json` plus
`signed.cdx.json.sig.bundle.json`.

The recorded identity is a **workload**, not a person — the certificate
subject is the workflow reference, e.g.
`repo:kusari-oss/waybill:ref:refs/heads/main`. Verification recipes that
expect an email address will not match it.

## Signing locally (unchanged)

```bash
export SIGSTORE_ID_TOKEN=$(sigstore get-identity-token)
waybill sbom scan --path . --format cyclonedx-json \
    --output signed.cdx.json --sign
```

Recorded identity is an **email**. Behaviour is unchanged from before this
feature.

## Verifying

waybill prints the command on success. Run it as printed:

```text
INFO waybill::attestation::signer: SBOM signed via Sigstore keyless
  rekor_log_index=48211903
  fulcio_cert_subject=repo:kusari-oss/waybill:ref:refs/heads/main
  fulcio_cert_identity_shape=workload
  fulcio_cert_oidc_issuer=https://token.actions.githubusercontent.com
  oidc_provider=github-actions-ambient

To verify:
  cosign verify-blob \
      --bundle signed.cdx.json.sig.bundle.json \
      --certificate-identity 'repo:kusari-oss/waybill:ref:refs/heads/main' \
      --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
      signed.cdx.json
```

**Do not reconstruct this command by hand from the token.** Under
federation the token's `iss` is the federating endpoint while the
certificate records the upstream provider; signing through Sigstore's dex
with a Google account yields a token claiming `https://oauth2.sigstage.dev/auth`
and a certificate claiming `https://accounts.google.com`. Supplying the
token's value produces

```text
Certificate's OIDCIssuer does not match (got ..., expected ...)
```

which reads like a broken signature and is not one. The emitted command is
built from the certificate precisely so this cannot happen.

### Staging

Signs against staging get a sigstore-CLI command instead, because cosign
has no built-in staging mode and would need a trusted-root file:

```bash
sigstore --staging verify identity \
    --bundle signed.cdx.json.sig.bundle.json \
    --cert-identity '<subject>' \
    --cert-oidc-issuer '<issuer>' \
    signed.cdx.json
```

## Running the keyless tests

Unattended, in CI, on the schedule — nothing to do by hand.

Locally, they remain identity-gated and report as `ignored` when the gate
is unmet:

```bash
export WAYBILL_TEST_KEYLESS=1
export SIGSTORE_ID_TOKEN=$(sigstore --staging get-identity-token)
cargo +stable test --workspace --test cisa_2026_signing -- --ignored
```

A skipped keyless test reports `ignored`, never `ok`. That is deliberate:
a skipped test that reads as a pass is how five tests went a year without
anyone noticing they had never run.

## When a scheduled run fails

It opens one issue, not one per night, stating whether the cause was an
external Sigstore outage or a waybill defect. The issue closes itself when
a later run succeeds.
