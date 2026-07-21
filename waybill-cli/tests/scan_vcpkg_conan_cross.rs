//! Cross-ecosystem dedup test (milestone 102 US3 / Q2 clarification /
//! FR-010 / Contract 10). When a project declares the same logical
//! dep (`openssl`) via BOTH `vcpkg.json` AND `conanfile.txt`, the
//! emitted SBOM contains TWO separate components — one per ecosystem
//! PURL — because the existing deduplicator's `(ecosystem, name,
//! version, parent_purl)` key naturally separates them.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("conan_vcpkg_cross")
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
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

#[test]
fn cross_ecosystem_same_name_emits_two_components() {
    let sbom = scan_fixture();
    let comps = sbom["components"].as_array().expect("components array");

    let vcpkg_openssl = comps
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:vcpkg/openssl"))
        })
        .count();
    let conan_openssl = comps
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:conan/openssl"))
        })
        .count();

    assert_eq!(
        vcpkg_openssl, 1,
        "expected exactly one pkg:vcpkg/openssl component"
    );
    assert_eq!(
        conan_openssl, 1,
        "expected exactly one pkg:conan/openssl component"
    );
    // Different versions (vcpkg version>= 3.0.0 vs Conan 3.2.1) preserved
    // separately per FR-010 — cross-ecosystem same-name does NOT collapse.
}
