//! DSSE envelope signer — feature 006 US2.
//!
//! Local-key (PEM + optional env-var passphrase) and keyless (OIDC →
//! Fulcio → Rekor) signing flows. Hard-fails on any pipeline error per
//! FR-006a: the caller gets a typed `SigningError`, no silent fallback
//! to unsigned output.

// Signer is invoked only from `cli/scan.rs::execute_scan` (Linux-only
// trace flow). On macOS the file compiles but is unreachable; allow
// dead_code on non-Linux.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use sigstore::crypto::signing_key::{SigStoreKeyPair, SigStoreSigner};
use sigstore::crypto::SigningScheme;

use waybill_common::attestation::envelope::{
    canonical_json_bytes, dsse_pae, IdentityMetadata, KeyAlgorithm, Signature, SignedEnvelope,
    IN_TOTO_PAYLOAD_TYPE,
};
use waybill_common::attestation::statement::InTotoStatement;

/// High-level signing configuration constructed from CLI flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningIdentity {
    /// No signing — legacy (pre-feature-006) behavior. Emits a raw
    /// in-toto Statement JSON file.
    None,
    /// Local-key signing with an on-disk PEM private key.
    LocalKey {
        path: PathBuf,
        /// Name of the env var holding the passphrase. `None` means the
        /// key is unencrypted.
        passphrase_env: Option<String>,
    },
    /// Keyless signing via OIDC → Fulcio → (optional) Rekor.
    Keyless {
        fulcio_url: String,
        rekor_url: String,
        oidc_provider: OidcProvider,
        /// Whether to upload to Rekor and embed the inclusion proof.
        transparency_log: bool,
    },
}

/// How to obtain an OIDC token for keyless signing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OidcProvider {
    /// GitHub Actions — use `ACTIONS_ID_TOKEN_REQUEST_URL` +
    /// `ACTIONS_ID_TOKEN_REQUEST_TOKEN` to mint a token.
    GitHubActions,
    /// Operator-supplied pre-fetched token via `SIGSTORE_ID_TOKEN` env.
    Explicit,
    /// Interactive browser flow. Rejected in non-interactive contexts.
    Interactive,
}

impl OidcProvider {
    /// Detect from the ambient environment. Order:
    /// 1. GitHub Actions (if OIDC endpoint + token env vars present)
    /// 2. Explicit (if `SIGSTORE_ID_TOKEN` set)
    /// 3. Interactive (fallback — only works in TTY contexts)
    pub fn detect() -> Self {
        if std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").is_ok()
            && std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").is_ok()
        {
            return Self::GitHubActions;
        }
        if std::env::var("SIGSTORE_ID_TOKEN").is_ok() {
            return Self::Explicit;
        }
        Self::Interactive
    }
}

