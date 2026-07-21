//! Milestone 137 US2 — source-discriminator distinction tests.
//!
//! Covers SC-002: a fixture mixing hosted / git / path / sdk + the
//! self-hosted hosted-source variant. Each entry must emit with the
//! correct PURL shape per FR-003 + the correct `waybill:source-type`
//! annotation value (prefixed `pub-hosted` / `pub-git` / `pub-path` /
//! `pub-sdk`).

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

fn component_with_purl<'a>(doc: &'a Value, purl: &str) -> Option<&'a Value> {
    if let Some(c) = doc.get("metadata").and_then(|m| m.get("component")) {
        if c.get("purl").and_then(|v| v.as_str()) == Some(purl) {
            return Some(c);
        }
    }
    doc.get("components")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|c| {
                c.get("purl").and_then(|v| v.as_str()) == Some(purl)
            })
        })
}

fn property_value<'a>(component: &'a Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value").and_then(|v| v.as_str()))
}

fn write_mixed_fixture(root: &Path) {
    std::fs::write(
        root.join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  http: ^1.1.0\n  window_size: any\n  my_local_lib: any\n",
    )
    .unwrap();
    let sha = "a".repeat(64);
    let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
    std::fs::write(
        root.join("pubspec.lock"),
        format!(
            "packages:\n\
             \x20\x20http:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: http\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.dev\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"1.1.0\"\n\
             \x20\x20window_size:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20path: \"plugins/window_size\"\n\
             \x20\x20\x20\x20\x20\x20ref: \"master\"\n\
             \x20\x20\x20\x20\x20\x20resolved-ref: \"{resolved}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://github.com/google/flutter-desktop-embedding.git\"\n\
             \x20\x20\x20\x20source: git\n\
             \x20\x20\x20\x20version: \"0.1.0\"\n\
             \x20\x20my_local_lib:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20path: \"../packages/my_local_lib\"\n\
             \x20\x20\x20\x20\x20\x20relative: true\n\
             \x20\x20\x20\x20source: path\n\
             \x20\x20\x20\x20version: \"0.1.0\"\n\
             \x20\x20flutter:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description: flutter\n\
             \x20\x20\x20\x20source: sdk\n\
             \x20\x20\x20\x20version: \"0.0.0\"\n\
             \x20\x20flutter_test:\n\
             \x20\x20\x20\x20dependency: \"direct dev\"\n\
             \x20\x20\x20\x20description: flutter\n\
             \x20\x20\x20\x20source: sdk\n\
             \x20\x20\x20\x20version: \"0.0.0\"\n",
        ),
    )
    .unwrap();
}

#[test]
fn hosted_default_pubdev_emits_bare_purl() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let http = component_with_purl(&doc, "pkg:pub/http@1.1.0")
        .expect("hosted-default http must emit as bare PURL");
    assert_eq!(
        property_value(http, "waybill:source-type"),
        Some("pub-hosted"),
    );
}

#[test]
fn hosted_self_hosted_emits_repository_url_qualifier() {
    // Standalone fixture with a self-hosted url.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("pubspec.yaml"),
        "name: my_app\nversion: 0.1.0\n\
         environment:\n  sdk: '>=3.0.0 <4.0.0'\n\
         dependencies:\n  internal_lib: ^2.0.0\n",
    )
    .unwrap();
    let sha = "a".repeat(64);
    std::fs::write(
        tmp.path().join("pubspec.lock"),
        format!(
            "packages:\n\
             \x20\x20internal_lib:\n\
             \x20\x20\x20\x20dependency: \"direct main\"\n\
             \x20\x20\x20\x20description:\n\
             \x20\x20\x20\x20\x20\x20name: internal_lib\n\
             \x20\x20\x20\x20\x20\x20sha256: \"{sha}\"\n\
             \x20\x20\x20\x20\x20\x20url: \"https://pub.acme.example.com\"\n\
             \x20\x20\x20\x20source: hosted\n\
             \x20\x20\x20\x20version: \"2.0.0\"\n",
        ),
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let expected = "pkg:pub/internal_lib@2.0.0?repository_url=https://pub.acme.example.com";
    let internal = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("self-hosted PURL {expected} not found"));
    assert_eq!(
        property_value(internal, "waybill:source-type"),
        Some("pub-hosted"),
    );
}

#[test]
fn git_source_emits_resolved_sha_plus_vcs_url() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let expected = "pkg:pub/window_size@eb39649a76b87e8451baf75d10ce82ca3a3d5601?vcs_url=git+https://github.com/google/flutter-desktop-embedding.git#plugins/window_size";
    let window = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("git-source PURL {expected} not found"));
    assert_eq!(
        property_value(window, "waybill:source-type"),
        Some("pub-git"),
    );
    assert_eq!(
        property_value(window, "waybill:vcs-ref"),
        Some("master"),
        "git source must surface the user-supplied ref via waybill:vcs-ref annotation",
    );
}

#[test]
fn path_source_emits_generic_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let my_local = component_with_purl(&doc, "pkg:generic/my_local_lib@0.1.0")
        .expect("path-source PURL must use pkg:generic/ placeholder");
    assert_eq!(
        property_value(my_local, "waybill:source-type"),
        Some("pub-path"),
    );
    assert_eq!(
        property_value(my_local, "waybill:path"),
        Some("../packages/my_local_lib"),
    );
}

#[test]
fn sdk_source_emits_zero_zero_zero_version() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    for sdk_purl in &["pkg:pub/flutter@0.0.0", "pkg:pub/flutter_test@0.0.0"] {
        let c = component_with_purl(&doc, sdk_purl)
            .unwrap_or_else(|| panic!("SDK pseudo-dep PURL {sdk_purl} not found"));
        assert_eq!(
            property_value(c, "waybill:source-type"),
            Some("pub-sdk"),
            "SDK component {sdk_purl} must carry waybill:source-type=pub-sdk",
        );
        assert_eq!(
            property_value(c, "waybill:sdk-name"),
            Some("flutter"),
            "SDK component {sdk_purl} must carry waybill:sdk-name=flutter",
        );
    }
}
