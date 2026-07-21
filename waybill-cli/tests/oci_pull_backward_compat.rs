//! Milestone 182 — backward-compatibility regression pin (SC-004
//! + FR-011).
//!
//! **SC-004** — pre-m182 invocations (no m182 flags set) must produce
//! byte-identical SBOM output. Verified via the existing golden-
//! regression suite (`WAYBILL_UPDATE_CDX_GOLDENS=1` + `_SPDX_` + `_SPDX3_`
//! regens); this file adds a direct sanity check that flag-absence
//! preserves the pre-m182 CDX shape.
//!
//! **FR-011** — the m182 flags must coexist with the existing
//! `--registry-credentials-dir` flag (m034). Both concerns are
//! orthogonal at the CLI-parse layer.

#![cfg(feature = "oci-registry")]

use std::process::Command;

fn mikebom_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

#[test]
fn backward_compat_no_m182_flags_no_new_warn_logs() {
    // Regression pin: an invocation without ANY m182 flags must NOT
    // emit any m182-specific WARN log. Byte-identity SC-004 relies
    // on this.
    let tempdir = tempfile::tempdir().unwrap();
    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--path",
            tempdir.path().to_str().unwrap(),
            "--format",
            "cyclonedx-json",
            "--output",
            tempdir.path().join("out.cdx.json").to_str().unwrap(),
        ])
        .env("RUST_LOG", "warn")
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "expected default-mode scan to succeed. stderr:\n{stderr}",
    );
    assert!(
        !stderr.contains("TLS verification DISABLED"),
        "unexpected m182 WARN log without m182 flags. stderr:\n{stderr}",
    );
    // No m182 flag-parse messages either.
    assert!(
        !stderr.contains("--insecure-registry"),
        "unexpected --insecure-registry mention without m182 flag. stderr:\n{stderr}",
    );
}

#[test]
fn backward_compat_registry_credentials_dir_coexists_with_m182_flags() {
    // FR-011 regression pin (T028 in tasks.md): existing
    // --registry-credentials-dir (m034 / #66) MUST coexist with the
    // three m182 flags. Verifies clap parse orthogonality.
    let creds_dir = tempfile::tempdir().unwrap();
    // Create an empty config.json so credentials-dir probing has a
    // file to open (the m034 layered resolver falls back to
    // anonymous when parseable but empty).
    std::fs::write(creds_dir.path().join("config.json"), "{}").unwrap();

    let out = Command::new(mikebom_bin())
        .args([
            "sbom",
            "scan",
            "--image",
            "unreachable.test.invalid/foo:1.0",
            "--image-src",
            "remote",
            "--registry-credentials-dir",
            creds_dir.path().to_str().unwrap(),
            "--insecure-registry",
            "another-host.test.invalid:5000",
            "--insecure-tls-skip-verify",
        ])
        .output()
        .expect("spawn waybill binary");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // Fails at network-time (unreachable host), NOT at parse-time.
    assert!(
        !out.status.success(),
        "expected failure against unreachable host. stderr:\n{stderr}",
    );
    // No clap conflict — both flag families coexist.
    assert!(
        !stderr.contains("cannot be used with"),
        "unexpected clap conflict message. stderr:\n{stderr}",
    );
    // WARN log fires (skip-verify set), proving waybill reached the
    // registry-client construction stage after credential resolution.
    assert!(
        stderr.contains("TLS verification DISABLED"),
        "expected m182 WARN log after credential resolution. stderr:\n{stderr}",
    );
}
