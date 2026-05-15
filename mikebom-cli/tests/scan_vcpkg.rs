//! End-to-end integration test for the vcpkg manifest-mode reader
//! (milestone 102 US3 / spec FR-007 / Contract 7). Scans the in-repo
//! `tests/fixtures/vcpkg/` directory and asserts the emitted CDX SBOM
//! contains the expected `pkg:vcpkg/...` components.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("vcpkg")
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

fn components_by_prefix<'a>(
    sbom: &'a serde_json::Value,
    prefix: &str,
) -> Vec<&'a serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with(prefix))
        })
        .collect()
}

/// Contract 7 (FR-007, SC-003): vcpkg.json `dependencies[]` produces
/// `pkg:vcpkg/<name>[@<version>]` components.
#[test]
fn vcpkg_simple_dependency_emits_no_version() {
    let sbom = scan_fixture();
    let zlibs = components_by_prefix(&sbom, "pkg:vcpkg/zlib");
    assert_eq!(
        zlibs.len(),
        1,
        "expected exactly one pkg:vcpkg/zlib component; got {zlibs:?}"
    );
    let purl = zlibs[0]["purl"].as_str().unwrap_or("");
    assert_eq!(
        purl, "pkg:vcpkg/zlib",
        "simple string dependency should emit no version segment"
    );
}

#[test]
fn vcpkg_detailed_dependency_emits_version_from_version_ge() {
    let sbom = scan_fixture();
    let openssl = components_by_prefix(&sbom, "pkg:vcpkg/openssl");
    assert_eq!(openssl.len(), 1, "expected one pkg:vcpkg/openssl");
    let purl = openssl[0]["purl"].as_str().unwrap_or("");
    assert_eq!(
        purl, "pkg:vcpkg/openssl@3.0.0",
        "object-form dependency should use version>= as the version"
    );
}

#[test]
fn vcpkg_components_carry_source_files_annotation() {
    let sbom = scan_fixture();
    let openssl = components_by_prefix(&sbom, "pkg:vcpkg/openssl");
    let props = openssl[0]["properties"]
        .as_array()
        .expect("properties array");
    let source_files = props.iter().find(|p| {
        p["name"].as_str() == Some("mikebom:source-files")
    });
    assert!(
        source_files.is_some(),
        "vcpkg components MUST carry mikebom:source-files (FR-012); got props={props:?}"
    );
    let val = source_files.unwrap()["value"].as_str().unwrap_or("");
    assert!(
        val.contains("vcpkg.json"),
        "source-files MUST point at vcpkg.json; got {val}"
    );
}
