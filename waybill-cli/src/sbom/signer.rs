//! Feature 221 US2a — SBOM-envelope-level signing.
//!
//! Extends the milestone-006 attestation-envelope signing primitives
//! (`crate::attestation::signer`, `waybill_common::attestation::envelope`)
//! to cover SBOM documents themselves rather than in-toto statements.
//!
//! Two emission paths:
//! - **CycloneDX**: sign in-place. The signature JSON object lands at
//!   `metadata.signature` inside the emitted document (native CDX 1.6
//!   slot). Uses the JSF (JSON Signature Format,
//!   draft-cyberphone-jsf-00) empty-value trick: canonicalize with
//!   `value = ""`, sign the canonical bytes, fill the actual base64
//!   signature back into `value`.
//! - **SPDX 2.3 / SPDX 3**: emit a companion DSSE envelope alongside
//!   the primary artifact at `<output>.sig.json`. Neither SPDX
//!   version has a native in-document envelope-signature slot.
//!
//! **Scope of this file** (US2a): static-key path only
//! (`SigningMode::StaticKey`). Sigstore keyless (`SigningMode::Keyless`)
//! is US2b — completing the m006 `sign_keyless()` scaffold with real
//! Fulcio/Rekor calls is deferred to a follow-up session.
//!
//! **Fail-close** per FR-009a: every fallible operation returns a
//! typed `SbomSigningError`; the CLI layer at `scan_cmd.rs` maps
//! this to exit code 1 with the offending output file unlinked
//! (matches cosign / gpg / notary conventions — no silent unsigned
//! fallback).

#![allow(dead_code)] // Some helpers land ahead of their US2b consumers.

use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use serde::Serialize;
use sigstore::crypto::signing_key::SigStoreKeyPair;
use sigstore::crypto::SigningScheme;
use thiserror::Error;

use waybill_common::attestation::envelope::{
    canonical_json_bytes, dsse_pae, IdentityMetadata, KeyAlgorithm, Signature, SignedEnvelope,
};

use crate::attestation::signer::{load_local_signer, SigningError};

/// The DSSE `payloadType` used for SBOM sidecars. Not an in-toto
/// statement; we use a distinct type URI so downstream tooling can
/// tell an SBOM signature apart from an attestation signature.
pub const SBOM_DSSE_PAYLOAD_TYPE: &str = "application/vnd.waybill.sbom+json";

/// High-level signing configuration, mirrors the CLI parse result.
/// Constructed once at CLI parse time and consumed by the emit path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigningMode {
    /// Neither `--sign` nor `--sign-key` set (the default). Emit is
    /// byte-identical to pre-m221 output per FR-009.
    Unsigned,
    /// `--sign-key <PATH>` — static key material (PEM file). US2a.
    StaticKey {
        key_ref: PathBuf,
        /// Env var holding the passphrase for encrypted keys.
        /// Defaults to `WAYBILL_SIGN_KEY_PASSPHRASE` when the operator
        /// omitted `--sign-key-passphrase-env`.
        passphrase_env: String,
    },
    /// `--sign` — Sigstore keyless (US2b, milestone 222). OIDC token
    /// acquisition is late-bound per FR-008 (resolved at signing time,
    /// not CLI parse time). Endpoints are env-var-overridable via
    /// `WAYBILL_FULCIO_URL` / `WAYBILL_REKOR_URL` /
    /// `WAYBILL_REKOR_TIMEOUT_SECS`.
    Keyless {
        fulcio_url: String,
        rekor_url: String,
        rekor_timeout: std::time::Duration,
    },
}

impl SigningMode {
    /// True when a signature should be produced. Cheap check that
    /// avoids the CLI layer having to reason about specific variants.
    pub fn is_enabled(&self) -> bool {
        !matches!(self, SigningMode::Unsigned)
    }
}

