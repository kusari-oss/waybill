# Phase 0 Research: Extensible signing identity + unattended keyless tests

**Feature**: 779-signing-identity-shapes
**Date**: 2026-09-08

All findings below were established from artifacts on this machine and in
this repository. Where a finding is inference from a reference
implementation rather than a live observation, it says so explicitly and
names the probe that must still be run.

---

## R1 — Does Fulcio derive the certificate SAN from the token's claims, or from the CSR subject the client sends?

**Decision**: From the token's claims. The CSR subject attribute is
cosmetic and Fulcio ignores it.

**Evidence**: sigstore-python 4.2.0 — the reference client that
`gh-action-sigstore-python` wraps, and which demonstrably signs from
GitHub Actions — builds its CSR at `sign.py:144-161` as:

```python
x509.CertificateSigningRequestBuilder()
    .subject_name(x509.Name([
        x509.NameAttribute(NameOID.EMAIL_ADDRESS,
                           self._identity_token._identity),
    ]))
```

For a GitHub Actions token, `_identity` resolves to the `sub` claim
(`oidc.py:117-131`), which is a workflow reference such as
`repo:org/repo:ref:refs/heads/main` — categorically not an email address.
sigstore-python nonetheless places it in an `EMAIL_ADDRESS` attribute and
Fulcio issues a correct certificate. The only consistent explanation is
that Fulcio reads the identity from the OIDC token, not from the CSR.

**Consequence for this feature**: the fork change is small. It does not
need per-shape subject encoding; it needs the claim to be optional and
*some* value in the subject attribute. This retires the largest sizing
risk carried in the spec.

**Still required**: this is strong inference from a reference client, not
a live observation. A live probe against staging with a token lacking
`email` MUST run before User Story 1 is called done. It no longer gates
the design, only the sign-off.

**Alternatives considered**: fetching Fulcio's server source or spec.
Rejected as unnecessary — a working reference client that provably sends
a non-email value in an email attribute is stronger evidence about
deployed behaviour than a spec document is.

---

## R2 — Which claim identifies the signer?

**Decision**: Adopt the Sigstore ecosystem's issuer-keyed claim map.
**This corrects an assumption in the spec.**

The spec assumed "where a token carries both a personal and a workload
claim, the personal claim is preferred". That is not how the ecosystem
behaves. sigstore-python `oidc.py:38-43` keys the choice off the
*issuer*:

| Issuer | Identity claim |
|---|---|
| `https://accounts.google.com` | `email` |
| `https://oauth2.sigstore.dev/auth` | `email` |
| `https://oauth2.sigstage.dev/auth` | `email` |
| `https://token.actions.githubusercontent.com` | `sub` |
| *(anything else)* | `sub` |

The docstring at `oidc.py:178-186` states this "corresponds to the
Sigstore ecosystem's behavior, e.g. in each issued certificate's SAN."

**Rationale**: an issuer-keyed map makes waybill's recorded identity match
the certificate's SAN *by construction*. A claim-priority rule would
diverge from the SAN for any issuer whose token carries both claims but
whose Fulcio configuration maps to the other one — and a recorded identity
that disagrees with the certificate is precisely the defect SC-004 exists
to prevent.

**Spec impact**: the Assumptions section must be amended. Recorded as a
required spec edit, not silently absorbed.

---

## R3 — Which issuer goes into the verification command?

**Decision**: Read it from the issued certificate. Cross-check against the
token where available.

Under federation the token's `iss` is the federating endpoint (Sigstore's
dex) while the certificate records the upstream provider
(`https://accounts.google.com`). Passing the token's `iss` produces
`Certificate's OIDCIssuer does not match`, which reads like a broken
signature and is not one — the failure this project has already paid for
once.

sigstore-python surfaces the upstream issuer from the token via
`federated_claims.connector_id` (`oidc.py:143-152`), so the token *can*
answer the question. The certificate is still the better source because it
is what the verifier compares against; the token claim is a useful
consistency check, not the authority. FR-009 already mandates the
certificate; this research confirms it is both correct and non-obvious.

---

## R4 — Which verifier can verify which environment?

**Decision**: Production → cosign. Staging → the sigstore CLI. Confirmed
by inspection of both tools on this machine.

