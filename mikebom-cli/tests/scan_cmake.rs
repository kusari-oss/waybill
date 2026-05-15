//! End-to-end integration test for the CMake source-tree reader
//! (milestone 102 US2 / milestone 103 / Contracts 4+5+6+7).

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("cmake")
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

fn component_property<'a>(c: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str())
}

/// Contract 4: FetchContent_Declare GIT_REPOSITORY on a GitHub URL
/// emits `pkg:github/<owner>/<repo>@<tag>`.
#[test]
fn cmake_fetchcontent_github_emits_pkg_github() {
    let sbom = scan_fixture();
    let googletest = components_by_prefix(&sbom, "pkg:github/google/googletest");
    assert_eq!(googletest.len(), 1);
    assert_eq!(
        googletest[0]["purl"].as_str(),
        Some("pkg:github/google/googletest@release-1.14.0")
    );
}

/// Contract 5: ExternalProject_Add URL+URL_HASH emits sha256 + URL.
#[test]
fn cmake_externalproject_url_emits_sha256_and_url() {
    let sbom = scan_fixture();
    let zlib = components_by_prefix(&sbom, "pkg:generic/zlib");
    assert!(!zlib.is_empty(), "expected pkg:generic/zlib component");
    assert_eq!(
        zlib[0]["purl"].as_str(),
        Some("pkg:generic/zlib@1.3.1"),
        "version must be parsed from URL filename"
    );
    assert_eq!(
        component_property(zlib[0], "mikebom:download-url"),
        Some("https://zlib.net/zlib-1.3.1.tar.gz")
    );
    let hashes = zlib[0]["hashes"].as_array().expect("hashes array");
    assert!(
        hashes.iter().any(|h| h["alg"].as_str() == Some("SHA-256")),
        "expected SHA-256 in hashes[]"
    );
}

/// Contract 6: included `.cmake` files under `cmake/` are walked;
/// declared deps attribute source-files to the included file.
#[test]
fn cmake_includes_walked() {
    let sbom = scan_fixture();
    let boost = components_by_prefix(&sbom, "pkg:generic/boost");
    assert_eq!(boost.len(), 1, "expected pkg:generic/boost from cmake/third_party.cmake");
    let source = component_property(boost[0], "mikebom:source-files")
        .expect("boost must carry mikebom:source-files");
    assert!(
        source.contains("third_party.cmake"),
        "source-files must point at cmake/third_party.cmake (the included file), not the top-level CMakeLists.txt; got {source:?}"
    );
}

/// Contract 7 (FR-007): find_package(OpenSSL REQUIRED) MUST NOT emit
/// `pkg:*/OpenSSL` components attributed to the cmake reader.
#[test]
fn cmake_find_package_not_emitted() {
    let sbom = scan_fixture();
    let openssl = sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| {
                    let lower = p.to_lowercase();
                    lower.contains("openssl") && !p.starts_with("pkg:conan/")
                        && !p.starts_with("pkg:vcpkg/")
                })
        })
        .count();
    assert_eq!(
        openssl, 0,
        "find_package(OpenSSL REQUIRED) MUST NOT emit components per FR-007"
    );
}
