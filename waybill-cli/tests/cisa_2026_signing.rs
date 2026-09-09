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
// CycloneDX/JSF signature values are base64url (FR-020); the DSSE
// sidecar assertions below stay on BASE64_STD, which its envelope
// format requires. Both alphabets are exercised in this one file.
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
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
// ---------------------------------------------------------------------------
// Milestone 777 — CycloneDX conformance of signed documents (FR-004, FR-008,
// FR-017, SC-001). Validates against the vendored upstream CDX 1.6 schema with
// the JSF reference resolved for real (see common::cdx_schema).
// ---------------------------------------------------------------------------

/// Verifying that this gate has teeth (milestone 777 SC-003).
///
/// A conformance gate that cannot fail is worse than none, and this
/// one was in exactly that state before m777: the CDX schema's JSON
/// Signature Format reference was stubbed with `{}`, so every
/// signature object validated. Both defects were re-introduced
/// deliberately and confirmed to fail this test:
///
/// 1. **Placement.** In `sign_cdx_document_in_place`, insert the
///    placeholder into `metadata` instead of the document root.
///    Result: `/metadata: Additional properties are not allowed
///    ('signature' was unexpected)`.
///
/// 2. **Public-key shape.** Add `#[serde(rename = "ktyMUTATED")]` to
///    `JsfPublicKey::kty`, so the emitted key lacks `kty`. Result:
///    `/signature: ... is not valid under any of the schemas listed in
///    the 'oneOf' keyword`.
///
/// Mutation 2 is the one that matters most: under the pre-m777 stub it
/// passed silently. If it ever stops failing, the JSF reference has
/// been re-stubbed and this gate is blind again — check
/// `common::cdx_schema::CdxRefRetriever`.
#[test]
fn m777_signed_cdx_validates_against_schema() {
    let (pem_file, _public_pem) = ephemeral_keypair();
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");

    let sign_flag = format!("--sign-key={}", pem_file.path().display());
    let out = run_scan(&scan_target(), &output, &[&sign_flag]);
    assert!(
        out.status.success(),
        "signed scan failed (FR-017: the static-key path must not be refused): stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read(&output).expect("read signed cdx");
    let doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");

    let errors = common::cdx_schema::cdx_validation_errors(&doc);
    assert!(
        errors.is_empty(),
        "signed CDX document must validate against the CycloneDX 1.6 schema \
         (FR-004); {} error(s):\n  {}",
        errors.len(),
        errors.join("\n  ")
    );
}

#[test]
fn m777_non_p256_key_is_refused_with_named_type() {
    // Milestone 777 FR-007 / SC-009. Before this, the algorithm was
    // hardcoded to ES256 regardless of the key supplied, so a non-P-256
    // key could not be reported honestly. JSF requires a key-type-
    // specific public-key representation, so emitting one that does not
    // match the key in use would misdescribe the signature.
    let scheme = SigningScheme::ED25519;
    let signer = scheme.create_signer().expect("ed25519 signer");
    let keypair: SigStoreKeyPair = signer.to_sigstore_keypair().expect("keypair");
    let private_pem = keypair.private_key_to_pem().expect("private pem");
    let key_file = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(key_file.path(), private_pem).expect("write ed25519 key");

    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let sign_flag = format!("--sign-key={}", key_file.path().display());
    let out = run_scan(&scan_target(), &output, &[&sign_flag]);

    assert!(
        !out.status.success(),
        "an Ed25519 key MUST be refused rather than signed as if it were P-256"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Ed25519"),
        "the diagnostic MUST name the unsupported key type; got: {stderr}"
    );
    assert!(
        !output.exists(),
        "a refused signing attempt MUST leave no output file (SC-009)"
    );
}

/// Milestone 809 — shared scaffolding for the identity-gated keyless
/// CycloneDX tests.
///
/// These run only with `WAYBILL_TEST_KEYLESS=1` AND a `SIGSTORE_ID_TOKEN`
/// in the environment. They target Sigstore **staging** (sigstage), not
/// production: signing throwaway test artifacts into the production
/// transparency log would be permanent and public. The token must
/// therefore come from the staging issuer —
/// `sigstore --staging get-identity-token`.
fn keyless_gate(test_name: &str) -> bool {
    if std::env::var("WAYBILL_TEST_KEYLESS").is_err() {
        eprintln!("INFO: {test_name} skipped (WAYBILL_TEST_KEYLESS unset)");
        return false;
    }
    // m779 T030 — either credential satisfies the gate. On a runner with
    // `id-token: write` there is an ambient one and no token needs
    // fetching; elsewhere SIGSTORE_ID_TOKEN still works.
    let explicit = std::env::var("SIGSTORE_ID_TOKEN").is_ok();
    let ambient = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").is_ok()
        && std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").is_ok();
    if !explicit && !ambient {
        eprintln!(
            "INFO: {test_name} skipped (no credential: neither SIGSTORE_ID_TOKEN \
             nor an ambient runner token). This reports as `ignored`, never `ok` \
             — a skipped keyless test must not read as coverage."
        );
        return false;
    }
    true
}

/// m779 T029 (FR-012) — the identity and issuer to verify against,
/// derived from the environment rather than written into the source.
///
/// The explicit env vars win when set. Otherwise, on a GitHub Actions
/// runner, both are derivable: the certificate subject is the workflow
/// reference the runner already publishes, and the issuer is GitHub's.
/// Off a runner with no override there is nothing to derive, and the
/// caller skips rather than asserting against a guess.
///
/// The previous version required `WAYBILL_TEST_CERT_IDENTITY` to be set
/// by hand, which in practice meant a personal address baked into a
/// developer's shell.
fn expected_cert_identity() -> Option<(String, String)> {
    if let (Ok(id), Ok(iss)) = (
        std::env::var("WAYBILL_TEST_CERT_IDENTITY"),
        std::env::var("WAYBILL_TEST_CERT_OIDC_ISSUER"),
    ) {
        return Some((id, iss));
    }
    // GitHub Actions. Fulcio does NOT put the raw `sub` claim
    // (`repo:owner/repo:ref:...`) in the SAN — it records the workflow
    // as a URI:
    //
    //   https://github.com/<owner>/<repo>/.github/workflows/<file>@<ref>
    //
    // Observed from the first real conformance run: reconstructing the
    // `sub` shape here produced a verification failure that looked like
    // a bad signature and was not one. `GITHUB_WORKFLOW_REF` already
    // carries exactly the path-and-ref portion, so prefix it rather
    // than assembling the pieces by hand.
    std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").ok()?;
    let workflow_ref = std::env::var("GITHUB_WORKFLOW_REF").ok()?;
    Some((
        format!("https://github.com/{workflow_ref}"),
        "https://token.actions.githubusercontent.com".to_string(),
    ))
}

/// m779 T031 (FR-014 / SC-007) — say whether a failure looks like the
/// external service being down or like waybill being wrong.
///
/// These call for opposite responses: an outage means wait, a defect
/// means investigate. Without a verdict in the output someone has to
/// read the log to tell them apart, and in a scheduled run that someone
/// usually does not exist.
fn classify_failure(stderr: &str) -> String {
    const OUTAGE_MARKERS: &[&str] = &[
        "connection refused",
        "dns error",
        "timed out",
        "timeout",
        "could not resolve",
        "network is unreachable",
        "503",
        "502",
        "504",
        "connection reset",
        "tls handshake",
    ];
    let lower = stderr.to_lowercase();
    if let Some(marker) = OUTAGE_MARKERS.iter().find(|m| lower.contains(**m)) {
        format!(
            "FAILURE CLASS: external-outage (matched {marker:?}). Sigstore \
             staging looks unreachable — this is very likely not a waybill \
             defect. Re-run before investigating."
        )
    } else {
        "FAILURE CLASS: waybill-defect. Nothing in the output looks like a \
         network or service failure, so treat this as a real regression."
            .to_string()
    }
}

/// Run a keyless scan against sigstage, returning the process output.
fn run_keyless_scan(output: &std::path::Path, extra: &[&str]) -> std::process::Output {
    let mut args: Vec<String> = vec![
        "--output".to_string(),
        output.display().to_string(),
    ];
    args.extend(extra.iter().map(|a| (*a).to_string()));
    run_keyless_scan_multi(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Same scan, but the caller supplies every argument including
/// `--output`. Needed for multi-format runs, where the CLI rejects a
/// bare `--output` and each format must be given as `fmt=path`.
fn run_keyless_scan_multi(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--no-deep-hash")
        .arg("--sign");
    for a in args {
        cmd.arg(a);
    }
    cmd.env(
        "WAYBILL_FULCIO_URL",
        std::env::var("WAYBILL_FULCIO_URL")
            .unwrap_or_else(|_| "https://fulcio.sigstage.dev".to_string()),
    )
    .env(
        "WAYBILL_REKOR_URL",
        std::env::var("WAYBILL_REKOR_URL")
            .unwrap_or_else(|_| "https://rekor.sigstage.dev".to_string()),
    );
    cmd.output().expect("waybill invocation")
}

/// Remove ANSI escape sequences so field assertions match the text a
/// human reads rather than the bytes `tracing` emits.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Path to the sidecar waybill writes beside `output`.
fn sidecar_path_for(output: &std::path::Path) -> std::path::PathBuf {
    let mut name = output.file_name().unwrap().to_os_string();
    name.push(".sig.bundle.json");
    output.with_file_name(name)
}

/// Cryptographically verify a bundle against the document it covers,
/// using sigstore-python rather than anything waybill ships — FR-004
/// requires the artifact be verifiable with standard tooling, so the
/// test must not reach for waybill's own primitives.
///
/// Returns `None` when the verifier is unavailable, so the crypto
/// assertions can be skipped explicitly rather than silently passing.
///
/// # The certificate issuer is NOT the token issuer
///
/// `--cert-oidc-issuer` must be the issuer recorded in the Fulcio
/// certificate, which is the **upstream** identity provider — not the
/// endpoint that minted the token. Sigstore staging's dex federates to
/// Google, so a token whose `iss` claim reads
/// `https://oauth2.sigstage.dev/auth` yields a certificate whose issuer
/// is `https://accounts.google.com`.
///
/// Deriving the expected value from the token claim produces:
///
/// ```text
/// Certificate's OIDCIssuer does not match
///   (got 'https://accounts.google.com',
///    expected 'https://oauth2.sigstage.dev/auth')
/// ```
///
/// which looks like a broken signature and is not one. `WAYBILL_TEST_CERT_OIDC_ISSUER`
/// must therefore carry the upstream issuer. Whoever runs these tests
/// with a different provider must supply the matching value.
fn sigstore_verify(doc: &std::path::Path, bundle: &std::path::Path) -> Option<bool> {
    let verifier = std::env::var("WAYBILL_SIGSTORE_BIN").ok()?;
    let (identity, issuer) = expected_cert_identity()?;
    let out = Command::new(&verifier)
        .arg("--staging")
        .arg("verify")
        .arg("identity")
        .arg("--bundle")
        .arg(bundle)
        .arg("--cert-identity")
        .arg(&identity)
        .arg("--cert-oidc-issuer")
        .arg(&issuer)
        .arg(doc)
        .output()
        .expect("sigstore verify invocation");
    if !out.status.success() {
        // Surface the verifier's own diagnostic. Without this a failure
        // is indistinguishable between "waybill produced a bad
        // signature" and "the harness invoked the verifier wrongly" —
        // and those call for opposite responses.
        eprintln!("{}", classify_failure(&String::from_utf8_lossy(&out.stderr)));
        eprintln!(
            "sigstore verify FAILED\n  doc:    {}\n  bundle: {}\n  \
             identity: {identity}\n  issuer: {issuer}\n  stdout: {}\n  stderr: {}",
            doc.display(),
            bundle.display(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim(),
        );
    }
    Some(out.status.success())
}

/// m779 T014 (FR-001) — a workload identity, one with no `email` claim,
/// signs and produces both artifacts.
///
/// Runs against whatever credential the environment supplies: a runner's
/// ambient token where one exists, otherwise `SIGSTORE_ID_TOKEN`. The
/// assertion that matters is the identity *shape* in the log, not the
/// subject value — hardcoding a subject would re-create the very problem
/// m779 FR-012 removes.
#[test]
#[ignore = "identity-gated (m779): needs WAYBILL_TEST_KEYLESS=1 plus either an ambient runner credential or a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn m779_us1_workload_identity_signs() {
    if !keyless_gate("m779_us1") {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = run_keyless_scan(&output, &["--format", "cyclonedx-json"]);
    assert!(
        out.status.success(),
        "keyless sign failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists(), "signed document was not written");
    let sidecar = sidecar_path_for(&output);
    assert!(sidecar.exists(), "sidecar was not written: {sidecar:?}");

    let stderr = strip_ansi(&String::from_utf8_lossy(&out.stderr));
    // Whichever shape the environment produced, it must be classified —
    // an unclassified identity means the record cannot tell an operator
    // what kind of thing signed.
    assert!(
        ["email", "workload", "unrecognized"]
            .iter()
            .any(|k| stderr.contains(&format!("fulcio_cert_identity_shape={k}"))),
        "no identity shape recorded:\n{stderr}"
    );
    // FR-009a: the operator must be handed a runnable command.
    assert!(
        stderr.contains("To verify:") || stderr.contains("To verify (template"),
        "no verification command emitted:\n{stderr}"
    );
}

#[test]
#[ignore = "identity-gated (m809): needs WAYBILL_TEST_KEYLESS=1 + a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn m809_t015_keyless_writes_document_and_sidecar_and_validates() {
    // FR-002, FR-003, SC-001.
    if !keyless_gate("m809_t015") { return; }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = run_keyless_scan(&output, &["--format", "cyclonedx-json"]);
    assert!(
        out.status.success(),
        "keyless CycloneDX sign failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let sidecar = sidecar_path_for(&output);
    assert!(output.exists(), "document missing at {}", output.display());
    assert!(sidecar.exists(), "companion artifact missing at {}", sidecar.display());

    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).expect("parse cdx");
    assert!(
        doc.pointer("/signature").is_none() && doc.pointer("/metadata/signature").is_none(),
        "keyless output must carry NO in-document signature (FR-003)"
    );
    let errors = common::cdx_schema::cdx_validation_errors(&doc);
    assert!(
        errors.is_empty(),
        "keyless-signed document must validate against CycloneDX 1.6; {} error(s):\n  {}",
        errors.len(),
        errors.join("\n  ")
    );

    // The bundle must carry the transparency-log material that motivated
    // the sidecar shape in the first place — without it the whole
    // rationale for not embedding collapses.
    let bundle = std::fs::read_to_string(&sidecar).expect("read bundle");
    assert!(
        bundle.contains("tlogEntries") || bundle.contains("verificationMaterial"),
        "bundle missing Rekor / verification material"
    );
}

#[test]
#[ignore = "identity-gated (m809): needs WAYBILL_TEST_KEYLESS=1 + a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn m809_t024_signature_reference_is_inside_the_signed_document() {
    // FR-012/FR-013, SC-009. The reference must be present AND covered by
    // the signature — injected before signing, not after.
    if !keyless_gate("m809_t024") { return; }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = run_keyless_scan(&output, &["--format", "cyclonedx-json"]);
    assert!(out.status.success(), "keyless sign failed");

    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).expect("parse cdx");
    let refs = doc["externalReferences"].as_array().expect("externalReferences present");
    let att: Vec<_> = refs.iter().filter(|r| r["type"] == "attestation").collect();
    assert_eq!(att.len(), 1, "expected exactly one attestation reference, got {}", att.len());

    let url = att[0]["url"].as_str().expect("reference url");
    assert!(
        !url.contains('/') && !url.contains('\\'),
        "reference must be a bare filename with no directory component; got {url}"
    );
    assert_eq!(
        url,
        sidecar_path_for(&output).file_name().unwrap().to_string_lossy(),
        "reference must name the artifact actually written"
    );

    // Resolve it the way a consumer would: relative to the document.
    assert!(
        output.parent().unwrap().join(url).exists(),
        "reference does not resolve to an existing file (SC-009)"
    );

    match sigstore_verify(&output, &sidecar_path_for(&output)) {
        Some(true) => {}
        Some(false) => panic!(
            "signature did not verify on a document carrying the reference — \
             injection is happening AFTER signing (FR-013)"
        ),
        None => eprintln!(
            "INFO: m809_t024 crypto check skipped (set WAYBILL_SIGSTORE_BIN, \
             WAYBILL_TEST_CERT_IDENTITY, WAYBILL_TEST_CERT_OIDC_ISSUER)"
        ),
    }
}

#[test]
#[ignore = "identity-gated (m809): needs WAYBILL_TEST_KEYLESS=1 + a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn m809_t016_signature_verifies_and_tamper_is_detected() {
    // FR-004, FR-005, SC-002, SC-003. The two claims m778 shipped unproven.
    if !keyless_gate("m809_t016") { return; }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = run_keyless_scan(&output, &["--format", "cyclonedx-json"]);
    assert!(out.status.success(), "keyless sign failed");
    let sidecar = sidecar_path_for(&output);

    match sigstore_verify(&output, &sidecar) {
        None => {
            eprintln!(
                "INFO: m809_t016 skipped — no verifier configured. FR-004/FR-005 \
                 remain UNPROVEN; this test passing means nothing without \
                 WAYBILL_SIGSTORE_BIN + WAYBILL_TEST_CERT_IDENTITY + \
                 WAYBILL_TEST_CERT_OIDC_ISSUER."
            );
            return;
        }
        Some(false) => panic!("signature failed to verify against the unmodified document (FR-004)"),
        Some(true) => {}
    }

    // Tamper: append a component. Any content change must break it.
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).expect("parse cdx");
    doc["components"]
        .as_array_mut()
        .expect("components array")
        .push(serde_json::json!({"name": "post-sign-tampered", "type": "library"}));
    std::fs::write(&output, serde_json::to_vec_pretty(&doc).unwrap()).expect("write tampered");

    assert_eq!(
        sigstore_verify(&output, &sidecar),
        Some(false),
        "verification MUST fail after the document is altered (FR-005, SC-003)"
    );
}

