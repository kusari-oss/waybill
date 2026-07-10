//! Milestone 179 US3 — end-to-end integration tests for the Cargo
//! optional-dep classifier. Scans the m179 fixture
//! (`tests/fixtures/optional_dep/cargo`) via the compiled `mikebom`
//! binary and asserts:
//!
//! - **T026** (Full mode): SPDX 2.3 emits `once_cell
//!   OPTIONAL_DEPENDENCY_OF <root>` (reversed direction per m052);
//!   CDX emits `once_cell` with `scope: "excluded"`; both formats
//!   carry the `mikebom:optional-derivation = "cargo-optional-true"`
//!   annotation; SPDX 3 carries the annotation (no native
//!   `lifecycleScope: "optional"` per FR-017).
//!
//! - **T027** (Basic mode): under
//!   `--spdx2-relationship-compat=basic`, SPDX 2.3 has zero
//!   `OPTIONAL_DEPENDENCY_OF` edges — every dep collapses to
//!   natural-direction `DEPENDS_ON` per m228; the annotation IS still
//!   present (annotation is orthogonal to relationship-compat).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/cargo")
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
        // The SPDX 2.3 wire-format key is `comment`, not
        // `annotationComment` (though the spec allows both — mikebom
        // emits `comment`).
        let comment = anno
            .get("comment")
            .or_else(|| anno.get("annotationComment"))
            .and_then(|v| v.as_str())?;
        // MikebomAnnotationCommentV1 envelope: JSON-encoded string.
        let parsed: Value = serde_json::from_str(comment).ok()?;
        let field = parsed.get("field").or(parsed.get("name")).and_then(|v| v.as_str())?;
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
fn t026_cargo_optional_full_mode_end_to_end() {
    let (cdx, spdx23, spdx3) = run_scan(&fixture_path(), &[]);

    // --- CDX 1.6 assertions ---
    let once_cell_cdx =
        find_component_by_name(&cdx, "once_cell").expect("once_cell present in CDX");
    // FR-006 / FR-016 — native CDX signal is `scope: "excluded"`,
    // auto-emitted via LifecycleScope::Optional's is_non_runtime().
    assert_eq!(
        once_cell_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX MUST emit `scope: \"excluded\"` on once_cell (Optional -> is_non_runtime)"
    );
    // FR-019 — annotation carries derivation source.
    assert_eq!(
        find_property(once_cell_cdx, "mikebom:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("cargo-optional-true"),
        "CDX MUST carry mikebom:optional-derivation on once_cell"
    );

    // --- SPDX 2.3 assertions (Full mode is default) ---
    // Locate the relationship: `once_cell OPTIONAL_DEPENDENCY_OF <root>`.
    // The reversed-direction convention (m052 + m179 T007) means
    // once_cell is the `spdxElementId` (source) and the root is the
    // `relatedSpdxElement` (target).
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
        .expect("SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge in Full mode");
    // Source SPDXID should map to once_cell.
    let once_cell_spdx =
        find_package_by_name(&spdx23, "once_cell").expect("once_cell package in SPDX 2.3");
    let once_cell_id = once_cell_spdx.get("SPDXID").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        opt_dep_of.get("spdxElementId").and_then(|v| v.as_str()),
        Some(once_cell_id),
        "reversed-direction: once_cell SPDXID MUST be the source (spdxElementId) of OPTIONAL_DEPENDENCY_OF"
    );
    // Annotation on the Package.
    assert_eq!(
        find_annotation_value(once_cell_spdx, "mikebom:optional-derivation"),
        Some("cargo-optional-true".to_string()),
        "SPDX 2.3 MUST carry mikebom:optional-derivation on once_cell Package"
    );

    // --- SPDX 3.0.1 assertions ---
    // No native lifecycleScope="optional" per FR-017; annotation
    // carries the classification instead.
    let elements = spdx3
        .get("@graph")
        .and_then(|v| v.as_array())
        .expect("SPDX 3 @graph present");
    let once_cell_elem = elements
        .iter()
        .find(|e| {
            e.get("name").and_then(|v| v.as_str()) == Some("once_cell")
                && e.get("type").and_then(|v| v.as_str()) == Some("software_Package")
        })
        .expect("once_cell software_Package in SPDX 3");
    let once_cell_iri = once_cell_elem.get("spdxId").and_then(|v| v.as_str()).unwrap();
    // Find any Annotation subject-linked to once_cell.
    let annotations: Vec<&Value> = elements
        .iter()
        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("Annotation"))
        .filter(|e| e.get("subject").and_then(|v| v.as_str()) == Some(once_cell_iri))
        .collect();
    let has_optional_deriv = annotations.iter().any(|a| {
        a.get("statement")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("mikebom:optional-derivation") && s.contains("cargo-optional-true"))
            .unwrap_or(false)
    });
    assert!(
        has_optional_deriv,
        "SPDX 3 MUST carry a mikebom:optional-derivation Annotation on once_cell software_Package"
    );
    // Confirm no LifecycleScopedRelationship with scope="optional" was
    // emitted (FR-017 — SPDX 3.0.1 has no such enum value).
    for e in elements {
        if e.get("type").and_then(|v| v.as_str()) == Some("LifecycleScopedRelationship") {
            let scope = e.get("scope").and_then(|v| v.as_str()).unwrap_or("");
            assert_ne!(
                scope, "optional",
                "SPDX 3 MUST NOT emit LifecycleScopedRelationship with scope=optional (FR-017)"
            );
        }
    }
}

#[test]
fn t027_cargo_optional_basic_mode_collapses() {
    // FR-003 / SC-006 — under --spdx2-relationship-compat=basic, ALL
    // typed dep-scope edges collapse to natural-direction DEPENDS_ON.
    let (cdx, spdx23, _) = run_scan(&fixture_path(), &["--spdx2-relationship-compat=basic"]);

    // CDX side is unchanged in basic mode — still emits
    // `scope: "excluded"` on once_cell (the flag only affects SPDX 2.3).
    let once_cell_cdx =
        find_component_by_name(&cdx, "once_cell").expect("once_cell present in CDX");
    assert_eq!(
        once_cell_cdx.get("scope").and_then(|v| v.as_str()),
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
        "SPDX 2.3 basic mode MUST NOT emit any OPTIONAL_DEPENDENCY_OF edges (m228 escape hatch)"
    );

    // The annotation IS still present — annotation is orthogonal.
    let once_cell_spdx =
        find_package_by_name(&spdx23, "once_cell").expect("once_cell package in SPDX 2.3");
    assert_eq!(
        find_annotation_value(once_cell_spdx, "mikebom:optional-derivation"),
        Some("cargo-optional-true".to_string()),
        "mikebom:optional-derivation annotation MUST be present in basic mode too"
    );
}
