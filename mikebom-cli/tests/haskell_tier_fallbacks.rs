//! Milestone 143 US3 — design-tier + Q2 multi-stanza + multi-package + Q3 Hpack-detect.
//!
//! Covers SC-003 (design-tier from *.cabal only) + SC-007 (test-stanza
//! dev-scope filterability) + SC-009 (multi-package one main-module per
//! local package) + SC-012 (Q2 multi-stanza union with scope merging —
//! per F1 remediation: 5 distinct deps + 1 main-module = 6 total components)
//! + Q3 Hpack-detect warn.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan_with_flags(project_root: &Path, extra: &[&str]) -> (Value, String) {
    let mut top_level_extra: Vec<&str> = Vec::new();
    let mut subcommand_extra: Vec<&str> = Vec::new();
    let mut iter = extra.iter().peekable();
    while let Some(a) = iter.next() {
        if *a == "--exclude-scope" {
            top_level_extra.push(a);
            if let Some(v) = iter.next() {
                top_level_extra.push(v);
            }
        } else {
            subcommand_extra.push(a);
        }
    }
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline");
    for a in &top_level_extra {
        cmd.arg(a);
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    for a in &subcommand_extra {
        cmd.arg(a);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    (doc, stderr)
}

fn run_scan(project_root: &Path) -> (Value, String) {
    run_scan_with_flags(project_root, &[])
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

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

#[test]
fn sc003_design_tier_from_cabal_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-lib.cabal"),
        r#"name: my-lib
version: 0.1.0
license: BSD-3-Clause

library
  build-depends: base, text
"#,
    )
    .unwrap();
    // NO lockfile — triggers design-tier emission per FR-007.
    let (doc, _) = run_scan(dir.path());
    for name in &["base", "text"] {
        let c = component_with_name(&doc, name).unwrap_or_else(|| panic!("expected {name} component"));
        assert_eq!(
            property_value(c, "mikebom:sbom-tier"),
            Some("design"),
        );
        assert_eq!(
            property_value(c, "mikebom:source-type"),
            Some("hackage-cabal-design"),
        );
    }
}

#[test]
fn sc007_test_stanza_dev_scope() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-lib.cabal"),
        r#"name: my-lib
version: 0.1.0

library
  build-depends: base

test-suite spec
  build-depends: base, hspec
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let hspec = component_with_name(&doc, "hspec").expect("hspec component");
    // CDX native scope = "excluded" per milestone-052 dev-scope bridge.
    let scope = hspec.get("scope").and_then(|v| v.as_str());
    assert_eq!(scope, Some("excluded"));
    // --exclude-scope dev suppresses hspec.
    let (doc_excluded, _) = run_scan_with_flags(dir.path(), &["--exclude-scope", "dev"]);
    assert!(
        component_with_name(&doc_excluded, "hspec").is_none(),
        "hspec must be suppressed under --exclude-scope dev",
    );
}

#[test]
fn sc009_multi_package_three_subpackages() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("cabal.project"), "packages:\n  ./pkg-a\n  ./pkg-b\n  ./pkg-c\n").unwrap();
    for sub in &["pkg-a", "pkg-b", "pkg-c"] {
        let pkg_dir = dir.path().join(sub);
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join(format!("{sub}.cabal")),
            format!(
                "name: {sub}\nversion: 0.1.0\n\nlibrary\n  build-depends: base\n"
            ),
        )
        .unwrap();
    }
    let (doc, _) = run_scan(dir.path());
    // Per FR-011 + SC-009: 3 main-modules emit (one per sub-package).
    let main_modules: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| property_value(c, "mikebom:component-role") == Some("main-module"))
        .collect();
    assert_eq!(main_modules.len(), 3, "expected 3 main-modules");
    for sub in &["pkg-a", "pkg-b", "pkg-c"] {
        assert!(
            component_with_name(&doc, sub).is_some(),
            "expected {sub} main-module",
        );
    }
}

#[test]
fn sc012_q2_multi_stanza_union_with_scope_merging() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-app.cabal"),
        r#"name: my-app
version: 0.1.0

library
  build-depends: base, text

executable cli
  build-depends: base, my-app, optparse-applicative

test-suite spec
  build-depends: base, my-app, hspec

benchmark perf
  build-depends: base, my-app, criterion
"#,
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    // Per F1 remediation: 5 distinct dep components + 1 main-module = 6 total.
    // The `my-app` self-ref in exe/test/benchmark stanzas dedups against
    // the main-module's PURL and is NOT separately emitted.
    let haskell_comps: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "mikebom:source-type")
                .map(|s| s.starts_with("hackage-"))
                .unwrap_or(false)
                || property_value(c, "mikebom:component-role") == Some("main-module")
        })
        .collect();
    let names: Vec<&str> = haskell_comps
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    // Assert presence of the 5 deps + 1 main-module = 6 components.
    assert!(names.contains(&"base"));
    assert!(names.contains(&"text"));
    assert!(names.contains(&"optparse-applicative"));
    assert!(names.contains(&"hspec"));
    assert!(names.contains(&"criterion"));
    assert!(names.contains(&"my-app"));
    // Per Q2 most-binding-wins: `base` appears in library/executable AND
    // test/benchmark stanzas; resolves to runtime-scope.
    // CDX 1.6: scope defaults to "required" when absent, so either
    // None or Some("required") indicates runtime-scope.
    let base = component_with_name(&doc, "base").unwrap();
    let base_scope = base.get("scope").and_then(|v| v.as_str());
    assert!(
        base_scope.is_none() || base_scope == Some("required"),
        "base must resolve to runtime-scope per Q2 most-binding-wins (library wins over test/bench); got scope={base_scope:?}",
    );
    // hspec is test-only → CDX scope = excluded
    let hspec = component_with_name(&doc, "hspec").unwrap();
    assert_eq!(hspec.get("scope").and_then(|v| v.as_str()), Some("excluded"));
    // criterion is benchmark-only → CDX scope = excluded
    let criterion = component_with_name(&doc, "criterion").unwrap();
    assert_eq!(criterion.get("scope").and_then(|v| v.as_str()), Some("excluded"));
}

#[test]
fn q3_hpack_detect_emits_warn() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.yaml"),
        "name: my-app\nversion: 0.1.0\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("my-app.cabal"),
        r#"-- This file has been generated from package.yaml by hpack version 0.36.0.
name: my-app
version: 0.1.0

library
  build-depends: base
"#,
    )
    .unwrap();
    let (_doc, stderr) = run_scan(dir.path());
    assert!(
        stderr.contains("haskell: Hpack-generated"),
        "expected Q3 Hpack-detect warning in stderr; stderr={stderr}",
    );
}
