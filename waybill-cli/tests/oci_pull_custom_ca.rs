//! Milestone 182 US2 — private-CA HTTPS OCI registry pull.
//!
//! **Scope of this file**: fail-fast validation tests (bad path,
//! missing file) — these don't require an HTTPS server and exercise
//! the `RegistryTlsConfig::from_args` → `load_ca_bundle_from_paths`
//! path at scan startup per FR-014.
//!
//! Full end-to-end HTTPS happy-path validation (T019, T022) is
//! deferred to T035 manual verification with a real private-CA
//! registry — building an in-test HTTPS server via `tokio-rustls` +
//! `hyper` would add three dev-deps (`tokio-rustls`, `hyper`, and
//! `hyper-util`) for a code path already covered by the
//! `tls_config::tests::load_ca_bundle_multi_cert_bundle_loads_all`
//! unit test and the `reqwest::ClientBuilder::add_root_certificate`
//! upstream contract.

#![cfg(feature = "oci-registry")]

use std::io::Write;
use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

#[test]
fn us2_bad_ca_cert_path_actionable_error() {
    // FR-014 fail-fast semantics: bad --registry-ca-cert path is
    // caught at scan startup, before any network call. Error message
    // must name the flag AND the offending path so the operator can
    // spot the typo without re-reading the invocation.
    let bad_path = "/nonexistent/mikebom-m182-test/ca-bundle.pem";
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "example.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--registry-ca-cert",
            bad_path,
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected mikebom to fail on bad --registry-ca-cert path. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains(bad_path),
        "expected stderr to name the bad path. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("--registry-ca-cert"),
        "expected stderr to name --registry-ca-cert. stderr:\n{stderr}",
    );
}

#[test]
fn us2_empty_pem_file_actionable_error() {
    // FR-014 sub-case: file exists but contains no PEM cert blocks.
    // Message must name the file and hint at the expected shape.
    let mut tf = tempfile::NamedTempFile::new().unwrap();
    writeln!(tf, "# empty file — no PEM cert blocks").unwrap();
    tf.flush().unwrap();
    let path = tf.path().to_path_buf();

    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "example.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--registry-ca-cert",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected mikebom to fail on empty PEM file. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains(path.to_str().unwrap()),
        "expected stderr to name the file. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("--registry-ca-cert"),
        "expected stderr to name the flag. stderr:\n{stderr}",
    );
}

#[test]
fn us2_valid_ca_bundle_passes_parse_stage() {
    // Regression pin: a well-formed PEM bundle (from rcgen) must NOT
    // trigger the FR-014 Case 4 parse error. This test does NOT
    // attempt a network pull — it just verifies mikebom loads the
    // bundle and reaches a *transport* failure (not a parse failure)
    // against a nonexistent host. Complements the
    // load_ca_bundle_multi_cert_bundle_loads_all unit test.
    let mut tf = tempfile::NamedTempFile::new().unwrap();
    let cert1 = rcgen::generate_simple_self_signed(vec!["ca1.test.invalid".to_string()]).unwrap();
    let cert2 = rcgen::generate_simple_self_signed(vec!["ca2.test.invalid".to_string()]).unwrap();
    tf.write_all(cert1.cert.pem().as_bytes()).unwrap();
    tf.write_all(cert2.cert.pem().as_bytes()).unwrap();
    tf.flush().unwrap();

    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--registry-ca-cert",
            tf.path().to_str().unwrap(),
        ])
        .output()
        .expect("spawn mikebom binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // We expect FAILURE (network unreachable), but the failure MUST
    // NOT mention "--registry-ca-cert" as the cause — that would
    // indicate the parse stage rejected the valid bundle.
    assert!(
        !out.status.success(),
        "expected mikebom to fail against unreachable host. stderr:\n{stderr}",
    );
    assert!(
        !stderr.contains("no PEM certificates found"),
        "valid rcgen-generated PEM bundle was mis-classified as empty. stderr:\n{stderr}",
    );
    assert!(
        !stderr.contains("parsing --registry-ca-cert"),
        "valid rcgen-generated PEM bundle triggered a parse error. stderr:\n{stderr}",
    );
}
