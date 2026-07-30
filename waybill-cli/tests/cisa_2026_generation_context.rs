//! Feature 221 US3 — document-scope SBOM Generation Context in
//! SPDX 2.3 + SPDX 3 (FR-010, FR-011) + CISA-vocabulary alias in all
//! three formats (FR-012).
//!
//! Every scan gets a doc-scope `waybill:generation-context` (C21,
//! pre-existing) PLUS a `waybill:cisa-2026-lifecycle` alias (C141,
//! new in US3). The alias maps waybill's variant to CISA's
//! `before-build` / `build` / `after-build` vocabulary.
//!
//! Scope of US3 assertion: for a filesystem scan (the common case),
//! `waybill:cisa-2026-lifecycle` MUST equal `"after-build"` in every
//! emitted format.

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

fn run_scan_multi(tmp: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
    let cdx = tmp.join("scan.cdx.json");
    let spdx23 = tmp.join("scan.spdx.json");
    let spdx3 = tmp.join("scan.spdx3.json");
    let status = Command::new(bin())
        .arg("--offline")
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
        .arg("--no-deep-hash")
        .status()
        .expect("waybill invocation");
    assert!(status.success(), "scan failed");
    (cdx, spdx23, spdx3)
}

fn parse_json(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).expect("read");
    serde_json::from_slice(&bytes).expect("parse json")
}

// ---------------------------------------------------------------------------
// FR-010 — SPDX 2.3 doc-scope Annotation on SPDXRef-DOCUMENT
// ---------------------------------------------------------------------------

#[test]
fn us3_spdx23_carries_doc_scope_generation_context_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, spdx23, _) = run_scan_multi(tmp.path());
    let doc = parse_json(&spdx23);

    // The waybill:generation-context annotation is emitted at
    // document scope. Find the annotation whose MikebomAnnotationCommentV1
    // envelope's `field` matches.
    let annos = doc["annotations"].as_array().expect("annotations array on SPDX 2.3 doc");
    let found = annos.iter().any(|a| {
        a["comment"]
            .as_str()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
            .map(|env| env["field"] == "waybill:generation-context")
            .unwrap_or(false)
    });
    assert!(
        found,
        "SPDX 2.3 doc-scope annotations MUST include waybill:generation-context; got: {annos:?}"
    );
}

#[test]
fn us3_spdx23_carries_cisa_2026_lifecycle_alias() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, spdx23, _) = run_scan_multi(tmp.path());
    let doc = parse_json(&spdx23);

    let annos = doc["annotations"].as_array().expect("annotations");
    let alias = annos.iter().find_map(|a| {
        let env: serde_json::Value =
            serde_json::from_str(a["comment"].as_str().unwrap_or("")).ok()?;
        if env["field"] == "waybill:cisa-2026-lifecycle" {
            Some(env["value"].clone())
        } else {
            None
        }
    });
    assert_eq!(
        alias,
        Some(serde_json::json!("after-build")),
        "SPDX 2.3 waybill:cisa-2026-lifecycle MUST be 'after-build' for a filesystem scan; got: {alias:?}"
    );
}

// ---------------------------------------------------------------------------
// FR-011 — SPDX 3 doc-scope Annotation on SpdxDocument
// ---------------------------------------------------------------------------

#[test]
fn us3_spdx3_carries_doc_scope_generation_context_annotation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, _, spdx3) = run_scan_multi(tmp.path());
    let doc = parse_json(&spdx3);

    let graph = doc["@graph"].as_array().expect("@graph array");
    let found = graph.iter().any(|el| {
        if el["type"] != "Annotation" {
            return false;
        }
        el["statement"]
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .map(|env| env["field"] == "waybill:generation-context")
            .unwrap_or(false)
    });
    assert!(
        found,
        "SPDX 3 @graph MUST include an Annotation element carrying waybill:generation-context"
    );
}

#[test]
fn us3_spdx3_carries_cisa_2026_lifecycle_alias() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_, _, spdx3) = run_scan_multi(tmp.path());
    let doc = parse_json(&spdx3);

    let graph = doc["@graph"].as_array().expect("@graph");
    let alias = graph.iter().find_map(|el| {
        if el["type"] != "Annotation" {
            return None;
        }
        let env: serde_json::Value =
            serde_json::from_str(el["statement"].as_str().unwrap_or("")).ok()?;
        if env["field"] == "waybill:cisa-2026-lifecycle" {
            Some(env["value"].clone())
        } else {
            None
        }
    });
    assert_eq!(
        alias,
        Some(serde_json::json!("after-build")),
        "SPDX 3 waybill:cisa-2026-lifecycle MUST be 'after-build' for filesystem scan; got: {alias:?}"
    );
}

// ---------------------------------------------------------------------------
// FR-012 courtesy — CDX also emits the alias as a metadata property
// ---------------------------------------------------------------------------

#[test]
fn us3_cdx_carries_cisa_2026_lifecycle_property() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (cdx, _, _) = run_scan_multi(tmp.path());
    let doc = parse_json(&cdx);
    let props = doc["metadata"]["properties"]
        .as_array()
        .expect("metadata.properties[]");
    let value = props.iter().find_map(|p| {
        if p["name"] == "waybill:cisa-2026-lifecycle" {
            Some(p["value"].clone())
        } else {
            None
        }
    });
    assert_eq!(
        value,
        Some(serde_json::json!("after-build")),
        "CDX metadata.properties[] MUST include waybill:cisa-2026-lifecycle for FR-012 vocab parity"
    );
}