/// Tagged failure modes for the sign pipeline (FR-006a).
#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("signing key file not found: {path}")]
    KeyFileMissing { path: String },

    #[error("signing key passphrase missing or invalid (env var: {env_var})")]
    KeyPassphraseInvalid { env_var: String },

    #[error("signing key could not be parsed: {detail}")]
    KeyParseError { detail: String },

    #[error("unsupported signing key algorithm: {algorithm}")]
    UnsupportedAlgorithm { algorithm: String },

    #[error("OIDC token acquisition failed: {detail}")]
    OidcTokenError { detail: String },

    #[error("Fulcio certificate issuance failed: {detail}")]
    FulcioError { detail: String },

    #[error("Rekor upload or inclusion-proof generation failed: {detail}")]
    RekorError { detail: String },

    #[error("canonical JSON serialization failed: {0}")]
    Serialization(#[from] waybill_common::attestation::envelope::SerializationError),

    #[error("low-level signing operation failed: {detail}")]
    CryptoError { detail: String },

    #[error("IO error during signing: {0}")]
    Io(#[from] std::io::Error),
}

/// Default local-key algorithm when `waybill` generates or imports a
/// key without explicit scheme information. ECDSA-P256 matches Fulcio.
pub(crate) const DEFAULT_KEY_ALGORITHM: KeyAlgorithm = KeyAlgorithm::EcdsaP256;

/// Load a PEM-encoded signing key from disk. If `passphrase_env` names
/// an env var, the key is treated as encrypted and decrypted in-process.
pub fn load_local_signer(
    path: &Path,
    passphrase_env: Option<&str>,
) -> Result<SigStoreKeyPair, SigningError> {
    if !path.exists() {
        return Err(SigningError::KeyFileMissing {
            path: path.display().to_string(),
        });
    }
    let pem_bytes = std::fs::read(path)?;

    match passphrase_env {
        Some(env_var) => {
            let passphrase = std::env::var(env_var).map_err(|_| {
                SigningError::KeyPassphraseInvalid {
                    env_var: env_var.to_string(),
                }
            })?;
            SigStoreKeyPair::from_encrypted_pem(&pem_bytes, passphrase.as_bytes()).map_err(|e| {
                SigningError::KeyPassphraseInvalid {
                    env_var: format!("{env_var}: {e}"),
                }
            })
        }
        None => SigStoreKeyPair::from_pem(&pem_bytes).map_err(|e| SigningError::KeyParseError {
            detail: e.to_string(),
        }),
    }
}

/// Infer a [`SigningScheme`] from a loaded keypair's algorithm. For
/// v1 we default to ECDSA-P256 (matches Fulcio + the vast majority of
/// sigstore-produced keys) and convert explicitly for Ed25519.
fn scheme_for_algorithm(alg: KeyAlgorithm) -> SigningScheme {
    match alg {
        KeyAlgorithm::EcdsaP256 => SigningScheme::ECDSA_P256_SHA256_ASN1,
        KeyAlgorithm::Ed25519 => SigningScheme::ED25519,
        KeyAlgorithm::RsaPkcs1 => SigningScheme::ECDSA_P256_SHA256_ASN1,
    }
}

/// Sign a statement with a local PEM keypair. Returns a fully-formed
/// DSSE envelope with the verifying key embedded for offline verify.
pub fn sign_local(
    statement: &InTotoStatement,
    keypair: &SigStoreKeyPair,
) -> Result<SignedEnvelope, SigningError> {
    let payload_bytes = canonical_json_bytes(statement)?;
    let pae = dsse_pae(IN_TOTO_PAYLOAD_TYPE, &payload_bytes);

    // SigStoreKeyPair doesn't expose `.sign()` directly; promote to a
    // SigStoreSigner via to_sigstore_signer, then sign the PAE bytes.
    let scheme = SigningScheme::ECDSA_P256_SHA256_ASN1;
    let signer = keypair
        .to_sigstore_signer(&scheme)
        .map_err(|e| SigningError::CryptoError {
            detail: format!("cannot build signer from key: {e}"),
        })?;
    let sig_bytes = signer
        .sign(&pae)
        .map_err(|e| SigningError::CryptoError {
            detail: format!("local signing failed: {e}"),
        })?;

    let public_key_pem =
        keypair
            .public_key_to_pem()
            .map_err(|e| SigningError::KeyParseError {
                detail: format!("cannot export public key PEM: {e}"),
            })?;

    // sigstore's SigStoreKeyPair doesn't expose the concrete scheme
    // directly; v1 ships with ECDSA-P256 as the default. Future work:
    // persist the scheme alongside the key or parse it from the PEM.
    let algorithm = DEFAULT_KEY_ALGORITHM;

    Ok(SignedEnvelope {
        payload_type: IN_TOTO_PAYLOAD_TYPE.to_string(),
        payload: BASE64_STD.encode(&payload_bytes),
        signatures: vec![Signature {
            keyid: Some(keyid_for_pem(&public_key_pem)),
            sig: BASE64_STD.encode(&sig_bytes),
            identity: IdentityMetadata::PublicKey {
                public_key: public_key_pem,
                algorithm,
            },
        }],
    })
}

/// Keyless signing skeleton (m006 attestation entry point — DSSE).
/// This entry is currently unreachable from the CLI; US2b of feature
/// 222 wires SBOM signing via the separate `sign_keyless_sbom()` entry
/// which returns a Sigstore Bundle. In-toto attestations still use the
/// scaffold below and remain unimplemented until an attestation-side
/// use case emerges.
pub fn sign_keyless(
    _statement: &InTotoStatement,
    identity: &SigningIdentity,
) -> Result<SignedEnvelope, SigningError> {
    let _ = identity;
    Err(SigningError::OidcTokenError {
        detail: "in-toto keyless attestations not yet implemented — SBOM \
                 keyless signing is available via `waybill sbom scan --sign` \
                 (see attestation::signer::sign_keyless_sbom)"
            .to_string(),
    })
}

// ---------------------------------------------------------------------------
// Milestone 222 US2b — Sigstore keyless SBOM signing
// ---------------------------------------------------------------------------

// Note: the m222-US2b GitHub Actions ambient OIDC helper was removed
// in a scope-down decision post PR #645's first CI run. sigstore-rs
// 0.11 requires the OIDC token's Claims struct to include an `email`
// String field (used as the CSR subject sent to Fulcio), which
// real GitHub Actions ambient tokens do not emit — they carry `sub`
// (workflow path) instead. This is not fixable via a minimal patch to
// our sigstore-rs fork; it requires ~30-50 LOC of behavior change in
// the CSR builder + issuer-aware claim dispatch. Deferred to a
// follow-up milestone. v1 keyless signing supports only the
// `SIGSTORE_ID_TOKEN` explicit-env path with email-emitting OIDC
// providers (cosign login, Sigstore-dex, Google, GitLab, etc.).
// GitHub Actions users must fetch a compatible token via a helper
// action (e.g., sigstore/gh-action-sigstore-python) that populates
// SIGSTORE_ID_TOKEN.

/// Return type from `sign_keyless_sbom()`. Carries the Sigstore Bundle
/// for callers to serialize into the CDX `metadata.signature` slot or
/// SPDX sidecar, plus the three fields FR-016 requires at INFO log.
///
/// T014 (feature 222 US2b).
#[derive(Debug)]
pub struct KeylessSignSuccess {
    pub bundle: sigstore::bundle::Bundle,
    /// FR-016 — Rekor log-index (transparency-log lookup key for
    /// post-hoc audit; positive integer per Rekor's 1-based indexing).
    pub rekor_log_index: u64,
    /// FR-016 — Fulcio-issued cert's Subject Alternative Name (who
    /// signed). Non-empty per validation in `sign_keyless_sbom`.
    pub fulcio_cert_subject: String,
    /// FR-016 — which OIDC provider variant was used. Closed set:
    /// `"github-actions-ambient"` or `"explicit-env"`.
    pub oidc_provider: &'static str,
}

/// T015 (feature 222 US2b) — read `SIGSTORE_ID_TOKEN` env var, parse
/// via `sigstore::oauth::IdentityToken::try_from(str)`, verify
/// `in_validity_period()`. Three failure modes → three distinct
/// `SigningError::OidcTokenError` detail strings.
fn identity_token_from_env_var() -> Result<sigstore::oauth::IdentityToken, SigningError> {
    let raw = std::env::var("SIGSTORE_ID_TOKEN").map_err(|_| SigningError::OidcTokenError {
        detail: "SIGSTORE_ID_TOKEN env var is not set. Fetch a token via \
                 `cosign login --identity-token` and export it, or run inside \
                 GitHub Actions with `permissions: id-token: write`."
            .to_string(),
    })?;
    let token = sigstore::oauth::IdentityToken::try_from(raw.as_str()).map_err(|e| {
        SigningError::OidcTokenError {
            detail: format!("SIGSTORE_ID_TOKEN could not be parsed as an OIDC JWT: {e}"),
        }
    })?;
    if !token.in_validity_period() {
        return Err(SigningError::OidcTokenError {
            detail: "SIGSTORE_ID_TOKEN is outside its validity period (exp/nbf claims). \
                     Fetch a fresh token via `cosign login --identity-token` and re-export."
                .to_string(),
        });
    }
    Ok(token)
}

/// T017 (feature 222 US2b) — dispatcher for OIDC token acquisition.
/// v1 supports only the `Explicit` variant (`SIGSTORE_ID_TOKEN` env
/// var). `GitHubActions` (ambient) and `Interactive` (browser) both
/// return fail-close diagnostics pointing at the explicit-env
/// workaround.
///
/// **v1 scope constraint**: sigstore-rs 0.11's `Claims` struct requires
/// a non-optional `email: String` field (used as the CSR subject sent
/// to Fulcio). Real GitHub Actions ambient tokens do not emit `email`
/// — they use `sub` (workflow path). Support for GHA-ambient requires
/// upstream sigstore-rs changes and is deferred to a follow-up
/// milestone. Users inside GitHub Actions must fetch a token via a
/// helper action that populates `SIGSTORE_ID_TOKEN` (see the
/// diagnostic message for pointers).
pub fn resolve_identity_token(
    provider: &OidcProvider,
) -> Result<sigstore::oauth::IdentityToken, SigningError> {
    match provider {
        OidcProvider::Explicit => identity_token_from_env_var(),
        OidcProvider::GitHubActions => Err(SigningError::OidcTokenError {
            detail: "GitHub Actions ambient OIDC is not supported in this version of \
                     waybill because sigstore-rs 0.11 requires an `email` claim, which \
                     GHA tokens do not emit. Workaround: fetch a token via a helper \
                     (e.g., `cosign login --identity-token`, or the \
                     `sigstore/gh-action-sigstore-python` action) and export it as \
                     SIGSTORE_ID_TOKEN before running `waybill sbom scan --sign`."
                .to_string(),
        }),
        OidcProvider::Interactive => Err(SigningError::OidcTokenError {
            detail: "no OIDC token available; set SIGSTORE_ID_TOKEN (e.g. via \
                     `cosign login --identity-token`). Interactive browser flow and \
                     GitHub Actions ambient OIDC are both deferred to a follow-up \
                     milestone."
                .to_string(),
        }),
    }
}

/// T018 (feature 222 US2b) — map a `sigstore::errors::SigstoreError`
/// variant to the corresponding `SigningError` variant. Preserves the
/// original error string in the `detail` field for operator diagnostics.
/// Contract source: `specs/222-sigstore-keyless-signing/contracts/keyless-signing-flow.md`
/// §Error variant mapping.
fn classify_sign_error(err: sigstore::errors::SigstoreError) -> SigningError {
    use sigstore::errors::SigstoreError as Se;
    match err {
        Se::FulcioClientError(detail) => SigningError::FulcioError { detail },
        Se::RekorClientError(detail) => SigningError::RekorError { detail },
        Se::PublicKeyUnsupportedAlgorithmError(detail) => SigningError::CryptoError { detail },
        Se::PublicKeyVerificationError => SigningError::CryptoError {
            detail: "sigstore-rs public-key verification failed".to_string(),
        },
        Se::IdentityTokenError(detail) => SigningError::OidcTokenError { detail },
        other => SigningError::CryptoError {
            detail: format!("sigstore-rs sign flow failed: {other}"),
        },
    }
}

/// Extract the Subject Alternative Name from a DER-encoded X.509 leaf
/// certificate. Fulcio-issued certs place the OIDC identity (e.g.
/// `https://github.com/kusari-sandbox/waybill/.github/workflows/ci.yml@refs/heads/main`
/// for a GHA-ambient token, or `mike@kusari.dev` for a personal login)
/// in a SAN URI or SAN email extension. We return whichever comes first.
///
/// Contract source: `specs/222-sigstore-keyless-signing/contracts/keyless-signing-flow.md`
/// §Step 3 — extraction of `fulcio_cert_subject` for FR-016 log field.
fn extract_fulcio_cert_subject(cert_der: &[u8]) -> Result<String, SigningError> {
    use x509_parser::extensions::GeneralName;
    let (_, cert) =
        x509_parser::parse_x509_certificate(cert_der).map_err(|e| SigningError::CryptoError {
            detail: format!("Fulcio cert DER could not be parsed: {e}"),
        })?;
    let san_ext = cert
        .subject_alternative_name()
        .map_err(|e| SigningError::CryptoError {
            detail: format!("Fulcio cert SAN extension parse failed: {e}"),
        })?
        .ok_or_else(|| SigningError::CryptoError {
            detail: "Fulcio-issued cert has no Subject Alternative Name extension \
                     (expected either URI-form workflow-identity or rfc822Name email)"
                .to_string(),
        })?;
    for name in &san_ext.value.general_names {
        match name {
            GeneralName::URI(uri) => return Ok((*uri).to_string()),
            GeneralName::RFC822Name(email) => return Ok((*email).to_string()),
            _ => continue,
        }
    }
    Err(SigningError::CryptoError {
        detail: "Fulcio cert SAN extension contains no URI or RFC822 (email) entries \
                 — cannot determine signer identity"
            .to_string(),
    })
}

/// T019 (feature 222 US2b) — the core Sigstore keyless sign flow.
///
/// End-to-end the function detects the OIDC provider, resolves an
/// identity token, builds a FulcioClient plus a RekorConfiguration
/// (with `rekor_timeout` applied at the `reqwest::Client` level per
/// T018a research) plus a CTFE keyring, constructs `SigningContext::new()`,
/// calls `blocking_signer(token)?.sign(bytes)?`, extracts the Rekor
/// log-index and Fulcio cert SAN, emits the FR-016 INFO log, and
/// returns `KeylessSignSuccess`.
///
/// Contract: `specs/222-sigstore-keyless-signing/contracts/keyless-signing-flow.md`.
/// Every fail-close error propagates up through `SigningError::*`; the
/// m221 FR-009a cleanup handler at the CLI layer unlinks any partial
/// output on error.
///
/// **Runtime isolation** (bug fix from PR #645 CI run): the entire
/// keyless flow is moved to a dedicated OS thread via
/// `std::thread::spawn` + `join()`. `sigstore::bundle::sign::blocking::SigningSession::new`
/// (used inside `SigningContext::blocking_signer`) constructs its own
/// tokio runtime internally; calling it from a thread already inside a
/// tokio runtime (which waybill's `#[tokio::main]` establishes for the
/// entire CLI dispatch) panics with `Cannot start a runtime from
/// within a runtime`. Moving the work to a fresh OS thread with no
/// tokio context avoids the panic and preserves the sync API.
pub fn sign_keyless_sbom(
    canonical_bytes: &[u8],
    fulcio_url: &str,
    rekor_url: &str,
    rekor_timeout: std::time::Duration,
) -> Result<KeylessSignSuccess, SigningError> {
    // Runtime isolation: escape the ambient tokio runtime by moving to
    // a fresh OS thread. Payload is copied by value into the thread.
    let bytes = canonical_bytes.to_vec();
    let fulcio_url_owned = fulcio_url.to_string();
    let rekor_url_owned = rekor_url.to_string();
    let handle = std::thread::spawn(move || {
        sign_keyless_sbom_no_tokio(&bytes, &fulcio_url_owned, &rekor_url_owned, rekor_timeout)
    });
    match handle.join() {
        Ok(result) => result,
        Err(payload) => {
            // Preserve the panic message when the thread panicked
            // rather than returned Err — surfaces upstream sigstore-rs
            // panics as clean SigningError variants.
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&'static str>()
                        .map(|s| (*s).to_string())
                })
                .unwrap_or_else(|| "keyless sign worker thread panicked".to_string());
            Err(SigningError::CryptoError {
                detail: format!("keyless sign worker thread panicked: {msg}"),
            })
        }
    }
}

