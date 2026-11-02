//! Milestone 1272 US2 — end-to-end integration tests for the bun
//! `bun.lock` optional-dep classifier. Same shape as the m180/m1272
//! pnpm e2e (`optional_dep_pnpm_e2e.rs`).
//!
//! The fixture is in the shape bun actually produces, captured on bun
//! 1.4.2 and rewritten with synthetic names. Measured, bun records only
//! the declaration and the edge — a contested graph and an
//! optional-only graph produce a byte-identical `packages` map — so
//! every assertion here is about a closure waybill computes itself.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/bun_measured")
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
    let spdx23: Value =
        serde_json::from_slice(&std::fs::read(&spdx23_path).unwrap()).unwrap();
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

/// The five classification cases through the full emission path.
///
/// This is the test that would have failed before milestone 1272: a
/// root `optionalDependencies` declaration lives in the `workspaces`
/// map, and m667's tracker keys through `parent_key_to_out_idx`, which
/// excludes workspace shapes — so the declaration was structurally
/// invisible and bun classified nothing optional.
#[test]
fn m1272_bun_measured_shape_classifies_every_case() {
    let (cdx, spdx23) = run_scan(&fixture_path());

    // --- the declared optional dependency ---
    let optional = find_component_by_name(&cdx, "waybill-fixture-optional")
        .expect("optional component in CDX");
    assert_eq!(
        optional.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "a root optionalDependencies declaration MUST emit CDX scope \"excluded\" (FR-003)"
    );
    assert_eq!(
        find_property(optional, "waybill:optional-derivation").and_then(|v| v.as_str()),
        Some("bun-optional-dependencies"),
        "FR-008: the newly Optional component MUST carry the derivation"
    );

    // --- transitive: optional although nothing declares it so ---
    assert_eq!(
        find_component_by_name(&cdx, "waybill-fixture-transitive")
            .expect("transitive component in CDX")
            .get("scope")
            .and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-007a: reachable only through an optional parent's hard edge \
         MUST be optional"
    );

    // --- the optional peer keeps its own derivation string ---
    let peerish = find_component_by_name(&cdx, "waybill-fixture-peerish")
        .expect("peerish component in CDX");
    assert_eq!(
        peerish.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "an optional peer of an optional parent is optional (Probe D p3)"
    );
    assert_eq!(
        find_property(peerish, "waybill:optional-derivation").and_then(|v| v.as_str()),
        Some("bun-optional-peers"),
        "m667's -peers derivation MUST be reachable and preserved (FR-009)"
    );

    // --- the runtime dependency is untouched ---
    assert_ne!(
        find_component_by_name(&cdx, "waybill-fixture-runtime")
            .expect("runtime component in CDX")
            .get("scope")
            .and_then(|v| v.as_str()),
        Some("excluded"),
        "a `dependencies` declaration MUST NOT be optional"
    );

    // --- contested: declared optional at the root, reached at runtime ---
    let contested = find_component_by_name(&cdx, "waybill-fixture-contested")
        .expect("contested component in CDX");
    assert_ne!(
        contested.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-006: a runtime path cancels optionality even against a root \
         optional declaration (measured in Probe C)"
    );
    assert!(
        find_property(contested, "waybill:optional-derivation").is_none(),
        "a cancelled component MUST NOT carry the derivation annotation"
    );

    // --- SC-001: exactly the optional closure, nothing more ---
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
        [
            "waybill-fixture-optional",
            "waybill-fixture-peerish",
            "waybill-fixture-transitive",
        ],
        "exactly the optional closure is optional"
    );

    // --- SPDX 2.3 carries the same claim natively ---
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
