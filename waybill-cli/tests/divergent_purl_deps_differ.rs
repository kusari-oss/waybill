//! Milestone 134 US1 (SC-001 + SC-002) — end-to-end test that two
//! `Cargo.toml` files claiming the same PURL with divergent
//! `[dependencies]` blocks produce a `waybill:duplicate-purl-divergent`
//! property on the deduped root component.
//!
//! Negative case (SC-002): two `Cargo.toml` files with the SAME PURL
//! and identical declared dep sets MUST NOT produce the divergence
//! annotation.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn run_scan(project_root: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn write_manifest(dir: &Path, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("Cargo.toml"), body).unwrap();
}

fn divergence_property_for_purl(doc: &Value, purl: &str) -> Option<Value> {
    let mut candidates: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        candidates.extend(arr);
    }
    // Single-crate workspaces promote the main-module to
    // `metadata.component`, NOT `components[]`. The divergence
    // annotation rides on whichever placement is in effect.
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

#[test]
fn deps_differ_emits_divergence_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    write_manifest(
        &root.join("crates/foo"),
        r#"
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
tokio = "1"
"#,
    );
    write_manifest(
        &root.join("mirrors/foo"),
        r#"
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
tokio = "1"
anyhow = "1"
"#,
    );

    let doc = run_scan(root);
    let payload = divergence_property_for_purl(&doc, "pkg:cargo/foo@1.2.3")
        .expect("divergence annotation present on deduped component");

    assert_eq!(payload.get("v").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        payload.get("purl").and_then(|v| v.as_str()),
        Some("pkg:cargo/foo@1.2.3"),
    );
    assert_eq!(
        payload.get("reason").and_then(|v| v.as_str()),
        Some("deps-differ"),
    );
    let paths = payload.get("paths").and_then(|v| v.as_array()).unwrap();
    let path_strs: Vec<&str> = paths.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        path_strs.iter().any(|p| p.ends_with("crates/foo/Cargo.toml")),
        "paths contains crates/foo/Cargo.toml: got {path_strs:?}",
    );
    assert!(
        path_strs.iter().any(|p| p.ends_with("mirrors/foo/Cargo.toml")),
        "paths contains mirrors/foo/Cargo.toml: got {path_strs:?}",
    );
    let dep_sets = payload
        .get("dep_sets_by_path")
        .and_then(|v| v.as_object())
        .unwrap();
    assert_eq!(dep_sets.len(), 2);
    // Either path's dep set must include the extra `anyhow` from the
    // vendor copy and the common `serde`/`tokio` set.
    let mut found_anyhow = false;
    for (_path, deps) in dep_sets {
        let names: Vec<&str> =
            deps.as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        if names.contains(&"anyhow") {
            found_anyhow = true;
        }
    }
    assert!(found_anyhow, "expected one of the dep sets to contain anyhow");

    // hashes_by_path MUST be absent under deps-differ (no --deep-hash).
    assert!(
        payload.get("hashes_by_path").is_none(),
        "hashes_by_path must not appear when reason is deps-differ",
    );
}

#[test]
fn identical_deps_emits_no_divergence_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    let body = r#"
[package]
name = "foo"
version = "1.2.3"

[dependencies]
serde = "1"
tokio = "1"
"#;
    write_manifest(&root.join("crates/foo"), body);
    write_manifest(&root.join("mirrors/foo"), body);

    let doc = run_scan(root);
    let payload = divergence_property_for_purl(&doc, "pkg:cargo/foo@1.2.3");
    assert!(
        payload.is_none(),
        "no divergence annotation expected for identical-dep collision, got {payload:?}",
    );
}

#[test]
fn no_collision_emits_no_divergence_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    write_manifest(
        &root.join("a"),
        r#"
[package]
name = "a"
version = "1.0.0"
"#,
    );
    write_manifest(
        &root.join("b"),
        r#"
[package]
name = "b"
version = "1.0.0"
"#,
    );

    let doc = run_scan(root);
    assert!(divergence_property_for_purl(&doc, "pkg:cargo/a@1.0.0").is_none());
    assert!(divergence_property_for_purl(&doc, "pkg:cargo/b@1.0.0").is_none());
}
