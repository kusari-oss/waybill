# Contract: Identity resolution and verification-command emission

**Feature**: 779-signing-identity-shapes

## 1. Token → signer identity (fork side)

Input: a decoded JWT claim set. Output: the identity string Fulcio will
place in the certificate SAN, or a typed failure.

```text
resolve(claims):
    claim_name = ISSUER_MAP.get(claims.iss)  or  "sub"
    value      = claims[claim_name]
    if value is absent or empty:
        FAIL  IdentityTokenError{
                looked_for: claim_name,
                issuer:     claims.iss,
                present:    <names of claims actually present>
              }
    return value
```

`ISSUER_MAP` (research.md R2):

| Issuer | Claim |
|---|---|
| `https://accounts.google.com` | `email` |
| `https://oauth2.sigstore.dev/auth` | `email` |
| `https://oauth2.sigstage.dev/auth` | `email` |
| `https://token.actions.githubusercontent.com` | `sub` |
| *default* | `sub` |

**Failure is pre-network** (FR-003, Principle III). The error names the
claim looked for, the issuer that selected it, and which claims the token
actually carried — the three facts that turn today's
`Malformed JWT: claims JSON malformed` into something actionable.

The resolved value goes into the CSR subject attribute. Per research.md
R1 Fulcio ignores that attribute; it is populated only because the
protocol requires a subject, and it uses the `EMAIL_ADDRESS` OID
regardless of shape to stay byte-identical with sigstore-python.

## 2. Certificate → recorded identity (waybill side)

Input: the issued leaf certificate. Output: `SignerIdentity`.

```text
classify(cert):
    issuer = cert.oidc_issuer_extension
            or FAIL CryptoError("cert has no OIDC issuer extension")
    for name in cert.subject_alternative_names:
        RFC822Name(v) -> return Email{ address: v, issuer }
        URI(v)        -> return Workload{ subject: v, issuer }
        other(v)      -> return Unrecognized{ value: v, issuer }
    FAIL CryptoError("cert SAN carries no usable entry")
```

Order matters only when a certificate carries several SAN entries;
first-match preserves today's `extract_fulcio_cert_subject` behaviour.

**The certificate is the authority, not the token.** A token-derived
record could disagree with what a verifier checks — under federation it
demonstrably does (research.md R3).

## 3. Environment → verifier

```text
classify_environment(fulcio_url, rekor_url):
    both match production endpoints -> Production
    both match staging endpoints    -> Staging
    otherwise                       -> Custom
```

A mixed pair (production Fulcio, staging Rekor) is `Custom`. It is a
misconfiguration, and emitting a confident command for it would be worse
than emitting a labelled template.

## 4. Verification-command emission

**Production**

```bash
cosign verify-blob \
    --bundle <sidecar> \
    --certificate-identity '<subject>' \
    --certificate-oidc-issuer '<issuer>' \
    <artifact>
```

**Staging**

```bash
sigstore --staging verify identity \
    --bundle <sidecar> \
    --cert-identity '<subject>' \
    --cert-oidc-issuer '<issuer>' \
    <artifact>
```

**Custom** — same shape as production, `is_template = true`, prefixed with
a line stating that the operator must supply `--trusted-root` for their
deployment (FR-009c).

Rules:

- Every substituted value comes from `SignerIdentity`, which came from
  the certificate. No value is taken from the token or from operator
  input (FR-009a).
- Subject and issuer are single-quoted. Workload subjects contain `:` and
  `/`, and an unquoted paste is a support ticket.
- `Unrecognized` still yields a command, built from the verbatim value.

## 5. Log record

```text
INFO waybill::attestation::signer: SBOM signed via Sigstore keyless
  rekor_log_index=<u64>
  fulcio_cert_subject=<subject>        # name and meaning unchanged, FR-008
  fulcio_cert_identity_shape=<email|workload|unrecognized>   # new
  fulcio_cert_oidc_issuer=<issuer>     # new
  oidc_provider=<github-actions-ambient|explicit-env>
```

Additive only. The three existing field names keep their spelling because
operators and the m809 assertions grep them.

Note for test authors: `tracing` writes ANSI escapes between a field name
and its `=`. Assertions must strip them — the m222 FR-016 test asserted
`contains("rekor_log_index=")` and was wrong for a year because it was
`#[ignore]`d and never ran.
