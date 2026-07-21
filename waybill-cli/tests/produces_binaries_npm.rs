//! Integration tests for the milestone 116 PR-B npm slice (FR-006).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("produces_binaries")
        .join("npm")
        .join(sub)
}

fn run_scan(path: &Path, out_path: &Path) -> Output {
    let bin = env!("CARGO_BIN_EXE_waybill");
    Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run")
}

fn read_sbom(path: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn produces_binaries_for_purl(sbom: &serde_json::Value, purl: &str) -> Option<Vec<String>> {
    let mut candidates: Vec<&serde_json::Value> = Vec::new();
    if let Some(c) = sbom.get("metadata").and_then(|m| m.get("component")) {
        candidates.push(c);
    }
    if let Some(arr) = sbom.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            candidates.push(c);
        }
    }
    for c in candidates {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            let Some(props) = c.get("properties").and_then(|v| v.as_array()) else {
                return Some(Vec::new());
            };
            for p in props {
                if p.get("name").and_then(|v| v.as_str()) == Some("waybill:produces-binaries") {
                    let v = p.get("value").and_then(|v| v.as_str())?;
                    let arr: Vec<String> = serde_json::from_str(v).ok()?;
                    return Some(arr);
                }
            }
            return Some(Vec::new());
        }
    }
    None
}

#[test]
fn string_form_uses_package_name_as_binary_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("string-form"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_for_purl(&sbom, "pkg:npm/fixture-baz@1.0.0")
        .expect("fixture-baz npm main-module component should be present");
    assert_eq!(produced, vec!["fixture-baz".to_string()]);
}

#[test]
fn object_form_uses_each_key_as_binary_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("object-form"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_for_purl(&sbom, "pkg:npm/fixture-baz@1.0.0")
        .expect("fixture-baz npm main-module component should be present");
    // Lex-sorted per FR-001's invariant.
    assert_eq!(
        produced,
        vec!["baz".to_string(), "baz-init".to_string()]
    );
}

#[test]
fn library_only_omits_produces_binaries_property() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("sbom.cdx.json");
    let output = run_scan(&fixture("library-only"), &out);
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let sbom = read_sbom(&out);
    let produced = produces_binaries_for_purl(&sbom, "pkg:npm/fixture-libonly@1.0.0");
    match produced {
        None => {} // component not emitted
        Some(v) => assert!(
            v.is_empty(),
            "library-only npm package must NOT carry produces-binaries; got {v:?}"
        ),
    }
}
