//! Feature 221 US4 — `--sbom-version <N>` flag threads the caller-
//! supplied SBOM document version into all three emitters per
//! FR-013 / FR-014.
//!
//! Coverage:
//! - CDX `metadata.version` = N when set; = 1 (default) when unset.
//! - CDX `metadata.properties[]` contains `waybill:sbom-version=<N>`
//!   (as string per CDX 1.6 property-value shape) when set; absent
//!   when unset (byte-identity preserved).
//! - SPDX 2.3 doc-scope `Annotation` on `SPDXRef-DOCUMENT` carries
//!   `waybill:sbom-version` when set; absent when unset.
//! - SPDX 3 doc-scope `Annotation` on `SpdxDocument` root IRI same.
//! - Non-integer / < 1 values rejected at CLI parse (exit 2).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::{bin, workspace_root};

fn scan_target() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let base = PathBuf::from(home)
            .join(".cache")
            .join("waybill")
            .join("fixtures");
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("transitive_parity").join("cargo");
                if candidate.join("Cargo.toml").exists() {
                    return candidate;
                }
            }
        }
    }
    workspace_root()
}

fn run_scan_multi(tmp: &std::path::Path, extra: &[&str]) -> (PathBuf, PathBuf, PathBuf) {
    let cdx = tmp.join("scan.cdx.json");
    let spdx23 = tmp.join("scan.spdx.json");
    let spdx3 = tmp.join("scan.spdx3.json");
    let mut cmd = Command::new(bin());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target())
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3.display()))
        .arg("--no-deep-hash");
    for a in extra {
        cmd.arg(a);
    }
    let status = cmd.status().expect("waybill invocation");
    assert!(status.success(), "scan failed");
    (cdx, spdx23, spdx3)
}

fn parse_json(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).expect("read");
    serde_json::from_slice(&bytes).expect("parse json")
}

// ---------------------------------------------------------------------------
// FR-013 — value threading, CDX native slot
// ---------------------------------------------------------------------------

#[test]
fn us4_cdx_metadata_version_is_native_integer_when_set() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (cdx, _, _) = run_scan_multi(tmp.path(), &["--sbom-version=7"]);
    let doc = parse_json(&cdx);
    assert_eq!(
        doc["version"], serde_json::json!(7),
        "CDX metadata.version MUST equal --sbom-version integer"
    );
}

#[test]
fn us4_cdx_metadata_version_defaults_to_1_when_unset() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (cdx, _, _) = run_scan_multi(tmp.path(), &[]);
    let doc = parse_json(&cdx);
    assert_eq!(
        doc["version"], serde_json::json!(1),
        "CDX metadata.version MUST be 1 when --sbom-version is unset (FR-009 byte-identity)"
    );

    // Parity check: waybill:sbom-version property is NOT present in the
    // default path.
    let props = doc["metadata"]["properties"]
        .as_array()
        .expect("metadata.properties[]");
    let has_key = props.iter().any(|p| p["name"] == "waybill:sbom-version");
    assert!(
        !has_key,
        "waybill:sbom-version property MUST be absent when --sbom-version is unset"
    );
}

#[test]
fn us4_cdx_carries_waybill_sbom_version_property_when_set() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (cdx, _, _) = run_scan_multi(tmp.path(), &["--sbom-version=42"]);
    let doc = parse_json(&cdx);
    let props = doc["metadata"]["properties"]
        .as_array()
        .expect("metadata.properties[]");
    let value = props.iter().find_map(|p| {
        if p["name"] == "waybill:sbom-version" {
            Some(p["value"].clone())
        } else {
            None
        }
    });
    assert_eq!(
        value,
        Some(serde_json::json!("42")),
        "CDX metadata.properties[waybill:sbom-version] MUST equal '42' (string) when --sbom-version=42"
    );
}

// ---------------------------------------------------------------------------
// FR-013 — SPDX 2.3 annotation
// ---------------------------------------------------------------------------

