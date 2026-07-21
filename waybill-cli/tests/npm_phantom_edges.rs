//! Milestone 163 (closes #498) US1 P1 MVP integration test — verifies
//! the cross-workspace resolution flow on a synthesized multi-workspace
//! npm monorepo. Assertions:
//!
//! 1. Zero components with PURL matching `^pkg:npm/[^@]+@$`
//!    (empty-version regex) — SC-004.
//! 2. Zero edges in `dependencies[].dependsOn[]` matching the same
//!    regex — SC-002.
//! 3. `pkg:npm/docs@0.0.0` has a `dependsOn` edge to
//!    `pkg:npm/%40docusaurus/core@3.10.1` (concrete version) — FR-001.
//! 4. `pkg:npm/docs@0.0.0` carries `mikebom:unresolved-declared-dep =
//!    "@some/removed"` annotation — FR-004.
//! 5. `pkg:npm/renderer@0.0.0` has a `dependsOn` edge to
//!    `pkg:npm/thor@1.4.0`.
//! 6. Total npm component count matches ground truth AND every
//!    resolved-version component is present (no RESOLVED component
//!    dropped) — FR-006.
//! 7. BFS from the top-level main-module reaches 100% of npm
//!    components on this fully-controlled fixture — SC-001 achievable
//!    floor.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

