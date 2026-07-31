# Quickstart — Sigstore keyless SBOM signing (US2b)

**Feature**: 222-sigstore-keyless-signing
**Audience**: platform teams running waybill in CI, developers with
a `cosign login` habit, procurement/compliance stakeholders
verifying signed SBOMs.

---

## v1 scope note (READ FIRST)

waybill's `--sign` uses `sigstore-rs 0.11` under the hood.
sigstore-rs 0.11 requires OIDC tokens to emit a non-optional
`email` claim, which it uses as the CSR subject sent to Fulcio.
That means:

- **Compatible providers** (v1): any OIDC provider that emits
  `email` — `cosign login` (backed by Sigstore-dex), Google, GitLab,
  most SSO/dex configurations.
- **NOT supported in v1**: GitHub Actions ambient tokens
  (`ACTIONS_ID_TOKEN_REQUEST_URL` + `ACTIONS_ID_TOKEN_REQUEST_TOKEN`).
  GHA tokens use `sub` (workflow path) instead of `email`.

Full GHA-ambient support is deferred to a follow-up milestone — it
requires ~30–50 LOC upstream sigstore-rs changes (make Claims.email
Optional AND make the CSR subject issuer-aware). GHA users today
should use a helper action that fetches a compatible token (see §1B).

---

## 1A. Local laptop / non-GHA CI (dominant path)

```bash
# 1. Fetch an OIDC token via cosign — opens a browser once, then
#    exports a token good for ~15 minutes. Sigstore-dex emits `email`.
export SIGSTORE_ID_TOKEN=$(cosign login --identity-token)

# 2. Sign the SBOM
waybill sbom scan \
    --path ./my-project \
    --format cyclonedx-json \
    --output signed.cdx.json \
    --sign
```

## 1B. Inside GitHub Actions (helper-action pattern)

```yaml
jobs:
  build-and-sign-sbom:
    runs-on: ubuntu-latest
    permissions:
      id-token: write     # for the helper action that fetches the token
      contents: read
    steps:
      - uses: actions/checkout@<sha>

      - name: Fetch email-carrying OIDC token via sigstore-python
        # sigstore-python handles GHA's OIDC issuer-aware claim
        # dispatch and produces a token in the shape sigstore-rs 0.11
        # can consume. We only need the token, not the signing step.
        uses: sigstore/gh-action-sigstore-python@<sha>
        with:
          dry-run: true

      - name: Sign SBOM with waybill
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

Verify the helper action's version supports the "fetch a token,
export it, don't sign" mode you need. Alternatives: any Github
Action that produces an email-emitting JWT and exports it as
`SIGSTORE_ID_TOKEN` works.

## 1C. What happens under the hood

Per `contracts/keyless-signing-flow.md`:

1. waybill scans the target normally.
2. Just before serialization, it reads `SIGSTORE_ID_TOKEN` (fetched
   in step 0 by cosign or a helper action).
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
- `SIGSTORE_ID_TOKEN` missing/malformed → `SigningError::OidcTokenError`,
  exit non-zero, partial output unlinked.
- Fulcio down → `SigningError::FulcioError`, ditto.
- Rekor timeout (30s default, tune via `WAYBILL_REKOR_TIMEOUT_SECS`)
  → `SigningError::RekorError`, ditto.
- Never silently emits an unsigned SBOM.

## 2. Verify a keyless-signed SBOM

Use `cosign` (any recent version — bundle spec v0.3 is stable):

```bash
cosign verify-blob \
  --bundle signed.cdx.json \
  --certificate-identity '<expected OIDC subject>' \
  --certificate-oidc-issuer '<expected OIDC issuer>'
```

The `certificate-identity` matches whatever waybill logged as
`fulcio_cert_subject` (typically the OIDC token's `email` claim
value). `certificate-oidc-issuer` is your OIDC provider's issuer URL
(e.g., `https://oauth2.sigstore.dev/auth` for sigstore-dex,
`https://accounts.google.com` for Google, etc.).

**Interactive browser flow and GitHub Actions ambient OIDC** are
both deferred to a follow-up milestone. If you run `--sign` with no
`SIGSTORE_ID_TOKEN` env-var, you get a fail-close diagnostic
pointing at the workarounds above.

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
  fulcio_cert_subject=you@example.com
  oidc_provider=explicit-env
```

SREs auditing signature provenance can:
- Look up the Rekor entry directly:
  `rekor-cli get --log-index 12345678`
- Confirm the signing identity matches expectations without
  re-parsing the SBOM Bundle.
- Verify which OIDC provider path fired — `oidc_provider=explicit-env`
  is the only supported value in v1 (see scope note above).

No new SBOM annotations for these fields (Q3 decision) — the log is
the audit surface, the Bundle-in-SBOM carries the same info in
machine-verifiable form.