#[test]
fn us4_spdx23_carries_waybill_sbom_version_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, spdx23, _) = run_scan_multi(tmp.path(), &["--sbom-version=2"]);
    let doc = parse_json(&spdx23);
    let annos = doc["annotations"].as_array().expect("annotations");
    let val = annos.iter().find_map(|a| {
        let env: serde_json::Value =
            serde_json::from_str(a["comment"].as_str().unwrap_or("")).ok()?;
        if env["field"] == "waybill:sbom-version" {
            Some(env["value"].clone())
        } else {
            None
        }
    });
    // MikebomAnnotationCommentV1 coerces scalar values to strings per
    // `coerce_envelope_value` — so the numeric 2 lands as "2".
    assert_eq!(
        val,
        Some(serde_json::json!("2")),
        "SPDX 2.3 doc-scope waybill:sbom-version annotation MUST equal '2' (envelope-coerced)"
    );
}

#[test]
fn us4_spdx23_omits_annotation_when_flag_unset() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, spdx23, _) = run_scan_multi(tmp.path(), &[]);
    let doc = parse_json(&spdx23);
    let annos = doc["annotations"].as_array().expect("annotations");
    let has_key = annos.iter().any(|a| {
        serde_json::from_str::<serde_json::Value>(a["comment"].as_str().unwrap_or(""))
            .map(|env| env["field"] == "waybill:sbom-version")
            .unwrap_or(false)
    });
    assert!(
        !has_key,
        "SPDX 2.3 MUST NOT emit waybill:sbom-version annotation when --sbom-version is unset (FR-009)"
    );
}

// ---------------------------------------------------------------------------
// FR-013 — SPDX 3 annotation
// ---------------------------------------------------------------------------

#[test]
fn us4_spdx3_carries_waybill_sbom_version_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, _, spdx3) = run_scan_multi(tmp.path(), &["--sbom-version=5"]);
    let doc = parse_json(&spdx3);
    let graph = doc["@graph"].as_array().expect("@graph");
    let val = graph.iter().find_map(|el| {
        if el["type"] != "Annotation" {
            return None;
        }
        let env: serde_json::Value =
            serde_json::from_str(el["statement"].as_str().unwrap_or("")).ok()?;
        if env["field"] == "waybill:sbom-version" {
            Some(env["value"].clone())
        } else {
            None
        }
    });
    // MikebomAnnotationCommentV1 coerces scalar values to strings.
    assert_eq!(
        val,
        Some(serde_json::json!("5")),
        "SPDX 3 doc-scope waybill:sbom-version annotation MUST equal '5' (envelope-coerced)"
    );
}

// ---------------------------------------------------------------------------
// FR-014 — invalid values rejected at CLI parse
// ---------------------------------------------------------------------------

#[test]
fn us4_invalid_values_rejected_at_parse() {
    // Parameterized reject cases per FR-014.
    let cases: &[(&str, &str)] = &[
        ("0", "must be >= 1"),
        ("2.0", "positive integer"),
        ("v2", "positive integer"),
        ("latest", "positive integer"),
        ("", "positive integer"),
    ];
    let tmp = tempfile::tempdir().expect("tempdir");
    for (bad, expected_msg_fragment) in cases {
        let output = Command::new(bin())
            .arg("--offline")
            .arg("sbom")
            .arg("scan")
            .arg("--path")
            .arg(scan_target())
            .arg("--format")
            .arg("cyclonedx-json")
            .arg("--output")
            .arg(tmp.path().join("scan.cdx.json"))
            .arg(format!("--sbom-version={bad}"))
            .arg("--no-deep-hash")
            .output()
            .expect("waybill invocation");
        assert!(
            !output.status.success(),
            "--sbom-version={bad:?} MUST be rejected but scan succeeded"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected_msg_fragment),
            "reject diagnostic for {bad:?} MUST contain {expected_msg_fragment:?}; got: {stderr}"
        );
    }
}

// ---------------------------------------------------------------------------
// SC-007 — unsigned goldens preserved on the default path
// ---------------------------------------------------------------------------

#[test]
fn us4_default_path_leaves_cdx_metadata_version_unchanged() {
    // Belt-and-suspenders: this test PLUS the workspace-wide golden
    // regression suite together guarantee FR-009 byte-identity for
    // the --sbom-version=None default path.
    let tmp = tempfile::tempdir().expect("tempdir");
    let (cdx, _, _) = run_scan_multi(tmp.path(), &[]);
    let doc = parse_json(&cdx);
    assert_eq!(doc["version"], serde_json::json!(1));
}
