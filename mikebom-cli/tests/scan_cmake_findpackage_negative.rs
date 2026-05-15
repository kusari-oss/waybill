//! Dedicated test for FR-007 negative-emission contract — `find_package(X)`
//! MUST NOT emit components attributed to the cmake reader.
//!
//! Fixture combines: (a) a FetchContent_Declare for googletest to prove
//! the cmake reader RAN, with (b) a `find_package(zlib REQUIRED)` line.
//! The test asserts ≥1 googletest component exists (reader ran) AND
//! zero zlib components (find_package negative-emission).

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
fn findpackage_only_no_emission() {
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

    // (b) find_package(zlib REQUIRED) MUST NOT emit pkg:*/zlib per FR-007.
    let zlib_from_cmake_reader = comps
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| {
                    let lower = p.to_lowercase();
                    // Match any zlib PURL EXCEPT the vcpkg/conan ones
                    // (those come from different readers and are not in this fixture).
                    lower.contains("zlib")
                })
        })
        .count();
    assert_eq!(
        zlib_from_cmake_reader, 0,
        "find_package(zlib REQUIRED) MUST NOT emit any pkg:*/zlib component per FR-007; \
         got {zlib_from_cmake_reader} zlib components"
    );
}