/// The output of `sign_sbom_bytes`. Two shapes today; US2b will add
/// `Keyless(SigstoreBundle)`.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum SbomSignatureEnvelope {
    /// JSF (JSON Signature Format) object — the CDX-native shape,
    /// destined for the `metadata.signature` slot of an emitted CDX
    /// document.
    StaticKeyJsf(JsfSignature),
    // Reserved for US2b:
    // Keyless(SigstoreBundle),
}

/// Return type for `sign_spdx_bytes_to_sidecar`. SPDX outputs get a
/// separate on-disk sidecar (SPDX has no in-document signature slot);
/// the shape depends on the `SigningMode` variant.
///
/// Milestone 222 US2b — extends the m221 US2a `SignedEnvelope`-only
/// return to accommodate the Sigstore keyless flow's `Bundle` output.
#[derive(Debug)]
pub enum Sidecar {
    /// DSSE envelope wrapping the SPDX bytes as payload — m221 US2a
    /// static-key flow. Sidecar filename `.sig.json`.
    Dsse(SignedEnvelope),
    /// Sigstore Bundle — m222 US2b keyless flow. Sidecar filename
    /// `.sig.bundle.json` per FR-004.
    SigstoreBundle(Box<sigstore::bundle::Bundle>),
}

impl Sidecar {
    /// Serialize the sidecar's inner payload to pretty JSON bytes ready
    /// to write to disk. Both DSSE + Bundle serialize via serde_json.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Sidecar::Dsse(envelope) => serde_json::to_vec_pretty(envelope),
            Sidecar::SigstoreBundle(bundle) => serde_json::to_vec_pretty(bundle.as_ref()),
        }
    }

    /// Filename suffix appended to the primary SPDX output path. DSSE
    /// gets `.sig.json`; Sigstore Bundle gets `.sig.bundle.json` per
    /// FR-004.
    pub fn sidecar_suffix(&self) -> &'static str {
        match self {
            Sidecar::Dsse(_) => ".sig.json",
            Sidecar::SigstoreBundle(_) => ".sig.bundle.json",
        }
    }

    /// Human-readable variant name for INFO log messages.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Sidecar::Dsse(_) => "DSSE",
            Sidecar::SigstoreBundle(_) => "Sigstore Bundle",
        }
    }
}

/// A JSF (JSON Signature Format, draft-cyberphone-jsf-00) signature
/// object. Wire shape follows the CycloneDX 1.6 `signature` schema.
///
/// For US2a we ship a minimal but conformant shape. Fields:
/// - `algorithm`: JWS-style algorithm identifier (`"ES256"` for
///   ECDSA-P256, `"EdDSA"` for Ed25519).
/// - `publicKey`: JWK-shaped public key so verifiers can validate
///   offline without out-of-band key distribution.
/// - `value`: base64url-encoded signature bytes.
#[derive(Clone, Debug, Serialize)]
pub struct JsfSignature {
    pub algorithm: String,
    #[serde(rename = "publicKey")]
    pub public_key: JsfPublicKey,
    /// Base64url-encoded signature bytes. During canonicalization for
    /// signing, this MUST be `""` (empty string) per JSF §4.3 to
    /// preserve determinism.
    pub value: String,
}

/// JWK-shaped public key material. Held as a PEM string for v1 —
/// verifier tooling can re-parse into a full JWK if needed. This
/// shape is intentionally simple; full JWK parameter split (kty/crv/
/// x/y for EC, kty/x for OKP) is a US2b enhancement.
#[derive(Clone, Debug, Serialize)]
pub struct JsfPublicKey {
    /// PEM-encoded SubjectPublicKeyInfo.
    pub pem: String,
    /// Human-readable algorithm hint (matches `JsfSignature.algorithm`).
    #[serde(rename = "algorithmHint")]
    pub algorithm_hint: String,
}

