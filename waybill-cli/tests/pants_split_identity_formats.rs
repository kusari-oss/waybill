//! Issue #914 (m912) — the resolve identity decodes to the same value in all
//! three formats.
//!
//! **Compare decoded values, not bytes.** CycloneDX spec'es
//! `properties[].value` as a string, so the array is carried as JSON-in-string
//! there, while SPDX 2.3 and SPDX 3 carry a real array in their annotation
//! envelopes. Encoding is each format's business; the decoded value is the
//! contract. This is the precedent C143 and C161 already set, and the trap
//! m911 fell into by asserting a native array against CycloneDX output.

use std::path::PathBuf;
use std::process::Command;

const IDENTITY: &str = "waybill:document-resolve";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Emit one repository in all three formats at once, so the comparison is
/// across formats of a single scan rather than across three scans.
fn split_all_formats(name: &str) -> PathBuf {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture(name).to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json,spdx-2.3-json,spdx-3-json",
            "--split=resolve",
            "--output-dir",
            dir.path().to_str().expect("outdir"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    let p = dir.path().to_path_buf();
    std::mem::forget(dir); // keep the tree alive for the test body
    p
}

fn read(path: &PathBuf) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("parse")
}

/// CycloneDX: JSON-in-string inside a doc-scope property.
fn decode_cdx(doc: &serde_json::Value) -> Option<Vec<String>> {
    let raw = doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(IDENTITY))?["value"]
        .as_str()?;
    serde_json::from_str(raw).ok()
}

/// SPDX 2.3: a real array inside the MikebomAnnotationCommentV1 envelope on
/// the document annotation.
fn decode_spdx23(doc: &serde_json::Value) -> Option<Vec<String>> {
    for a in doc["annotations"].as_array()? {
        let env: serde_json::Value = serde_json::from_str(a["comment"].as_str()?).ok()?;
        if env["field"].as_str() == Some(IDENTITY) {
            return serde_json::from_value(env["value"].clone()).ok();
        }
    }
    None
}

/// SPDX 3: the same envelope, in `Annotation.statement` on a graph element.
fn decode_spdx3(doc: &serde_json::Value) -> Option<Vec<String>> {
    for e in doc["@graph"].as_array()? {
        let Some(stmt) = e["statement"].as_str() else {
            continue;
        };
        let Ok(env) = serde_json::from_str::<serde_json::Value>(stmt) else {
            continue;
        };
        if env["field"].as_str() == Some(IDENTITY) {
            return serde_json::from_value(env["value"].clone()).ok();
        }
    }
    None
}

/// The cross-format contract (C-1). All three must decode to the same value.
#[test]
fn the_identity_decodes_identically_in_all_three_formats() {
    let dir = split_all_formats("pants_discovered_resolves");

    for (stem, expected) in [
        ("default.generic", vec!["python:default".to_string()]),
        ("lint.generic", vec!["python:lint".to_string()]),
    ] {
        let cdx = decode_cdx(&read(&dir.join(format!("{stem}.cdx.json"))));
        let s23 = decode_spdx23(&read(&dir.join(format!("{stem}.spdx.json"))));
        let s3 = decode_spdx3(&read(&dir.join(format!("{stem}.spdx3.json"))));

        assert_eq!(cdx.as_ref(), Some(&expected), "{stem}: CycloneDX");
        assert_eq!(s23.as_ref(), Some(&expected), "{stem}: SPDX 2.3");
        assert_eq!(s3.as_ref(), Some(&expected), "{stem}: SPDX 3");
    }
}

/// The encodings genuinely DIFFER, which is why the test above decodes rather
/// than diffing bytes. If this ever fails, one of the emitters has changed
/// carrier shape and the decode helpers above are hiding it.
#[test]
fn cyclonedx_carries_a_string_and_spdx_carries_an_array() {
    let dir = split_all_formats("pants_discovered_resolves");

    let cdx = read(&dir.join("default.generic.cdx.json"));
    let raw = cdx["metadata"]["properties"]
        .as_array()
        .expect("properties")
        .iter()
        .find(|p| p["name"].as_str() == Some(IDENTITY))
        .expect("identity property")["value"]
        .clone();
    assert!(raw.is_string(), "CycloneDX property values are strings");

    let s23 = read(&dir.join("default.generic.spdx.json"));
    let env: serde_json::Value = s23["annotations"]
        .as_array()
        .expect("annotations")
        .iter()
        .find_map(|a| {
            let e: serde_json::Value = serde_json::from_str(a["comment"].as_str()?).ok()?;
            (e["field"].as_str() == Some(IDENTITY)).then_some(e)
        })
        .expect("identity annotation");
    assert!(
        env["value"].is_array(),
        "SPDX 2.3 carries the array itself, not a string containing one"
    );
}
