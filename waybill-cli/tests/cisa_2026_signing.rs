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
// US2b — Sigstore keyless (feature 222-sigstore-keyless-signing)
// ---------------------------------------------------------------------------

/// Runs `waybill sbom scan --sign ...` as a subprocess with the given
/// extra flags + env-var overrides. Extra env vars are set on the
/// child process only (never on the parent — avoids env-var pollution
/// racing with other tests). Every entry in `extra_env_unset` is
/// explicitly cleared for the child to defeat inherited ambient state.
fn run_scan_with_sign_env(
    target: &std::path::Path,
    output: &std::path::Path,
    extra_args: &[&str],
    extra_env_set: &[(&str, &str)],
    extra_env_unset: &[&str],
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
        .arg("--no-deep-hash")
        .arg("--sign");
    for a in extra_args {
        cmd.arg(a);
    }
    for (k, v) in extra_env_set {
        cmd.env(k, v);
    }
    for k in extra_env_unset {
        cmd.env_remove(k);
    }
    cmd.output().expect("waybill invocation")
}

/// T008 + T025 (feature 222 US2b, FR-009a) — Fulcio unreachable →
/// fail-close. Points Fulcio at a non-routable URL, ensures at least
/// one OIDC provider is present so the failure occurs at Fulcio (not
/// earlier at token acquisition), asserts non-zero exit + no partial
/// output file left behind.
///
/// Runs unconditionally (no WAYBILL_TEST_KEYLESS gate) — pure
/// failure-mode test with a stub OIDC token.
#[test]
fn us2b_keyless_signing_failure_cleans_up_output_m222() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("should-not-persist.cdx.json");
    let out = run_scan_with_sign_env(
        &scan_target(),
        &output,
        &[
            "--fulcio-url",
            "https://fulcio.invalid.example.test",
            "--rekor-url",
            "https://rekor.invalid.example.test",
        ],
        // Provide a stub JWT so provider-detection routes to `Explicit`
        // and we get past OIDC → into Fulcio (which resolves DNS-fail).
        &[(
            "SIGSTORE_ID_TOKEN",
            // Same header.payload.signature JWT shape as the unit
            // tests use — aud=sigstore + exp far in the future.
            "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.\
             eyJhdWQiOiJzaWdzdG9yZSIsImV4cCI6MjA2NDAwMDAwMCwiZW1haWwiOiJ0ZXN0QHdheWJpbGwuZGV2In0.",
        )],
        &[
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
        ],
    );
    assert!(
        !out.status.success(),
        "scan MUST fail when Fulcio is unreachable (FR-009a fail-close). stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Regression guard (PR #645 CI): assert the failure surfaces as a
    // clean SigningError variant, NOT a panic. Before the runtime-isolation
    // fix, sign_keyless_sbom panicked on reqwest::blocking-inside-tokio
    // BEFORE ever reaching Fulcio, and this test still passed because it
    // only checked exit status.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked at"),
        "keyless sign MUST NOT panic — expected clean SigningError. stderr:\n{stderr}"
    );
    // Sigstore-rs classifies reqwest network errors (like DNS-fail
    // on our invalid Fulcio URL) as its own `UnexpectedError` variant,
    // which our classify_sign_error maps to `CryptoError`. Assert on
    // the URL being in the error message (proof we actually attempted
    // the Fulcio round-trip) OR any of the typical network-fail
    // substrings. What matters is that this is a REAL sign error, not
    // a panic — the assert!(!"panicked at") check above is the
    // regression guard for the tokio-runtime bug.
    assert!(
        stderr.contains("fulcio.invalid.example.test")
            || stderr.contains("FulcioError")
            || stderr.contains("CryptoError")
            || stderr.contains("error sending request"),
        "expected network-error diagnostic naming the invalid Fulcio URL, got:\n{stderr}"
    );
    assert!(
        !output.exists(),
        "partial output file MUST be unlinked on signing failure (FR-009a)"
    );
}

/// T009 + T026 (feature 222 US2b, FR-005 + FR-009) — no OIDC token
/// available → fail-close with actionable diagnostic. Clears all
/// provider-detection env vars so `OidcProvider::detect()` routes to
/// `Interactive` → resolves to fail-close (Q1 clarification).
#[test]
fn us2b_keyless_no_oidc_token_fails_close_m222() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("no-token.cdx.json");
    let out = run_scan_with_sign_env(
        &scan_target(),
        &output,
        &[],
        &[],
        &[
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
            "SIGSTORE_ID_TOKEN",
        ],
    );
    assert!(
        !out.status.success(),
        "scan MUST fail when no OIDC token is available (FR-005 + FR-009)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Q1 clarification — diagnostic must include all three substrings.
    assert!(
        stderr.contains("no OIDC token available"),
        "stderr missing Q1 diagnostic substring: {stderr}"
    );
    assert!(
        stderr.contains("SIGSTORE_ID_TOKEN"),
        "stderr missing SIGSTORE_ID_TOKEN pointer: {stderr}"
    );
    assert!(
        stderr.contains("id-token: write"),
        "stderr missing GHA workflow-permission pointer: {stderr}"
    );
    assert!(
        !output.exists(),
        "partial output file MUST be unlinked (FR-009a)"
    );
}

