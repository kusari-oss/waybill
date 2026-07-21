//! Dedicated test for milestone 155's REVERSAL of milestone-102 FR-007
//! — `find_package(X)` NOW emits `pkg:generic/x` tagged with
//! `mikebom:source-mechanism = "cmake-find-package"`.
//!
//! Fixture combines: (a) a FetchContent_Declare for googletest to prove
//! the cmake reader RAN, with (b) a `find_package(zlib REQUIRED)` line.
//! The test asserts ≥1 googletest component exists (reader ran) AND
//! exactly ONE `pkg:generic/zlib` component (find_package emission
//! per milestone 155).

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("cmake_findpackage_only")
}

fn scan_fixture() -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

#[test]
fn findpackage_emits_since_milestone_155() {
    let sbom = scan_fixture();
    let comps = sbom["components"].as_array().expect("components array");

    // (a) cmake reader RAN — googletest from FetchContent_Declare must emit.
    let googletest_count = comps
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:github/google/googletest"))
        })
        .count();
    assert_eq!(
        googletest_count, 1,
        "cmake reader MUST have run (and emitted googletest from FetchContent_Declare); \
         got {googletest_count} googletest components"
    );

    // (b) Milestone 155 REVERSAL of FR-007: find_package(zlib REQUIRED)
    // MUST now emit exactly one pkg:generic/zlib component tagged with
    // mikebom:source-mechanism = "cmake-find-package".
    let zlib_from_find_package: Vec<&serde_json::Value> = comps
        .iter()
        .filter(|c| c["purl"].as_str() == Some("pkg:generic/zlib"))
        .filter(|c| {
            c["properties"].as_array().is_some_and(|arr| {
                arr.iter().any(|p| {
                    p["name"].as_str() == Some("mikebom:source-mechanism")
                        && p["value"].as_str() == Some("cmake-find-package")
                })
            })
        })
        .collect();
    assert_eq!(
        zlib_from_find_package.len(),
        1,
        "milestone 155 REVERSAL of FR-007: find_package(zlib REQUIRED) MUST now emit \
         exactly one pkg:generic/zlib component tagged cmake-find-package; \
         got {}",
        zlib_from_find_package.len()
    );
}
