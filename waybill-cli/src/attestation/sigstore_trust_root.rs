//! Vendored Sigstore CTFE (Certificate Transparency Front End)
//! public keys, and a helper that builds a `sigstore::crypto::Keyring`
//! for consumption by `SigningContext::new()`.
//!
//! Why vendored instead of `SigningContext::production()`:
//! `SigningContext::production()` requires the `sigstore-trust-root-*`
//! feature on the `sigstore` crate, which transitively pulls
//! `aws-lc-rs` via the `tough` TUF client — a Constitution Principle I
//! violation (see specs/222-sigstore-keyless-signing/research.md §R1).
//! The current base feature set (`bundle`, `fulcio-rustls-tls`,
//! `rekor-rustls-tls`) is sufficient once we supply the CTFE keyring
//! ourselves.
//!
//! Vendoring recipe + rotation policy: `docs/sigstore-trust-keys.md`.

use sigstore::crypto::Keyring;

use crate::attestation::signer::SigningError;

/// Sigstore **production** CTFE public key (DER SPKI, P-256).
/// LogID (base64): `3T0wasbHETJjGR4cmWc3AqJKXrjePK3/h4pygC8p7o4=`
/// validFor.start: 2022-10-20
pub const SIGSTORE_PROD_CTFE_KEY_DER: &[u8] =
    include_bytes!("../../vendor/sigstore/ctfe_prod.der");

/// Sigstore **staging** CTFE public keys (DER SPKI, P-256).
/// Sigstage runs multiple currently-active CTLogs concurrently; Rekor
/// may write SCTs to any of them, so we include every log whose
/// `validFor.end` is unset in the sigstage trust root at vendoring time.
pub const SIGSTORE_STAGE_CTFE_KEYS_DER: &[&[u8]] = &[
    include_bytes!("../../vendor/sigstore/ctfe_stage_20220701.der"),
    include_bytes!("../../vendor/sigstore/ctfe_stage_20260114.der"),
    include_bytes!("../../vendor/sigstore/ctfe_stage_20260612.der"),
];

/// Build a CTFE `Keyring` for the given Rekor URL. Dispatches on
/// substring match against `sigstage.dev`; anything else uses the
/// production keyring.
pub fn ctfe_keyring(rekor_url: &str) -> Result<Keyring, SigningError> {
    if rekor_url.contains("sigstage.dev") {
        Keyring::new(SIGSTORE_STAGE_CTFE_KEYS_DER.iter().copied()).map_err(|e| {
            SigningError::CryptoError {
                detail: format!("failed to build sigstage CTFE keyring: {e}"),
            }
        })
    } else {
        Keyring::new([SIGSTORE_PROD_CTFE_KEY_DER]).map_err(|e| SigningError::CryptoError {
            detail: format!("failed to build production CTFE keyring: {e}"),
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn prod_key_is_valid_der_spki() {
        assert_eq!(SIGSTORE_PROD_CTFE_KEY_DER.len(), 91);
        Keyring::new([SIGSTORE_PROD_CTFE_KEY_DER]).unwrap();
    }

    #[test]
    fn stage_keys_are_valid_der_spki() {
        for (i, key) in SIGSTORE_STAGE_CTFE_KEYS_DER.iter().enumerate() {
            assert_eq!(key.len(), 91, "sigstage key {i} unexpected length");
        }
        Keyring::new(SIGSTORE_STAGE_CTFE_KEYS_DER.iter().copied()).unwrap();
    }

    #[test]
    fn ctfe_keyring_dispatches_prod_by_default() {
        assert!(ctfe_keyring("https://rekor.sigstore.dev").is_ok());
    }

    #[test]
    fn ctfe_keyring_dispatches_stage_for_sigstage_url() {
        assert!(ctfe_keyring("https://rekor.sigstage.dev").is_ok());
    }

    #[test]
    fn ctfe_keyring_dispatches_prod_for_unknown_url() {
        assert!(ctfe_keyring("https://custom-rekor.example.com").is_ok());
    }

    /// T007 API-sanity: verify the full SigningContext::new() chain
    /// composes without hitting a hidden `pub(crate)` blocker. We do
    /// NOT invoke `.sign()` here — that would need a live Fulcio +
    /// Rekor + a real OIDC token. Compile is the goal.
    #[test]
    fn signing_context_new_chain_compiles() {
        use sigstore::bundle::sign::SigningContext;
        use sigstore::fulcio::oauth::OauthTokenProvider;
        use sigstore::fulcio::{FulcioClient, TokenProvider};
        use sigstore::rekor::apis::configuration::Configuration as RekorConfiguration;
        use url::Url;

        // TokenProvider here is a placeholder — request_cert_v2 (the
        // sign path) bypasses it, using the IdentityToken passed to
        // SigningContext::blocking_signer directly. This mirrors what
        // production() does internally.
        let fulcio = FulcioClient::new(
            Url::parse("https://fulcio.sigstore.dev").unwrap(),
            TokenProvider::Oauth(OauthTokenProvider::default()),
        );
        let rekor_cfg = RekorConfiguration::default();
        let ctfe = ctfe_keyring("https://rekor.sigstore.dev").unwrap();

        // The critical line — SigningContext::new() accepts a Keyring
        // that is now reachable via our sigstore-rs [patch.crates-io].
        let _ctx = SigningContext::new(fulcio, rekor_cfg, ctfe);
    }
}
