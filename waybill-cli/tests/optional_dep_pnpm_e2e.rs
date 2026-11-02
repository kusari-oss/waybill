//! Milestone 180 US2 — end-to-end integration tests for the pnpm
//! `pnpm-lock.yaml` v9 optional-dep classifier. Same shape as the m180
//! US1 npm e2e (`optional_dep_npm_e2e.rs`).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/optional_dep/pnpm")
}

/// Milestone 1272 — the fixture in the shape pnpm v9 ACTUALLY emits.
///
/// `fixture_path()` above is the m180 fixture: it declares
/// `lockfileVersion: '9.0'` but uses v6/v7 slash-prefixed keys and
/// repeats `optional: true` in `packages:`, which no real v9 lockfile
/// does. It is kept because other assertions depend on it, but FR-013
/// forbids it being an arm's only evidence.
fn measured_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/optional_dep/pnpm_v9_measured")
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
fn t017_pnpm_optional_full_mode_end_to_end() {
    let (cdx, spdx23) = run_scan(&fixture_path(), &[]);

    // ---- CDX 1.6 ----
    let fsevents_cdx =
        find_component_by_name(&cdx, "fsevents").expect("fsevents component in CDX");
    assert_eq!(
        fsevents_cdx.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "pnpm-classified Optional MUST emit CDX scope: \"excluded\""
    );
    assert_eq!(
        find_property(fsevents_cdx, "waybill:optional-derivation")
            .and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "pnpm-classified Optional MUST carry waybill:optional-derivation"
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
        "SPDX 2.3 MUST emit at least one OPTIONAL_DEPENDENCY_OF edge for the pnpm fixture"
    );
}

// ----------------------------------------------------------------------
// Milestone 1272 — end-to-end, against the measured v9 shape.
// ----------------------------------------------------------------------

/// T010 + T011 + T012 — the four classification cases in one scan,
/// asserted through the full emission path rather than at the reader.
///
/// This is the test that would have failed before milestone 1272: a
/// real-shape v9 lockfile classified nothing optional, because the
/// reader looked for the marker in the `packages:` entry and v9 keeps
/// it under `snapshots:`.
#[test]
fn m1272_pnpm_v9_measured_shape_classifies_the_four_cases() {
    let (cdx, spdx23) = run_scan(&measured_fixture_path(), &[]);

    // --- the declared optional dependency ---
    let optional = find_component_by_name(&cdx, "waybill-fixture-optional")
        .expect("optional component in CDX");
    assert_eq!(
        optional.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "a root optionalDependencies declaration MUST emit CDX scope \"excluded\" (FR-001)"
    );
    assert_eq!(
        find_property(optional, "waybill:optional-derivation").and_then(|v| v.as_str()),
        Some("npm-optional-dependencies"),
        "FR-008: the newly Optional component MUST carry the derivation"
    );

    // --- transitive: optional although nothing declares it so ---
    let transitive = find_component_by_name(&cdx, "waybill-fixture-transitive")
        .expect("transitive component in CDX");
    assert_eq!(
        transitive.get("scope").and_then(|v| v.as_str()),
        Some("excluded"),
        "FR-007a: reachable only through an optional edge MUST be optional"
    );

    // --- the runtime dependency is untouched ---
    let runtime = find_component_by_name(&cdx, "waybill-fixture-runtime")
        .expect("runtime component in CDX");
    assert_ne!(
        runtime.get("scope").and_then(|v| v.as_str()),
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
    let excluded: Vec<&str> = cdx
        .get("components")
        .and_then(|v| v.as_array())
        .expect("components array")
        .iter()
        .filter(|c| c.get("scope").and_then(|v| v.as_str()) == Some("excluded"))
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    let mut sorted = excluded.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        ["waybill-fixture-optional", "waybill-fixture-transitive"],
        "exactly the optional closure is optional -- got {excluded:?}"
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
