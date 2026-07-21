//! Milestone 181 US1 — end-to-end integration tests for the yarn v1
//! optional-dep classifier. Scans the m181 fixture
//! (`tests/fixtures/optional_dep/yarn-v1/`) via the compiled `waybill`
//! binary and asserts the same shape m180 established for npm/pnpm.
//!
//! Contract references:
//! - `specs/181-yarn-optional-dep/contracts/yarn-classifier-extension.md`
//! - `specs/181-yarn-optional-dep/spec.md` FR-001, FR-002, FR-008..FR-011
//! - Success criteria SC-001, SC-006

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/yarn-v1")
}

fn run_scan(project_root: &Path, extra_args: &[&str]) -> (Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
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
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_path.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()));
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
    (cdx, spdx23)
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

#[test]
fn t011_yarn_v1_optional_full_mode_end_to_end() {
    let (cdx, spdx23) = run_scan(&fixture_path(), &[]);

    // ---- CDX 1.6 ----
    let optional_child = find_component_by_name(&cdx, "optional-child-lib")
        .expect("optional-child-lib component in CDX");
    assert_eq!(
        optional_child.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "yarn v1 optional-classified optional-child-lib MUST emit CDX scope: \"excluded\""
    );
    assert_eq!(
        find_property(optional_child, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "yarn v1 optional-classified optional-child-lib MUST carry waybill:optional-derivation"
    );

    // Regression guard: runtime-util (runtime) MUST stay Runtime.
    let runtime = find_component_by_name(&cdx, "runtime-util")
        .expect("runtime-util component in CDX");
    assert!(
        runtime.get("scope").and_then(|v| v.as_str()) != Some("excluded"),
        "runtime-util MUST NOT be marked excluded (regular runtime dep)"
    );
    assert!(
        find_property(runtime, "waybill:optional-derivation").is_none(),
        "runtime-util MUST NOT carry waybill:optional-derivation"
    );

    // ---- SPDX 2.3 (Full mode) ----
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let has_optional_dep_of = rels.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str()) == Some("OPTIONAL_DEPENDENCY_OF")
    });
    assert!(
        has_optional_dep_of,
        "SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge under Full mode"
    );
}

#[test]
fn t011b_yarn_v1_optional_basic_mode_collapses() {
    // FR-010 / SC-006 — under --spdx2-relationship-compat=basic, ALL
    // typed dep-scope edges collapse to natural-direction DEPENDS_ON
    // per m228's contract. CDX emission is INDEPENDENT of the compat
    // flag; annotation is orthogonal.
    let (cdx, spdx23) = run_scan(&fixture_path(), &["--spdx2-relationship-compat=basic"]);

    // CDX side unchanged — scope: "excluded" still emitted.
    let optional_child = find_component_by_name(&cdx, "optional-child-lib")
        .expect("optional-child-lib component in CDX");
    assert_eq!(
        optional_child.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "CDX scope emission is INDEPENDENT of --spdx2-relationship-compat"
    );

    // Annotation still present (orthogonal).
    assert!(
        find_property(optional_child, "waybill:optional-derivation").is_some(),
        "waybill:optional-derivation MUST be present in CDX regardless of compat mode"
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
}
