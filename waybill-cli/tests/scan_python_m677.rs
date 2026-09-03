//! Feature 677 (issue #768) — integration test for PEP 508 name-validation
//! at the pip reader's `read()` filter pass.
//!
//! The fixture `pip/malformed_name_placeholder/pyproject.toml` has
//! `name = "{{package-name}}"` (Cookiecutter Jinja placeholder) AND
//! `dependencies = [...]` containing valid PEP 508 names. Per Session
//! 2026-09-03 Q1 whole-manifest-reject, scanning it MUST emit zero
//! `pkg:pypi/*` components — the malformed name causes the entire
//! manifest (main-module + declared deps) to be dropped.
//!
//! Also verifies the operator-facing signals: exactly one WARN log line
//! naming the offending path + name, and `names_rejected=1` in the
//! reader-complete log.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pip/malformed_name_placeholder")
}

/// Strip ANSI color escapes so `stderr.contains(...)` works against
/// `tracing`'s ANSI-colored key-value formatter. Same pattern as
/// `scan_pants_m672.rs`.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() && !bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[test]
fn malformed_name_pyproject_emits_zero_components_with_warn() {
    let fixture = fixture_path();
    let tmp = tempfile::tempdir().unwrap();
    let out_path = tmp.path().join("out.cdx.json");

    let output = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&fixture)
        .arg("--no-deep-hash")
        .arg("--output")
        .arg(&out_path)
        .env("RUST_LOG", "info")
        .output()
        .expect("waybill invocation");

    let stderr_raw = String::from_utf8_lossy(&output.stderr);
    let stderr = strip_ansi(&stderr_raw);

    assert!(
        output.status.success(),
        "scan failed: stderr={stderr}"
    );

    // ---- Assertion 1 (SC-001 + Session 2026-09-03 Q1) ----
    // Zero pkg:pypi/* components. Whole-manifest reject drops the
    // main-module component AND the two valid declared deps.
    let bytes = std::fs::read(&out_path).unwrap();
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let pypi_count = doc["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:pypi/"))
        })
        .count();
    assert_eq!(
        pypi_count, 0,
        "expected 0 pkg:pypi/* components (whole-manifest reject), got {pypi_count}. \
         SBOM components: {:?}",
        doc["components"].as_array().unwrap()
    );

    // ---- Assertion 2 (SC-002 + FR-002) ----
    // Exactly one WARN log line naming the offending path + name.
    let warn_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| {
            l.contains("WARN")
                && l.contains("pip: pyproject.toml [project].name failed PEP 508 validation")
        })
        .collect();
    assert_eq!(
        warn_lines.len(),
        1,
        "expected exactly 1 PEP 508 validation WARN line, got {}. All WARN lines:\n{}",
        warn_lines.len(),
        stderr
            .lines()
            .filter(|l| l.contains("WARN"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let warn = warn_lines[0];
    assert!(
        warn.contains("{{package-name}}"),
        "WARN should name the offending placeholder; got: {warn}"
    );
    assert!(
        warn.contains("pyproject.toml"),
        "WARN should name the manifest path; got: {warn}"
    );

    // ---- Assertion 3 (SC-003 + FR-003) ----
    // Reader-complete log reports names_rejected=1.
    assert!(
        stderr.contains("names_rejected=1"),
        "expected 'names_rejected=1' in reader-complete log, stderr:\n{stderr}"
    );

    // ---- Assertion 4 (whole-manifest-reject sanity per FR-007 clause i) ----
    // The valid declared deps in the fixture (waybill-fixture-real-dep-*)
    // MUST NOT appear as separate components — they die with the manifest.
    let real_dep_count = doc["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.contains("waybill-fixture-real-dep"))
        })
        .count();
    assert_eq!(
        real_dep_count, 0,
        "expected 0 waybill-fixture-real-dep-* components (whole-manifest reject drops declared deps), got {real_dep_count}"
    );
}
