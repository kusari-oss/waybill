# Contract: OIDC provider dispatch (feature 222 US2b)

**Consumer surface**: `waybill-cli/src/attestation/signer.rs`
**Function**: `resolve_identity_token(&OidcProvider) -> Result<IdentityToken, SigningError>`

Documents the exact behavior each provider variant MUST implement.

---

## Provider selection order (unchanged from m006)

`OidcProvider::detect()` — pre-existing at `signer.rs:66` — selects
by walking this order and returning the first match:

1. **GitHubActions** — iff both `ACTIONS_ID_TOKEN_REQUEST_URL` AND
   `ACTIONS_ID_TOKEN_REQUEST_TOKEN` are set in the process env.
2. **Explicit** — iff `SIGSTORE_ID_TOKEN` is set (regardless of
   whether GitHubActions env is also present; the ambient path
   takes precedence per m006 order).
3. **Interactive** — fallback when neither GitHubActions env nor
   `SIGSTORE_ID_TOKEN` is set.

**Behavior per Q1 clarification**: v1 supports GitHubActions +
Explicit; Interactive is a fail-close branch.

---

## Provider: `GitHubActions`

**Input**:
- `ACTIONS_ID_TOKEN_REQUEST_URL`: base URL for GitHub's OIDC
  endpoint (`https://token.actions.githubusercontent.com/...`).
- `ACTIONS_ID_TOKEN_REQUEST_TOKEN`: opaque bearer token for the
  endpoint.

**Behavior**:
1. HTTP GET to `<url>&audience=sigstore` (the `&` prefix is safe;
   GitHub's URL always contains query params).
2. Bearer authorization header with the token.
3. Response JSON shape: `{"value": "<JWT string>"}` per GitHub
   Actions OIDC docs.
4. Parse via `IdentityToken::try_from(response.value.as_str())` to
   validate JWT-shape (3 dot-separated base64 segments).

**Timeouts**:
- Connect timeout: 10s.
- Total request timeout: 30s (soft cap; GitHub's OIDC endpoint
  responds in <1s in normal operation).

**Failure modes** → `SigningError::OidcTokenError` with `detail:`
naming the specific failure class:

| Failure | Detail wording |
|---------|----------------|
| Env var unset (either) | `"ACTIONS_ID_TOKEN_REQUEST_URL/TOKEN env vars missing — did you set permissions: id-token: write?"` |
| HTTP non-2xx | `"GitHub OIDC endpoint returned {status}: {body head}"` |
| HTTP timeout | `"GitHub OIDC endpoint timed out after 30s"` |
| Malformed JSON response | `"GitHub OIDC response missing 'value' field: {raw}"` |
| Malformed JWT | `"IdentityToken parse failed: {sigstore-rs error}"` |
| Network / DNS error | `"GitHub OIDC endpoint unreachable: {reqwest error}"` |

**No retries** in v1. GitHub Actions OIDC is a reliable
GitHub-infra service; per-request retry adds complexity without
solving a real problem. If a specific CI environment sees repeated
failures, the caller can wrap invocation with their own retry
logic.

---

## Provider: `Explicit`

**Input**:
- `SIGSTORE_ID_TOKEN`: raw JWT string (usually fetched via
  `cosign login` or a Sigstore-aware token-issuing tool).

**Behavior**:
1. Read `std::env::var("SIGSTORE_ID_TOKEN")`.
2. Parse via `IdentityToken::try_from(value.as_str())`.

**No network I/O** — this path is a pure env-var read + JWT-shape
validation.

**Failure modes** → `SigningError::OidcTokenError` with `detail:`:

| Failure | Detail wording |
|---------|----------------|
| Env var unset | `"SIGSTORE_ID_TOKEN env var not set"` |
| Malformed JWT | `"IdentityToken parse failed for SIGSTORE_ID_TOKEN: {sigstore-rs error}"` |
| Expired token (via `token.in_validity_period()`) | `"SIGSTORE_ID_TOKEN is expired (exp: {claims.exp})"` |

---

## Provider: `Interactive` (v1: fail-close only)

Per Q1 clarification, US2b v1 does NOT implement the browser flow.
Instead, `resolve_identity_token(&OidcProvider::Interactive)`
returns:

```rust
Err(SigningError::OidcTokenError {
    detail: "no OIDC token available; set SIGSTORE_ID_TOKEN \
             (e.g. via `cosign login`) or run inside GitHub Actions \
             with `id-token: write`. Interactive browser flow is \
             deferred to a follow-up milestone."
        .to_string(),
})
```

The caller (`sign_keyless()`) MUST propagate this error unchanged so
the CLI's fail-close cleanup fires (unlink partial output + exit
non-zero per m221 FR-009a).

**No TTY detection in v1**. The two supported providers both work
in non-TTY contexts; the Interactive variant is unreachable in
practice unless the operator has neither ambient nor explicit
setup — in which case the diagnostic points them at the fix.

---

## Success return value

On success, `resolve_identity_token(&provider)` returns
`sigstore::oauth::IdentityToken`. The subsequent Fulcio flow uses
`token.unverified_claims()` to read the JWT's OIDC subject +
issuer (for FR-016 INFO logging + downstream verifier's
`--certificate-identity` / `--certificate-oidc-issuer` matching).

The token's `in_validity_period()` MUST be checked before passing
to `SigningContext::blocking_signer()`; if the token is already
expired (rare — usually indicates clock skew or a stale
`SIGSTORE_ID_TOKEN`), fail-close with `SigningError::OidcTokenError`
naming the exp/nbf claims.

---

## Testing contract

Per FR-010 + FR-011, the integration test runs against Sigstore
staging with `WAYBILL_TEST_KEYLESS=1` gating. The test MUST cover:

1. **GitHubActions ambient path** — verified by the CI job
   `lint-and-test-keyless-sbom` (which runs with `id-token: write`).
   The test itself doesn't check "am I in GitHub Actions?"; it just
   runs `waybill sbom scan --sign ...` and asserts the emitted
   Bundle verifies.
2. **Explicit path** — a separate test case (`us2b_keyless_explicit_env_var`)
   runs `SIGSTORE_ID_TOKEN=$(cosign login ...)` outside of CI to
   validate the env-var-only path. In CI, this test is combined
   with the ambient case (`SIGSTORE_ID_TOKEN` env var populated
   from the ambient endpoint before running waybill), so both
   provider branches exercise their happy path per CI run.
3. **Interactive fail-close** — a unit test in `signer.rs::tests`
   with `OidcProvider::Interactive` passed directly asserts the
   returned error matches the FR-005/FR-009 diagnostic wording.
   No network dependency.

Failure-mode tests (env var missing, HTTP timeout, malformed JWT)
lean on mocking `reqwest` responses via a test-only injection
seam or (simpler) direct unit tests on the specific failure
branches without invoking the real reqwest client. Preference: the
simpler approach — each `SigningError::OidcTokenError` variant is
covered by a unit test that manipulates env vars directly and
observes the returned diagnostic.
