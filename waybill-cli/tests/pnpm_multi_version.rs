//! Milestone 164 US1 P1 MVP integration test — verifies the pnpm-lock
//! v9 multi-version edge disambiguation on a synthesized workspace
//! monorepo with two versions of the same package + two parents each
//! declaring a different version.
//!
//! Assertions (per SC-008):
//!  1. Both `pkg:npm/foo@1.0.0` and `pkg:npm/foo@2.0.0` present in
//!     `components[]`.
//!  2. `pkg:npm/parent-a@1.0.0`'s `dependsOn` contains
//!     `pkg:npm/foo@1.0.0` (NOT `2.0.0`) — correct version-specific
//!     resolution.
//!  3. `pkg:npm/parent-b@1.0.0`'s `dependsOn` contains
//!     `pkg:npm/foo@2.0.0` (NOT `1.0.0`).
//!  4. BFS from `metadata.component` reaches BOTH `foo@1.0.0` AND
//!     `foo@2.0.0` — zero multi-version orphans.
//!  5. Milestone-163 SC-002 + SC-004 invariants preserved:
//!     zero empty-version PURLs AND zero phantom edges.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

/// Build a synthesized pnpm-workspace monorepo with:
/// - workspace root package.json + pnpm-workspace.yaml
/// - pnpm-lock.yaml v9 with two versions of `foo` + two parents (each
///   consumes a different version) + two workspace peers that consume
///   the parents
/// - two `packages/consumer-*` peer directories
fn build_fixture(tmp: &std::path::Path) {
    // Workspace root package.json.
    std::fs::write(
        tmp.join("package.json"),
        r#"{
  "name": "monorepo-root",
  "version": "0.0.0",
  "private": true
}
"#,
    )
    .unwrap();

    // pnpm-workspace.yaml declares peer directories.
    std::fs::write(
        tmp.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n",
    )
    .unwrap();

    // pnpm-lock.yaml v9 — the load-bearing multi-version scenario.
    // `foo` exists at both 1.0.0 and 2.0.0; `parent-a` depends on
    // foo@1.0.0, `parent-b` depends on foo@2.0.0. Consumer-a/b import
    // parent-a/b at the workspace-peer level.
    std::fs::write(
        tmp.join("pnpm-lock.yaml"),
        r#"lockfileVersion: '9.0'

importers:
  packages/consumer-a:
    dependencies:
      parent-a:
        specifier: ^1.0.0
        version: 1.0.0
  packages/consumer-b:
    dependencies:
      parent-b:
        specifier: ^1.0.0
        version: 1.0.0

packages:
  foo@1.0.0:
    resolution: {integrity: sha512-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}
  foo@2.0.0:
    resolution: {integrity: sha512-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb}
  parent-a@1.0.0:
    resolution: {integrity: sha512-ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc}
  parent-b@1.0.0:
    resolution: {integrity: sha512-ddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd}

snapshots:
  foo@1.0.0: {}
  foo@2.0.0: {}
  parent-a@1.0.0:
    dependencies:
      foo: 1.0.0
  parent-b@1.0.0:
    dependencies:
      foo: 2.0.0
"#,
    )
    .unwrap();

    // Consumer peer directories.
    let consumer_a = tmp.join("packages").join("consumer-a");
    std::fs::create_dir_all(&consumer_a).unwrap();
    std::fs::write(
        consumer_a.join("package.json"),
        r#"{
  "name": "consumer-a",
  "version": "0.0.0",
  "dependencies": {
    "parent-a": "^1.0.0"
  }
}
"#,
    )
    .unwrap();

    let consumer_b = tmp.join("packages").join("consumer-b");
    std::fs::create_dir_all(&consumer_b).unwrap();
    std::fs::write(
        consumer_b.join("package.json"),
        r#"{
  "name": "consumer-b",
  "version": "0.0.0",
  "dependencies": {
    "parent-b": "^1.0.0"
  }
}
"#,
    )
    .unwrap();
}

fn scan_fixture(tmp: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(tmp)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn dependencies_of<'a>(sbom: &'a serde_json::Value, purl: &str) -> Vec<&'a str> {
    let Some(arr) = sbom.get("dependencies").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .find(|d| d.get("ref").and_then(|r| r.as_str()) == Some(purl))
        .and_then(|d| d.get("dependsOn"))
        .and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default()
}