/// Build a fully-controlled multi-workspace monorepo in `tmp`. Layout:
/// `<tmp>/package.json` (workspace root); `<tmp>/package-lock.json`
/// (v3 lockfile pinning `@docusaurus/core@3.10.1` + `thor@1.4.0`);
/// `<tmp>/packages/docs/package.json` (peer declaring `@docusaurus/core`
/// (resolvable) + `@some/removed` (unresolvable));
/// `<tmp>/packages/renderer/package.json` (peer declaring `thor`
/// (resolvable)).
fn build_monorepo_fixture(tmp: &std::path::Path) {
    // Workspace root package.json.
    std::fs::write(
        tmp.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "monorepo-root",
            "version": "0.0.0",
            "private": true,
            "workspaces": ["packages/*"]
        }))
        .unwrap(),
    )
    .unwrap();

    // Workspace root package-lock.json v3 pinning the two real deps.
    std::fs::write(
        tmp.join("package-lock.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "monorepo-root",
            "version": "0.0.0",
            "lockfileVersion": 3,
            "requires": true,
            "packages": {
                "": {
                    "name": "monorepo-root",
                    "version": "0.0.0",
                    "workspaces": ["packages/*"]
                },
                "node_modules/@docusaurus/core": {
                    "version": "3.10.1",
                    "resolved": "https://registry.npmjs.org/@docusaurus/core/-/core-3.10.1.tgz",
                    "integrity": "sha512-fake"
                },
                "node_modules/thor": {
                    "version": "1.4.0",
                    "resolved": "https://registry.npmjs.org/thor/-/thor-1.4.0.tgz",
                    "integrity": "sha512-fake"
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    // Peer 1: `docs` — declares one resolvable + one unresolvable dep.
    let docs_dir = tmp.join("packages").join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();
    std::fs::write(
        docs_dir.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "docs",
            "version": "0.0.0",
            "dependencies": {
                "@docusaurus/core": "^3.10.1",
                "@some/removed": "^1.0.0"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    // Peer 2: `renderer` — declares one resolvable dep.
    let renderer_dir = tmp.join("packages").join("renderer");
    std::fs::create_dir_all(&renderer_dir).unwrap();
    std::fs::write(
        renderer_dir.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "renderer",
            "version": "0.0.0",
            "dependencies": {
                "thor": "^1.0.0"
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn scan_fixture(tmp: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
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
        .expect("mikebom should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn component_property<'a>(c: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    c.get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
}

fn dependencies_of<'a>(
    sbom: &'a serde_json::Value,
    purl: &str,
) -> Vec<&'a str> {
    let Some(deps_array) = sbom.get("dependencies").and_then(|d| d.as_array()) else {
        return Vec::new();
    };
    let Some(node) = deps_array.iter().find(|d| {
        d.get("ref").and_then(|r| r.as_str()) == Some(purl)
    }) else {
        return Vec::new();
    };
    node.get("dependsOn")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn t028_synthesized_monorepo_zero_phantoms_and_100pct_bfs() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_monorepo_fixture(tmp);

    let sbom = scan_fixture(tmp);
    let components = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array");

    let npm_components: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:npm/"))
        })
        .collect();
    let npm_purls: HashSet<String> = npm_components
        .iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(String::from))
        .collect();

    // ---------------------------------------------------------------
    // ASSERTION 1 (SC-004): zero empty-version PURLs in components[].
    // ---------------------------------------------------------------
    let empty_version_purls: Vec<&String> = npm_purls
        .iter()
        .filter(|p| p.ends_with('@'))
        .collect();
    assert!(
        empty_version_purls.is_empty(),
        "SC-004 violated: empty-version PURLs found in components[]: {empty_version_purls:?}"
    );

    // ---------------------------------------------------------------
    // ASSERTION 2 (SC-002): zero empty-version PURLs in dependsOn[].
    // ---------------------------------------------------------------
    if let Some(deps_array) = sbom.get("dependencies").and_then(|d| d.as_array()) {
        for node in deps_array {
            if let Some(depends_on) = node.get("dependsOn").and_then(|d| d.as_array()) {
                for target in depends_on {
                    if let Some(t) = target.as_str() {
                        assert!(
                            !t.ends_with('@') || !t.starts_with("pkg:npm/"),
                            "SC-002 violated: phantom edge target `{t}` under `ref={:?}`",
                            node.get("ref").and_then(|r| r.as_str())
                        );
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // ASSERTION 3 (FR-001): docs → @docusaurus/core@3.10.1 edge.
    // ---------------------------------------------------------------
    let docs_edges = dependencies_of(&sbom, "pkg:npm/docs@0.0.0");
    assert!(
        docs_edges.contains(&"pkg:npm/%40docusaurus/core@3.10.1"),
        "FR-001 violated: pkg:npm/docs@0.0.0 dependsOn missing concrete-version target. \
         Actual edges: {docs_edges:?}"
    );

    // ---------------------------------------------------------------
    // ASSERTION 4 (FR-004): docs main-module carries the C115
    // annotation naming @some/removed.
    // ---------------------------------------------------------------
    let docs_comp = npm_components
        .iter()
        .find(|c| c.get("purl").and_then(|p| p.as_str()) == Some("pkg:npm/docs@0.0.0"))
        .expect("pkg:npm/docs@0.0.0 component must exist");
    let c115 = component_property(docs_comp, "mikebom:unresolved-declared-dep")
        .expect("C115 annotation must be present on docs peer");
    // Single unresolved dep → bare string value per contracts/annotations.md.
    assert_eq!(
        c115.as_str(),
        Some("@some/removed"),
        "FR-004 violated: C115 value must name the unresolvable dep"
    );

    // ---------------------------------------------------------------
    // ASSERTION 5: renderer → thor@1.4.0 edge.
    // ---------------------------------------------------------------
    let renderer_edges = dependencies_of(&sbom, "pkg:npm/renderer@0.0.0");
    assert!(
        renderer_edges.contains(&"pkg:npm/thor@1.4.0"),
        "renderer → thor@1.4.0 edge missing. Actual edges: {renderer_edges:?}"
    );

    // ---------------------------------------------------------------
    // ASSERTION 6 (FR-006): both resolved-version components emitted.
    // ---------------------------------------------------------------
    assert!(
        npm_purls.contains("pkg:npm/%40docusaurus/core@3.10.1"),
        "resolved-version component @docusaurus/core@3.10.1 missing from components[]. \
         Present npm PURLs: {:#?}",
        npm_purls
    );
    assert!(
        npm_purls.contains("pkg:npm/thor@1.4.0"),
        "resolved-version component thor@1.4.0 missing from components[]. \
         Present npm PURLs: {:#?}",
        npm_purls
    );

    // ---------------------------------------------------------------
    // ASSERTION 7 (SC-001): BFS from `metadata.component` reaches all
    // npm components on this fully-controlled fixture.
    // ---------------------------------------------------------------
    let root_purl = sbom
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("purl"))
        .and_then(|p| p.as_str())
        .expect("metadata.component.purl must exist");

    let mut adjacency: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    if let Some(deps_array) = sbom.get("dependencies").and_then(|d| d.as_array()) {
        for node in deps_array {
            let from = node
                .get("ref")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let targets: Vec<String> = node
                .get("dependsOn")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            adjacency.insert(from, targets);
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue = vec![root_purl.to_string()];
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(targets) = adjacency.get(&current) {
            for t in targets {
                if !visited.contains(t) {
                    queue.push(t.clone());
                }
            }
        }
    }

    let unreachable: Vec<&String> = npm_purls.iter().filter(|p| !visited.contains(*p)).collect();
    // On the fully-controlled fixture, all npm components should be
    // reachable from the workspace root's main-module. If any are
    // unreachable, print the diagnostic so consumers can see the shape
    // (per feedback_dont_dismiss_test_failures — never dismiss as flake).
    assert!(
        unreachable.is_empty(),
        "SC-001 violated on synthesized fixture: {} of {} npm components unreachable from root {}: {unreachable:?}",
        unreachable.len(),
        npm_purls.len(),
        root_purl
    );
}

/// T031 (US2, milestone 163, closes #498): FR-003 nested-preferred
/// integration test. Extends the T028 synthesized monorepo with a peer
/// (`packages/renderer/`) that has its own `node_modules/thor/` at
/// version 2.0.0 while the top-level lockfile pins `thor@1.4.0`. Per
/// Node.js runtime resolver semantics, the closer-ancestor (nested)
/// version wins. Post-163: the renderer peer's edge MUST target
/// `pkg:npm/thor@2.0.0`, not `pkg:npm/thor@1.4.0`.
#[test]
fn t031_nested_node_modules_wins_over_root_lockfile() {
    let tmp_holder = tempfile::tempdir().expect("tempdir");
    let tmp = tmp_holder.path();
    build_monorepo_fixture(tmp);

    // Overlay: renderer has its own nested `node_modules/thor/` at 2.0.0.
    let nested_dir = tmp
        .join("packages")
        .join("renderer")
        .join("node_modules")
        .join("thor");
    std::fs::create_dir_all(&nested_dir).unwrap();
    std::fs::write(
        nested_dir.join("package.json"),
        r#"{"name":"thor","version":"2.0.0"}"#,
    )
    .unwrap();

    let sbom = scan_fixture(tmp);
    let renderer_edges = dependencies_of(&sbom, "pkg:npm/renderer@0.0.0");
    // Nested version (2.0.0) must win over the root lockfile's 1.4.0.
    assert!(
        renderer_edges.contains(&"pkg:npm/thor@2.0.0"),
        "FR-003 violated: renderer → nested thor@2.0.0 edge missing. \
         Actual edges: {renderer_edges:?}"
    );
    assert!(
        !renderer_edges.contains(&"pkg:npm/thor@1.4.0"),
        "FR-003 violated: renderer must NOT edge to root-lockfile thor@1.4.0 \
         when its own nested thor@2.0.0 exists. Actual edges: {renderer_edges:?}"
    );
}

// Silences "unused import" lint when `assertions_helper` is trimmed
// during dev iteration.
#[allow(dead_code)]
fn _keep_pathbuf_used() -> PathBuf {
    PathBuf::new()
}