#[test]
#[ignore = "identity-gated (m809): needs WAYBILL_TEST_KEYLESS=1 + a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn m809_t017_two_formats_produce_two_correctly_associated_artifacts() {
    // FR-007, SC-008 — the gap /speckit.analyze caught: no task covered
    // the multi-format case, and T013 only looked like it did.
    if !keyless_gate("m809_t017") { return; }
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx = tmp.path().join("out.cdx.json");
    let spdx = tmp.path().join("out.spdx.json");
    // `run_keyless_scan` appends a bare `--output <path>`, which the CLI
    // rejects alongside multiple `--format` values. Both outputs must be
    // given in `fmt=path` form. The test has carried this defect since
    // m809 and never surfaced it, because it was `#[ignore]`d and had
    // never actually run.
    let out = run_keyless_scan_multi(&[
        "--format",
        "cyclonedx-json,spdx-2.3-json",
        "--output",
        &format!("cyclonedx-json={}", cdx.display()),
        "--output",
        &format!("spdx-2.3-json={}", spdx.display()),
    ]);
    assert!(
        out.status.success(),
        "two-format keyless sign failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cdx_sig = sidecar_path_for(&cdx);
    let spdx_sig = sidecar_path_for(&spdx);
    assert!(cdx_sig.exists(), "CycloneDX artifact missing");
    assert!(spdx_sig.exists(), "SPDX artifact missing");
    assert_ne!(
        std::fs::read(&cdx_sig).unwrap(),
        std::fs::read(&spdx_sig).unwrap(),
        "the two artifacts are byte-identical — they cannot both be correct"
    );

    if let (Some(a), Some(b)) = (sigstore_verify(&cdx, &cdx_sig), sigstore_verify(&spdx, &spdx_sig)) {
        assert!(a && b, "each artifact must verify against its own document");
        // Cross-check: an artifact must NOT verify the other's document.
        assert_eq!(
            sigstore_verify(&cdx, &spdx_sig),
            Some(false),
            "the SPDX artifact verified the CycloneDX document — mis-association (FR-007)"
        );
    } else {
        eprintln!("INFO: m809_t017 cross-association check skipped (no verifier configured)");
    }
}