/// Tagged failure modes for the SBOM-signing pipeline. Every variant
/// is user-actionable; the CLI diagnostic surfaces the enum variant
/// name so operators know exactly which subsystem failed.
#[derive(Debug, Error)]
pub enum SbomSigningError {
    #[error("could not load signing key: {0}")]
    KeyLoadFailed(#[from] SigningError),

    #[error(
        "unsupported signing key algorithm: {algorithm} \
         (US2a supports ECDSA-P256 and Ed25519; other algorithms are \
         deferred to a follow-up milestone)"
    )]
    AlgorithmUnsupported { algorithm: String },

    #[error("low-level signing operation failed: {detail}")]
    SignFailed { detail: String },

    #[error("canonical JSON serialization failed: {0}")]
    Serialization(#[from] waybill_common::attestation::envelope::SerializationError),

    #[error("serde_json error while constructing signature envelope: {0}")]
    JsonEncoding(#[from] serde_json::Error),

    #[error("cannot export public key PEM for signature envelope: {detail}")]
    PublicKeyExportFailed { detail: String },

    #[error(
        "unsupported operation: {operation} \
         (US2a covers static-key JSF/DSSE only; keyless is US2b)"
    )]
    NotImplemented { operation: String },
}

// ---------------------------------------------------------------------------
// Top-level signing entrypoints
// ---------------------------------------------------------------------------

/// Sign the CycloneDX document's `metadata.signature` slot **in place**.
///
/// Reads the `metadata.signature` slot, sets its `value` to `""`,
/// canonicalizes the entire document via `canonical_json_bytes`, signs
/// the canonical bytes with the key referenced by `mode`, and writes
/// the actual base64 signature back into `value`.
///
/// When `mode == SigningMode::Unsigned`, this is a no-op — the passed
/// `Value` is returned unchanged, guaranteeing FR-009 byte-identity.
pub fn sign_cdx_document_in_place(
    doc: &mut serde_json::Value,
    mode: &SigningMode,
) -> Result<(), SbomSigningError> {
    if !mode.is_enabled() {
        return Ok(());
    }

    // Milestone 222 US2b — Sigstore keyless dispatch. Per
    // `specs/222-sigstore-keyless-signing/contracts/keyless-signing-flow.md`
    // §CDX-embedded Bundle canonical-bytes contract, we canonicalize
    // the CDX doc WITHOUT `metadata.signature` populated (unlike the
    // JSF empty-value trick below), sign those bytes, then insert the
    // Bundle at `metadata.signature`. Verifiers reproduce the signed
    // bytes by parsing the doc, REMOVING `metadata.signature`, and
    // re-canonicalizing.
    if let SigningMode::Keyless {
        fulcio_url,
        rekor_url,
        rekor_timeout,
    } = mode
    {
        // Ensure metadata.signature is absent before canonicalizing.
        if let Some(meta) = doc.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            meta.remove("signature");
        } else {
            return Err(SbomSigningError::SignFailed {
                detail: "CDX document has no `metadata` object; cannot insert signature slot"
                    .to_string(),
            });
        }
        let canonical = canonical_json_bytes(doc)?;
        let success = crate::attestation::signer::sign_keyless_sbom(
            &canonical,
            fulcio_url,
            rekor_url,
            *rekor_timeout,
        )
        .map_err(|e| SbomSigningError::SignFailed {
            detail: format!("Sigstore keyless sign failed: {e}"),
        })?;
        let bundle_json =
            serde_json::to_value(&success.bundle).map_err(SbomSigningError::from)?;
        let meta = doc
            .get_mut("metadata")
            .and_then(|m| m.as_object_mut())
            .expect("metadata presence verified above");
        meta.insert("signature".to_string(), bundle_json);
        return Ok(());
    }

    let keypair = load_key(mode)?;
    let algorithm = KeyAlgorithm::EcdsaP256; // See note on scheme in load_key.
    let scheme = signing_scheme_for(algorithm);
    let public_key_pem = export_public_key_pem(&keypair)?;

    // JSF empty-value trick: populate metadata.signature with the fully-
    // shaped envelope EXCEPT `value = ""`, canonicalize, sign, then
    // fill the real base64 value in.
    let jwa = jwa_alg(algorithm);
    let placeholder = JsfSignature {
        algorithm: jwa.to_string(),
        public_key: JsfPublicKey {
            pem: public_key_pem.clone(),
            algorithm_hint: jwa.to_string(),
        },
        value: String::new(),
    };

    // Insert placeholder under metadata.signature. Requires that a
    // `metadata` object exists — every CDX emit path creates one.
    let placeholder_json = serde_json::to_value(&placeholder)?;
    let meta = doc
        .get_mut("metadata")
        .and_then(|m| m.as_object_mut())
        .ok_or_else(|| SbomSigningError::SignFailed {
            detail: "CDX document has no `metadata` object; cannot insert signature slot"
                .to_string(),
        })?;
    meta.insert("signature".to_string(), placeholder_json);

    let canonical = canonical_json_bytes(doc)?;
    let signer = keypair
        .to_sigstore_signer(&scheme)
        .map_err(|e| SbomSigningError::SignFailed {
            detail: format!("cannot build signer from key: {e}"),
        })?;
    let sig_bytes = signer.sign(&canonical).map_err(|e| SbomSigningError::SignFailed {
        detail: format!("signature computation failed: {e}"),
    })?;
    let sig_b64 = BASE64_STD.encode(&sig_bytes);

    // Fill in the real value.
    let meta = doc
        .get_mut("metadata")
        .and_then(|m| m.as_object_mut())
        .expect("metadata inserted above still present");
    let sig = meta
        .get_mut("signature")
        .and_then(|s| s.as_object_mut())
        .expect("signature inserted above still present");
    sig.insert("value".to_string(), serde_json::Value::String(sig_b64));

    Ok(())
}

