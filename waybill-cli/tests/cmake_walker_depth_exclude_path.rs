//! Milestone 156 SC-006 + FR-017 integration — exclude-path integration
//! + src/ boundary enforcement.
//!
//! Fixture:
//! - `CMakeLists.txt` (depth-0, no find_package at top level; only
//!   `add_subdirectory(src)`).
//! - `cmake/defs.cmake` (depth-1) contains `find_package(Bar 1.0)`.
//! - `cmake/modules/FindFoo.cmake` (depth-2) contains
//!   `find_package(Foo)` — excluded via `--exclude-path cmake/modules/`.
//! - `src/CMakeLists.txt` contains `find_package(NotEmitted 9.9.9)` —
//!   F6 remediation from /speckit-analyze: FR-017 forbids walking
//!   src/, so this MUST NOT emit regardless of exclude-path.
//!
//! Asserts exactly ONE `pkg:generic/bar@1.0` component + ZERO
//! `pkg:generic/foo` (excluded) + ZERO `pkg:generic/notemitted`
//! (FR-017 boundary).

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
        .join("tests/fixtures/cmake-walker-depth/exclude-path-integration")
}

fn run_scan_with_exclude(project_root: &std::path::Path, exclude: &str) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--exclude-path")
        .arg(exclude)
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

fn count_purls_matching(doc: &Value, prefix: &str) -> usize {
    doc.get("components")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with(prefix))
        })
        .count()
}

#[test]
fn cmake_walker_exclude_path_filters_and_src_not_walked() {
    let doc = run_scan_with_exclude(&fixture_root(), "cmake/modules");

    // Bar at depth-1 in cmake/defs.cmake IS emitted.
    assert_eq!(
        count_purls_matching(&doc, "pkg:generic/bar"),
        1,
        "expected exactly ONE pkg:generic/bar component from cmake/defs.cmake"
    );

    // Foo at cmake/modules/FindFoo.cmake is EXCLUDED via --exclude-path.
    assert_eq!(
        count_purls_matching(&doc, "pkg:generic/foo"),
        0,
        "milestone 156 FR-005: cmake/modules/ excluded → no Foo emission"
    );

    // F6 remediation — src/CMakeLists.txt has find_package(NotEmitted 9.9.9)
    // but src/ is NOT in the walked top-level dir set per FR-017.
    // Explicitly assert NotEmitted MUST NOT appear.
    assert_eq!(
        count_purls_matching(&doc, "pkg:generic/notemitted"),
        0,
        "milestone 156 FR-017: src/ is NOT walked; find_package(NotEmitted) in src/CMakeLists.txt MUST NOT emit"
    );
}