/// Inner sign flow — same body that was inline in `sign_keyless_sbom`
/// before the runtime-isolation wrapper. Runs on a fresh OS thread
/// with no ambient tokio runtime; sigstore-rs's blocking layer and
/// reqwest::blocking are both safe to invoke here.
fn sign_keyless_sbom_no_tokio(
    canonical_bytes: &[u8],
    fulcio_url: &str,
    rekor_url: &str,
    rekor_timeout: std::time::Duration,
) -> Result<KeylessSignSuccess, SigningError> {
    use crate::attestation::sigstore_trust_root::ctfe_keyring;
    use sigstore::bundle::sign::SigningContext;
    use sigstore::fulcio::oauth::OauthTokenProvider;
    use sigstore::fulcio::{FulcioClient, TokenProvider};
    use sigstore::rekor::apis::configuration::Configuration as RekorConfiguration;

    // Step 1: OIDC token acquisition (late-bound per FR-008).
    let provider = OidcProvider::detect();
    let identity_token = resolve_identity_token(&provider)?;
    let oidc_provider_label: &'static str = match provider {
        OidcProvider::GitHubActions => "github-actions-ambient",
        OidcProvider::Explicit => "explicit-env",
        // Interactive was already rejected inside resolve_identity_token.
        OidcProvider::Interactive => unreachable!("Interactive fails-close at resolve_identity_token"),
    };

    // Step 2: SigningContext construction (per research §R1 — vendored
    // CTFE keys, NOT SigningContext::production()).
    let fulcio_client_url = url::Url::parse(fulcio_url).map_err(|e| SigningError::FulcioError {
        detail: format!("--fulcio-url {fulcio_url:?} is not a valid URL: {e}"),
    })?;
    // TokenProvider is a placeholder here — request_cert_v2 (the modern
    // sign path used inside session.sign()) uses the IdentityToken
    // passed to SigningContext::blocking_signer directly, bypassing the
    // FulcioClient's stored TokenProvider. Upstream `production()` also
    // uses OauthTokenProvider::default() as a no-op placeholder.
    let fulcio = FulcioClient::new(
        fulcio_client_url,
        TokenProvider::Oauth(OauthTokenProvider::default()),
    );

    let mut rekor_cfg = RekorConfiguration::default();
    if rekor_url != "https://rekor.sigstore.dev" {
        rekor_cfg.base_path = rekor_url.to_string();
    }
    // Per T018a research (§R4): sigstore-rs 0.11 has no dedicated Rekor
    // timeout knob; instead, override the reqwest::Client on the
    // RekorConfiguration to apply .timeout(rekor_timeout). The timeout
    // applies to every Rekor HTTP call including inclusion-proof wait.
    rekor_cfg.client = reqwest::Client::builder()
        .timeout(rekor_timeout)
        .build()
        .map_err(|e| SigningError::RekorError {
            detail: format!(
                "failed to build Rekor HTTP client with timeout {rekor_timeout:?}: {e}"
            ),
        })?;

    let ctfe = ctfe_keyring(rekor_url)?;
    let ctx = SigningContext::new(fulcio, rekor_cfg, ctfe);

    // Step 3-4: Signing session + sign + Rekor upload.
    // session.sign() runs the entire Fulcio+sign+Rekor+inclusion-proof
    // flow inside sigstore-rs; any error surfaces as SigstoreError which
    // classify_sign_error maps to a typed SigningError.
    let session = ctx
        .blocking_signer(identity_token)
        .map_err(classify_sign_error)?;
    let artifact = session
        .sign(std::io::Cursor::new(canonical_bytes))
        .map_err(classify_sign_error)?;
    let bundle = artifact.to_bundle();

    // Step 5: Extract Rekor log-index from the Bundle's transparency
    // log entries. The Bundle's `verification_material` is Option-wrapped
    // in the protobuf; sigstore-rs always populates it on successful
    // sign, but we treat absence as a RekorError for defense-in-depth.
    let verification_material =
        bundle
            .verification_material
            .as_ref()
            .ok_or_else(|| SigningError::RekorError {
                detail: "Bundle missing verification_material after successful sign — \
                         sigstore-rs contract violation"
                    .to_string(),
            })?;
    let log_index_i64 = verification_material
        .tlog_entries
        .first()
        .map(|e| e.log_index)
        .ok_or_else(|| SigningError::RekorError {
            detail: "Bundle has empty tlog_entries after successful sign — \
                     Rekor upload apparently silently skipped (FR-007 violation)"
                .to_string(),
        })?;
    if log_index_i64 <= 0 {
        return Err(SigningError::RekorError {
            detail: format!(
                "Rekor returned invalid log-index {log_index_i64} \
                 (log-index must be positive per Rekor's 1-based indexing)"
            ),
        });
    }
    let rekor_log_index = log_index_i64 as u64;

    // Extract the leaf Fulcio cert's SAN for FR-016 audit-trail logging.
    let leaf_cert_der = extract_leaf_cert_der(verification_material)?;
    let fulcio_cert_subject = extract_fulcio_cert_subject(&leaf_cert_der)?;
    if fulcio_cert_subject.is_empty() {
        return Err(SigningError::CryptoError {
            detail: "Fulcio cert SAN is empty — cannot determine signer identity".to_string(),
        });
    }

    // Step 6: FR-016 — INFO log with the three audit-trail fields.
    tracing::info!(
        rekor_log_index,
        fulcio_cert_subject = %fulcio_cert_subject,
        oidc_provider = oidc_provider_label,
        "SBOM signed via Sigstore keyless"
    );

    // Step 7: Return the KeylessSignSuccess.
    Ok(KeylessSignSuccess {
        bundle,
        rekor_log_index,
        fulcio_cert_subject,
        oidc_provider: oidc_provider_label,
    })
}

