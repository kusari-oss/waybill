//! Milestone 181 US2 — end-to-end integration tests for the yarn
//! Berry `dependenciesMeta.<name>.optional` classifier.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/yarn-berry")
}

/// Milestone 1272 US3 — the REALISTIC Berry shape: a manifest with no
/// `dependenciesMeta` key, the flag on the lockfile's workspace entry.
///
/// `fixture_path()` above is the m181 fixture, which hand-writes
/// `dependenciesMeta` into package.json. That is a valid source and is
/// kept, but FR-013 forbids it being this arm's only evidence: measured,
/// `yarn install` leaves no such key in the manifest, so the m181
/// fixture is the one shape a real project does not have.
fn measured_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/yarn_berry_measured")
}

fn run_scan(project_root: &Path) -> (Value, Value) {
    let out_dir = tempfile::tempdir().unwrap();
    let cdx_path = out_dir.path().join("out.cdx.json");
    let spdx23_path = out_dir.path().join("out.spdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
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
        .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
        .output()
        .unwrap();
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
fn t017_yarn_berry_optional_full_mode_end_to_end() {
    let (cdx, spdx23) = run_scan(&fixture_path());

    let optional_lib = find_component_by_name(&cdx, "optional-lib")
        .expect("optional-lib component in CDX");
    assert_eq!(
        optional_lib.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "Berry-classified Optional MUST emit CDX scope: \"excluded\""
    );
    assert_eq!(
        find_property(optional_lib, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "Berry-classified Optional MUST carry waybill:optional-derivation"
    );

    // Regression guard: runtime-util (regular runtime dep) stays Runtime.
    let runtime = find_component_by_name(&cdx, "runtime-util")
        .expect("runtime-util component in CDX");
    assert!(
        runtime.get("scope").and_then(|v| v.as_str()) != Some("excluded"),
        "runtime-util MUST NOT be marked excluded"
    );

    // SPDX 2.3 (Full mode default) — expect at least one OPTIONAL_DEPENDENCY_OF.
    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    let has_optional_dep_of = rels.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str()) == Some("OPTIONAL_DEPENDENCY_OF")
    });
    assert!(
        has_optional_dep_of,
        "SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge for Berry fixture"
    );
}

// ----------------------------------------------------------------------
// Milestone 1272 US3 — end-to-end against the measured Berry shape.
// ----------------------------------------------------------------------

/// The four classification cases through the full emission path, from a
/// manifest that carries no optional information at all.
#[test]
fn m1272_berry_measured_shape_classifies_every_case() {
    let (cdx, spdx23) = run_scan(&measured_fixture_path());

    let optional = find_component_by_name(&cdx, "waybill-fixture-optional")
        .expect("optional component in CDX");
    assert_eq!(
        optional.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "the lockfile workspace entry's dependenciesMeta MUST classify, \
         with no manifest dependenciesMeta present at all (FR-004)"
    );
    assert_eq!(
        find_property(optional, "waybill:optional-derivation").and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "FR-008: the newly Optional component MUST carry the derivation"
    );

    assert_eq!(
        find_component_by_name(&cdx, "waybill-fixture-transitive")
            .expect("transitive component in CDX")
            .get("scope")
            .and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-007a: transitive propagation, which the manifest source can \
         never deliver because a manifest describes only the root"
    );

    assert_ne!(
        find_component_by_name(&cdx, "waybill-fixture-runtime")
            .expect("runtime component in CDX")
            .get("scope")
            .and_then(|v| v.as_str()),
        Some("excluded"),
        "a `dependencies` declaration MUST NOT be optional"
    );

    let contested = find_component_by_name(&cdx, "waybill-fixture-contested")
        .expect("contested component in CDX");
    assert_ne!(
        contested.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-006: declared optional at the root but reached via the \
         runtime parent's hard edge (measured in Probe C)"
    );
    assert!(
        find_property(contested, "waybill:optional-derivation").is_none(),
        "a cancelled component MUST NOT carry the derivation annotation"
    );

    let mut excluded: Vec<&str> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .expect("components array")
        .iter()
        .filter(|c| c.get("scope").and_then(|v| v.as_str()) == Some("excluded"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    excluded.sort_unstable();
    assert_eq!(
        excluded,
        ["waybill-fixture-optional", "waybill-fixture-transitive"],
        "exactly the optional closure is optional"
    );

    let rels = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");
    assert!(
        rels.iter().any(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("OPTIONAL_DEPENDENCY_OF")
        }),
        "SPDX 2.3 MUST emit OPTIONAL_DEPENDENCY_OF for the measured fixture"
    );
}
