//! Milestone 180 US1 — end-to-end integration tests for the npm
//! `package-lock.json` optional-dep classifier. Scans the m180 fixture
//! (`tests/fixtures/optional_dep/npm/`) via the compiled `waybill`
//! binary and asserts:
//!
//! - **T010** (Full mode): SPDX 2.3 emits `fsevents
//!   OPTIONAL_DEPENDENCY_OF <root>` (reversed direction per m052); CDX
//!   emits `fsevents` with `scope: "excluded"`; both formats carry the
//!   `waybill:optional-derivation = "npm-optional-dependencies"`
//!   annotation; `lodash` (regular runtime dep) stays Runtime with no
//!   `scope: "excluded"` (regression guard against over-classification).
//!
//! - **T011** (Basic mode): under `--spdx2-relationship-compat=basic`,
//!   SPDX 2.3 has zero `OPTIONAL_DEPENDENCY_OF` edges — every dep
//!   collapses to natural-direction `DEPENDS_ON` per m228; the
//!   annotation IS still present (annotation is orthogonal to
//!   relationship-compat).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/npm")
}

fn run_scan(project_root: &Path, extra_args: &[&str]) -> (Value, Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
    let spdx3_path = out_dir.path().join("out.spdx3.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_path.display()));
    for arg in extra_args {
        cmd.arg(arg);
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let cdx: Value = serde_json::from_slice(&std::fs::read(&cdx_path).unwrap()).unwrap();
    let spdx23: Value = serde_json::from_slice(&std::fs::read(&spdx23_path).unwrap()).unwrap();
    let spdx3: Value = serde_json::from_slice(&std::fs::read(&spdx3_path).unwrap()).unwrap();
    (cdx, spdx23, spdx3)
}

fn find_component_by_name<'a>(cdx: &'a Value, name: &str) -> Option<&'a Value> {
    cdx.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_property<'a>(component: &'a Value, name: &str) -> Option<&'a Value> {
    component
        .get("properties")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
}