#[test]
fn m778_keyless_with_cyclonedx_reaches_signing_not_refusal() {
    // Milestone 778 (FR-001) replaced m777's argument-time refusal of
    // keyless + CycloneDX. The combination is now routed to the detached
    // sidecar path, exactly as SPDX is.
    //
    // Without an OIDC identity the run still fails — but it must fail at
    // *signing*, not at argument validation, and the diagnostic must name
    // the sidecar path. That distinction is what this test pins, and it
    // is checkable with no identity available.
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("keyless.cdx.json");
    let out = run_scan(&scan_target(), &output, &["--sign"]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("cannot currently produce a conformant"),
        "the m777 argument-time refusal MUST be gone (FR-001); got: {stderr}"
    );
    assert!(
        stderr.contains("sidecar"),
        "keyless CycloneDX MUST route to the sidecar path, not the in-document \
         signer — the diagnostic should name it; got: {stderr}"
    );
    assert!(
        !output.exists(),
        "a failed signing attempt MUST leave no document behind (FR-008, SC-004)"
    );
}

#[test]
fn m778_signature_reference_shape_is_schema_valid() {
    // Milestone 778 US2 acceptance scenario 3 / FR-003: a document
    // carrying the signature reference must still validate against the
    // CycloneDX 1.6 schema.
    //
    // Scope note: this validates the *shape* of the reference against a
    // real emitted document. Verifying that the keyless path actually
    // emits it end-to-end requires a signing identity and lives in the
    // WAYBILL_TEST_KEYLESS-gated tests. Splitting it this way means the
    // schema question — the one with a real chance of being wrong, since
    // a document-level `attestation` reference is new here — is answered
    // now rather than deferred behind credentials.
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("plain.cdx.json");
    let out = run_scan(&scan_target(), &output, &[]);
    assert!(out.status.success(), "unsigned scan must succeed");

    let raw = std::fs::read(&output).expect("read cdx");
    let mut doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");

    doc.as_object_mut()
        .expect("document root is an object")
        .entry("externalReferences")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .expect("externalReferences is an array")
        .push(serde_json::json!({
            "type": "attestation",
            "url": "plain.cdx.json.sig.bundle.json",
            "comment": "Detached signature for this document.",
        }));

    let errors = common::cdx_schema::cdx_validation_errors(&doc);
    assert!(
        errors.is_empty(),
        "a document carrying the signature reference must validate against \
         CycloneDX 1.6; {} error(s):\n  {}",
        errors.len(),
        errors.join("\n  ")
    );
}