/// Produce a signature sidecar for the given SPDX bytes. Returns
/// `Ok(None)` when `mode == Unsigned` (no sidecar file written).
///
/// - `SigningMode::StaticKey{..}` → `Sidecar::Dsse(SignedEnvelope)`
///   with `payloadType = SBOM_DSSE_PAYLOAD_TYPE`.
/// - `SigningMode::Keyless{..}` → `Sidecar::SigstoreBundle(Bundle)`
///   with the raw SPDX bytes as the signed payload (detached; the
///   SPDX doc on disk equals the signed bytes byte-for-byte per the
///   `contracts/keyless-signing-flow.md` CDX/SPDX contract split).
///
/// Milestone 222 US2b renamed the entry from `sign_spdx_bytes_to_dsse`
/// to reflect the Sidecar shape shift; a thin backwards-compatible
/// alias is preserved for the m221 US2a call site until the CLI
/// migrates to the new return type.
pub fn sign_spdx_bytes_to_sidecar(
    spdx_bytes: &[u8],
    mode: &SigningMode,
) -> Result<Option<Sidecar>, SbomSigningError> {
    if !mode.is_enabled() {
        return Ok(None);
    }

    // Milestone 222 US2b — Sigstore keyless dispatch.
    if let SigningMode::Keyless {
        fulcio_url,
        rekor_url,
        rekor_timeout,
    } = mode
    {
        let success = crate::attestation::signer::sign_keyless_sbom(
            spdx_bytes,
            fulcio_url,
            rekor_url,
            *rekor_timeout,
        )
        .map_err(|e| SbomSigningError::SignFailed {
            detail: format!("Sigstore keyless sign failed: {e}"),
        })?;
        return Ok(Some(Sidecar::SigstoreBundle(Box::new(success.bundle))));
    }

    let keypair = load_key(mode)?;
    let algorithm = KeyAlgorithm::EcdsaP256;
    let scheme = signing_scheme_for(algorithm);
    let public_key_pem = export_public_key_pem(&keypair)?;

    let pae = dsse_pae(SBOM_DSSE_PAYLOAD_TYPE, spdx_bytes);
    let signer = keypair
        .to_sigstore_signer(&scheme)
        .map_err(|e| SbomSigningError::SignFailed {
            detail: format!("cannot build signer from key: {e}"),
        })?;
    let sig_bytes = signer.sign(&pae).map_err(|e| SbomSigningError::SignFailed {
        detail: format!("signature computation failed: {e}"),
    })?;

    Ok(Some(Sidecar::Dsse(SignedEnvelope {
        payload_type: SBOM_DSSE_PAYLOAD_TYPE.to_string(),
        payload: BASE64_STD.encode(spdx_bytes),
        signatures: vec![Signature {
            keyid: None,
            sig: BASE64_STD.encode(&sig_bytes),
            identity: IdentityMetadata::PublicKey {
                public_key: public_key_pem,
                algorithm,
            },
        }],
    })))
}

