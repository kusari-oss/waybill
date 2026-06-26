//! Milestone 143 US1 — cabal.project.freeze baseline integration tests.
//!
//! Covers SC-001 (3 direct + 5 transitive = 8 freeze entries + 1
//! main-module) + SC-008 (main-module emission from *.cabal) + SC-011
//! (Q1 GHC-stdlib annotation).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
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

fn haskell_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    let predicate = |c: &Value| -> bool {
        c.get("properties")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().any(|p| {
                    p.get("name").and_then(|v| v.as_str()) == Some("mikebom:source-type")
                        && p.get("value")
                            .and_then(|v| v.as_str())
                            .map(|s| s.starts_with("hackage-"))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if predicate(c) {
                out.push(c);
            }
        }
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if predicate(c) {
            out.push(c);
        }
    }
    out
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("name").and_then(|v| v.as_str()) == Some(name) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
        })
}

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn write_cabal_fixture(root: &Path) {
    std::fs::write(
        root.join("my-app.cabal"),
        r#"name: my-app
version: 1.2.3
license: BSD-3-Clause

library
  build-depends: base, text, aeson
"#,
    )
    .unwrap();
    // 3 direct deps + 5 transitives = 8 freeze entries.
    std::fs::write(
        root.join("cabal.project.freeze"),
        r#"constraints: aeson ==2.2.0.0,
             text ==2.0.2,
             base ==4.18.0.0,
             bytestring ==0.11.5.3,
             containers ==0.6.7,
             attoparsec ==0.14.4,
             vector ==0.13.0.0,
             time ==1.12.2
"#,
    )
    .unwrap();
}

#[test]
fn sc001_baseline_eight_freeze_components() {
    let dir = tempfile::tempdir().unwrap();
    write_cabal_fixture(dir.path());
    let doc = run_scan(dir.path());
    // 8 freeze-derived components (mikebom:source-type = hackage-freeze).
    let freeze_components: Vec<&Value> = haskell_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "mikebom:source-type") == Some("hackage-freeze"))
        .collect();
    assert_eq!(
        freeze_components.len(),
        8,
        "expected 8 pkg:hackage components from cabal.project.freeze",
    );
}

#[test]
fn sc001_main_module_emission() {
    let dir = tempfile::tempdir().unwrap();
    write_cabal_fixture(dir.path());
    let doc = run_scan(dir.path());
    let main = component_with_name(&doc, "my-app").expect("my-app main-module");
    assert_eq!(
        main.get("purl").and_then(|v| v.as_str()),
        Some("pkg:hackage/my-app@1.2.3"),
    );
    assert_eq!(
        property_value(main, "mikebom:component-role"),
        Some("main-module"),
    );
}

#[test]
fn sc011_q1_ghc_stdlib_annotation_emitted() {
    let dir = tempfile::tempdir().unwrap();
    write_cabal_fixture(dir.path());
    let doc = run_scan(dir.path());

    // Boot libraries: base, text, bytestring, containers, time → all in allowlist.
    for boot_name in &["base", "text", "bytestring", "containers", "time"] {
        let c = component_with_name(&doc, boot_name).unwrap_or_else(|| panic!("expected {boot_name} component"));
        assert_eq!(
            property_value(c, "mikebom:ghc-stdlib"),
            Some("true"),
            "{boot_name} should carry mikebom:ghc-stdlib = true per Q1 + FR-014",
        );
    }

    // Non-boot: aeson, attoparsec, vector → no annotation.
    for non_boot in &["aeson", "attoparsec", "vector"] {
        let c = component_with_name(&doc, non_boot).unwrap_or_else(|| panic!("expected {non_boot} component"));
        assert!(
            property_value(c, "mikebom:ghc-stdlib").is_none(),
            "{non_boot} should NOT carry mikebom:ghc-stdlib annotation",
        );
    }
}
