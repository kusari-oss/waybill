//! Milestone 180 US4 — end-to-end regression guard for the m178/m180
//! peer-optional precedence rule (FR-006).
//!
//! **Contract**: when a dep is BOTH `peerDependencies.<name>` AND
//! `peerDependenciesMeta.<name>.optional = true`, m178's
//! `PROVIDED_DEPENDENCY_OF` classification MUST win over m180's
//! `OPTIONAL_DEPENDENCY_OF`. See:
//! - `specs/180-npm-optional-dep-reader/contracts/peer-precedence-guard.md`
//! - `specs/180-npm-optional-dep-reader/spec.md` FR-006 + US4
//! - `specs/180-npm-optional-dep-reader/spec.md` SC-007
//!
//! **Fixture**: `tests/fixtures/optional_dep/peer-optional/` — root
//! declares react as peer-optional; the lockfile entry carries BOTH
//! `peer: true` AND `optional: true`, exercising the m180 reader guard
//! path.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/peer-optional")
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

fn find_package_by_name<'a>(spdx: &'a Value, name: &str) -> Option<&'a Value> {
    spdx.get("packages")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
}

#[test]
fn t027_peer_optional_react_emits_provided_not_optional() {
    // FR-006 flagship: react is peer-optional; m178's
    // PROVIDED_DEPENDENCY_OF MUST win over m180's OPTIONAL_DEPENDENCY_OF.
    let (cdx, spdx23) = run_scan(&fixture_path());

    let react_spdx =
        find_package_by_name(&spdx23, "react").expect("react package present in SPDX 2.3");
    let react_spdx_id = react_spdx.get("SPDXID").and_then(|v| v.as_str()).unwrap();

    let relationships = spdx23
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("SPDX 2.3 has relationships");

    // React MUST appear as the source (spdxElementId) of at least one
    // PROVIDED_DEPENDENCY_OF edge — the m178 emission fires normally.
    let has_provided = relationships.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str())
            == Some("PROVIDED_DEPENDENCY_OF")
            && r.get("spdxElementId").and_then(|v| v.as_str()) == Some(react_spdx_id)
    });
    assert!(
        has_provided,
        "SC-007: peer-optional react MUST emit as PROVIDED_DEPENDENCY_OF (m178 wins per FR-006)"
    );

    // React MUST NOT appear as the source of any OPTIONAL_DEPENDENCY_OF
    // edge — m180's guard short-circuited the Optional classification.
    let has_optional = relationships.iter().any(|r| {
        r.get("relationshipType").and_then(|v| v.as_str())
            == Some("OPTIONAL_DEPENDENCY_OF")
            && r.get("spdxElementId").and_then(|v| v.as_str()) == Some(react_spdx_id)
    });
    assert!(
        !has_optional,
        "FR-006: peer-optional react MUST NOT emit as OPTIONAL_DEPENDENCY_OF (m178 wins over m180)"
    );

    // CDX side: react MUST NOT carry the m180 derivation annotation —
    // the reader guard short-circuited it.
    let react_cdx = find_component_by_name(&cdx, "react").expect("react component in CDX");
    assert!(
        find_property(react_cdx, "waybill:optional-derivation").is_none(),
        "FR-006: peer-optional react MUST NOT carry waybill:optional-derivation"
    );
    // React MUST NOT be marked `scope: "excluded"` either — its
    // lifecycle_scope stayed Runtime (the guard prevented Optional).
    assert!(
        react_cdx.get("scope").and_then(|v| v.as_str()) != Some("excluded"),
        "FR-006: peer-optional react MUST NOT be marked scope: \"excluded\" (lifecycle_scope stays Runtime)"
    );
}

#[test]
fn t028_some_lib_carries_peer_edge_targets_for_react() {
    // Sibling assertion — some-lib (the parent that declared react
    // as peer-optional) MUST still carry m147's
    // `waybill:peer-edge-targets` annotation listing react. This is
    // the unchanged m147/m178 behavior; m180 MUST NOT disturb it.
    let (cdx, _spdx23) = run_scan(&fixture_path());

    let some_lib = find_component_by_name(&cdx, "some-lib")
        .expect("some-lib component in CDX");
    let peer_targets_val = find_property(some_lib, "waybill:peer-edge-targets");
    assert!(
        peer_targets_val.is_some(),
        "some-lib MUST carry waybill:peer-edge-targets (unchanged m147 behavior)"
    );
    let peer_targets_str = peer_targets_val.unwrap().as_str().unwrap();
    assert!(
        peer_targets_str.contains("react"),
        "waybill:peer-edge-targets MUST list react (peer-declared): got {}",
        peer_targets_str
    );
}
