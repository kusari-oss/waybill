# Phase 1: Data Model — Sigstore keyless SBOM signing (US2b)

**Feature**: 222-sigstore-keyless-signing
**Date**: 2026-07-30

The feature adds 3 new types + extends 1 existing enum. All new
types satisfy Constitution Principle IV (newtypes over raw String,
no `.unwrap()` in production, `thiserror` for library errors).

The design intent: **fill the m221 scaffold, don't restructure it**.
`SigningIdentity::Keyless{}`, `SigningError::{Oidc,Fulcio,Rekor}Error`,
and the CLI dispatch layer already exist; US2b implements the
function bodies + adds a `Keyless` variant to the parallel
`SigningMode` enum in `sbom/signer.rs`.

---

## Extended types

### `SigningMode` (extends existing enum in `waybill-cli/src/sbom/signer.rs`)

Extends the two-variant enum shipped in m221 US2a with a third
variant covering the keyless path. Constructed once at CLI parse
time and consumed by emitters.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningMode {
    /// m221 US2a — `--sign-key <PATH>` — static PEM signing.
    StaticKey {
        key_ref: PathBuf,
        passphrase_env: String,
    },
    /// m222 US2b (NEW) — `--sign` — Sigstore keyless signing.
    Keyless {
        fulcio_url: String,      // default: https://fulcio.sigstore.dev
        rekor_url: String,       // default: https://rekor.sigstore.dev
        rekor_timeout: Duration, // default: 30s, tuned via
                                 // WAYBILL_REKOR_TIMEOUT_SECS
    },
    /// Neither signing flag set (the default). Emitters produce
    /// byte-identical output to today's goldens per FR-015.
    Unsigned,
}

impl SigningMode {
    pub fn is_enabled(&self) -> bool {
        !matches!(self, SigningMode::Unsigned)
    }
}
```

**State transitions**: None. Constructed once at CLI parse; consumed
by emit path. Mutually exclusive with `--sign-key` per m221 FR-007
(clap-level exclusion already in place).

---

## New types

### `GitHubOidcResponse` (new in `waybill-cli/src/attestation/signer.rs`)

Deserialize target for the GitHub Actions ambient OIDC endpoint
response. Small helper struct; not exposed publicly.

```rust
#[derive(Debug, serde::Deserialize)]
struct GitHubOidcResponse {
    /// The JWT-shaped OIDC token issued by GitHub's OIDC provider,
    /// bound to `audience=sigstore` per the request query parameter.
    value: String,
}
```

**Validation**: `IdentityToken::try_from(response.value.as_str())`
performs the JWT-shape validation downstream; this struct just gets
the raw string out of the JSON envelope.

---

### `KeylessSignSuccess` (new, non-public — attestation/signer.rs)

Return type from the completed `sign_keyless()` function. Carries
the fields FR-016 requires to log at INFO on successful sign, plus
the `sigstore::bundle::Bundle` for callers to serialize into the
CDX `metadata.signature` slot or the SPDX sidecar.

```rust
#[derive(Debug)]
pub struct KeylessSignSuccess {
    pub bundle: sigstore::bundle::Bundle,
    /// For FR-016 INFO log — Rekor log-index (transparency-log
    /// lookup key for post-hoc audit).
    pub rekor_log_index: u64,
    /// For FR-016 INFO log — Fulcio-issued cert's Subject
    /// Alternative Name (who signed).
    pub fulcio_cert_subject: String,
    /// For FR-016 INFO log — which OIDC provider variant was used
    /// (`github-actions-ambient` or `explicit-env`).
    pub oidc_provider: &'static str,
}
```

**Validation rules**:
- `rekor_log_index` MUST be `> 0` (Rekor log-index space is
  1-based). Zero indicates the log-index extraction failed and
  the sign should be treated as a `RekorError`.
- `fulcio_cert_subject` MUST be non-empty (empty subject means we
  failed to parse the cert's SAN extension, which is a
  `CryptoError`).
- `oidc_provider` is a `&'static str` from a closed set to keep
  the log-field cardinality bounded — the two values above; no
  runtime interpolation.

---

## Existing types being extended (no new fields, just body work)

### `SigningError` (existing in `waybill-cli/src/attestation/signer.rs`)

Already carries the 8 variants needed for FR-009a fail-close
dispatch. US2b makes them reachable:

- `KeyFileMissing`, `KeyPassphraseInvalid`, `KeyParseError`,
  `UnsupportedAlgorithm` — static-key path (already reachable
  post-m221 US2a).
- `OidcTokenError` — reachable via US2b for missing/malformed
  ambient token, missing SIGSTORE_ID_TOKEN, or Interactive
  provider fail-close.