/// Milestone 221 US2a legacy alias — returns just the DSSE envelope,
/// or `Err(NotImplemented)` for the m222 US2b Keyless variant (that
/// path must go through `sign_spdx_bytes_to_sidecar` for the correct
/// Bundle return type). Kept during the m222 transition; delete once
/// the CLI's SPDX sidecar-writer is fully on `Sidecar`.
pub fn sign_spdx_bytes_to_dsse(
    spdx_bytes: &[u8],
    mode: &SigningMode,
) -> Result<Option<SignedEnvelope>, SbomSigningError> {
    match sign_spdx_bytes_to_sidecar(spdx_bytes, mode)? {
        None => Ok(None),
        Some(Sidecar::Dsse(env)) => Ok(Some(env)),
        Some(Sidecar::SigstoreBundle(_)) => Err(SbomSigningError::NotImplemented {
            operation: "sign_spdx_bytes_to_dsse called with SigningMode::Keyless — \
                        Sigstore keyless signing produces a Bundle sidecar, not DSSE. \
                        Callers must migrate to sign_spdx_bytes_to_sidecar."
                .to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_key(mode: &SigningMode) -> Result<SigStoreKeyPair, SbomSigningError> {
    match mode {
        SigningMode::Unsigned => Err(SbomSigningError::NotImplemented {
            operation: "load_key called on Unsigned mode".to_string(),
        }),
        SigningMode::StaticKey {
            key_ref,
            passphrase_env,
        } => {
            let passphrase_env_ref = if std::env::var(passphrase_env).is_ok() {
                Some(passphrase_env.as_str())
            } else {
                None
            };
            let keypair = load_local_signer(key_ref, passphrase_env_ref)?;
            Ok(keypair)
        }
        SigningMode::Keyless { .. } => Err(SbomSigningError::NotImplemented {
            operation: "load_key called on Keyless mode — keyless sign path does not load \
                        a local key; it uses an ephemeral Fulcio-issued cert. Callers must \
                        dispatch on SigningMode::Keyless before reaching load_key."
                .to_string(),
        }),
    }
}

fn signing_scheme_for(alg: KeyAlgorithm) -> SigningScheme {
    match alg {
        KeyAlgorithm::EcdsaP256 => SigningScheme::ECDSA_P256_SHA256_ASN1,
        KeyAlgorithm::Ed25519 => SigningScheme::ED25519,
        // RSA falls back to ECDSA-P256 for now — matches
        // m006 `scheme_for_algorithm` behavior. US2b may split this
        // out when the JSF vocabulary grows.
        KeyAlgorithm::RsaPkcs1 => SigningScheme::ECDSA_P256_SHA256_ASN1,
    }
}

fn jwa_alg(alg: KeyAlgorithm) -> &'static str {
    // JWA identifiers per RFC 7518 — the JSF spec references these.
    match alg {
        KeyAlgorithm::EcdsaP256 => "ES256",
        KeyAlgorithm::Ed25519 => "EdDSA",
        KeyAlgorithm::RsaPkcs1 => "RS256",
    }
}

fn export_public_key_pem(keypair: &SigStoreKeyPair) -> Result<String, SbomSigningError> {
    keypair
        .public_key_to_pem()
        .map_err(|e| SbomSigningError::PublicKeyExportFailed {
            detail: e.to_string(),
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;
    use sigstore::crypto::signing_key::SigStoreKeyPair;
    use sigstore::crypto::SigningScheme;
    use tempfile::NamedTempFile;

    /// Generate an ephemeral P-256 keypair, write its PEM to a
    /// tempfile, and return both the tempfile (private-key PEM on
    /// disk) and the loaded `SigStoreKeyPair` for downstream
    /// verification with the same key material.
    fn ephemeral_p256_pem_file() -> (NamedTempFile, SigStoreKeyPair) {
        let scheme = SigningScheme::ECDSA_P256_SHA256_ASN1;
        let signer = scheme.create_signer().expect("create_signer");
        let keypair = signer.to_sigstore_keypair().expect("to_sigstore_keypair");
        let pem = keypair.private_key_to_pem().expect("private_key_to_pem");
        let f = NamedTempFile::new().expect("tempfile");
        std::fs::write(f.path(), pem).expect("write pem");
        (f, keypair)
    }

    #[test]
    fn signing_mode_is_enabled_reflects_variant_m221() {
        assert!(!SigningMode::Unsigned.is_enabled());
        assert!(SigningMode::StaticKey {
            key_ref: PathBuf::from("/tmp/x.pem"),
            passphrase_env: "WAYBILL_SIGN_KEY_PASSPHRASE".to_string(),
        }
        .is_enabled());
    }

    #[test]
    fn signing_mode_keyless_is_enabled_m222() {
        let mode = SigningMode::Keyless {
            fulcio_url: "https://fulcio.sigstore.dev".to_string(),
            rekor_url: "https://rekor.sigstore.dev".to_string(),
            rekor_timeout: std::time::Duration::from_secs(30),
        };
        assert!(mode.is_enabled());
    }

    #[test]
    fn load_key_rejects_keyless_mode_m222() {
        let mode = SigningMode::Keyless {
            fulcio_url: "https://fulcio.sigstore.dev".to_string(),
            rekor_url: "https://rekor.sigstore.dev".to_string(),
            rekor_timeout: std::time::Duration::from_secs(30),
        };
        match load_key(&mode) {
            Err(SbomSigningError::NotImplemented { .. }) => {}
            Err(other) => panic!("expected NotImplemented, got {other:?}"),
            Ok(_) => panic!("expected keyless mode to not go through load_key"),
        }
    }

    #[test]
    fn sign_cdx_document_in_place_noop_when_unsigned_m221() {
        let mut doc = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "metadata": {"timestamp": "2026-07-29T00:00:00Z"},
        });
        let baseline = doc.clone();
        sign_cdx_document_in_place(&mut doc, &SigningMode::Unsigned)
            .expect("no-op signing returns Ok");
        assert_eq!(doc, baseline, "Unsigned mode MUST leave document byte-identical");
        assert!(
            doc.get("metadata").and_then(|m| m.get("signature")).is_none(),
            "no signature slot should be inserted in Unsigned mode"
        );
    }

    #[test]
    fn sign_cdx_document_in_place_populates_signature_slot_m221() {
        let (pem_file, _keypair) = ephemeral_p256_pem_file();
        let mut doc = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "metadata": {"timestamp": "2026-07-29T00:00:00Z"},
            "components": [],
        });
        let mode = SigningMode::StaticKey {
            key_ref: pem_file.path().to_path_buf(),
            passphrase_env: "WAYBILL_UNSET_TEST_ENV_M221".to_string(),
        };
        sign_cdx_document_in_place(&mut doc, &mode).expect("static-key sign");

        let sig = doc.pointer("/metadata/signature").expect("signature slot exists");
        assert_eq!(sig["algorithm"], "ES256");
        assert!(sig["publicKey"]["pem"].as_str().unwrap().contains("BEGIN PUBLIC KEY"));
        assert!(
            !sig["value"].as_str().unwrap().is_empty(),
            "signature value must be non-empty base64"
        );
    }

    #[test]
    fn sign_cdx_document_in_place_rejects_missing_metadata_m221() {
        let (pem_file, _keypair) = ephemeral_p256_pem_file();
        let mut doc = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [],
        });
        let mode = SigningMode::StaticKey {
            key_ref: pem_file.path().to_path_buf(),
            passphrase_env: "WAYBILL_UNSET_TEST_ENV_M221".to_string(),
        };
        let err = sign_cdx_document_in_place(&mut doc, &mode).unwrap_err();
        assert!(
            matches!(err, SbomSigningError::SignFailed { .. }),
            "missing metadata should surface as SignFailed, got {err:?}"
        );
    }

    #[test]
    fn sign_spdx_bytes_to_dsse_noop_when_unsigned_m221() {
        let result = sign_spdx_bytes_to_dsse(b"any", &SigningMode::Unsigned)
            .expect("no-op returns Ok");
        assert!(result.is_none(), "Unsigned mode returns None");
    }

    #[test]
    fn sign_spdx_bytes_to_dsse_wraps_payload_m221() {
        let (pem_file, _keypair) = ephemeral_p256_pem_file();
        let mode = SigningMode::StaticKey {
            key_ref: pem_file.path().to_path_buf(),
            passphrase_env: "WAYBILL_UNSET_TEST_ENV_M221".to_string(),
        };
        let payload = br#"{"spdxVersion":"SPDX-2.3"}"#.to_vec();
        let env = sign_spdx_bytes_to_dsse(&payload, &mode)
            .expect("sign ok")
            .expect("envelope present");

        assert_eq!(env.payload_type, SBOM_DSSE_PAYLOAD_TYPE);
        let decoded = BASE64_STD.decode(&env.payload).expect("base64 decode");
        assert_eq!(decoded, payload);
        assert_eq!(env.signatures.len(), 1);
        assert!(!env.signatures[0].sig.is_empty());
    }

    #[test]
    fn sign_cdx_signature_verifies_with_matching_pubkey_m221() {
        use sigstore::crypto::verification_key::CosignVerificationKey;
        use sigstore::crypto::Signature as SigstoreSig;

        // Full round-trip: sign a CDX doc, extract the signature
        // value, reset the slot to "", recanonicalize, and verify
        // against the ephemeral pubkey.
        let (pem_file, keypair) = ephemeral_p256_pem_file();
        let mut doc = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "metadata": {"timestamp": "2026-07-29T00:00:00Z"},
            "components": [{"name": "example", "type": "library"}],
        });
        let mode = SigningMode::StaticKey {
            key_ref: pem_file.path().to_path_buf(),
            passphrase_env: "WAYBILL_UNSET_TEST_ENV_M221".to_string(),
        };
        sign_cdx_document_in_place(&mut doc, &mode).expect("sign ok");

        let sig_b64 = doc
            .pointer("/metadata/signature/value")
            .and_then(|v| v.as_str())
            .expect("signature value populated")
            .to_string();
        let sig_bytes = BASE64_STD.decode(&sig_b64).expect("base64 decode");

        // Reset value → recanonicalize (matches sign-side JCS input).
        let meta = doc.as_object_mut().unwrap().get_mut("metadata").unwrap()
            .as_object_mut().unwrap();
        let sig = meta.get_mut("signature").unwrap().as_object_mut().unwrap();
        sig.insert("value".to_string(), json!(""));
        let canonical = canonical_json_bytes(&doc).expect("canonicalize");

        let pubkey_pem = keypair.public_key_to_pem().expect("pub pem");
        let vk = CosignVerificationKey::from_pem(
            pubkey_pem.as_bytes(),
            &SigningScheme::ECDSA_P256_SHA256_ASN1,
        )
        .expect("verification key");
        vk.verify_signature(SigstoreSig::Raw(&sig_bytes), &canonical)
            .expect("signature must verify against matching pubkey");

        // Mutation of any byte in the canonical payload must flip verify.
        let mut mutated = canonical.clone();
        // Flip a byte in the middle of the payload to avoid altering
        // structural JSON like `{}` at the boundaries.
        let mid = mutated.len() / 2;
        mutated[mid] ^= 0x01;
        assert!(
            vk.verify_signature(SigstoreSig::Raw(&sig_bytes), &mutated).is_err(),
            "mutation of signed bytes MUST cause verify to fail"
        );
    }
}