#[test]
fn t015_synthesized_multi_version_zero_orphans_and_correct_edges() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_fixture(tmp);

    let sbom = scan_fixture(tmp);
    let components = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array");

    let npm_purls: HashSet<String> = components
        .iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(String::from))
        .filter(|p| p.starts_with("pkg:npm/"))
        .collect();

    // ---------------------------------------------------------------
    // Assertion 1: both `foo@1.0.0` AND `foo@2.0.0` emitted.
    // ---------------------------------------------------------------
    assert!(
        npm_purls.contains("pkg:npm/foo@1.0.0"),
        "foo@1.0.0 missing from components[]. Present npm PURLs: {npm_purls:#?}"
    );
    assert!(
        npm_purls.contains("pkg:npm/foo@2.0.0"),
        "foo@2.0.0 missing from components[]. Present npm PURLs: {npm_purls:#?}"
    );

    // ---------------------------------------------------------------
    // Assertion 2: parent-a → foo@1.0.0 (correct version).
    // ---------------------------------------------------------------
    let parent_a_edges = dependencies_of(&sbom, "pkg:npm/parent-a@1.0.0");
    assert!(
        parent_a_edges.contains(&"pkg:npm/foo@1.0.0"),
        "parent-a@1.0.0 → foo@1.0.0 edge missing. Actual edges: {parent_a_edges:?}"
    );
    assert!(
        !parent_a_edges.contains(&"pkg:npm/foo@2.0.0"),
        "parent-a@1.0.0 must NOT point at foo@2.0.0 (wrong version). Actual: {parent_a_edges:?}"
    );

    // ---------------------------------------------------------------
    // Assertion 3: parent-b → foo@2.0.0 (correct version).
    // ---------------------------------------------------------------
    let parent_b_edges = dependencies_of(&sbom, "pkg:npm/parent-b@1.0.0");
    assert!(
        parent_b_edges.contains(&"pkg:npm/foo@2.0.0"),
        "parent-b@1.0.0 → foo@2.0.0 edge missing. Actual edges: {parent_b_edges:?}"
    );
    assert!(
        !parent_b_edges.contains(&"pkg:npm/foo@1.0.0"),
        "parent-b@1.0.0 must NOT point at foo@1.0.0 (wrong version). Actual: {parent_b_edges:?}"
    );

    // ---------------------------------------------------------------
    // Assertion 4: BFS reaches BOTH foo versions (zero multi-version orphans).
    // ---------------------------------------------------------------
    let root_purl = sbom
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("purl"))
        .and_then(|p| p.as_str())
        .expect("metadata.component.purl must exist");

    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(arr) = sbom.get("dependencies").and_then(|d| d.as_array()) {
        for node in arr {
            let from = node
                .get("ref")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let targets: Vec<String> = node
                .get("dependsOn")
                .and_then(|d| d.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            adj.insert(from, targets);
        }
    }
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue = vec![root_purl.to_string()];
    while let Some(cur) = queue.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        if let Some(targets) = adj.get(&cur) {
            for t in targets {
                if !visited.contains(t) {
                    queue.push(t.clone());
                }
            }
        }
    }
    assert!(
        visited.contains("pkg:npm/foo@1.0.0"),
        "foo@1.0.0 not BFS-reachable from root {root_purl}"
    );
    assert!(
        visited.contains("pkg:npm/foo@2.0.0"),
        "foo@2.0.0 not BFS-reachable from root {root_purl}"
    );

    // ---------------------------------------------------------------
    // Assertion 5 (SC-004 milestone-163 invariants preserved):
    // zero empty-version PURLs AND zero phantom edges.
    // ---------------------------------------------------------------
    let empty_version_purls: Vec<&String> =
        npm_purls.iter().filter(|p| p.ends_with('@')).collect();
    assert!(
        empty_version_purls.is_empty(),
        "SC-004 violated: empty-version PURLs found: {empty_version_purls:?}"
    );
    if let Some(arr) = sbom.get("dependencies").and_then(|d| d.as_array()) {
        for node in arr {
            if let Some(targets) = node.get("dependsOn").and_then(|d| d.as_array()) {
                for tgt in targets {
                    if let Some(s) = tgt.as_str() {
                        assert!(
                            !(s.starts_with("pkg:npm/") && s.ends_with('@')),
                            "SC-004 violated: phantom edge target `{s}`"
                        );
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn _keep_pathbuf() -> PathBuf {
    PathBuf::new()
}
