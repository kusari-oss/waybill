# Phase 0: Research — Sigstore keyless SBOM signing (US2b)

**Feature**: 222-sigstore-keyless-signing
**Date**: 2026-07-30
**Status**: Complete

Resolves the technical unknowns identified in the plan's Technical
Context. Each item follows the Decision / Rationale / Alternatives
shape.

---

## R1 — Vendored CTFE keys + `SigningContext::new()` (Principle I)

<!-- verified: 2026-07-30 -->

**Decision**: Ship `SigningContext::new(fulcio, rekor_config,
ctfe_keyring)` with a vendored CTFE keyring. Do NOT enable
`sigstore-trust-root-rustls-tls` — the feature transitively pulls
`aws-lc-rs` (violates Constitution Principle I).

**Empirical audit (T001, 2026-07-30)** — feature-toggle path FAILED:

```text
$ cargo tree -p waybill --target x86_64-unknown-linux-gnu -e normal \
    | grep -Ei 'openssl-sys|libz-sys|aws-lc-rs|native-tls|mbedtls-sys'
├── aws-lc-rs v1.17.1
├── aws-lc-sys v0.42.0
│   ├── cmake v0.1.58
│   └── pkg-config v0.3.33
(3 hits)
```

Root cause: `sigstore-trust-root-*` variants require the `tough`
TUF client (both `tough = 0.19` and `0.22` have unconditional
`[dependencies.aws-lc-rs]` — no `ring` alternative feature). No
clean feature-toggle workaround exists.

**Chosen path (this was R1-alt in the pre-audit plan)**:
`SigningContext::new()` is NOT gated by the trust-root feature — it
lives at `sigstore-0.11.0/src/bundle/sign.rs:268` and is reachable
with the base `bundle` + `fulcio-rustls-tls` + `rekor-rustls-tls`
features waybill already enables. `Keyring::new()` at
`sigstore/src/crypto/keyring.rs:89` accepts
`IntoIterator<Item = &'a [u8]>` where each `&[u8]` is DER-encoded
SPKI. Vendor Sigstore's production + sigstage CTFE public keys as
`&'static [u8]` and build the `Keyring` in-process.

**Concrete shape**:

```rust
// waybill-cli/src/attestation/sigstore_trust_root.rs (new module)
pub const SIGSTORE_PROD_CTFE_KEY_DER: &[u8] =
    include_bytes!("../../vendor/sigstore/ctfe_prod.der");
pub const SIGSTORE_STAGE_CTFE_KEY_DER: &[u8] =
    include_bytes!("../../vendor/sigstore/ctfe_stage.der");

pub fn ctfe_keyring(rekor_url: &str) -> Result<Keyring, SigningError> {
    let key: &[u8] = if rekor_url.contains("sigstage.dev") {
        SIGSTORE_STAGE_CTFE_KEY_DER
    } else {
        SIGSTORE_PROD_CTFE_KEY_DER
    };
    Keyring::new([key]).map_err(|e| SigningError::CryptoError {
        detail: format!("failed to build CTFE keyring: {e}"),
    })
}

// In sign_keyless():
let fulcio = FulcioClient::new(
    Url::parse(fulcio_url)?,
    TokenProvider::from(oidc_token.clone()),
);
let mut rekor_cfg = RekorConfiguration::default(); // rekor.sigstore.dev
if rekor_url != "https://rekor.sigstore.dev" {
    rekor_cfg.base_path = rekor_url.to_string();
}
let ctx = SigningContext::new(fulcio, rekor_cfg, ctfe_keyring(rekor_url)?);
```

**Key sourcing (path A — approved 2026-07-30)**:
1. Run `cosign initialize --mirror <sigstore-tuf-root-mirror>`
   locally to fetch + verify Sigstore's TUF-signed trust root.
2. Extract the CTFE public keys from `~/.sigstore/root/targets/`.
3. Convert to DER SPKI (`openssl pkey -pubin -inform PEM -outform
   DER`) and vendor at
   `waybill-cli/vendor/sigstore/ctfe_{prod,stage}.der`.
4. Document the vendoring recipe + rotation policy in
   `docs/sigstore-trust-keys.md` (cosign version + pinned trust-root
   root.json SHA + regen command).

**Rotation cost estimate**: Sigstore rotates the CTFE root
approximately once per year. Rotation delta is: rerun the vendoring
recipe, commit the two new DER files, bump the doc's "vendored:"
date. ~30 minutes of human effort per rotation cycle.

