//! Milestone 155 SC-003 integration — same-PURL cross-mechanism dedup.
//!
//! Verifies that when the same package is declared via BOTH
//! `find_package(<Name> <Version>)` and `FetchContent_Declare(<name> URL
//! ...archive-with-embedded-version.tar.gz)`, the production
//! `resolve::deduplicator` pass merges them into exactly ONE emitted
//! component. The surviving `waybill:source-mechanism` value is one of
//! `{"cmake-find-package", "cmake-fetchcontent-url"}` — the winner is
//! confidence-tie-break-dependent and NOT prescribed by the spec.
//!
//! NOTE: cross-namespace scenarios (e.g., cmake vs dpkg) are OUT OF
//! SCOPE for milestone 155 — those require milestone-111 alias-binding
//! or milestone-105 `scan_fs::dedup` completion follow-ups.

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

fn openssl_components(doc: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    if let Some(arr) = doc.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/openssl@1.1.0") {
                out.push(c);
            }
        }
    }
    out
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
fn cmake_find_package_and_fetchcontent_url_same_purl_dedup() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("cmake")).unwrap();

    // Depth-0 discoverable: find_package(openssl 1.1.0)
    // → pkg:generic/openssl@1.1.0 (cmake-find-package).
    std::fs::write(
        tmp.path().join("CMakeLists.txt"),
        r#"
cmake_minimum_required(VERSION 3.10)
project(openssl-dedup C)
find_package(openssl 1.1.0)
include(cmake/deps.cmake)
"#,
    )
    .unwrap();

    // Depth-1 discoverable: FetchContent_Declare with URL containing
    // a semver-shaped version → pkg:generic/openssl@1.1.0
    // (cmake-fetchcontent-url).
    std::fs::write(
        tmp.path().join("cmake").join("deps.cmake"),
        r#"
FetchContent_Declare(openssl URL https://example.com/openssl-1.1.0.tar.gz)
"#,
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let openssls = openssl_components(&doc);
    assert_eq!(
        openssls.len(),
        1,
        "SC-003: exactly ONE pkg:generic/openssl@1.1.0 component expected after dedup; got {}: {}",
        openssls.len(),
        serde_json::to_string_pretty(&openssls).unwrap()
    );

    // The winner's waybill:source-mechanism must be one of the two
    // possible values. The spec does NOT prescribe which one.
    let mechanism = property_value(openssls[0], "waybill:source-mechanism");
    assert!(
        matches!(mechanism, Some("cmake-find-package") | Some("cmake-fetchcontent-url")),
        "expected waybill:source-mechanism ∈ {{cmake-find-package, cmake-fetchcontent-url}}; got {:?}",
        mechanism
    );
}
