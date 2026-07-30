//! Feature 221 US2a — end-to-end signing tests for static-key JSF (CDX)
//! + DSSE sidecar (SPDX 2.3 / SPDX 3).
//!
//! Sub-slice A ships static-key only; the Sigstore keyless test
//! (`us2b_keyless_bundle_sign_and_verify`) is marked `#[ignore]` and
//! only runs when `WAYBILL_TEST_KEYLESS=1` is set in the environment.
//! CI enables that env in a dedicated `lint-and-test-keyless-sbom`
//! job — see feature 221 tasks.md T036.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::PathBuf;
use std::process::Command;

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use sigstore::crypto::signing_key::SigStoreKeyPair;
use sigstore::crypto::verification_key::CosignVerificationKey;
use sigstore::crypto::{Signature as SigstoreSig, SigningScheme};
use waybill_common::attestation::envelope::{canonical_json_bytes, dsse_pae};

mod common;
use common::{bin, workspace_root};

/// Generate an ephemeral P-256 keypair, write private-key PEM to a
/// tempfile, and return `(PEM path, public-key PEM string)`.
fn ephemeral_keypair() -> (tempfile::NamedTempFile, String) {
    let scheme = SigningScheme::ECDSA_P256_SHA256_ASN1;
    let signer = scheme.create_signer().expect("signer");
    let keypair: SigStoreKeyPair =
        signer.to_sigstore_keypair().expect("keypair");
    let private_pem = keypair.private_key_to_pem().expect("private pem");
    let public_pem = keypair.public_key_to_pem().expect("public pem");
    let f = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(f.path(), private_pem).expect("write");
    (f, public_pem)
}

/// Path to a small, deterministic scan target. Prefers the m090
/// fixture cache's `transitive_parity/cargo` (populated by the
/// milestone-090 harness — ~400 components, ~5s scan). Falls back to
/// the workspace root only if the cache is absent, so this test
/// stays hermetic on fresh clones AT THE COST of a longer scan.
fn scan_target() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home)
            .join(".cache")
            .join("waybill")
            .join("fixtures");
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("transitive_parity").join("cargo");
                if candidate.join("Cargo.toml").exists() {
                    return candidate;
                }
            }
        }
    }
    workspace_root()
}

fn run_scan(
    target: &std::path::Path,
    output: &std::path::Path,
    extra_args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(target)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(output)
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.output().expect("waybill invocation")
}

// ---------------------------------------------------------------------------
// US2a — static-key JSF sign into CDX metadata.signature
// ---------------------------------------------------------------------------