- `cosign verify-blob --help` offers `--trusted-root <file>` for
  non-production Sigstore instances but has **no built-in staging mode**;
  using it against staging requires the operator to already hold a staging
  trusted-root JSON.
- The sigstore CLI takes `--staging` directly. This is why the existing
  m809 harness invokes it (`cisa_2026_signing.rs:281-297`).
- Every operator-facing doc, plus `release.yml` and `test-signing.yml`,
  uses `cosign verify-blob`.

This is the concrete basis for FR-009b. Emitting a cosign command after a
staging sign would hand the operator something that does not run as
printed — the exact failure FR-009a forbids.

---

## R5 — Where does the identity type live, and does `#[non_exhaustive]` do anything?

**Decision**: Define it in `waybill-cli/src/attestation/signer.rs`
alongside `KeylessSignSuccess`. Apply `#[non_exhaustive]`. Do **not**
export the module to make the attribute meaningful.

**Honest statement of effect**: `waybill-cli/src/lib.rs` exports only
`parity`, `binding`, `identifiers`, and `testing`. `attestation` is
reachable only from the binary. `#[non_exhaustive]` has **no effect
within the defining crate** — same-crate matches may still be exhaustive.
So today the attribute is intent-signalling, not enforcement.

It is still worth applying: it costs nothing, it documents the contract
FR-006 states, and it is already correct on the day the module is
exported. Exporting `attestation` *solely* to give the attribute teeth
would be scope creep for no user-visible gain, and is rejected.

FR-006 is therefore cheap to satisfy today. The plan says so rather than
implying the enum is load-bearing compatibility machinery.

---

## R6 — Shape of the fork patch

**Decision**: three edits in `kusari-sandbox/sigstore-rs`, kept minimal and
upstreamable.

1. `src/oauth/token.rs` — `Claims.email` becomes `Option<String>`; add
   `sub: Option<String>` and `federated_claims`.
2. `src/oauth/token.rs` — add identity resolution mirroring R2's
   issuer-keyed map, exposed as an accessor.
3. `src/bundle/sign.rs::materials` (lines 84-101) — the subject currently
   hardcodes `token.unverified_claims().email.as_ref()` into an
   `EMAIL_ADDRESS` attribute. Take the resolved identity instead. Per R1
   the attribute is ignored by Fulcio, so the OID need not change; keeping
   it matches sigstore-python byte-for-byte.

Edit 3 is the one missed in the first read of this problem: making the
claim optional alone moves the failure from parse time to CSR-build time.

**Upstream**: this is a genuine upstream bug (OIDC Core requires `sub`, not
`email`). A contribution is worth making but requires separate explicit
approval per project policy, and is not on this feature's critical path.

---

## R7 — Scheduled run and failure reporting

**Decision**: reuse the `ebpf-canary.yml` mechanism verbatim.

`.github/workflows/ebpf-canary.yml` already implements exactly what
FR-015a describes: exact-title dedupe (`:134`), search-then-comment rather
than open-a-second (`:176-197`), and a close-on-recovery path (`:211+`).
Copying a mechanism already load-bearing in this repo beats writing a
second one.

---

## R8 — The GitHub Actions ambient token path is not implemented

**Discovery — this is additional scope beyond the fork patch.**

`resolve_identity_token` (`waybill-cli/src/attestation/signer.rs:311-333`)
implements only `OidcProvider::Explicit`. The `GitHubActions` arm returns a
hard error. The ambient exchange — calling `ACTIONS_ID_TOKEN_REQUEST_URL`
with `audience=sigstore`, bearing `ACTIONS_ID_TOKEN_REQUEST_TOKEN` — has
never been written.

So User Story 1 is two pieces of work, not one: patch the fork *and*
implement the ambient fetch. The fork patch alone would leave waybill
still unable to sign in CI without a helper action.

`OidcProvider::detect()` already returns `GitHubActions` when the env vars
are present, so the dispatch site exists; only the fetch is missing.

---

## Summary of impacts on the spec

| Finding | Spec impact |
|---|---|
| R1 | Confirms the flagged assumption; live probe still required before US1 sign-off |
| R2 | **Corrects** the "personal claim preferred" assumption → issuer-keyed map |
| R5 | FR-006 is cheaper than implied; recorded honestly rather than restated as-is |
| R8 | US1 grows to include the ambient token fetch |