- `FulcioError` — reachable via US2b for Fulcio HTTP failures /
  cert issuance rejections.
- `RekorError` — reachable via US2b for Rekor HTTP failures or
  inclusion-proof timeout per FR-007.
- `CryptoError` — reachable via US2b for local signing operation
  failures (ephemeral keypair generation, DSSE PAE errors).

**No new variants**. The enum is closed at m006-vintage design; US2b
just wires its function bodies to return the appropriate variant.

---

### `OidcProvider` (existing in `waybill-cli/src/attestation/signer.rs:51`)

Three variants pre-existing:

```rust
pub enum OidcProvider {
    GitHubActions,
    Explicit,
    Interactive,  // US2b returns fail-close for this variant per Q1
}
```

`OidcProvider::detect()` (existing at signer.rs:66) already returns
the correct variant based on ambient env vars. **No changes
required** — US2b just uses it as-is + adds `resolve_identity_token()`
as the dispatcher.

---

## Entity relationship diagram

```text
┌─────────────────────────┐
│   CLI arg parsing       │
│   --sign / --sign-key   │
└───────────┬─────────────┘
            │constructs
            ▼
┌─────────────────────────┐
│   SigningMode           │
│   { Unsigned            │
│   , StaticKey{..}       │───US2a shipped, unchanged
│   , Keyless{..} }       │───US2b: new variant
└───────────┬─────────────┘
            │consumed by
            ▼
┌────────────────────────────────────────┐
│   sign_cdx_document_in_place / _spdx_ │
│   in waybill-cli/src/sbom/signer.rs   │
│                                        │
│   match mode {                         │
│       Unsigned    => …                 │
│       StaticKey   => sign_local(…)     │───US2a shipped
│       Keyless{fu,re,to} =>             │
│         sign_keyless_sbom(bytes, …)    │───US2b: new dispatch arm
│   }                                    │
└───────────┬────────────────────────────┘
            │calls
            ▼
┌────────────────────────────────────────┐
│   sign_keyless_sbom(bytes, mode)       │───US2b: new wrapper
│   in waybill-cli/src/attestation/…     │
│                                        │
│   1. token = resolve_identity_token(   │───R3
│         OidcProvider::detect())        │
│   2. ctx = SigningContext::production()│───R2 + R1
│   3. session = ctx.blocking_signer(    │
│         token)?                        │
│   4. artifact = session.sign(bytes)?   │───the ENTIRE Fulcio+Rekor+Bundle flow
│   5. success = KeylessSignSuccess {    │
│         bundle: artifact.to_bundle(),  │
│         rekor_log_index: extract(…),   │
│         fulcio_cert_subject: extract(…)│
│         oidc_provider: …               │
│      }                                 │
│   6. tracing::info!(FR-016 fields)     │
│   7. return success                    │
└───────────┬────────────────────────────┘
            │returned to
            ▼
┌────────────────────────────────────────┐
│   CDX slot (metadata.signature)        │
│   OR SPDX sidecar (<out>.sig.bundle.json)│
└────────────────────────────────────────┘
```

---

## Storage / persistence

None. All keyless-signing state is in-process for the duration of a
single scan:

- **OIDC token**: fetched just-in-time per FR-008, single-use, never
  written to disk or cached.
- **Fulcio-issued cert**: embedded in the Sigstore Bundle in the
  emitted SBOM (metadata.signature or sidecar), never written
  separately.
- **Ephemeral P-256 keypair**: created inside sigstore-rs's
  `SigningSession::new`, used once, dropped at session end. Never
  serialized.
- **Rekor entry**: embedded in the Bundle, never fetched again.

Matches m221 US2a's zero-persistence posture. Constitution Principle
XII's "no external source unavailability blocks SBOM generation" is
intentionally overridden per FR-007/FR-009a + operator's explicit
opt-in (already documented on m221's Constitution check).

---

## Compatibility

- **Existing `SigningMode::Unsigned` + `SigningMode::StaticKey{...}`
  paths**: unchanged. Adding the `Keyless{}` variant is additive at
  the enum level and each `match` on `SigningMode` explicitly
  exhausts the three variants — the Rust compiler enforces at
  build time that no site accidentally routes through `Keyless`
  without an implementation.
- **Existing goldens**: unchanged when `--sign` is unset (FR-015 +
  m221 FR-009 both enforce this).
- **Existing types**: `SigningError` + `OidcProvider` + `SigningIdentity`
  enums all reused at m006 vintage — no serde-boundary shifts, no
  consumer breaks.
- **CI**: adds one new job (`lint-and-test-keyless-sbom`); does
  not modify existing job shape.
