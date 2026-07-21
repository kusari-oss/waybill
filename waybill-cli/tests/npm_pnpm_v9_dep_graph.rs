//! Milestone 157 SC-008 integration + SC-002 monotonic-additive helper.
//!
//! - `pnpm_v9_synthetic_argo_cd_shape` (SC-008): synthesizes a 5-package
//!   v9 testbed with peer-dep suffixes + snapshots-only edges, invokes
//!   the release binary, and asserts the emitted CDX has the expected
//!   graph shape.
//! - `assert_monotonic_additive` (SC-002 helper): pure-function diff
//!   asserting every edge in an OLD CDX doc still appears in NEW.
//! - `monotonic_additive_helper_catches_missing_edge` (T009 self-test):
//!   proves the helper catches its own failure mode via
//!   `std::panic::catch_unwind`.
//! - `monotonic_additive_real_goldens_from_snapshot` (T010 Step-3):
//!   verifies the REAL pre-157 vs post-157 golden diff via
//!   WAYBILL_PRE157_SNAPSHOT_DIR env var. Gracefully skips when the
//!   snapshot dir isn't populated (post-merge CI).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn run_scan(project_root: &std::path::Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// SC-008 — synthesized argo-cd-shape testbed (peer-dep suffixes +
/// snapshots-only edges + one leaf).
#[test]
fn pnpm_v9_synthetic_argo_cd_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    std::fs::write(
        root.join("package.json"),
        r#"{
  "name": "argo-cd-shape-testbed",
  "version": "1.0.0",
  "dependencies": {
    "foo": "1.0.0",
    "@octokit/plugin-paginate-rest": "14.0.0"
  }
}
"#,
    )
    .unwrap();

    std::fs::write(
        root.join("pnpm-lock.yaml"),
        r#"lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      foo:
        specifier: 1.0.0
        version: 1.0.0
      '@octokit/plugin-paginate-rest':
        specifier: 14.0.0
        version: 14.0.0(@octokit/core@7.0.6)

packages:

  foo@1.0.0:
    resolution: {integrity: sha512-aaaaaaaa}
  bar@2.0.0:
    resolution: {integrity: sha512-bbbbbbbb}
  baz@3.0.0:
    resolution: {integrity: sha512-cccccccc}
  '@octokit/plugin-paginate-rest@14.0.0':
    resolution: {integrity: sha512-dddddddd}
  '@octokit/core@7.0.6':
    resolution: {integrity: sha512-eeeeeeee}

snapshots:

  foo@1.0.0:
    dependencies:
      bar: 2.0.0
    peerDependencies:
      baz: 3.0.0

  bar@2.0.0: {}

  baz@3.0.0: {}

  '@octokit/plugin-paginate-rest@14.0.0(@octokit/core@7.0.6)':
    dependencies:
      '@octokit/core': 7.0.6

  '@octokit/core@7.0.6': {}
"#,
    )
    .unwrap();

    let doc = run_scan(root);

    // Assertion A: identity path still works (≥5 npm components).
    let npm_count = doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with("pkg:npm/"))
        })
        .count();
    assert!(
        npm_count >= 5,
        "expected ≥5 npm components; got {npm_count}"
    );

    // Assertion B: foo has 2 edges (dep + peer union), sorted.
    let foo_deps = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ref"].as_str() == Some("pkg:npm/foo@1.0.0"))
        .expect("foo dependency entry")
        ["dependsOn"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<BTreeSet<_>>();
    assert!(
        foo_deps.contains("pkg:npm/bar@2.0.0"),
        "foo missing bar edge; got {foo_deps:?}"
    );
    assert!(
        foo_deps.contains("pkg:npm/baz@3.0.0"),
        "foo missing baz peer edge (Q1 union of 3 sub-mappings); got {foo_deps:?}"
    );

    // Assertion C: peer-dep-suffixed snapshot key resolves to canonical PURL.
    let paginate_purl = "pkg:npm/%40octokit/plugin-paginate-rest@14.0.0";
    let has_paginate = doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["purl"].as_str() == Some(paginate_purl));
    assert!(
        has_paginate,
        "peer-suffixed @octokit/plugin-paginate-rest MUST emit at canonical PURL; got dump: {}",
        serde_json::to_string_pretty(&doc["components"]).unwrap()
    );

    // Assertion D: leaf node (bar) has empty dependsOn.
    let bar_deps = doc["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["ref"].as_str() == Some("pkg:npm/bar@2.0.0"))
        .expect("bar dependency entry")
        ["dependsOn"]
        .as_array()
        .unwrap();
    assert!(
        bar_deps.is_empty(),
        "bar (leaf) MUST have empty dependsOn; got {bar_deps:?}"
    );
}

// ============================================================
// SC-002 monotonic-additive helper + self-test + real-golden test.
// ============================================================

