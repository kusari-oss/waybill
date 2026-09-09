# Phase 1 Data Model: Extensible signing identity

**Feature**: 779-signing-identity-shapes

## `SignerIdentity`

Home: `waybill-cli/src/attestation/signer.rs`, beside `KeylessSignSuccess`.

**Named `SignerIdentity`, not `SigningIdentity`** — the latter is already
taken in that file by the signing *configuration* enum
(`None` / `LocalKey` / `Keyless`, used across three modules). Renaming
that would be an unrelated refactor. `SignerIdentity` also matches the
spec's Key Entity wording exactly.

```rust
/// Who a keyless signature attests was the signer.
///
/// Replaces the bare `fulcio_cert_subject: String`, which could not
/// distinguish a person from a workload — two things that verify
/// differently and mean different things about accountability.
///
/// `#[non_exhaustive]` states the FR-006 contract. Note that it has no
/// enforcement effect while `attestation` stays private to the binary
/// (see research.md R5); it is correct-on-day-one insurance, not
/// machinery doing work today.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignerIdentity {
    /// A person, identified by an email address claim.
    Email {
        address: String,
        /// The issuer the CERTIFICATE attests — not the token's `iss`,
        /// which differs under federation (FR-009).
        issuer: String,
    },
    /// A workload, identified by a subject claim. For GitHub Actions
    /// this is a workflow reference such as
    /// `repo:org/repo:ref:refs/heads/main`.
    Workload { subject: String, issuer: String },
    /// A SAN form the current vocabulary does not recognise. Recorded
    /// verbatim so signing never fails on an unfamiliar identity
    /// (FR-007), and so the verification command can still be built.
    Unrecognized { value: String, issuer: String },
}
```

### Invariants

- Every variant carries `issuer`. Verification needs both halves and
  neither is derivable from the other; making the pair inseparable in the
  type means a caller cannot build a verification command missing one.
- No variant may hold an empty `issuer` or empty subject value.
  Construction fails closed (Principle III) rather than recording a blank.
- `Unrecognized` is a real recorded outcome, not an error path.

### Accessors

| Accessor | Purpose |
|---|---|
| `subject_value() -> &str` | The SAN string, whatever the shape. Feeds the existing `fulcio_cert_subject` log field unchanged (FR-008). |
| `issuer() -> &str` | The certificate's issuer, for `--certificate-oidc-issuer`. |
| `shape_name() -> &'static str` | Stable classification token for logs. Closed set: `email`, `workload`, `unrecognized`. |

### Construction

From the issued leaf certificate, extending the existing
`extract_fulcio_cert_subject` (`signer.rs:364`), which already walks SAN
general names and returns URI or RFC822 — the two forms map onto
`Workload` and `Email` respectively. The issuer comes from the
certificate's OIDC-issuer extension.

Classifying from the certificate rather than from the token is deliberate:
the certificate is what a verifier checks, so a record derived from it
cannot disagree with verification.

## `KeylessSignSuccess` (modified)

```rust
pub struct KeylessSignSuccess {
    pub bundle: sigstore::bundle::Bundle,
    pub rekor_log_index: u64,
    /// Unchanged name and meaning per FR-008. Now derived from
    /// `identity.subject_value()`.
    pub fulcio_cert_subject: String,
    pub oidc_provider: &'static str,
    /// New (FR-005).
    pub identity: SignerIdentity,
    /// New (FR-009b) — which Sigstore deployment was signed against.
    pub environment: SigstoreEnvironment,
}

**Design change found during implementation**: `verification_command`
does *not* live on this struct. Rendering needs the artifact and sidecar
paths, which are a CLI-layer concern the signer has no business knowing.
The signer returns `identity` and `environment`; the CLI calls
`VerificationCommand::render(env, &identity, artifact, sidecar)` where the
paths are already in scope.
```

`fulcio_cert_subject` is retained rather than replaced. Operators and the
m809 tests grep it; FR-008 makes keeping it a requirement, and the
redundancy with `identity` is the price of not breaking them.

## `VerificationCommand`

```rust
pub struct VerificationCommand {
    /// The full command, ready to run as printed when `is_template`
    /// is false.
    pub rendered: String,
    /// True when the Sigstore deployment is neither standard production
    /// nor standard staging, so the operator must supply trust
    /// configuration (FR-009c).
    pub is_template: bool,
}
```

## `SigstoreEnvironment`

```rust
enum SigstoreEnvironment { Production, Staging, Custom }
```

Derived by comparing the configured Fulcio/Rekor URLs against the known
production and staging endpoints. Selects the verifier per research.md R4:
production → cosign, staging → the sigstore CLI, custom → template.

## Fork-side types

In `kusari-sandbox/sigstore-rs`:

```rust
pub struct Claims {
    pub aud: String,
    pub exp: DateTime<Utc>,
    pub nbf: Option<DateTime<Utc>>,
    pub iss: String,
    pub email: Option<String>,   // was: String
    pub sub: Option<String>,     // new
    pub federated_claims: Option<FederatedClaims>,  // new
}
```

with identity resolution keyed on `iss` per research.md R2.
