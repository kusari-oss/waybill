//! Milestone 134 US2 (SC-003) — end-to-end test that two `Cargo.toml`
//! files claiming the same PURL with identical declared deps but
//! divergent `src/lib.rs` contents produce a divergence annotation
//! with `reason: hashes-differ` ONLY when `--deep-hash` is set.
//!
//! Negative case: same fixture WITHOUT `--deep-hash` MUST NOT
//! produce the `hashes-differ` annotation (FR-005 gating invariant).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &Path, deep_hash: bool) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    if !deep_hash {
        cmd.arg("--no-deep-hash");
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn divergence_property_for_purl(doc: &Value, purl: &str) -> Option<Value> {
    let mut candidates: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        candidates.extend(arr);
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        candidates.push(c);
    }
    for c in candidates {
        if c.get("purl").and_then(|v| v.as_str()) != Some(purl) {
            continue;
        }
        let properties = c.get("properties")?.as_array()?;
        for p in properties {
            if p.get("name").and_then(|v| v.as_str())
                == Some("waybill:duplicate-purl-divergent")
            {
                let raw = p.get("value")?.as_str()?;
                return serde_json::from_str(raw).ok();
            }
        }
    }
    None
}

fn write_fixture(root: &Path) {
    // Two crates with IDENTICAL Cargo.toml but DIFFERENT src/lib.rs.
    let body = r#"
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
"#;
    std::fs::create_dir_all(root.join("crates/foo/src")).unwrap();
    std::fs::create_dir_all(root.join("mirrors/foo/src")).unwrap();
    std::fs::write(root.join("crates/foo/Cargo.toml"), body).unwrap();
    std::fs::write(root.join("mirrors/foo/Cargo.toml"), body).unwrap();
    std::fs::write(
        root.join("crates/foo/src/lib.rs"),
        "pub fn safe() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("mirrors/foo/src/lib.rs"),
        "pub fn malicious() {}\n",
    )
    .unwrap();
}

#[test]
fn hashes_differ_emits_annotation_with_deep_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_fixture(root);

    let doc = run_scan(root, /*deep_hash=*/ true);
    let payload = divergence_property_for_purl(&doc, "pkg:cargo/foo@1.2.3")
        .expect("divergence annotation present under --deep-hash");

    let reason = payload.get("reason").and_then(|v| v.as_str()).unwrap();
    assert_eq!(reason, "hashes-differ");

    let hashes = payload
        .get("hashes_by_path")
        .and_then(|v| v.as_object())
        .expect("hashes_by_path populated");
    assert_eq!(hashes.len(), 2);
    let hash_values: Vec<&str> =
        hashes.values().filter_map(|v| v.as_str()).collect();
    assert_eq!(hash_values.len(), 2);
    assert_ne!(
        hash_values[0], hash_values[1],
        "the two crate dirs must hash differently"
    );

    // dep_sets_by_path MUST be absent under hashes-differ (deps identical).
    assert!(
        payload.get("dep_sets_by_path").is_none(),
        "dep_sets_by_path must not appear when reason is hashes-differ",
    );
}

#[test]
fn hashes_differ_emits_no_annotation_without_deep_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_fixture(root);

    let doc = run_scan(root, /*deep_hash=*/ false);
    let payload = divergence_property_for_purl(&doc, "pkg:cargo/foo@1.2.3");
    assert!(
        payload.is_none(),
        "no annotation expected without --deep-hash, got {payload:?}",
    );
}