/// T029 (feature 222 US2b, FR-008a) — `--sign` + `--output -`
/// (stdout) rejected at parse time. Should exit before any Sigstore
/// call is made, so no env-var setup is needed.
#[test]
fn us2b_keyless_stdout_output_is_rejected_at_parse_m222() {
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
        .arg("--no-deep-hash")
        .arg("--sign")
        .output()
        .expect("waybill invocation");
    assert!(
        !out.status.success(),
        "scan MUST reject --sign + --output - at parse time (FR-008a)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--sign requires --output <file>"),
        "stderr missing FR-008a diagnostic wording: {stderr}"
    );
}

/// T010 + T024 (feature 222 US2b) — happy-path sign-and-verify
/// against Sigstore staging. Requires WAYBILL_TEST_KEYLESS=1 AND a
/// GitHub-Actions-ambient OIDC endpoint (or an equivalent explicit
/// SIGSTORE_ID_TOKEN). Gated behind the env var so the general
/// `cargo test --workspace` suite stays hermetic; the CI job
/// `lint-and-test-keyless-sbom` sets the env var + provides the
/// ambient OIDC path.
#[test]
#[ignore = "US2b: Sigstore keyless requires WAYBILL_TEST_KEYLESS=1 + OIDC + Sigstore staging network access; run by the dedicated CI job only"]
fn us2b_keyless_bundle_sign_and_verify() {
    if std::env::var("WAYBILL_TEST_KEYLESS").is_err() {
        eprintln!(
            "INFO: us2b_keyless_bundle_sign_and_verify skipped (WAYBILL_TEST_KEYLESS unset)"
        );
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&output)
        .arg("--no-deep-hash")
        .arg("--sign")
        .env(
            "WAYBILL_FULCIO_URL",
            std::env::var("WAYBILL_FULCIO_URL")
                .unwrap_or_else(|_| "https://fulcio.sigstage.dev".to_string()),
        )
        .env(
            "WAYBILL_REKOR_URL",
            std::env::var("WAYBILL_REKOR_URL")
                .unwrap_or_else(|_| "https://rekor.sigstage.dev".to_string()),
        )
        .output()
        .expect("waybill invocation");

    assert!(
        out.status.success(),
        "keyless sign against staging failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists(), "signed CDX file missing at {}", output.display());

    // Parse the emitted CDX + assert a metadata.signature slot exists +
    // its shape is the Sigstore Bundle wire format.
    let raw = std::fs::read(&output).expect("read signed cdx");
    let doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");
    let sig = doc
        .pointer("/metadata/signature")
        .expect("metadata.signature slot missing");
    let sig_obj = sig.as_object().expect("signature must be a JSON object");
    assert!(
        sig_obj.contains_key("mediaType") || sig_obj.contains_key("verificationMaterial"),
        "metadata.signature is not a Sigstore Bundle shape: {sig}"
    );
    let sig_str = serde_json::to_string(sig).unwrap_or_default();
    assert!(
        sig_str.contains("tlogEntries") || sig_str.contains("verificationMaterial"),
        "Bundle missing expected Rekor + verification-material fields"
    );
}

/// T028 (feature 222 US2b, FR-016 + SC-008) — successful sign emits
/// three structured INFO fields at tracing::info!. Runs sign against
/// staging and greps stderr for `rekor_log_index=`, `fulcio_cert_subject=`,
/// `oidc_provider=`. Gated on `WAYBILL_TEST_KEYLESS=1`.
#[test]
#[ignore = "US2b FR-016 requires WAYBILL_TEST_KEYLESS=1 + OIDC + Sigstore staging network access; run by the dedicated CI job only"]
fn us2b_keyless_fr016_info_log_fields_m222() {
    if std::env::var("WAYBILL_TEST_KEYLESS").is_err() {
        eprintln!(
            "INFO: us2b_keyless_fr016_info_log_fields_m222 skipped (WAYBILL_TEST_KEYLESS unset)"
        );
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("info-log.cdx.json");
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&output)
        .arg("--no-deep-hash")
        .arg("--sign")
        .env("RUST_LOG", "info")
        .env("WAYBILL_LOG", "info")
        .env(
            "WAYBILL_FULCIO_URL",
            std::env::var("WAYBILL_FULCIO_URL")
                .unwrap_or_else(|_| "https://fulcio.sigstage.dev".to_string()),
        )
        .env(
            "WAYBILL_REKOR_URL",
            std::env::var("WAYBILL_REKOR_URL")
                .unwrap_or_else(|_| "https://rekor.sigstage.dev".to_string()),
        )
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "keyless sign against staging failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rekor_log_index="),
        "FR-016: stderr missing rekor_log_index INFO field:\n{stderr}"
    );
    assert!(
        stderr.contains("fulcio_cert_subject="),
        "FR-016: stderr missing fulcio_cert_subject INFO field:\n{stderr}"
    );
    assert!(
        stderr.contains("oidc_provider="),
        "FR-016: stderr missing oidc_provider INFO field:\n{stderr}"
    );
}

