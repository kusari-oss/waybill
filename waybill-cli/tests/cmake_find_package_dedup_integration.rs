//! Milestone 155 SC-003 integration — a package both required with
//! `find_package(<Name> <Version>)` and fetched with
//! `FetchContent_Declare(<name> URL ...-<version>.tar.gz)`.
//!
//! Until #1520 both produced `pkg:generic/<name>@<version>` and the
//! deduplicator merged them by PURL. Since #1520 a `find_package` version
//! is a requirement: the call emits a versionless design-tier
//! `pkg:generic/<name>`, and the fetch is what resolves it. The milestone-191
//! reconciler collapses the requirement into the fetch when both are
//! declared in the same directory, its match scope. Declared in different
//! directories, they stay two components.
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
            if c.get("name").and_then(|v| v.as_str()) == Some("openssl") {
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

fn purls_and_mechanisms(doc: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = openssl_components(doc)
        .into_iter()
        .map(|c| {
            (
                c.get("purl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                property_value(c, "waybill:source-mechanism").unwrap_or("").to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn cmake_find_package_requirement_collapses_into_a_same_directory_fetch() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("CMakeLists.txt"),
        r#"
cmake_minimum_required(VERSION 3.10)
project(openssl-dedup C)
find_package(openssl 1.1.0)
FetchContent_Declare(openssl URL https://example.com/openssl-1.1.0.tar.gz)
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert_eq!(
        purls_and_mechanisms(&doc),
        [("pkg:generic/openssl@1.1.0".to_string(), "cmake-fetchcontent-url".to_string())],
        "SC-003: the requirement collapses into the fetch that resolves it",
    );
}

#[test]
fn cmake_find_package_requirement_and_fetch_in_different_directories_stay_apart() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("cmake")).unwrap();
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
    std::fs::write(
        tmp.path().join("cmake").join("deps.cmake"),
        r#"
FetchContent_Declare(openssl URL https://example.com/openssl-1.1.0.tar.gz)
"#,
    )
    .unwrap();
    let doc = run_scan(tmp.path());
    assert_eq!(
        purls_and_mechanisms(&doc),
        [
            ("pkg:generic/openssl".to_string(), "cmake-find-package".to_string()),
            ("pkg:generic/openssl@1.1.0".to_string(), "cmake-fetchcontent-url".to_string()),
        ],
        "#1520: outside the reconciler's directory scope the requirement is its own component",
    );
}
