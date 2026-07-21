//! Milestone 134 US3 — three independent divergent-PURL collisions
//! in one scan must produce a single document-scope
//! `waybill:purl-collisions-detected` annotation listing all three
//! in deterministic sort order (by `purl`), AND each collision must
//! also appear as a per-component property on its respective
//! component (the redundancy invariant from
//! `contracts/document-scope-annotation.md`).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
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

fn write_divergent_pair(root: &Path, name: &str) {
    let dir_a = root.join(format!("crates/{name}"));
    let dir_b = root.join(format!("mirrors/{name}"));
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    let body_a = format!(
        r#"
[package]
name = "{name}"
version = "1.0.0"

[dependencies]
serde = "1"
"#
    );
    let body_b = format!(
        r#"
[package]
name = "{name}"
version = "1.0.0"

[dependencies]
serde = "1"
anyhow = "1"
"#
    );
    std::fs::write(dir_a.join("Cargo.toml"), body_a).unwrap();
    std::fs::write(dir_b.join("Cargo.toml"), body_b).unwrap();
}

fn document_scope_summary(doc: &Value) -> Option<Value> {
    let metadata_properties =
        doc.get("metadata")?.get("properties")?.as_array()?;
    for p in metadata_properties {
        if p.get("name").and_then(|v| v.as_str())
            == Some("waybill:purl-collisions-detected")
        {
            let raw = p.get("value")?.as_str()?;
            return serde_json::from_str(raw).ok();
        }
    }
    None
}

fn per_component_purls(doc: &Value) -> HashSet<String> {
    let mut found: HashSet<String> = HashSet::new();
    let mut visit = |c: &Value| {
        let Some(purl) = c.get("purl").and_then(|v| v.as_str()) else {
            return;
        };
        let Some(properties) = c.get("properties").and_then(|v| v.as_array()) else {
            return;
        };
        for p in properties {
            if p.get("name").and_then(|v| v.as_str())
                == Some("waybill:duplicate-purl-divergent")
            {
                found.insert(purl.to_string());
                break;
            }
        }
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            visit(c);
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        visit(c);
    }
    found
}

#[test]
fn three_divergent_collisions_emit_sorted_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_divergent_pair(root, "foo");
    write_divergent_pair(root, "bar");
    write_divergent_pair(root, "baz");

    let doc = run_scan(root);
    let summary =
        document_scope_summary(&doc).expect("document-scope summary present");

    assert_eq!(summary.get("v").and_then(|v| v.as_u64()), Some(1));
    let collisions = summary
        .get("collisions")
        .and_then(|v| v.as_array())
        .expect("collisions array");
    assert_eq!(collisions.len(), 3, "expected 3 collisions, got {collisions:?}");

    // (a) Deterministic lex sort order by PURL.
    let purls: Vec<&str> = collisions
        .iter()
        .filter_map(|c| c.get("purl").and_then(|v| v.as_str()))
        .collect();
    let mut sorted = purls.clone();
    sorted.sort();
    assert_eq!(purls, sorted, "collisions must be sorted lex by PURL");
    assert_eq!(
        purls,
        vec![
            "pkg:cargo/bar@1.0.0",
            "pkg:cargo/baz@1.0.0",
            "pkg:cargo/foo@1.0.0",
        ],
    );

    // (b) + (c) Redundancy invariant — every summary entry MUST also
    // appear as a per-component property on its component.
    let per_component = per_component_purls(&doc);
    for purl in &purls {
        assert!(
            per_component.contains(*purl),
            "PURL {purl} in document-scope summary MUST also appear as a \
             per-component waybill:duplicate-purl-divergent property; \
             per_component={per_component:?}",
        );
    }
}

#[test]
fn no_collisions_emits_no_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("crates/foo")).unwrap();
    std::fs::write(
        root.join("crates/foo/Cargo.toml"),
        r#"
[package]
name = "foo"
version = "1.0.0"

[dependencies]
serde = "1"
"#,
    )
    .unwrap();
    let doc = run_scan(root);
    assert!(
        document_scope_summary(&doc).is_none(),
        "no document-scope summary expected on clean scans (FR-009)",
    );
}