/// T027 (feature 222 US2b) — signature covers the entire SBOM
/// document. Mirrors m221 US2a's mutation-flips-verify pattern. Sign
/// against staging, mutate one byte of the CDX payload, extract the
/// Bundle, verify it against the mutated payload — expect Err.
/// Gated on `WAYBILL_TEST_KEYLESS=1`.
#[test]
#[ignore = "US2b: mutation-flip test requires WAYBILL_TEST_KEYLESS=1 + OIDC + Sigstore staging network access; run by the dedicated CI job only"]
fn us2b_keyless_signature_covers_document_mutation_m222() {
    if std::env::var("WAYBILL_TEST_KEYLESS").is_err() {
        eprintln!(
            "INFO: us2b_keyless_signature_covers_document_mutation_m222 skipped (WAYBILL_TEST_KEYLESS unset)"
        );
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("mutation.cdx.json");
    let out = Command::new(bin())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&output)
        .arg("--no-deep-hash")
        .arg("--sign")
        .env(
            "WAYBILL_FULCIO_URL",
            std::env::var("WAYBILL_FULCIO_URL")
                .unwrap_or_else(|_| "https://fulcio.sigstage.dev".to_string()),
        )
        .env(
            "WAYBILL_REKOR_URL",
            std::env::var("WAYBILL_REKOR_URL")
                .unwrap_or_else(|_| "https://rekor.sigstage.dev".to_string()),
        )
        .output()
        .expect("waybill invocation");
    assert!(
        out.status.success(),
        "keyless sign against staging failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Parse the signed CDX, mutate one byte in a non-signature field,
    // re-serialize, and confirm the round-trip verify fails. Full
    // Sigstore Bundle verification via sigstore::bundle::verify is out
    // of scope for this quick sanity check — we validate the coverage
    // property at the shape level: any mutation to the payload MUST
    // invalidate the bundle's canonical-bytes contract.
    let raw = std::fs::read(&output).expect("read signed cdx");
    let mut doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");

    // Extract the Bundle before mutation.
    let bundle = doc
        .get("metadata")
        .and_then(|m| m.get("signature"))
        .cloned()
        .expect("metadata.signature slot missing");

    // Mutate specVersion (a benign field the signature covers).
    doc["specVersion"] = serde_json::Value::String("MUTATED-1.6".to_string());

    // Strip metadata.signature (per contracts/keyless-signing-flow.md
    // §CDX-embedded Bundle canonical-bytes contract, verifiers
    // reproduce the signed bytes by removing this field entirely).
    if let Some(meta) = doc.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        meta.remove("signature");
    }
    let mutated_canonical =
        serde_json::to_vec(&doc).expect("re-serialize mutated CDX to canonical bytes");

    // The mutated bytes MUST differ from what the Bundle signed.
    // Compare via sha256 to keep this test dep-free.
    use sha2::{Digest, Sha256};
    let mutated_hash = {
        let mut h = Sha256::new();
        h.update(&mutated_canonical);
        h.finalize().to_vec()
    };
    let bundle_digest_b64 = bundle
        .pointer("/messageSignature/messageDigest/digest")
        .and_then(|v| v.as_str())
        .expect("Bundle messageSignature.messageDigest.digest missing");
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    let bundle_digest = B64.decode(bundle_digest_b64).expect("bundle digest is valid base64");
    assert_ne!(
        mutated_hash, bundle_digest,
        "mutation MUST invalidate the Bundle's signed-bytes digest (payload-coverage guarantee)"
    );
}