fn index_dependencies_by_ref(doc: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(deps) = doc.get("dependencies").and_then(|v| v.as_array()) else {
        return out;
    };
    for entry in deps {
        let Some(ref_str) = entry.get("ref").and_then(|v| v.as_str()) else {
            continue;
        };
        let targets: BTreeSet<String> = entry
            .get("dependsOn")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        out.insert(ref_str.to_string(), targets);
    }
    out
}

/// Assert every edge in `old` still appears in `new`. Extra edges in
/// `new` that weren't in `old` are permitted (that's the additive
/// part). Missing edges fire an assertion failure naming the ref +
/// missing target(s).
pub(crate) fn assert_monotonic_additive(old: &Value, new: &Value) {
    let old_deps = index_dependencies_by_ref(old);
    let new_deps = index_dependencies_by_ref(new);
    let empty = BTreeSet::new();
    for (ref_str, old_targets) in &old_deps {
        let new_targets = new_deps.get(ref_str).unwrap_or(&empty);
        let missing: Vec<&String> = old_targets.difference(new_targets).collect();
        assert!(
            missing.is_empty(),
            "monotonic-additive violation for {ref_str}: pre-existing edges MUST all still appear in NEW; missing: {missing:?}"
        );
    }
}

#[test]
fn monotonic_additive_helper_catches_missing_edge() {
    // T009 self-test — proves the helper catches the failure mode.
    let old = serde_json::json!({
        "dependencies": [
            {"ref": "pkg:npm/foo@1.0.0", "dependsOn": ["pkg:npm/bar@2.0.0"]}
        ]
    });
    let new = serde_json::json!({
        "dependencies": [
            {"ref": "pkg:npm/foo@1.0.0", "dependsOn": []}
        ]
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_monotonic_additive(&old, &new);
    }));
    let err = result.expect_err("monotonic-additive helper MUST panic on missing edge");
    let msg = err.downcast_ref::<String>().map(|s| s.as_str())
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("monotonic-additive violation"),
        "expected panic message to mention 'monotonic-additive violation'; got: {msg:?}"
    );
}

#[test]
fn monotonic_additive_helper_accepts_pure_additive_change() {
    // Complementary check: adding edges is FINE.
    let old = serde_json::json!({
        "dependencies": [
            {"ref": "pkg:npm/foo@1.0.0", "dependsOn": ["pkg:npm/bar@2.0.0"]}
        ]
    });
    let new = serde_json::json!({
        "dependencies": [
            {"ref": "pkg:npm/foo@1.0.0", "dependsOn": ["pkg:npm/bar@2.0.0", "pkg:npm/new-peer@3.0.0"]}
        ]
    });
    assert_monotonic_additive(&old, &new); // MUST NOT panic.
}

/// T010 Step-3 real-golden verification. Reads a snapshotted pre-157
/// golden from WAYBILL_PRE157_SNAPSHOT_DIR and compares against the
/// working-tree regenerated golden. Gracefully skips when the snapshot
/// dir isn't populated (post-merge CI). Prints an edge-count summary
/// that the maintainer pastes into the PR description as SC-002's
/// real-golden verification receipt.
#[test]
fn monotonic_additive_real_goldens_from_snapshot() {
    let Ok(snapshot_dir) = std::env::var("WAYBILL_PRE157_SNAPSHOT_DIR") else {
        eprintln!(
            "skip: WAYBILL_PRE157_SNAPSHOT_DIR not set. T010 Step-1 must snapshot pre-157 goldens \
             before running this test. Example: WAYBILL_PRE157_SNAPSHOT_DIR=/tmp/waybill-m157-pre-goldens"
        );
        return;
    };
    let pre_dir = PathBuf::from(&snapshot_dir);
    let old_path = pre_dir.join("npm.cdx.json");
    if !old_path.exists() {
        eprintln!(
            "skip: {} not present; T010 Step-1 git-show snapshot not run",
            old_path.display()
        );
        return;
    }
    let old: Value = serde_json::from_str(&std::fs::read_to_string(&old_path).unwrap()).unwrap();

    let new_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/cyclonedx/npm.cdx.json");
    let new: Value = serde_json::from_str(&std::fs::read_to_string(&new_path).unwrap()).unwrap();

    assert_monotonic_additive(&old, &new);

    // Diagnostic: measure edge growth for PR-description receipt.
    let count_edges = |doc: &Value| -> usize {
        doc["dependencies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|d| {
                        d["dependsOn"]
                            .as_array()
                            .map(|a| a.len())
                            .unwrap_or(0)
                    })
                    .sum()
            })
            .unwrap_or(0)
    };
    let old_count = count_edges(&old);
    let new_count = count_edges(&new);
    let delta = new_count.saturating_sub(old_count);
    println!(
        "pnpm CDX golden edges: {old_count} → {new_count} (Δ +{delta}) — SC-002 monotonic-additive PASS"
    );
}