#[test]
fn us2a_static_key_jsf_sign_and_verify() {
    let (pem_file, public_pem) = ephemeral_keypair();
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");

    let sign_flag = format!("--sign-key={}", pem_file.path().display());
    let out = run_scan(&scan_target(), &output, &[&sign_flag]);
    assert!(
        out.status.success(),
        "signed scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists(), "signed CDX output missing");

    // Extract the signature slot.
    let raw = std::fs::read(&output).expect("read signed cdx");
    let mut doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");
    let sig_slot = doc
        .pointer("/metadata/signature")
        .expect("metadata.signature slot populated")
        .clone();
    assert_eq!(
        sig_slot["algorithm"], "ES256",
        "signature.algorithm must be ES256 for ECDSA-P256"
    );
    assert!(
        sig_slot["publicKey"]["pem"]
            .as_str()
            .unwrap()
            .contains("BEGIN PUBLIC KEY"),
        "signature.publicKey.pem must contain a PEM header"
    );
    let sig_b64 = sig_slot["value"].as_str().expect("signature value string");
    let sig_bytes = BASE64_STD.decode(sig_b64).expect("base64 sig decode");

    // Reset value → recanonicalize → verify against the pubkey we
    // handed waybill.
    let meta = doc
        .as_object_mut()
        .unwrap()
        .get_mut("metadata")
        .unwrap()
        .as_object_mut()
        .unwrap();
    let sig = meta.get_mut("signature").unwrap().as_object_mut().unwrap();
    sig.insert("value".to_string(), serde_json::json!(""));
    let canonical = canonical_json_bytes(&doc).expect("canonicalize");

    let vk = CosignVerificationKey::from_pem(
        public_pem.as_bytes(),
        &SigningScheme::ECDSA_P256_SHA256_ASN1,
    )
    .expect("verification key");
    vk.verify_signature(SigstoreSig::Raw(&sig_bytes), &canonical)
        .expect("signature MUST verify against matching pubkey");
}

#[test]
fn us2a_signature_covers_document_mutation_flips_verify() {
    let (pem_file, public_pem) = ephemeral_keypair();
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let sign_flag = format!("--sign-key={}", pem_file.path().display());
    let out = run_scan(&scan_target(), &output, &[&sign_flag]);
    assert!(out.status.success(), "signed scan failed");

    let raw = std::fs::read(&output).expect("read");
    let mut doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse");
    let sig_b64 = doc
        .pointer("/metadata/signature/value")
        .and_then(|v| v.as_str())
        .expect("signature value")
        .to_string();
    let sig_bytes = BASE64_STD.decode(&sig_b64).expect("base64");

    // Mutate: append a component to `components[]` — any byte-level
    // change to the signed document MUST invalidate verify.
    let comps = doc
        .as_object_mut()
        .unwrap()
        .get_mut("components")
        .and_then(|c| c.as_array_mut())
        .expect("components array");
    comps.push(serde_json::json!({"name": "post-sign-tampered", "type": "library"}));

    // Reset value → recanonicalize the MUTATED doc → verify MUST fail.
    let meta = doc
        .as_object_mut()
        .unwrap()
        .get_mut("metadata")
        .unwrap()
        .as_object_mut()
        .unwrap();
    let sig = meta.get_mut("signature").unwrap().as_object_mut().unwrap();
    sig.insert("value".to_string(), serde_json::json!(""));
    let canonical = canonical_json_bytes(&doc).expect("canonicalize");

    let vk = CosignVerificationKey::from_pem(
        public_pem.as_bytes(),
        &SigningScheme::ECDSA_P256_SHA256_ASN1,
    )
    .expect("vk");
    let verify_result = vk.verify_signature(SigstoreSig::Raw(&sig_bytes), &canonical);
    assert!(
        verify_result.is_err(),
        "post-sign mutation MUST cause verify to fail"
    );
}

#[test]
fn us2a_signing_with_stdout_output_is_rejected_at_parse() {
    let (pem_file, _) = ephemeral_keypair();
    let sign_flag = format!("--sign-key={}", pem_file.path().display());
    // Combine --sign-key with --output '-' — FR-008a: reject.
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg("-")
        .arg(&sign_flag)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill invocation");
    assert!(
        !out.status.success(),
        "signing + --output - MUST be rejected"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--sign-key") && stderr.contains("stdout"),
        "diagnostic must name both --sign-key and stdout; got: {stderr}"
    );
}

#[test]
fn us2a_signing_failure_cleans_up_output_file() {
    // Point --sign-key at a non-existent path → signing fails hard.
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("should-not-persist.cdx.json");
    let bogus_key = tmp.path().join("nope.pem");
    let sign_flag = format!("--sign-key={}", bogus_key.display());

    let out = run_scan(&scan_target(), &output, &[&sign_flag]);
    assert!(
        !out.status.success(),
        "scan MUST fail when signing key is missing (FR-009a fail-close)"
    );
    assert!(
        !output.exists(),
        "partial output file MUST be unlinked on signing failure (FR-009a)"
    );
}

#[test]
fn us2a_unsigned_output_lacks_signature_slot_no_regression() {
    // FR-009: unsigned emit stays byte-identical to pre-m221. This
    // test asserts the narrower "no signature slot" contract; the
    // full byte-identity check lives in the milestone-wide golden
    // suite (spdx_regression, cdx_regression) which m221 doesn't
    // regenerate for the CDX path.
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("unsigned.cdx.json");
    let out = run_scan(&scan_target(), &output, &[]);
    assert!(out.status.success(), "unsigned scan failed");

    let raw = std::fs::read(&output).expect("read");
    let doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse");
    assert!(
        doc.pointer("/metadata/signature").is_none(),
        "unsigned CDX MUST NOT contain a metadata.signature slot"
    );
}

// ---------------------------------------------------------------------------
// US2a — SPDX 2.3 + SPDX 3 sidecar tests
// ---------------------------------------------------------------------------

#[test]
fn us2a_spdx23_dsse_sidecar_written_and_verifies() {
    let (pem_file, public_pem) = ephemeral_keypair();
    let tmp = tempfile::tempdir().expect("tempdir");
    let primary = tmp.path().join("scan.spdx.json");
    let sidecar = tmp.path().join("scan.spdx.json.sig.json");
    let sign_flag = format!("--sign-key={}", pem_file.path().display());

    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(&primary)
        .arg(&sign_flag)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "SPDX+sign failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(primary.exists(), "primary SPDX file missing");
    assert!(sidecar.exists(), "DSSE sidecar missing at {}", sidecar.display());

    // Parse the sidecar and verify the signature.
    let env_raw = std::fs::read(&sidecar).expect("read sidecar");
    let env: serde_json::Value = serde_json::from_slice(&env_raw).expect("parse dsse");
    assert_eq!(
        env["payloadType"], "application/vnd.waybill.sbom+json",
        "sidecar payloadType must match waybill SBOM DSSE type"
    );
    let payload_b64 = env["payload"].as_str().expect("payload field");
    let payload = BASE64_STD.decode(payload_b64).expect("base64");
    // The DSSE payload equals the primary SPDX bytes.
    let primary_bytes = std::fs::read(&primary).expect("read primary");
    assert_eq!(
        payload, primary_bytes,
        "sidecar payload must equal primary SPDX bytes"
    );

    let sig_b64 = env["signatures"][0]["sig"]
        .as_str()
        .expect("signature bytes");
    let sig_bytes = BASE64_STD.decode(sig_b64).expect("base64 sig");
    let pae = dsse_pae("application/vnd.waybill.sbom+json", &primary_bytes);

    let vk = CosignVerificationKey::from_pem(
        public_pem.as_bytes(),
        &SigningScheme::ECDSA_P256_SHA256_ASN1,
    )
    .expect("vk");
    vk.verify_signature(SigstoreSig::Raw(&sig_bytes), &pae)
        .expect("SPDX DSSE signature MUST verify against matching pubkey");
}

// ---------------------------------------------------------------------------
// US2b — reserved: Sigstore keyless (deferred to a follow-up session)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "US2b: Sigstore keyless requires WAYBILL_TEST_KEYLESS=1 + OIDC + Sigstore staging network access; run by the dedicated CI job only"]
fn us2b_keyless_bundle_sign_and_verify() {
    if std::env::var("WAYBILL_TEST_KEYLESS").is_err() {
        eprintln!(
            "INFO: us2b_keyless_bundle_sign_and_verify skipped (WAYBILL_TEST_KEYLESS unset)"
        );
        return;
    }
    // Full implementation lands with US2b — completing the m006
    // sign_keyless() scaffold at attestation/signer.rs:170+.
    unimplemented!("US2b — Sigstore keyless implementation deferred to a follow-up session");
}
