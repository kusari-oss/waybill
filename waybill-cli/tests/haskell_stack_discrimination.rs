//! Milestone 143 US2 — Stack lockfile + snapshot placeholder + Q1 GHC-stdlib.
//!
//! Covers SC-002 (4 total Haskell-derived components: 2 extra-deps +
//! 1 snapshot placeholder + 1 main-module) + SC-010 (snapshot placeholder
//! PURL + annotations) + the lts-* / nightly-* / ghc-* resolver dispatch.

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

fn all_components(doc: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        out.extend(arr.iter());
    }
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        out.push(c);
    }
    out
}

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
}

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn write_stack_fixture(root: &Path) {
    std::fs::write(
        root.join("my-app.cabal"),
        r#"name: my-app
version: 0.5.0
license: BSD-3-Clause

library
  build-depends: base, aeson, lens
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("stack.yaml"),
        r#"resolver: lts-22.0
packages:
- .
extra-deps:
- aeson-2.2.0.0
- lens-5.2.3
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("stack.yaml.lock"),
        r#"# Lock file, version 1
snapshots:
  - completed:
      sha256: 5cf7f73716ab1bff7c0e34dee5e6b69077c93e3c447bb71e2ae3a45f0b5c1018
      size: 654321
      url: https://example.com/lts.yaml
    original:
      resolver: lts-22.0
packages:
  - completed:
      hackage: aeson-2.2.0.0@sha256:abc,200
    original:
      hackage: aeson-2.2.0.0
  - completed:
      hackage: lens-5.2.3@sha256:def,300
    original:
      hackage: lens-5.2.3
"#,
    )
    .unwrap();
}

#[test]
fn sc002_stack_baseline_four_components() {
    let dir = tempfile::tempdir().unwrap();
    write_stack_fixture(dir.path());
    let doc = run_scan(dir.path());

    // 2 hackage-stack-lock extra-deps
    let stack_lock_components: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "waybill:source-type") == Some("hackage-stack-lock"))
        .collect();
    assert_eq!(stack_lock_components.len(), 2, "expected 2 hackage-stack-lock components");

    // 1 hackage-snapshot placeholder
    let snapshot_components: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "waybill:source-type") == Some("hackage-snapshot"))
        .collect();
    assert_eq!(snapshot_components.len(), 1, "expected 1 hackage-snapshot placeholder");

    // 1 main-module (use waybill:component-role which IS propagated to
    // metadata.component when the main-module is promoted; the
    // waybill:source-type "hackage-main-module" annotation gets stripped
    // on promotion per the metadata.rs curated allowlist — same gap as
    // milestones 141/142).
    let main_module_components: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "waybill:component-role") == Some("main-module"))
        .collect();
    assert_eq!(main_module_components.len(), 1, "expected 1 main-module");
}

#[test]
fn sc010_snapshot_placeholder_purl_and_annotations() {
    let dir = tempfile::tempdir().unwrap();
    write_stack_fixture(dir.path());
    let doc = run_scan(dir.path());
    let purl = "pkg:generic/stackage-lts-22.0@5cf7f73716ab1bff7c0e34dee5e6b69077c93e3c447bb71e2ae3a45f0b5c1018";
    let snap = component_with_purl(&doc, purl).expect("snapshot placeholder PURL must emit");
    assert_eq!(
        property_value(snap, "waybill:source-type"),
        Some("hackage-snapshot"),
    );
    assert_eq!(
        property_value(snap, "waybill:stackage-resolver"),
        Some("lts-22.0"),
    );
}

#[test]
fn stack_nightly_resolver_purl() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("stack.yaml"),
        "resolver: nightly-2024-01-15\npackages:\n- .\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("stack.yaml.lock"),
        r#"# Lock file, version 1
snapshots:
  - completed:
      sha256: nightlysha
      size: 100
    original:
      resolver: nightly-2024-01-15
packages: []
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let _ = component_with_purl(&doc, "pkg:generic/stackage-nightly-2024-01-15@nightlysha")
        .expect("nightly resolver placeholder");
}

#[test]
fn stack_ghc_only_resolver_no_stackage_prefix() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("stack.yaml"),
        "resolver: ghc-9.6.4\npackages:\n- .\n",
    )
    .unwrap();
    // No stack.yaml.lock — F3 fallback path.
    let doc = run_scan(dir.path());
    let _ = component_with_purl(&doc, "pkg:generic/ghc-9.6.4@unspecified")
        .expect("ghc-only resolver placeholder (no stackage- prefix)");
}

#[test]
fn stack_lock_q1_ghc_stdlib_annotation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("stack.yaml"),
        "resolver: lts-22.0\npackages:\n- .\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("stack.yaml.lock"),
        r#"# Lock file, version 1
snapshots:
  - completed:
      sha256: abc
      size: 100
    original:
      resolver: lts-22.0
packages:
  - completed:
      hackage: base-4.18.0.0
    original:
      hackage: base-4.18.0.0
"#,
    )
    .unwrap();
    let doc = run_scan(dir.path());
    let base = component_with_purl(&doc, "pkg:hackage/base@4.18.0.0").expect("base component");
    assert_eq!(
        property_value(base, "waybill:ghc-stdlib"),
        Some("true"),
        "base is in boot-library allowlist; must carry waybill:ghc-stdlib per Q1 + FR-014",
    );
}
