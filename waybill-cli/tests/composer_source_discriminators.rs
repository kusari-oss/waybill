//! Milestone 138 US2 — source-discriminator distinction tests.
//!
//! Covers SC-002: a fixture mixing packagist + vcs + path + plugin +
//! metapackage. Each entry must emit with the correct PURL shape per
//! FR-003 + the correct `mikebom:source-type` annotation value
//! (prefixed `composer-packagist` / `composer-vcs` / `composer-path` /
//! `composer-plugin` / `composer-metapackage`). Plus vendor/name
//! lowercasing per purl-spec canonical form.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
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
        root.join("composer.json"),
        r#"{
  "name": "acme/my-app",
  "version": "1.0.0",
  "require": {
    "symfony/console": "^7.0",
    "acme/my-fork": "dev-main",
    "acme/local-lib": "*",
    "composer/installers": "^2.3"
  }
}
"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    let resolved = "eb39649a76b87e8451baf75d10ce82ca3a3d5601";
    std::fs::write(
        root.join("composer.lock"),
        format!(
            r#"{{
  "content-hash": "x",
  "packages": [
    {{
      "name":"symfony/console","version":"v7.0.4","type":"library",
      "dist":{{"type":"zip","url":"https://api.github.com/repos/symfony/console/zipball/abc","shasum":"{sha1}"}}
    }},
    {{
      "name":"acme/my-fork","version":"dev-main","type":"library",
      "source":{{"type":"git","url":"https://github.com/acme/my-fork.git","reference":"{resolved}"}}
    }},
    {{
      "name":"acme/local-lib","version":"0.1.0","type":"library",
      "source":{{"type":"path","url":"../packages/local-lib","reference":"local"}}
    }},
    {{
      "name":"composer/installers","version":"v2.3.0","type":"composer-plugin",
      "dist":{{"type":"zip","url":"https://api.github.com/repos/composer/installers/zipball/def","shasum":"{sha1}"}}
    }},
    {{
      "name":"symfony/symfony","version":"v7.0.4","type":"metapackage"
    }}
  ],
  "packages-dev": []
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn packagist_default_emits_bare_purl() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:composer/symfony/console@v7.0.4")
        .expect("packagist-default symfony/console must emit as bare PURL");
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-packagist"),
    );
}

#[test]
fn packagist_self_hosted_emits_repository_url_qualifier() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/app","version":"1.0.0","require":{"acme/internal_lib":"^2.0"}}"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("composer.lock"),
        format!(
            r#"{{
  "content-hash":"x",
  "packages":[{{
    "name":"acme/internal_lib","version":"2.0.0","type":"library",
    "dist":{{"type":"zip","url":"https://repo.acme.example.com/dist/acme/internal_lib/abc.zip","shasum":"{sha1}"}}
  }}],
  "packages-dev":[]
}}
"#
        ),
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let expected = "pkg:composer/acme/internal_lib@2.0.0?repository_url=https://repo.acme.example.com";
    let c = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("self-hosted PURL {expected} not found"));
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-packagist"),
    );
}

#[test]
fn vcs_source_emits_vcs_url_and_vcs_ref() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let expected = "pkg:composer/acme/my-fork@dev-main?vcs_url=git+https://github.com/acme/my-fork.git";
    let c = component_with_purl(&doc, expected)
        .unwrap_or_else(|| panic!("VCS PURL {expected} not found"));
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-vcs"),
    );
    assert_eq!(
        property_value(c, "mikebom:vcs-ref"),
        Some("eb39649a76b87e8451baf75d10ce82ca3a3d5601"),
        "VCS source must surface the resolved SHA via mikebom:vcs-ref",
    );
}

#[test]
fn path_source_emits_generic_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:generic/acme-local-lib@0.1.0")
        .expect("path-source PURL must use pkg:generic/ placeholder with flattened vendor");
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-path"),
    );
    assert_eq!(
        property_value(c, "mikebom:path"),
        Some("../packages/local-lib"),
    );
}

#[test]
fn composer_plugin_emits_packagist_purl_with_plugin_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:composer/composer/installers@v2.3.0")
        .expect("composer-plugin must emit with standard Packagist PURL");
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-plugin"),
    );
    assert_eq!(
        property_value(c, "mikebom:composer-type"),
        Some("composer-plugin"),
    );
}

#[test]
fn composer_metapackage_emits_packagist_purl_with_metapackage_annotation() {
    let tmp = tempfile::tempdir().unwrap();
    write_mixed_fixture(tmp.path());

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:composer/symfony/symfony@v7.0.4")
        .expect("metapackage must emit with standard Packagist PURL");
    assert_eq!(
        property_value(c, "mikebom:source-type"),
        Some("composer-metapackage"),
    );
    // Metapackages have no dist → no SHA-1 hash.
    let hashes = c.get("hashes").and_then(|v| v.as_array());
    assert!(
        hashes.is_none() || hashes.unwrap().is_empty(),
        "metapackage must NOT carry hashes (no downloadable artifact); got {hashes:?}",
    );
}

#[test]
fn vendor_name_lowercased_in_purl() {
    // Phase 0 correction: vendor + name lowercased per purl-spec
    // canonical form. The `name` field on the component preserves
    // source case for display.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"name":"acme/app","version":"1.0.0","require":{"ACME/MyLib":"^1.0"}}"#,
    )
    .unwrap();
    let sha1 = "a".repeat(40);
    std::fs::write(
        tmp.path().join("composer.lock"),
        format!(
            r#"{{
  "content-hash":"x",
  "packages":[{{
    "name":"ACME/MyLib","version":"1.0.0","type":"library",
    "dist":{{"type":"zip","url":"https://api.github.com/repos/x/x/zipball/x","shasum":"{sha1}"}}
  }}],
  "packages-dev":[]
}}
"#
        ),
    )
    .unwrap();

    let doc = run_scan(tmp.path());
    let c = component_with_purl(&doc, "pkg:composer/acme/mylib@1.0.0")
        .expect("ACME/MyLib must be lowercased to acme/mylib in PURL per purl-spec");
    let name = c.get("name").and_then(|v| v.as_str());
    assert_eq!(name, Some("ACME/MyLib"), "name field preserves source case for display");
}