**Alternatives considered**:
- **Enable `sigstore-trust-root-rustls-tls`** — REJECTED per T001
  audit (Principle I violation via `tough` → `aws-lc-rs`).
- **Fork `tough` to swap `aws-lc-rs` for `ring`** — rejected:
  maintenance burden orders of magnitude larger than the ~30
  min/year vendoring cost.
- **Fetch trust root at runtime via a hand-rolled TUF client** —
  rejected: reinvents `tough` badly; net negative on both
  Principle I compliance AND attack surface.
- **Fetch from `sigstore/root-signing` GitHub repo (SHA-pinned)
  instead of cosign** — considered as path B; rejected in favor of
  path A because cosign's `initialize` performs the TUF signature
  verification, whereas raw curl trusts GitHub's transport.

---

## R2 — sigstore-rs 0.11 signing-flow API surface

**Decision**: Use `sigstore::bundle::sign::SigningContext` with the
blocking API, constructed via `SigningContext::new()` (see R1 for
why NOT `::production()`). Full flow:

```rust
use sigstore::bundle::sign::SigningContext;
use sigstore::fulcio::{FulcioClient, oauth::TokenProvider};
use sigstore::rekor::apis::configuration::Configuration as RekorConfiguration;
use sigstore::oauth::IdentityToken;
use url::Url;

let fulcio = FulcioClient::new(
    Url::parse(fulcio_url)?,
    TokenProvider::from(oidc_token.clone()),
);
let mut rekor_cfg = RekorConfiguration::default();
if rekor_url != "https://rekor.sigstore.dev" {
    rekor_cfg.base_path = rekor_url.to_string();
}
let ctx = SigningContext::new(fulcio, rekor_cfg, ctfe_keyring(rekor_url)?);
let session = ctx.blocking_signer(oidc_token)?;
let artifact = session.sign(&mut sbom_bytes_reader)?;
let bundle: sigstore::bundle::Bundle = artifact.to_bundle();
serde_json::to_writer(output, &bundle)?;
```

Ephemeral keypair generation, Fulcio cert issuance, signing, Rekor
upload, inclusion-proof wait, and Bundle assembly all happen inside
`session.sign()`. Verified against `sigstore-0.11.0/src/bundle/sign.rs`
and `sigstore-0.11.0/examples/bundle/main.rs` in the crate source
(the example uses `production()`; we swap in `new()` per R1).

