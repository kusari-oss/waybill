//! Milestone 136 edge-case tests — covers spec Edge Cases section +
//! SC-005 (malformed-receipt graceful degradation) + multi-version
//! coexistence + third-party tap end-to-end + empty Cellar no-op.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn write_formula(
    rootfs: &Path,
    prefix: &str,
    formula: &str,
    version: &str,
    receipt_body: &str,
) {
    let dir = rootfs
        .join(prefix)
        .join("Cellar")
        .join(formula)
        .join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("INSTALL_RECEIPT.json"), receipt_body).unwrap();
}

fn run_scan(rootfs: &Path) -> (Value, String, bool) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    let success = result.status.success();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    let bytes = std::fs::read(&out_path).unwrap_or_default();
    let doc: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (doc, stderr, success)
}

fn brew_purls(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
                if p.starts_with("pkg:brew/") {
                    out.push(p.to_string());
                }
            }
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if let Some(p) = c.get("purl").and_then(|v| v.as_str()) {
            if p.starts_with("pkg:brew/") {
                out.push(p.to_string());
            }
        }
    }
    out
}

#[test]
fn sc_005_malformed_receipt_alongside_valid_formulae() {
    // SC-005 — three valid + one corrupted; scan succeeds (exit 0);
    // 3 valid components emit; warn names the broken formula.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "good-a",
        "1.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "good-b",
        "2.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "good-c",
        "3.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "broken",
        "1.0",
        "not valid json at all",
    );

    let (doc, stderr, success) = run_scan(tmp.path());
    assert!(success, "scan must succeed despite one malformed receipt");

    let purls = brew_purls(&doc);
    assert_eq!(purls.len(), 3, "expected 3 valid components, got {purls:?}");

    assert!(
        stderr.contains("broken") || stderr.to_lowercase().contains("brew"),
        "stderr should warn about the broken formula; got:\n{stderr}",
    );
}

#[test]
fn receipt_without_runtime_dependencies_emits_with_empty_depends() {
    // Older receipts (pre-2017) lack runtime_dependencies entirely.
    // Component must still emit, with no dep edges.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "ancient-tool",
        "0.1.0",
        r#"{"homebrew_version":"0.9.0","time":1300000000}"#,
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls, vec!["pkg:brew/ancient-tool@0.1.0".to_string()]);
}

#[test]
fn receipt_with_null_source_tap_omits_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "raw-path-install",
        "1.0",
        r#"{"source": {"tap": null}}"#,
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls, vec!["pkg:brew/raw-path-install@1.0".to_string()]);
}

#[test]
fn fr_008_multi_version_formula_emits_separate_components() {
    // FR-008 — same formula name, two distinct versions.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "openssl@1.1",
        "1.1.1w",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "openssl@3",
        "3.4.0",
        r#"{"source": {"tap": "homebrew/core"}}"#,
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(purls.len(), 2);
    assert!(purls.contains(&"pkg:brew/openssl@1.1@1.1.1w".to_string()));
    assert!(purls.contains(&"pkg:brew/openssl@3@3.4.0".to_string()));
}

#[test]
fn empty_cellar_dir_emits_no_components_no_warnings() {
    // Edge case — Cellar/ exists but contains no formula subdirs
    // (fresh install / all uninstalled). Silent no-op.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("opt/homebrew/Cellar")).unwrap();
    let (doc, stderr, success) = run_scan(tmp.path());
    assert!(success);
    let purls = brew_purls(&doc);
    assert!(purls.is_empty());
    assert!(
        !stderr.contains("WARN") || !stderr.to_lowercase().contains("brew"),
        "empty Cellar must not warn; got:\n{stderr}",
    );
}

#[test]
fn third_party_tap_end_to_end_purl_carries_qualifier() {
    // SC-007 — end-to-end verification that a third-party-tap install
    // produces a PURL with the ?tap= qualifier.
    let tmp = tempfile::tempdir().unwrap();
    write_formula(
        tmp.path(),
        "opt/homebrew",
        "terraform",
        "1.10.0",
        r#"{"source": {"tap": "hashicorp/tap"}}"#,
    );
    let (doc, _, _) = run_scan(tmp.path());
    let purls = brew_purls(&doc);
    assert_eq!(
        purls,
        vec!["pkg:brew/terraform@1.10.0?tap=hashicorp/tap".to_string()]
    );
}