/// Pull the DER-encoded leaf certificate out of the Bundle's
/// `verification_material.content`. The content is a protobuf oneof;
/// keyless flows always produce the `X509CertificateChain` variant.
fn extract_leaf_cert_der(
    vm: &sigstore_protobuf_specs::dev::sigstore::bundle::v1::VerificationMaterial,
) -> Result<Vec<u8>, SigningError> {
    use sigstore_protobuf_specs::dev::sigstore::bundle::v1::verification_material::Content;
    let content = vm.content.as_ref().ok_or_else(|| SigningError::CryptoError {
        detail: "Bundle verification_material has no content field — \
                 sigstore-rs contract violation"
            .to_string(),
    })?;
    match content {
        Content::X509CertificateChain(chain) => {
            let leaf = chain
                .certificates
                .first()
                .ok_or_else(|| SigningError::CryptoError {
                    detail: "Bundle X509CertificateChain is empty — no leaf cert".to_string(),
                })?;
            Ok(leaf.raw_bytes.clone())
        }
        Content::Certificate(cert) => Ok(cert.raw_bytes.clone()),
        Content::PublicKey(_) => Err(SigningError::CryptoError {
            detail: "Bundle content is a bare PublicKey, not an X509CertificateChain \
                     (keyless signing flow always produces a cert chain)"
                .to_string(),
        }),
    }
}