fn find_package_by_name<'a>(spdx: &'a Value, name: &str) -> Option<&'a Value> {
    spdx.get("packages")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn find_annotation_value(pkg: &Value, key: &str) -> Option<String> {
    let annotations = pkg.get("annotations").and_then(|v| v.as_array())?;
    for anno in annotations {
        let comment = anno
            .get("comment")
            .or_else(|| anno.get("annotationComment"))
            .and_then(|v| v.as_str())?;
        let parsed: Value = serde_json::from_str(comment).ok()?;
        let field = parsed
            .get("field")
            .or(parsed.get("name"))
            .and_then(|v| v.as_str())?;
        if field == key {
            let value = parsed.get("value")?;
            return Some(match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        }
    }
    None
}

#[test]
fn t010_npm_optional_full_mode_end_to_end() {
    let (cdx, spdx23, spdx3) = run_scan(&fixture_path(), &[]);

    // ---- CDX 1.6 ----
    let fsevents_cdx =
        find_component_by_name(&cdx, "fsevents").expect("fsevents component in CDX");
    // FR-009 — auto-inherited scope: "excluded" via LifecycleScope::Optional -> is_non_runtime().
    assert_eq!(
        fsevents_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX MUST emit scope: \"excluded\" on fsevents"
    );
    // FR-005 — derivation annotation identifies the JavaScript ecosystem.
    assert_eq!(
        find_property(fsevents_cdx, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "CDX MUST carry waybill:optional-derivation = \"npm-optional-dependencies\""
    );
    // Regression guard — lodash MUST stay Runtime.
    let lodash_cdx = find_component_by_name(&cdx, "lodash").expect("lodash component in CDX");
    assert!(
        lodash_cdx.get("scope").is_none()
            || lodash_cdx.get("scope").and_then(|v| v.as_str()) == Some("required"),
        "lodash MUST NOT be marked excluded (runtime dep)"
    );
    assert!(
        find_property(lodash_cdx, "waybill:optional-derivation").is_none(),
        "lodash MUST NOT carry waybill:optional-derivation (runtime dep)"
    );

    // ---- SPDX 2.3 (Full mode default) ----
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let opt_dep_of = rels
        .iter()
        .find(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .expect("SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge under Full mode");
    let fsevents_spdx =
        find_package_by_name(&spdx23, "fsevents").expect("fsevents package in SPDX 2.3");
    let fsevents_id = fsevents_spdx.get("SPDXID").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        opt_dep_of.get("spdxElementId").and_then(|v| v.as_str()),
        Some(fsevents_id),
        "reversed-direction: fsevents SPDXID MUST be the source (spdxElementId) of OPTIONAL_DEPENDENCY_OF"
    );
    assert_eq!(
        find_annotation_value(fsevents_spdx, "waybill:optional-derivation"),
        Some("npm-optional-dependencies".to_string()),
        "SPDX 2.3 MUST carry waybill:optional-derivation on fsevents Package"
    );

    // ---- SPDX 3.0.1 ----
    // FR-010 — no native lifecycleScope: "optional" on 3.0.1; annotation only.
    let elements = spdx3
        .get("@graph")
        .and_then(|v| v.as_array())
        .expect("SPDX 3 @graph present");
    let fsevents_elem = elements
        .iter()
        .find(|e| {
            e.get("name").and_then(|v| v.as_str()) == Some("fsevents")
                && e.get("type").and_then(|v| v.as_str()) == Some("software_Package")
        })
        .expect("fsevents software_Package in SPDX 3");
    let fsevents_iri = fsevents_elem
        .get("spdxId")
        .and_then(|v| v.as_str())
        .unwrap();
    let has_optional_deriv = elements
        .iter()
        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("Annotation"))
        .filter(|e| e.get("subject").and_then(|v| v.as_str()) == Some(fsevents_iri))
        .any(|a| {
            a.get("statement")
                .and_then(|v| v.as_str())
                .map(|s| {
                    s.contains("waybill:optional-derivation")
                        && s.contains("npm-optional-dependencies")
                })
                .unwrap_or(false)
        });
    assert!(
        has_optional_deriv,
        "SPDX 3 MUST carry a waybill:optional-derivation Annotation on fsevents"
    );
    // Confirm no LifecycleScopedRelationship with scope="optional".
    for e in elements {
        if e.get("type").and_then(|v| v.as_str()) == Some("LifecycleScopedRelationship") {
            let scope = e.get("scope").and_then(|v| v.as_str()).unwrap_or("");
            assert_ne!(
                scope, "optional",
                "SPDX 3 MUST NOT emit LifecycleScopedRelationship with scope=optional (FR-010)"
            );
        }
    }
}

#[test]
fn t011_npm_optional_basic_mode_collapses() {
    // FR-011 / SC-005 — under --spdx2-relationship-compat=basic, ALL
    // typed dep-scope edges collapse to natural-direction DEPENDS_ON.
    let (cdx, spdx23, _) = run_scan(&fixture_path(), &["--spdx2-relationship-compat=basic"]);

    // CDX side is unchanged in basic mode.
    let fsevents_cdx =
        find_component_by_name(&cdx, "fsevents").expect("fsevents component in CDX");
    assert_eq!(
        fsevents_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX scope emission is INDEPENDENT of --spdx2-relationship-compat"
    );

    // SPDX 2.3 has zero OPTIONAL_DEPENDENCY_OF edges under basic mode.
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let optional_count = rels
        .iter()
        .filter(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        })
        .count();
    assert_eq!(
        optional_count, 0,
        "SPDX 2.3 basic mode MUST NOT emit any OPTIONAL_DEPENDENCY_OF edges (m228)"
    );

    // Annotation IS still present (orthogonal to relationship-compat).
    let fsevents_spdx =
        find_package_by_name(&spdx23, "fsevents").expect("fsevents package in SPDX 2.3");
    assert_eq!(
        find_annotation_value(fsevents_spdx, "waybill:optional-derivation"),
        Some("npm-optional-dependencies".to_string()),
        "waybill:optional-derivation annotation MUST be present in basic mode too"
    );
}
