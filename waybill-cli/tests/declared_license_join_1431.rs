//! #1431 end to end: a reader that combines several declared licenses with
//! `AND` must bracket an operand that carries its own `OR`.
//!
//! `declared_license::resolve_many` is shared by the gem, maven, elixir,
//! erlang and scala readers. Unbracketed, `["MIT OR Apache-2.0",
//! "BSD-3-Clause"]` became `MIT OR Apache-2.0 AND BSD-3-Clause`, which SPDX
//! precedence reads as `MIT OR (Apache-2.0 AND BSD-3-Clause)`.
//!
//! Two readers are scanned so the test covers the shared path rather than one
//! reader's call site.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;
use std::process::Command;

use common::normalize::apply_fake_home_env;
use serde_json::Value;

const WANT: &str = "(MIT OR Apache-2.0) AND BSD-3-Clause";
const UNBRACKETED: &str = "MIT OR Apache-2.0 AND BSD-3-Clause";

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Scan once, returning (CycloneDX, SPDX 2.3, SPDX 3) documents.
fn scan(root: &Path) -> (Value, Value, Value) {
    let out = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let cdx = out.path().join("out.cdx.json");
    let spdx23 = out.path().join("out.spdx.json");
    let spdx3 = out.path().join("out.spdx3.json");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
    apply_fake_home_env(&mut cmd, fake_home.path());
    let result = cmd
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let read = |p: &Path| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    (read(&cdx), read(&spdx23), read(&spdx3))
}

/// The named component's `licenses`, whether it is the document's primary
/// component or an entry in `components`.
fn cdx_licenses(doc: &Value, name: &str) -> Value {
    let primary = &doc["metadata"]["component"];
    if primary["name"] == name {
        return primary["licenses"].clone();
    }
    doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no CDX component {name}"))["licenses"]
        .clone()
}

fn spdx23_declared(doc: &Value, name: &str) -> String {
    doc["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no SPDX 2.3 package {name}"))["licenseDeclared"]
        .as_str()
        .unwrap()
        .to_string()
}

fn spdx3_expressions(doc: &Value) -> Vec<String> {
    doc["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "simplelicensing_LicenseExpression")
        .map(|e| e["simplelicensing_licenseExpression"].as_str().unwrap().to_string())
        .collect()
}

fn assert_bracketed_everywhere(root: &Path, name: &str) {
    let (cdx, spdx23, spdx3) = scan(root);

    assert_eq!(spdx23_declared(&spdx23, name), WANT, "SPDX 2.3 licenseDeclared");

    let v3 = spdx3_expressions(&spdx3);
    assert!(v3.iter().any(|e| e == WANT), "SPDX 3 expressions: {v3:?}");
    assert!(
        !v3.iter().any(|e| e == UNBRACKETED),
        "unbracketed join still present in SPDX 3: {v3:?}"
    );

    assert_eq!(
        cdx_licenses(&cdx, name),
        serde_json::json!([{"expression": WANT, "acknowledgement": "declared"}]),
        "CycloneDX must carry the bracketed join as one expression"
    );
}

#[test]
fn issue_1431_gem_licenses_array_brackets_an_or_operand() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("waybill-fixture-gemjoin.gemspec"),
        r#"Gem::Specification.new do |spec|
  spec.name    = "waybill-fixture-gemjoin"
  spec.version = "1.0.0"
  spec.licenses = ["MIT OR Apache-2.0", "BSD-3-Clause"]
end
"#,
    );
    assert_bracketed_everywhere(tmp.path(), "waybill-fixture-gemjoin");
}

#[test]
fn issue_1431_maven_license_list_brackets_an_or_operand() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("pom.xml"),
        "<project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
         <modelVersion>4.0.0</modelVersion>\n\
         <groupId>dev.waybill.fixture</groupId>\n\
         <artifactId>waybill-fixture-mvnjoin</artifactId>\n\
         <version>1.0.0</version>\n\
         <licenses>\n\
         <license><name>MIT OR Apache-2.0</name></license>\n\
         <license><name>BSD-3-Clause</name></license>\n\
         </licenses>\n\
         </project>\n",
    );
    assert_bracketed_everywhere(tmp.path(), "waybill-fixture-mvnjoin");
}