#[test]
fn m778_static_key_cyclonedx_is_untouched() {
    // Milestone 778 FR-009 / SC-006: the static-key path keeps its
    // in-document signature and gains nothing from this feature — no
    // companion artifact, no signature reference.
    let (pem_file, _public_pem) = ephemeral_keypair();
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");

    let sign_flag = format!("--sign-key={}", pem_file.path().display());
    let out = run_scan(&scan_target(), &output, &[&sign_flag]);
    assert!(out.status.success(), "static-key signing must still succeed");

    let raw = std::fs::read(&output).expect("read signed cdx");
    let doc: serde_json::Value = serde_json::from_slice(&raw).expect("parse cdx");
    assert!(
        doc.pointer("/signature").is_some(),
        "static-key CycloneDX keeps its in-document signature (FR-009)"
    );

    let sidecar = output.with_extension("json.sig.bundle.json");
    assert!(
        !sidecar.exists(),
        "static-key CycloneDX MUST NOT emit a companion artifact (FR-009)"
    );

    let doc_refs = doc
        .get("externalReferences")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        doc_refs, 0,
        "static-key CycloneDX MUST NOT carry a signature reference (FR-009, SC-006)"
    );
}

#[test]
fn m777_keyless_with_spdx_only_is_not_refused() {
    // Milestone 777 FR-016 / SC-008: the refusal is scoped to
    // CycloneDX. SPDX signing is a detached sidecar with no in-document
    // signature slot and must be unaffected. Without a real OIDC token
    // the keyless sign itself still fails downstream — what this pins
    // is that it fails for that reason and NOT via the m777 refusal.
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("keyless.spdx.json");
    let out = run_scan(
        &scan_target(),
        &output,
        &["--sign", "--format", "spdx-2.3-json"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("cannot currently produce a conformant"),
        "SPDX-only keyless MUST NOT hit the CycloneDX refusal (FR-016); got: {stderr}"
    );
}

// US2a — static-key JSF sign into the CDX root `signature` slot
// (relocated from `metadata.signature` by milestone 777)
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
    assert!(
        doc.pointer("/metadata/signature").is_none(),
        "milestone 777: the signature MUST NOT be under metadata (FR-002)"
    );
    let sig_slot = doc
        .pointer("/signature")
        .expect("root signature slot populated (FR-001)")
        .clone();
    assert_eq!(
        sig_slot["algorithm"], "ES256",
        "signature.algorithm must be ES256 for ECDSA-P256"
    );
    let pk = &sig_slot["publicKey"];
    assert_eq!(pk["kty"], "EC", "JSF requires a JWK public key (FR-003)");
    assert_eq!(pk["crv"], "P-256");
    assert!(
        pk.get("pem").is_none(),
        "JSF's EC branch forbids additional properties such as `pem` (FR-003)"
    );
    let sig_b64 = sig_slot["value"].as_str().expect("signature value string");
    let sig_bytes = BASE64_URL.decode(sig_b64).expect("base64url sig decode");

    // Reset value → recanonicalize → verify against the pubkey we
    // handed waybill.
    let meta = doc
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
        .pointer("/signature/value")
        .and_then(|v| v.as_str())
        .expect("signature value")
        .to_string();
    let sig_bytes = BASE64_URL.decode(&sig_b64).expect("base64url");

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
    let meta = doc.as_object_mut().unwrap();
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
    format: &str,
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
        .arg(format)
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
    // Milestone 777 retargeted this from CycloneDX to SPDX for the same
    // reason as the no-OIDC test above: the fail-close cleanup this
    // pins (FR-009a) is unchanged, but it must be reached through a
    // format where keyless signing still runs.
    let output = tmp.path().join("should-not-persist.spdx.json");
    let out = run_scan_with_sign_env(
        &scan_target(),
        &output,
        "spdx-2.3-json",
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
    // Milestone 777 retargeted this from CycloneDX to SPDX. What it
    // pins — the m222 OIDC fail-close diagnostic — is unchanged and
    // still valuable, but keyless signing of CycloneDX is now refused
    // before the OIDC path is reached (FR-014), so exercising it
    // through CDX would assert the refusal instead. SPDX keyless still
    // runs and still reaches the OIDC failure.
    let output = tmp.path().join("no-token.spdx.json");
    let out = run_scan_with_sign_env(
        &scan_target(),
        &output,
        "spdx-2.3-json",
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
        stderr.contains("sigstore get-identity-token"),
        "stderr missing token-acquisition pointer: {stderr}"
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

// ---------------------------------------------------------------------------
// Milestone 809 — the three m777-superseded keyless tests, resolved.
//
// They asserted that `--sign` embeds a Sigstore bundle at
// `metadata.signature`. That behaviour is gone: m777 refused keyless
// CycloneDX, m778 replaced it with a detached sidecar. Rather than
// mechanically retarget all three into near-duplicates, their coverage was
// consolidated:
//
//   us2b_keyless_bundle_sign_and_verify
//       -> m809_t015_keyless_writes_document_and_sidecar_and_validates
//          (both files written, schema-valid, bundle carries tlog material)
//
//   us2b_keyless_signature_covers_document_mutation_m222
//       -> m809_t016_signature_verifies_and_tamper_is_detected
//          (real cryptographic verify, not the shape check the original did)
//
//   us2b_keyless_fr016_info_log_fields_m222
//       -> retained below, retargeted. Its subject — the FR-016 diagnostic
//          fields — is not covered by anything else.
// ---------------------------------------------------------------------------

/// FR-016 (m222): the keyless path emits `rekor_log_index`,
/// `fulcio_cert_subject`, and `oidc_provider` at INFO. Retargeted by m809
/// from the in-document path to the sidecar path; the fields are emitted by
/// the signing flow itself, so they are independent of where the signature
/// lands.
#[test]
#[ignore = "identity-gated (m809): needs WAYBILL_TEST_KEYLESS=1 + a staging SIGSTORE_ID_TOKEN. Run with `-- --ignored`. Reported as `ignored` rather than `ok` on purpose — a skipped keyless test must not read as coverage."]
fn us2b_keyless_fr016_info_log_fields_m222() {
    if !keyless_gate("us2b_keyless_fr016_info_log_fields_m222") { return; }
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("signed.cdx.json");
    let out = run_keyless_scan(&output, &["--format", "cyclonedx-json"]);
    assert!(
        out.status.success(),
        "keyless sign failed. stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // `tracing` writes ANSI escapes between the field name and its `=`,
    // so a literal "name=" match never hits. The m222 original asserted
    // exactly that and was #[ignore]d, so it never ran to reveal it.
    let raw = String::from_utf8_lossy(&out.stderr);
    let stderr = strip_ansi(&raw);
    // The three m222 field names keep their spelling; m779 adds two.
    for field in [
        "rekor_log_index=",
        "fulcio_cert_subject=",
        "oidc_provider=",
        "fulcio_cert_identity_shape=",
        "fulcio_cert_oidc_issuer=",
    ] {
        assert!(
            stderr.contains(field),
            "m222-FR-016 / m779-FR-005: stderr missing {field} INFO field:\n{stderr}"
        );
    }
    // The shape must be one of the closed set, not an empty or debug
    // rendering — an unconstrained value here would let a formatting
    // regression pass unnoticed.
    assert!(
        ["email", "workload", "unrecognized"]
            .iter()
            .any(|s| stderr.contains(&format!("fulcio_cert_identity_shape={s}"))),
        "m779: identity shape is not one of the closed set:\n{stderr}"
    );
}
