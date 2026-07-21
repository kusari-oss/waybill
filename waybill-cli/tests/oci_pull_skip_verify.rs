//! Milestone 182 US3 — `--insecure-tls-skip-verify` WARN log.
//!
//! **Scope of this file**: verifies the FR-007 WARN log fires when
//! the flag is set, and the flag is orthogonal to `--registry-ca-cert`
//! for parse-order semantics (T025 clarification).
//!
//! Full end-to-end HTTPS validation (hostname-mismatch cert served
//! by an in-test TLS server) is deferred to T035 manual verification
//! — see `oci_pull_custom_ca.rs` for the rationale (avoiding three
//! new dev-deps for a code path already covered by unit tests and
//! `reqwest::ClientBuilder::danger_accept_invalid_certs`'s upstream
//! contract).

#![cfg(feature = "oci-registry")]

use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

#[test]
fn us3_skip_verify_flag_emits_warn_log() {
    // FR-007 / Constitution Principle X: whenever the operator
    // disables TLS verification, waybill MUST emit a WARN log naming
    // the flag and the affected image ref. This audit trail is
    // required by security-review workflows.
    //
    // The log fires at RegistryClient::new — i.e., BEFORE any
    // network activity — so we can observe it even against an
    // unreachable host.
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--insecure-tls-skip-verify",
        ])
        .env("RUST_LOG", "warn")
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // The invocation WILL fail (unreachable host) — but the WARN log
    // must appear before the transport failure.
    assert!(
        stderr.contains("--insecure-tls-skip-verify"),
        "expected WARN log to mention --insecure-tls-skip-verify. stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("TLS verification DISABLED"),
        "expected WARN log to contain 'TLS verification DISABLED'. stderr:\n{stderr}",
    );
}

#[test]
fn us3_no_flag_no_warn_log() {
    // Regression pin: default (no --insecure-tls-skip-verify) MUST
    // NOT emit the disabled-verification WARN. Byte-identity SC-004.
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
        ])
        .env("RUST_LOG", "warn")
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("TLS verification DISABLED"),
        "unexpected WARN log without --insecure-tls-skip-verify. stderr:\n{stderr}",
    );
}

#[test]
fn us3_skip_verify_and_bad_ca_cert_both_set() {
    // FR-008 clarification (T025 in tasks.md): what happens when
    // both --insecure-tls-skip-verify AND --registry-ca-cert are set
    // and the CA path is bad?
    //
    // Spec-driven decision: `RegistryTlsConfig::from_args` validates
    // the CA path unconditionally (fail-fast per FR-014). Skip-verify
    // does NOT bypass the parse-time validation of other flags — it
    // only affects the runtime TLS decision. Two orthogonal knobs.
    //
    // Rationale: silently skipping CA validation when skip-verify is
    // set would hide typos from the operator. Better to name the bad
    // path than to succeed with unrelated flags.
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--insecure-tls-skip-verify",
            "--registry-ca-cert",
            "/nonexistent/waybill-m182-us3.pem",
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "expected waybill to fail on bad CA path even with skip-verify set. \
         stderr:\n{stderr}",
    );
    assert!(
        stderr.contains("/nonexistent/waybill-m182-us3.pem"),
        "expected stderr to name the bad path. stderr:\n{stderr}",
    );
}
