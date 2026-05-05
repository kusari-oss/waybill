//! Milestone 072 T018 — `mikebom sbom verify-binding` end-to-end test.
//!
//! Two scenarios:
//!
//!   (a) clean verify — image asserts hash X, source asserts hash X →
//!       exit 0, summary `verified=N, failures=0`.
//!   (b) wrong-source verify — image asserts hash X, source asserts
//!       hash Y → exit non-zero (per FR-005 / VR-005), each row's
//!       `reason` is `verification-failed`.
//!
//! Hermetic: synthesizes both SBOMs as JSON literals in tempdirs.

use std::process::Command;

mod common;
use common::bin;

/// Build a CDX SBOM JSON byte string with one component carrying a
/// `mikebom:source-document-binding` property.
fn cdx_with_binding(purl: &str, asserted_hash: &str) -> String {
    let binding = serde_json::json!({
        "algo": "v1",
        "hash": asserted_hash,
        "source_doc_id": { "sha256": "0".repeat(64) },
        "strength": "verified"
    });
    let binding_str = serde_json::to_string(&binding).unwrap();
    let doc = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "components": [{
            "type": "library",
            "name": "foo",
            "version": "1.0.0",
            "purl": purl,
            "bom-ref": format!("{}-bom", purl),
            "properties": [{
                "name": "mikebom:source-document-binding",
                "value": binding_str,
            }],
        }],
    });
    serde_json::to_string_pretty(&doc).unwrap()
}

/// (a) Clean verify — exit 0, no failures.
#[test]
fn verify_binding_clean_match_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let purl = "pkg:cargo/foo@1.0.0";
    let hash = "deadbeef".repeat(8); // 64 hex chars.

    let image_path = dir.path().join("image.cdx.json");
    let source_path = dir.path().join("source.cdx.json");
    std::fs::write(&image_path, cdx_with_binding(purl, &hash)).unwrap();
    std::fs::write(&source_path, cdx_with_binding(purl, &hash)).unwrap();

    let out = Command::new(bin())
        .arg("sbom")
        .arg("verify-binding")
        .arg("--image-sbom")
        .arg(&image_path)
        .arg("--source-sbom")
        .arg(&source_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("verify-binding runs");

    assert!(
        out.status.success(),
        "expected clean verify to exit 0; stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse JSON report");
    assert_eq!(report["summary"]["verification_failures"], 0);
    assert_eq!(report["summary"]["verified"], 1);
    assert_eq!(report["summary"]["components_checked"], 1);
}

/// (b) Wrong-source verify — exit non-zero, `verification-failed`.
#[test]
fn verify_binding_wrong_source_exits_nonzero_with_failed_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let purl = "pkg:cargo/foo@1.0.0";
    let image_hash = "deadbeef".repeat(8);
    let source_hash = "abcd1234".repeat(8); // different hash.

    let image_path = dir.path().join("image.cdx.json");
    let source_path = dir.path().join("source.cdx.json");
    std::fs::write(&image_path, cdx_with_binding(purl, &image_hash)).unwrap();
    std::fs::write(&source_path, cdx_with_binding(purl, &source_hash)).unwrap();

    let out = Command::new(bin())
        .arg("sbom")
        .arg("verify-binding")
        .arg("--image-sbom")
        .arg(&image_path)
        .arg("--source-sbom")
        .arg(&source_path)
        .arg("--format")
        .arg("json")
        .output()
        .expect("verify-binding runs");

    assert!(
        !out.status.success(),
        "expected non-zero exit when hashes mismatch (FR-005 / VR-005); \
         stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse JSON report");
    assert_eq!(report["summary"]["verification_failures"], 1);
    assert_eq!(report["summary"]["verified"], 0);
    let rows = report["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["reason"].as_str(), Some("verification-failed"));
}
