//! Milestone 156 SC-004 integration — depth-3 emission.
//!
//! Fixture at `cmake/modules/vendor/Extra.cmake` (depth-3 from scan
//! root) contains `find_package(Foo 2.5)` +
//! `pkg_check_modules(BAR REQUIRED bar)`. Milestone 156's extended
//! walker discovers this file via safe_walk recursive descent under
//! cmake/. Verifies FR-001 + FR-006 + FR-007 (F5 remediation folded
//!   in — pkg_check_modules shape preservation at depth-3).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cmake-walker-depth/depth3-emission")
}

fn run_scan(project_root: &std::path::Path) -> Value {
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

fn find_component<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find(|c| c.get("purl").and_then(|v| v.as_str()) == Some(purl)))
}

fn property_value<'a>(component: &'a Value, key: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
                .and_then(|p| p.get("value").and_then(|v| v.as_str()))
        })
}

#[test]
fn cmake_walker_depth3_emits_find_package() {
    let doc = run_scan(&fixture_root());

    let foo = find_component(&doc, "pkg:generic/foo@2.5")
        .expect("expected pkg:generic/foo@2.5 component from depth-3 find_package");
    assert_eq!(
        property_value(foo, "mikebom:source-mechanism"),
        Some("cmake-find-package")
    );
    let source_files = property_value(foo, "mikebom:source-files").unwrap_or("");
    assert!(
        source_files.contains("cmake/modules/vendor/Extra.cmake")
            || source_files.contains("cmake\\modules\\vendor\\Extra.cmake"),
        "mikebom:source-files should name the depth-3 path; got {source_files:?}"
    );
}

#[test]
fn cmake_walker_depth3_emits_pkg_check_modules() {
    // F5 remediation from /speckit-analyze — verify pkg_check_modules
    // emission shape (FR-007) at depth-3.
    let doc = run_scan(&fixture_root());

    let bar = find_component(&doc, "pkg:generic/bar")
        .expect("expected pkg:generic/bar component from depth-3 pkg_check_modules");
    assert_eq!(
        property_value(bar, "mikebom:source-mechanism"),
        Some("cmake-pkg-check-modules")
    );
}