**Rationale**: sigstore-rs 0.11 is the upstream-supported way to
consume Sigstore from Rust. Rolling our own protobuf assembly
against `sigstore-protobuf-specs` (as m221 research §R6 estimated
we'd need to) is a real waste of code — the library exists
specifically to hide that plumbing. This finding dramatically
shrinks the v1 diff from the ~150 LOC estimate to ~50 LOC of
signing-adapter code (plus ~50 LOC for OIDC-provider dispatch,
covered in R3).

**Alternatives considered**:
- **Hand-roll via `sigstore::fulcio` + `sigstore::rekor` +
  `sigstore::bundle::models` directly** — rejected: `SigningContext`
  is exactly the composition of those; using the composed API is
  the intended entrypoint.
- **Async API (`ctx.signer(token).await`)** — rejected for v1:
  the m006 `sign_local` code path is blocking (`sign_local()` at
  `attestation/signer.rs:162`), and consistency between static-key
  + keyless dispatch matters more than the ~50ms of savings from
  async I/O overlap. The `SigningContext::blocking_signer()` API
  is designed for exactly this consistency.

---

## R3 — OIDC token acquisition: ambient (GitHub Actions) + explicit

**Decision**: Two functions in
`waybill-cli/src/attestation/signer.rs`:

```rust
/// Explicit env var — trivial, no I/O.
fn identity_token_from_env_var() -> Result<IdentityToken, SigningError> {
    let raw = std::env::var("SIGSTORE_ID_TOKEN")
        .map_err(|_| SigningError::OidcTokenError { ... })?;
    IdentityToken::try_from(raw.as_str())
        .map_err(|e| SigningError::OidcTokenError { detail: e.to_string() })
}

/// GitHub Actions ambient — hit the ACTIONS_ID_TOKEN_REQUEST_URL
/// with the token in `ACTIONS_ID_TOKEN_REQUEST_TOKEN` and
/// audience=sigstore.
fn identity_token_from_github_actions() -> Result<IdentityToken, SigningError> {
    let url = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL")?;
    let bearer = std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN")?;
    let response: GitHubOidcResponse = reqwest::blocking::Client::new()
        .get(format!("{url}&audience=sigstore"))
        .bearer_auth(bearer)
        .send()?
        .error_for_status()?
        .json()?;
    IdentityToken::try_from(response.value.as_str()).map_err(...)
}

// Dispatcher — matches on OidcProvider::detect().
fn resolve_identity_token(provider: &OidcProvider) -> Result<IdentityToken, SigningError> {
    match provider {
        OidcProvider::GitHubActions => identity_token_from_github_actions(),
        OidcProvider::Explicit => identity_token_from_env_var(),
        OidcProvider::Interactive => Err(SigningError::OidcTokenError {
            detail: "no OIDC token available; set SIGSTORE_ID_TOKEN \
                     (e.g. via `cosign login`) or run inside GitHub \
                     Actions with `id-token: write`. Interactive \
                     browser flow is deferred to a follow-up milestone."
                .into(),
        }),
    }
}
```

**Rationale**:
- `IdentityToken::try_from(&str)` per sigstore-rs source at
  `oauth/token.rs` accepts a raw JWT and validates the "malformed
  JWT" shape (3 dot-separated base64 segments). It does NOT verify
  the signature — that happens Fulcio-side. This is the correct
  primitive for both env-var and ambient paths.
- GitHub Actions OIDC endpoint returns `{"value": "<jwt>"}` per
  GitHub's documented API. The `audience=sigstore` query parameter
  is the Sigstore convention (not GitHub's default audience).
- `reqwest` is already in the workspace with `blocking` + `json` +
  `rustls-tls` features — no new dep.
- Explicit `Err(...)` on `OidcProvider::Interactive` satisfies
  FR-005 + FR-009 per Q1 clarification (fail-close with a
  diagnostic pointing at the two supported paths).

**Alternatives considered**:
- **Use `sigstore::oauth::openidflow::OpenIDAuthorize` for the
  interactive path in v1 too** — rejected per Q1 clarification;
  browser-launch UX + TTY-guard test infrastructure defers to v2.
- **Fetch OIDC token at CLI parse time** — rejected per FR-008;
  tokens are typically valid for 5–15 minutes, so long scans
  would burn the token window on scan work before reaching the
  signing step. Fetch happens right before `SigningContext::signer()`.

---

## R4 — Rekor timeout + fail-close semantics

<!-- resolved: 2026-07-31: sigstore-rs 0.11 exposes NO explicit
     Rekor timeout knob (grep for "timeout" in src/rekor/ +
     bundle/sign.rs returns zero hits) BUT `RekorConfiguration.client`
     is a `pub reqwest::Client` field — we build a Client with a
     custom `.timeout(rekor_timeout)` and assign it. This is the
     built-in knob; no mpsc wrapper needed. -->

**Decision**: Configure the Rekor timeout via `reqwest::Client::builder()
.timeout(rekor_timeout).build()?` and assign to `RekorConfiguration.client`
(the field is `pub`). No `std::thread::spawn` + `mpsc::recv_timeout`
wrapper required — the reqwest client applies the timeout to every
HTTP call sigstore-rs makes against Rekor, including the
inclusion-proof wait.

**Empirical basis**: `grep -rn 'timeout' /path/to/sigstore-0.11.0/src/rekor/`
returns zero hits. `RekorConfiguration` at `src/rekor/apis/configuration.rs:17`
has `pub client: reqwest::Client`, initialized at line 44 with
`reqwest::Client::new()` (no timeout). Overriding this field with a
tuned Client is well-supported.

**Concrete shape**:

```rust
let mut rekor_cfg = RekorConfiguration::default();
if rekor_url != "https://rekor.sigstore.dev" {
    rekor_cfg.base_path = rekor_url.to_string();
}
rekor_cfg.client = reqwest::Client::builder()
    .timeout(rekor_timeout)
    .build()
    .map_err(|e| SigningError::CryptoError {
        detail: format!("failed to build Rekor HTTP client with {rekor_timeout:?} timeout: {e}"),
    })?;
```

**Rationale**: FR-007 requires Rekor inclusion as mandatory + fail
on timeout. The reqwest-level timeout applies uniformly to every
Rekor HTTP call sigstore-rs makes — POST the entry, GET the
inclusion proof, etc. — and surfaces as a `RekorError` back through
sigstore-rs. Our `classify_sign_error` maps that to
`SigningError::RekorError`, which the m221 FR-009a cleanup handler
already handles as fail-close.

Saves ~20 LOC vs the initial-plan mpsc wrapper approach and keeps
the timeout applied consistently to the async internals as well as
the top-level sign call.

**Empirical Rekor latency** (from published Sigstore SLO): p95
< 3s for inclusion-proof after entry acceptance. The 30s default is
generous; operators dealing with abnormal Sigstore load can bump
via env var.

**Alternatives considered**:
- **No timeout — trust Sigstore SLO** — rejected: fail-close
  contract per FR-007 requires an upper bound; a Rekor hang would
  otherwise wedge the whole signing operation indefinitely.
- **Configurable timeout as a CLI flag** — rejected: env var is
  the canonical waybill knob for endpoint tuning (matches
  `WAYBILL_FULCIO_URL` + `WAYBILL_REKOR_URL`); a CLI flag would
  bloat the surface area for a rarely-tuned value.

---

## R5 — Sigstore staging test infrastructure + CI wiring

**Decision**: Model the `lint-and-test-keyless-sbom` CI job after
the existing `lint-and-test-ebpf` job in
`.github/workflows/ci.yml` — feature-gated integration test in an
isolated job that doesn't block the main lint+test lane.

Job shape:

```yaml
lint-and-test-keyless-sbom:
  name: Lint + test keyless SBOM signing (sigstore staging)
  runs-on: ubuntu-latest
  permissions:
    id-token: write        # GitHub Actions ambient OIDC
    contents: read
  env:
    WAYBILL_TEST_KEYLESS: "1"
    WAYBILL_FULCIO_URL: https://fulcio.sigstage.dev
    WAYBILL_REKOR_URL: https://rekor.sigstage.dev
  steps:
    - uses: actions/checkout@<sha>
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - run: cargo +stable test --workspace --test cisa_2026_signing
```

**Rationale**:
- `permissions: id-token: write` is the standard GitHub Actions
  incantation to unlock the ambient OIDC endpoint. Without it,
  `ACTIONS_ID_TOKEN_REQUEST_TOKEN` is unset and the ambient path
  fails.
- Isolating in a dedicated job means a Sigstore-staging outage
  doesn't block unrelated PRs (satisfies FR-012).
- Sigstore staging is deliberately provisioned for CI test loads
  by the Sigstore project; used by cosign / chainguard / actions /
  sigstore-python's own test suites without rate-limit issues at
  the workload sizes waybill produces (~5 sign events per
  integration test run).
- Real-network path per Q2 clarification (mock backends drift
  from real Sigstore over time and hide regressions).

**Test verification strategy**: Reuse the m221 verification pattern
from `us2a_static_key_jsf_sign_and_verify` at
`waybill-cli/tests/cisa_2026_signing.rs:78`:
1. Run `waybill sbom scan --sign --output signed.cdx.json` as a
   subprocess.
2. Parse the output, extract the Sigstore Bundle from
   `metadata.signature`.
3. Verify with sigstore-rs's `bundle::verify::Verifier` primitives
   (available in the same 0.11 crate) OR shell out to `cosign
   verify-blob --bundle signed.cdx.json` if the binary is on
   `$PATH`. The Verifier primitives are preferred (no external
   binary dependency for the test).

**Alternatives considered**:
- **Nightly cron for staging verify instead of per-PR** — rejected:
  PR-time coverage catches regressions before merge, which is the
  point. Nightly-only means a PR that breaks keyless can merge
  and only trip an alert 24h later.
- **Skip the test entirely in CI, manual-verify only** — rejected:
  FR-010 mandates an integration test; without it, US2b would ship
  without confirmation the flow works end-to-end.

---

## Summary of resolved unknowns

| Plan Technical Context item | Status | Resolved by |
|-----------------------------|--------|-------------|
| `sigstore-trust-root-rustls-tls` C-cleanliness under 0.11 | ✅ | R1 — audit FAILED (aws-lc-rs via tough); adopted vendored-CTFE + `SigningContext::new()` path |
| sigstore-rs 0.11 signing-flow API — is `SigningContext` sufficient? | ✅ | R2 (yes, `blocking_signer(token).sign(&bytes)` is the entire flow) |
| OIDC token acquisition for GitHub Actions ambient | ✅ | R3 (`reqwest` GET against `ACTIONS_ID_TOKEN_REQUEST_URL?audience=sigstore` with bearer token) |
| Rekor timeout semantics for fail-close | ✅ | R4 (in-library timeout knob preferred; wrapper as fallback) |
| CI job shape for Sigstore staging integration | ✅ | R5 (mirror `lint-and-test-ebpf` job pattern) |

All NEEDS CLARIFICATION items resolved. R1 has a documented pivot
plan if the C-cleanliness audit fails. All others have concrete
sigstore-rs 0.11 API entry points identified.
