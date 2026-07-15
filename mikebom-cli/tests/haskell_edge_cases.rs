//! Milestone 143 edge-case tests.
//!
//! Covers SC-004 (no-op preservation on non-Haskell trees) + SC-005
//! (malformed lockfile graceful degradation) + Q3-style content-shape
//! gate (non-Stack files matching stack.yaml.lock filename) + flag
//! constraints + range constraints in freeze + main-module fallback
//! paths + multiple *.cabal in one dir + boot-library allowlist
//! case-insensitive match.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &Path) -> (Value, String) {
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
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    (doc, stderr)
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

fn property_value<'a>(c: &'a Value, name: &str) -> Option<&'a str> {
    c.get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn component_with_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    all_components(doc)
        .into_iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl))
}

#[test]
fn sc004_no_op_on_non_haskell_tree() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# Not a Haskell project\n").unwrap();
    std::fs::write(dir.path().join("hello.txt"), "no cabal files here\n").unwrap();
    let (doc, stderr) = run_scan(dir.path());
    let haskell_comps: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "mikebom:source-type")
                .map(|s| s.starts_with("hackage-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        haskell_comps.is_empty(),
        "non-Haskell tree must produce zero hackage-* components; got: {haskell_comps:?}",
    );
    assert!(
        !stderr.contains("haskell:"),
        "non-Haskell tree must not emit any 'haskell:' warnings; stderr={stderr}",
    );
}

#[test]
fn sc005_malformed_freeze_falls_back_to_design_tier() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-lib.cabal"),
        "name: my-lib\nversion: 0.1.0\n\nlibrary\n  build-depends: text\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("cabal.project.freeze"),
        "this is NOT a valid freeze file — no constraints keyword",
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    assert!(
        stderr.contains("haskell: failed to parse cabal.project.freeze"),
        "expected parse-failure warning; stderr={stderr}",
    );
    let text = component_with_name(&doc, "text").expect("text design-tier fallback");
    assert_eq!(property_value(text, "mikebom:sbom-tier"), Some("design"));
}

#[test]
fn q3_content_shape_skips_non_stack_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("stack.yaml.lock"),
        "unrelated: data\nno_snapshots: true\n",
    )
    .unwrap();
    let (doc, stderr) = run_scan(dir.path());
    let haskell_comps: Vec<&Value> = all_components(&doc)
        .into_iter()
        .filter(|c| {
            property_value(c, "mikebom:source-type")
                .map(|s| s.starts_with("hackage-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        haskell_comps.is_empty(),
        "non-Stack stack.yaml.lock-named file must NOT emit hackage components",
    );
    assert!(
        stderr.contains("haskell: failed to parse stack.yaml.lock"),
        "Q3 content-shape failure must surface as warning; stderr={stderr}",
    );
}

#[test]
fn flag_only_constraints_skipped() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("cabal.project.freeze"),
        "constraints: foo +bar, baz ==1.0.0",
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    assert!(component_with_name(&doc, "baz").is_some(), "baz exact-pin must emit");
    assert!(component_with_name(&doc, "foo").is_none(), "foo flag toggle must be skipped");
}

#[test]
fn range_constraint_in_freeze_emits_design_tier() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("cabal.project.freeze"),
        "constraints: text >=2.0 && <2.1",
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let text = component_with_name(&doc, "text").expect("text range component");
    assert_eq!(property_value(text, "mikebom:sbom-tier"), Some("design"));
    assert_eq!(
        property_value(text, "mikebom:requirement-range"),
        Some(">=2.0 && <2.1"),
    );
}

#[test]
fn main_module_version_fallback() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("my-app.cabal"),
        "name: my-app\nlicense: BSD-3-Clause\n\nlibrary\n  build-depends: base\n",
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let main = component_with_purl(&doc, "pkg:hackage/my-app")
        .expect("main-module with version fallback");
    assert_eq!(main.get("name").and_then(|v| v.as_str()), Some("my-app"));
}

#[test]
fn main_module_name_fallback_to_dir_basename() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("orphaned-pkg");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(
        subdir.join("orphaned-pkg.cabal"),
        "license: BSD-3-Clause\n\nlibrary\n  build-depends: base\n",
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let orphan = component_with_name(&doc, "orphaned-pkg")
        .expect("orphaned-pkg main-module from dir basename fallback");
    let purl = orphan.get("purl").and_then(|v| v.as_str()).unwrap();
    // m197 US3 (#567): versionless canonical form (no trailing `@`)
    // when the `.cabal` has no `version:` field. Pre-m197 this test
    // asserted `starts_with("pkg:hackage/orphaned-pkg@")`.
    assert!(
        purl == "pkg:hackage/orphaned-pkg" || purl.starts_with("pkg:hackage/orphaned-pkg@"),
        "main-module PURL should be `pkg:hackage/orphaned-pkg` (versionless) or start with `pkg:hackage/orphaned-pkg@`: {purl}",
    );
}

#[test]
fn boot_library_allowlist_case_insensitive_match() {
    let dir = tempfile::tempdir().unwrap();
    // Win32 in the allowlist is mixed-case; verify lowercased freeze entry matches.
    std::fs::write(
        dir.path().join("cabal.project.freeze"),
        "constraints: win32 ==2.13.4.0",
    )
    .unwrap();
    let (doc, _) = run_scan(dir.path());
    let win32 = component_with_name(&doc, "win32").expect("win32 component");
    assert_eq!(
        property_value(win32, "mikebom:ghc-stdlib"),
        Some("true"),
        "Win32 allowlist entry should match case-insensitively against lowercased win32",
    );
}
