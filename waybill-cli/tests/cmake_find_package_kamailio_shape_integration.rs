//! Milestone 155 SC-004 integration — the Kamailio-shape synthetic
//! fixture exercises find_package + pkg_check_modules extraction
//! end-to-end through the emitted CDX SBOM.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cmake-find-package/kamailio-shape")
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

fn components_by_mechanism<'a>(doc: &'a Value, mechanism: &str) -> Vec<&'a Value> {
    let mut out: Vec<&'a Value> = Vec::new();
    let components = match doc.get("components").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return out,
    };
    for c in components {
        let props = match c.get("properties").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        if props.iter().any(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some("mikebom:source-mechanism")
                && p.get("value").and_then(|v| v.as_str()) == Some(mechanism)
        }) {
            out.push(c);
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
fn kamailio_shape_emits_expected_cmake_components() {
    let doc = run_scan(&fixture_root());

    // SC-004 assertion (a): ≥5 components with mechanism cmake-find-package.
    let fp = components_by_mechanism(&doc, "cmake-find-package");
    assert!(
        fp.len() >= 5,
        "expected ≥5 cmake-find-package components; got {}: {:?}",
        fp.len(),
        fp.iter()
            .map(|c| c.get("purl").and_then(|v| v.as_str()).unwrap_or("<none>"))
            .collect::<Vec<_>>()
    );

    // SC-004 assertion (b): ≥1 component with mechanism cmake-pkg-check-modules.
    let pcm = components_by_mechanism(&doc, "cmake-pkg-check-modules");
    assert!(
        !pcm.is_empty(),
        "expected ≥1 cmake-pkg-check-modules component; got {}",
        pcm.len()
    );

    // SC-004 assertion (c): OpenSSL component with expected PURL + name annotation.
    let openssl = fp
        .iter()
        .find(|c| {
            c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/openssl@1.1.0")
        })
        .expect("expected pkg:generic/openssl@1.1.0 in cmake-find-package emissions");
    assert_eq!(
        property_value(openssl, "mikebom:cmake-find-package-name"),
        Some("OpenSSL"),
        "OpenSSL component must preserve original casing"
    );

    // SC-004 assertion (d): radcli component with no name annotation.
    let radcli = pcm
        .iter()
        .find(|c| c.get("purl").and_then(|v| v.as_str()) == Some("pkg:generic/radcli"))
        .expect("expected pkg:generic/radcli in cmake-pkg-check-modules emissions");
    assert!(
        property_value(radcli, "mikebom:cmake-find-package-name").is_none(),
        "pkg_check_modules emissions MUST NOT carry mikebom:cmake-find-package-name"
    );
}