/// Unified signing entrypoint. Dispatches on `identity`.
///
/// Returns:
/// - `Ok(Some(envelope))` — signed envelope ready to serialize
/// - `Ok(None)` — caller requested no signing; emit raw Statement
/// - `Err(_)` — hard-fail per FR-006a
pub fn sign(
    statement: &InTotoStatement,
    identity: &SigningIdentity,
) -> Result<Option<SignedEnvelope>, SigningError> {
    match identity {
        SigningIdentity::None => Ok(None),
        SigningIdentity::LocalKey {
            path,
            passphrase_env,
        } => {
            let keypair = load_local_signer(path, passphrase_env.as_deref())?;
            let envelope = sign_local(statement, &keypair)?;
            Ok(Some(envelope))
        }
        SigningIdentity::Keyless { .. } => {
            let envelope = sign_keyless(statement, identity)?;
            Ok(Some(envelope))
        }
    }
}

fn keyid_for_pem(pem: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(pem.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

// Silence unused-import lint when the signer module is compiled but no
// downstream caller instantiates a `SigStoreSigner` directly. The type
// stays in scope because sigstore re-exports `SigStoreKeyPair` through
// it during cert/key plumbing in follow-on tasks.
#[allow(dead_code)]
fn _unused_but_reserved(_s: Option<SigStoreSigner>) {}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::testing::EnvGuard;
    use std::io::Write;

    fn minimal_statement() -> InTotoStatement {
        use waybill_common::attestation::file::{FileAccess, FileAccessSummary};
        use waybill_common::attestation::integrity::TraceIntegrity;
        use waybill_common::attestation::metadata::{
            GenerationContext, HostInfo, ProcessInfo, ToolInfo, TraceMetadata,
        };
        use waybill_common::attestation::network::{NetworkSummary, NetworkTrace};
        use waybill_common::attestation::statement::{
            BuildTracePredicate, InTotoStatement, ResourceDescriptor,
        };
        use waybill_common::types::timestamp::Timestamp;
        let mut digest = std::collections::BTreeMap::new();
        digest.insert("sha256".to_string(), "a".repeat(64));
        InTotoStatement {
            statement_type: InTotoStatement::STATEMENT_TYPE.to_string(),
            subject: vec![ResourceDescriptor {
                name: "test".to_string(),
                digest,
            }],
            predicate_type: InTotoStatement::PREDICATE_TYPE.to_string(),
            predicate: BuildTracePredicate {
                metadata: TraceMetadata {
                    tool: ToolInfo {
                        name: "waybill".to_string(),
                        version: "0.1.0".to_string(),
                    },
                    trace_start: Timestamp::now(),
                    trace_end: Timestamp::now(),
                    target_process: ProcessInfo {
                        pid: 1,
                        command: "test".to_string(),
                        cgroup_id: 0,
                    },
                    host: HostInfo {
                        os: "linux".to_string(),
                        kernel_version: "6.5".to_string(),
                        arch: "x86_64".to_string(),
                        distro_codename: None,
                    },
                    generation_context: GenerationContext::BuildTimeTrace,
                },
                network_trace: NetworkTrace {
                    connections: vec![],
                    summary: NetworkSummary {
                        total_connections: 0,
                        unique_hosts: vec![],
                        unique_ips: vec![],
                        protocol_counts: std::collections::BTreeMap::new(),
                        total_bytes_received: 0,
                    },
                },
                file_access: FileAccess {
                    operations: vec![],
                    summary: FileAccessSummary {
                        total_operations: 0,
                        unique_paths: 0,
                        operations_by_type: std::collections::BTreeMap::new(),
                    },
                },
                trace_integrity: TraceIntegrity {
                    ring_buffer_overflows: 0,
                    events_dropped: 0,
                    uprobe_attach_failures: vec![],
                    kprobe_attach_failures: vec![],
                    partial_captures: vec![],
                    bloom_filter_capacity: 100_000,
                    bloom_filter_false_positive_rate: 0.01,
                    filter_categories_applied: vec![],
                },
                compiler_pipeline: None,
            },
        }
    }

    /// Generate an unencrypted PEM keypair and write it to a tempfile.
    /// Returns the tempfile (keeps it alive) and the PEM public key.
    fn pem_tempfile() -> (tempfile::NamedTempFile, String) {
        let scheme = SigningScheme::ECDSA_P256_SHA256_ASN1;
        let signer = scheme.create_signer().unwrap();
        let keypair = signer.to_sigstore_keypair().unwrap();
        let private_pem = keypair.private_key_to_pem().unwrap();
        let public_pem = keypair.public_key_to_pem().unwrap();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(private_pem.as_bytes()).unwrap();
        (tmp, public_pem)
    }

    #[test]
    fn signing_identity_none_is_default_shape() {
        let id = SigningIdentity::None;
        assert_eq!(id, SigningIdentity::None);
    }

    #[test]
    fn signing_error_displays_detail() {
        let err = SigningError::KeyFileMissing {
            path: "/tmp/missing.pem".to_string(),
        };
        assert!(err.to_string().contains("/tmp/missing.pem"));
    }

    #[test]
    fn default_key_algorithm_is_ecdsa_p256() {
        assert_eq!(DEFAULT_KEY_ALGORITHM, KeyAlgorithm::EcdsaP256);
    }

    // Env-var tests are consolidated into one serial test because
    // `std::env::set_var` / `remove_var` mutate process-wide state that
    // races with parallel test execution. All env-mutating tests in
    // this module hold `keyless_env_lock()` so nothing races.
    #[test]
    fn oidc_detect_resolves_all_providers_in_precedence_order() {
        let _guard = keyless_env_lock();

        // 1. GitHub Actions wins when its two env vars are set.
        let _g1 = EnvGuard::setup(&[
            ("ACTIONS_ID_TOKEN_REQUEST_URL", Some("https://x")),
            ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", Some("abc")),
            ("SIGSTORE_ID_TOKEN", None),
        ]);
        assert_eq!(OidcProvider::detect(), OidcProvider::GitHubActions);
        drop(_g1);

        // 2. Explicit when only SIGSTORE_ID_TOKEN is set.
        let _g2 = EnvGuard::setup(&[
            ("ACTIONS_ID_TOKEN_REQUEST_URL", None),
            ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", None),
            ("SIGSTORE_ID_TOKEN", Some("jwt-token")),
        ]);
        assert_eq!(OidcProvider::detect(), OidcProvider::Explicit);
        drop(_g2);

        // 3. Interactive is the last-resort fallback.
        let _g3 = EnvGuard::setup(&[
            ("ACTIONS_ID_TOKEN_REQUEST_URL", None),
            ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", None),
            ("SIGSTORE_ID_TOKEN", None),
        ]);
        assert_eq!(OidcProvider::detect(), OidcProvider::Interactive);
    }

    #[test]
    fn load_local_signer_missing_path_errors() {
        let bogus = Path::new("/nonexistent/waybill-test-key.pem");
        let err = load_local_signer(bogus, None).err().expect("should error");
        match err {
            SigningError::KeyFileMissing { path } => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("expected KeyFileMissing, got {other:?}"),
        }
    }

    #[test]
    fn load_local_signer_passphrase_env_missing_errors() {
        let (tmp, _pub) = pem_tempfile();
        let err = load_local_signer(tmp.path(), Some("WAYBILL_NONEXISTENT_PASSPHRASE_ENV"))
            .err()
            .expect("should error");
        match err {
            SigningError::KeyPassphraseInvalid { env_var } => {
                assert!(env_var.contains("WAYBILL_NONEXISTENT"));
            }
            other => panic!("expected KeyPassphraseInvalid, got {other:?}"),
        }
    }

    #[test]
    fn sign_local_roundtrips_through_verifier() {
        use crate::attestation::verifier::{
            verify_attestation, VerificationReport, VerifyOptions,
        };
        let (tmp, _pub_pem) = pem_tempfile();
        let keypair = load_local_signer(tmp.path(), None).unwrap();
        let stmt = minimal_statement();
        let envelope = sign_local(&stmt, &keypair).unwrap();
        let json = serde_json::to_string(&envelope).unwrap();
        match verify_attestation(&json, &VerifyOptions::default()) {
            VerificationReport::Pass { .. } => {}
            VerificationReport::Fail { mode, detail, .. } => {
                panic!("round-trip should pass, got Fail {mode:?}: {detail}");
            }
        }
    }

    #[test]
    fn sign_local_canonical_payload_is_deterministic() {
        let (tmp, _pub_pem) = pem_tempfile();
        let keypair = load_local_signer(tmp.path(), None).unwrap();
        let stmt = minimal_statement();
        let env1 = sign_local(&stmt, &keypair).unwrap();
        let env2 = sign_local(&stmt, &keypair).unwrap();
        assert_eq!(env1.payload, env2.payload, "payload bytes must be identical");
    }

    #[test]
    fn sign_dispatches_none_to_no_envelope() {
        let stmt = minimal_statement();
        let res = sign(&stmt, &SigningIdentity::None).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn sign_dispatches_local_key_path() {
        let (tmp, _pub_pem) = pem_tempfile();
        let stmt = minimal_statement();
        let identity = SigningIdentity::LocalKey {
            path: tmp.path().to_path_buf(),
            passphrase_env: None,
        };
        let envelope = sign(&stmt, &identity).unwrap().expect("some envelope");
        assert_eq!(envelope.signatures.len(), 1);
        assert!(envelope.signatures[0].keyid.as_ref().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn sign_keyless_returns_structured_error_for_unimplemented_path() {
        let stmt = minimal_statement();
        let identity = SigningIdentity::Keyless {
            fulcio_url: "https://fulcio.sigstore.dev".to_string(),
            rekor_url: "https://rekor.sigstore.dev".to_string(),
            oidc_provider: OidcProvider::Interactive,
            transparency_log: true,
        };
        match sign(&stmt, &identity) {
            Err(SigningError::OidcTokenError { detail }) => {
                assert!(detail.contains("keyless"));
            }
            Err(other) => panic!("expected OidcTokenError variant, got {other:?}"),
            Ok(_) => panic!("expected fail-close error, got Ok(IdentityToken)"),
        }
    }

    #[test]
    fn sign_hard_fails_on_missing_key_file() {
        let stmt = minimal_statement();
        let identity = SigningIdentity::LocalKey {
            path: PathBuf::from("/nonexistent/waybill-test.pem"),
            passphrase_env: None,
        };
        match sign(&stmt, &identity) {
            Err(SigningError::KeyFileMissing { .. }) => {}
            other => panic!("expected KeyFileMissing, got {other:?}"),
        }
    }

    #[test]
    fn keyid_is_sha256_prefixed_hex() {
        let kid = keyid_for_pem("-----BEGIN PUBLIC KEY-----\nabc\n-----END PUBLIC KEY-----");
        assert!(kid.starts_with("sha256:"));
        assert_eq!(kid.len(), 7 + 64); // "sha256:" + 32 bytes hex
    }

    // ----- Milestone 222 US2b (feature 222-sigstore-keyless-signing) -----

    /// Cross-test env-mutex re-declaration so keyless helper tests
    /// serialize with the existing `oidc_detect_resolves_all_providers_in_precedence_order`
    /// test above (same env-var namespace + process-wide state).
    fn keyless_env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Minimal JWT with `aud=sigstore` claim + `exp` far in the future.
    /// Header + payload base64url, unsigned (sigstore-rs's
    /// `IdentityToken::try_from` does not verify the signature —
    /// verification happens Fulcio-side).
    fn mint_test_jwt() -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
        use base64::Engine as _;
        // aud=sigstore is REQUIRED by IdentityToken::try_from at
        // sigstore-0.11.0/src/oauth/token.rs:79.
        // exp in year 2035 so `in_validity_period()` returns true.
        let header = B64URL.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload =
            B64URL.encode(br#"{"aud":"sigstore","exp":2064000000,"email":"test@waybill.dev"}"#);
        format!("{header}.{payload}.")
    }

    #[test]
    fn identity_token_from_env_var_reads_sigstore_id_token_m222() {
        let _lock = keyless_env_lock();
        let _g = EnvGuard::setup(&[("SIGSTORE_ID_TOKEN", Some(&mint_test_jwt()))]);
        let token = identity_token_from_env_var().expect("valid JWT should parse");
        assert!(token.in_validity_period(), "token should be non-expired");
    }

    #[test]
    fn identity_token_from_env_var_missing_env_reports_actionable_m222() {
        let _lock = keyless_env_lock();
        let _g = EnvGuard::setup(&[("SIGSTORE_ID_TOKEN", None)]);
        match identity_token_from_env_var() {
            Err(SigningError::OidcTokenError { detail }) => {
                assert!(detail.contains("SIGSTORE_ID_TOKEN"), "detail: {detail}");
                assert!(detail.contains("cosign login"), "detail: {detail}");
            }
            Err(other) => panic!("expected OidcTokenError variant, got {other:?}"),
            Ok(_) => panic!("expected fail-close error, got Ok(IdentityToken)"),
        }
    }

    #[test]
    fn identity_token_from_env_var_rejects_malformed_jwt_m222() {
        let _lock = keyless_env_lock();
        let _g = EnvGuard::setup(&[("SIGSTORE_ID_TOKEN", Some("not.a.jwt"))]);
        match identity_token_from_env_var() {
            Err(SigningError::OidcTokenError { detail }) => {
                assert!(
                    detail.to_lowercase().contains("parsed") || detail.contains("JWT"),
                    "detail: {detail}"
                );
            }
            Err(other) => panic!("expected OidcTokenError variant, got {other:?}"),
            Ok(_) => panic!("expected fail-close error, got Ok(IdentityToken)"),
        }
    }

    #[test]
    fn resolve_identity_token_interactive_returns_fail_close_diagnostic_m222() {
        // No env-var mutation needed — Interactive variant is pure fail-close.
        // Post-scope-down (2026-07-31): both Interactive AND GHA-ambient
        // deferred; diagnostic points at cosign login as the local
        // workaround.
        match resolve_identity_token(&OidcProvider::Interactive) {
            Err(SigningError::OidcTokenError { detail }) => {
                assert!(detail.contains("no OIDC token available"), "detail: {detail}");
                assert!(detail.contains("SIGSTORE_ID_TOKEN"), "detail: {detail}");
                assert!(detail.contains("cosign login"), "detail: {detail}");
                assert!(
                    detail.contains("deferred to a follow-up milestone"),
                    "detail: {detail}"
                );
            }
            Err(other) => panic!("expected OidcTokenError variant, got {other:?}"),
            Ok(_) => panic!("expected fail-close diagnostic, got Ok(IdentityToken)"),
        }
    }

    #[test]
    fn resolve_identity_token_explicit_delegates_to_env_var_helper_m222() {
        let _lock = keyless_env_lock();
        let _g = EnvGuard::setup(&[("SIGSTORE_ID_TOKEN", Some(&mint_test_jwt()))]);
        resolve_identity_token(&OidcProvider::Explicit).expect("explicit path resolves");
    }

    #[test]
    fn resolve_identity_token_github_actions_returns_fail_close_diagnostic_m222() {
        // v1 scope-down (post PR #645): GHA-ambient returns fail-close
        // with actionable diagnostic pointing at the helper-action
        // workaround. sigstore-rs 0.11's email-claim requirement is
        // the reason (see resolve_identity_token doc-comment).
        match resolve_identity_token(&OidcProvider::GitHubActions) {
            Err(SigningError::OidcTokenError { detail }) => {
                assert!(
                    detail.contains("GitHub Actions ambient OIDC is not supported"),
                    "detail: {detail}"
                );
                assert!(detail.contains("email"), "detail: {detail}");
                assert!(detail.contains("SIGSTORE_ID_TOKEN"), "detail: {detail}");
                assert!(
                    detail.contains("sigstore/gh-action-sigstore-python")
                        || detail.contains("cosign login"),
                    "detail: {detail}"
                );
            }
            Err(other) => panic!("expected OidcTokenError variant, got {other:?}"),
            Ok(_) => panic!("expected fail-close error, got Ok(IdentityToken)"),
        }
    }

    #[test]
    fn classify_sign_error_maps_fulcio_to_fulcio_error_m222() {
        use sigstore::errors::SigstoreError as Se;
        let out = classify_sign_error(Se::FulcioClientError("bad cert".to_string()));
        assert!(matches!(out, SigningError::FulcioError { detail } if detail == "bad cert"));
    }

    #[test]
    fn classify_sign_error_maps_rekor_to_rekor_error_m222() {
        use sigstore::errors::SigstoreError as Se;
        let out = classify_sign_error(Se::RekorClientError("timeout".to_string()));
        assert!(matches!(out, SigningError::RekorError { detail } if detail == "timeout"));
    }

    #[test]
    fn classify_sign_error_maps_identity_token_to_oidc_error_m222() {
        use sigstore::errors::SigstoreError as Se;
        let out = classify_sign_error(Se::IdentityTokenError("expired".to_string()));
        assert!(matches!(out, SigningError::OidcTokenError { detail } if detail == "expired"));
    }

    #[test]
    fn classify_sign_error_catchall_maps_to_crypto_error_m222() {
        use sigstore::errors::SigstoreError as Se;
        let out = classify_sign_error(Se::UnexpectedError("something else".to_string()));
        match out {
            SigningError::CryptoError { detail } => {
                assert!(detail.contains("something else"), "detail: {detail}");
            }
            other => panic!("expected CryptoError, got {other:?}"),
        }
    }

    // NB: the ad-hoc `EnvGuard` that used to live here was promoted
    // to the shared workspace helper at `waybill-cli/src/testing/env_guard.rs`
    // when the pattern was needed by tests outside this file — the
    // shared version adds a process-global mutex that serializes
    // env-var-mutating tests across the whole binary, fixing the
    // podman + cargo m205 flake class. Import shape:
    // `use crate::testing::EnvGuard;` (see the top-level `use` in
    // this `mod tests { ... }` block).
}
