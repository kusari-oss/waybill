//! Milestone 156 SC-005 integration — cross-depth version consolidation.
//!
//! Fixture:
//! - `CMakeLists.txt` (depth-0) contains `find_package(OpenSSL 1.1.0)`.
//! - `cmake/modules/FindOpenSSL.cmake` (depth-3) contains
//!   `find_package(OpenSSL 3.0)`.
//!
//! Milestone-155's Q1 highest-version-wins consolidation fires across
//! the depths that milestone-156's extended walker now sees together.
//! Asserts one merged `pkg:generic/openssl@3.0` component with both
//! source paths in `waybill:source-files`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cmake-walker-depth/cross-depth-version")
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

#[test]
fn cmake_walker_cross_depth_version_consolidation() {
    let doc = run_scan(&fixture_root());
    let comps = doc.get("components").and_then(|v| v.as_array()).unwrap();

    // Exactly ONE openssl component post-dedup.
    let openssls: Vec<_> = comps
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p == "pkg:generic/openssl@3.0")
        })
        .collect();
    assert_eq!(
        openssls.len(),
        1,
        "expected exactly ONE pkg:generic/openssl@3.0 component after Q1 highest-version-wins consolidation across depths; got {}",
        openssls.len()
    );

    // Both source paths captured in waybill:source-files (via milestone-148
    // union pass).
    let props = openssls[0]
        .get("properties")
        .and_then(|v| v.as_array())
        .expect("openssl component has properties[]");
    let source_files_prop = props
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some("waybill:source-files"))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
        .expect("waybill:source-files annotation present");
    assert!(
        source_files_prop.contains("CMakeLists.txt"),
        "source-files should include depth-0 CMakeLists.txt; got {source_files_prop:?}"
    );
    assert!(
        source_files_prop.contains("FindOpenSSL.cmake"),
        "source-files should include depth-3 cmake/modules/FindOpenSSL.cmake; got {source_files_prop:?}"
    );
}
