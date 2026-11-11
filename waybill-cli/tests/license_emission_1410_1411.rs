//! #1410 and #1411 end to end: scan a pacman rootfs whose packages declare an
//! "or later" license and a multi-license list containing an OR compound, and
//! check what each output format emits.
//!
//! - #1410: CycloneDX `license.id` is constrained to the SPDX License List,
//!   which has no `+` forms. The test validator stubs `spdx.schema.json`, so
//!   this file asserts list membership directly with the `spdx` crate.
//! - #1411: joining several licenses with ` AND ` must bracket an OR operand,
//!   in both SPDX emitters.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn write_pkg(rootfs: &Path, name: &str, version: &str, licenses: &[&str]) {
    let pkg_dir = rootfs
        .join("var/lib/pacman/local")
        .join(format!("{name}-{version}"));
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let desc = format!(
        "%NAME%\n{name}\n\n%VERSION%\n{version}\n\n%ARCH%\nx86_64\n\n%LICENSE%\n{}\n\n",
        licenses.join("\n")
    );
    std::fs::write(pkg_dir.join("desc"), desc).unwrap();
}

/// Scan once, returning (CycloneDX, SPDX 2.3, SPDX 3) documents.
fn scan(rootfs: &Path) -> (Value, Value, Value) {
    let out = tempfile::tempdir().unwrap();
    let cdx = out.path().join("out.cdx.json");
    let spdx23 = out.path().join("out.spdx.json");
    let spdx3 = out.path().join("out.spdx3.json");
    let result = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(rootfs)
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

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_pkg(tmp.path(), "waybill-fixture-plus", "2.0-1", &["Apache-2.0+"]);
    write_pkg(tmp.path(), "waybill-fixture-plusand", "3.0-1", &["EUPL-1.1+ AND MIT"]);
    write_pkg(
        tmp.path(),
        "waybill-fixture-multi",
        "1.0-1",
        &["MIT OR Apache-2.0", "BSD-3-Clause"],
    );
    tmp
}

fn cdx_component<'a>(doc: &'a Value, name: &str) -> &'a Value {
    doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no CDX component {name}"))
}

fn spdx23_declared<'a>(doc: &'a Value, name: &str) -> &'a str {
    doc["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no SPDX 2.3 package {name}"))["licenseDeclared"]
        .as_str()
        .unwrap()
}

fn spdx3_expressions(doc: &Value) -> Vec<String> {
    let mut out: Vec<String> = doc["@graph"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "simplelicensing_LicenseExpression")
        .map(|e| e["simplelicensing_licenseExpression"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

fn collect_license_ids(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            if let Some(Value::Object(lic)) = map.get("license") {
                if let Some(Value::String(id)) = lic.get("id") {
                    out.push(id.clone());
                }
            }
            for child in map.values() {
                collect_license_ids(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collect_license_ids(i, out)),
        _ => {}
    }
}

#[test]
fn issue_1410_every_cdx_license_id_is_on_the_spdx_list() {
    let tmp = fixture();
    let (cdx, _, _) = scan(tmp.path());
    let mut ids = Vec::new();
    collect_license_ids(&cdx, &mut ids);
    // Positive control: the walk must find ids, or "none unlisted" is vacuous.
    assert!(ids.iter().any(|i| i == "BSD-3-Clause"), "walk found no ids: {ids:?}");
    for id in &ids {
        let listed = spdx::license_id(id).is_some_and(|l| l.name == id)
            || spdx::exception_id(id).is_some_and(|x| x.name == id);
        assert!(listed, "license.id {id:?} is not an SPDX License List identifier");
    }
}

#[test]
fn issue_1410_or_later_takes_the_expression_slot_in_cdx_and_matches_spdx() {
    let tmp = fixture();
    let (cdx, spdx23, spdx3) = scan(tmp.path());

    assert_eq!(
        cdx_component(&cdx, "waybill-fixture-plus")["licenses"],
        serde_json::json!([{"expression": "Apache-2.0+", "acknowledgement": "declared"}])
    );
    assert_eq!(
        cdx_component(&cdx, "waybill-fixture-plusand")["licenses"],
        serde_json::json!([{"expression": "EUPL-1.1+ AND MIT", "acknowledgement": "declared"}])
    );

    // The same strings the SPDX emitters write for the same declarations.
    assert_eq!(spdx23_declared(&spdx23, "waybill-fixture-plus"), "Apache-2.0+");
    assert_eq!(spdx23_declared(&spdx23, "waybill-fixture-plusand"), "EUPL-1.1+ AND MIT");
    let v3 = spdx3_expressions(&spdx3);
    assert!(v3.iter().any(|e| e == "Apache-2.0+"), "{v3:?}");
    assert!(v3.iter().any(|e| e == "EUPL-1.1+ AND MIT"), "{v3:?}");
}

#[test]
fn issue_1411_spdx_emitters_bracket_an_or_operand_before_joining() {
    let tmp = fixture();
    let (cdx, spdx23, spdx3) = scan(tmp.path());

    let want = "(MIT OR Apache-2.0) AND BSD-3-Clause";
    assert_eq!(spdx23_declared(&spdx23, "waybill-fixture-multi"), want);
    let v3 = spdx3_expressions(&spdx3);
    assert!(v3.iter().any(|e| e == want), "SPDX 3 expressions: {v3:?}");
    assert!(
        !v3.iter().any(|e| e == "MIT OR Apache-2.0 AND BSD-3-Clause"),
        "unbracketed join still present: {v3:?}"
    );

    // CycloneDX does not join: one entry per declared license.
    let entries = cdx_component(&cdx, "waybill-fixture-multi")["licenses"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_eq!(entries[0]["license"]["name"], "MIT OR Apache-2.0");
    assert_eq!(entries[1]["license"]["id"], "BSD-3-Clause");
}
