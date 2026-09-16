//! Milestone 868 (#887) — integration tests for resolve ownership and
//! anchoring.
//!
//! A Pants resolve's dependency graph is read correctly today and connected
//! to nothing: on `lablup/backend.ai` @ `809fcd3`, 760 well-formed edges sat
//! in a document where 1 of 331 components was reachable from the root.
//! Nothing in the model said which component a resolve belongs to.
//!
//! Every assertion here walks the EMITTED graph. None reads the
//! graph-completeness annotation, which consumes the same edges this feature
//! adds — asking it whether the graph improved is asking the change to grade
//! itself (contract A-8). Milestone 866 shipped a document reporting
//! `complete` over a graph it had itself filled in with fabricated edges.
//!
//! Fixtures are composed at test time via `tempfile::tempdir()`; every
//! synthetic package name carries the `waybill-fixture-*` prefix per memory
//! `feedback_fixture_synthetic_package_names`.
//!
//! Cross-linked: `specs/868-resolve-ownership/contracts/resolve-anchoring.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn write_repo(root: &Path, layout: &[(&str, &[u8])]) {
    for (rel, contents) in layout {
        let abs = root.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, contents).unwrap();
    }
}

/// A PEX lockfile carrying locked packages, their inter-package edges, and
/// the top-level `requirements` the resolve was asked to provide.
fn synth_lockfile(
    packages: &[(&str, &str, &[&str])],
    requirements: &[&str],
) -> Vec<u8> {
    let locked: Vec<String> = packages
        .iter()
        .map(|(name, version, deps)| {
            let reqs: Vec<String> = deps.iter().map(|d| format!("\"{d}\"")).collect();
            format!(
                r#"{{"project_name":"{name}","version":"{version}","requires_dists":[{reqs}],"artifacts":[{{"algorithm":"sha256","hash":"{h}","url":"https://files.pythonhosted.org/packages/xx/{m}-{version}-py3-none-any.whl"}}]}}"#,
                reqs = reqs.join(","),
                h = "a".repeat(64),
                m = name.replace('-', "_"),
            )
        })
        .collect();
    let reqs: Vec<String> = requirements.iter().map(|r| format!("\"{r}\"")).collect();
    format!(
        r#"{{"pex_version":"2.10.0","requirements":[{reqs}],"locked_resolves":[{{"locked_requirements":[{locked}]}}]}}"#,
        reqs = reqs.join(","),
        locked = locked.join(","),
    )
    .into_bytes()
}

fn run_scan(root: &Path) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&result.stderr),
    );
    serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap()
}

/// Every `bom-ref` reachable from the document root by following
/// `dependencies[].dependsOn`. This is the measurement contract A-8
/// requires: the graph as emitted, walked here, not a verdict the document
/// reports about itself.
fn reachable_from_root(doc: &Value) -> HashSet<String> {
    let root = doc["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("root bom-ref")
        .to_string();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for d in doc["dependencies"].as_array().into_iter().flatten() {
        let from = d["ref"].as_str().unwrap_or_default().to_string();
        let tos: Vec<String> = d["dependsOn"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str().map(String::from))
            .collect();
        edges.entry(from).or_default().extend(tos);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::from(vec![root.clone()]);
    seen.insert(root);
    while let Some(node) = queue.pop_front() {
        for next in edges.get(&node).into_iter().flatten() {
            if seen.insert(next.clone()) {
                queue.push_back(next.clone());
            }
        }
    }
    seen
}

/// Depth of the deepest node reachable from the root, root itself being 0.
fn max_depth_from_root(doc: &Value) -> usize {
    let root = doc["metadata"]["component"]["bom-ref"]
        .as_str()
        .expect("root bom-ref")
        .to_string();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for d in doc["dependencies"].as_array().into_iter().flatten() {
        let from = d["ref"].as_str().unwrap_or_default().to_string();
        let tos: Vec<String> = d["dependsOn"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str().map(String::from))
            .collect();
        edges.entry(from).or_default().extend(tos);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::from(vec![(root.clone(), 0)]);
    seen.insert(root);
    let mut deepest = 0;
    while let Some((node, depth)) = queue.pop_front() {
        deepest = deepest.max(depth);
        for next in edges.get(&node).into_iter().flatten() {
            if seen.insert(next.clone()) {
                queue.push_back((next.clone(), depth + 1));
            }
        }
    }
    deepest
}

fn purls(doc: &Value) -> Vec<String> {
    doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect()
}

/// Map `purl` -> `bom-ref` so reachability can be asserted about identities
/// a reader recognises rather than about opaque refs.
fn ref_by_purl(doc: &Value) -> HashMap<String, String> {
    doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some((
                c["purl"].as_str()?.to_string(),
                c["bom-ref"].as_str()?.to_string(),
            ))
        })
        .collect()
}

/// A two-resolve repo. `waybill-fixture-shared` is locked in BOTH resolves,
/// which is the FR-002b / SC-006a case: it must be reachable via each.
fn two_resolve_repo(root: &Path) {
    write_repo(
        root,
        &[
            // The root MUST have a dependency edge of its own, or these
            // tests have no teeth. CycloneDX's primary-dependency fallback
            // (`target_has_no_edges`, `cyclonedx/dependencies.rs:77`)
            // attaches every orphan to a root that has no edges, which
            // manufactures reachability and depth the graph has not earned.
            // Measured: without this file the pre-change binary already
            // reports `max_depth=2` on this fixture and `t009b` passes
            // against the defect. With it, the pre-change binary reports
            // `reachable_from_root=1 max_depth=1` — the real fingerprint.
            (
                "pyproject.toml",
                br#"
[project]
name = "waybill-fixture-app"
version = "0.1.0"
dependencies = ["waybill-fixture-direct"]
"#,
            ),
            (
                "pants.toml",
                br#"
[python.resolves]
app-runtime = "locks/app.lock"
lint-tools = "locks/lint.lock"
"#,
            ),
            (
                "locks/app.lock",
                &synth_lockfile(
                    &[
                        (
                            "waybill-fixture-web",
                            "1.0.0",
                            &["waybill-fixture-shared>=2.0"],
                        ),
                        ("waybill-fixture-shared", "2.0.0", &[]),
                    ],
                    &["waybill-fixture-web~=1.0"],
                ),
            ),
            (
                "locks/lint.lock",
                &synth_lockfile(
                    &[
                        (
                            "waybill-fixture-linter",
                            "3.0.0",
                            &["waybill-fixture-shared>=2.0"],
                        ),
                        ("waybill-fixture-shared", "2.0.0", &[]),
                    ],
                    &["waybill-fixture-linter~=3.0"],
                ),
            ),
        ],
    );
}

#[test]
fn t009_each_resolves_requirements_are_reachable_from_the_document_root() {
    // FR-001 / contract A-1. Walked from the emitted graph (A-8).
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let doc = run_scan(dir.path());

    let refs = ref_by_purl(&doc);
    let reached = reachable_from_root(&doc);

    for purl in [
        "pkg:generic/app-runtime",
        "pkg:generic/lint-tools",
        "pkg:pypi/waybill-fixture-web@1.0.0",
        "pkg:pypi/waybill-fixture-linter@3.0.0",
    ] {
        let r = refs
            .get(purl)
            .unwrap_or_else(|| panic!("{purl} missing from components; got {:#?}", purls(&doc)));
        assert!(
            reached.contains(r),
            "{purl} must be reachable from the document root by following \
             dependency edges; reached {} of {} components",
            reached.len(),
            refs.len() + 1,
        );
    }
}

#[test]
fn t009b_the_document_is_no_longer_flat() {
    // SC-002. A flat document is one where everything reachable sits one
    // hop from the root. Anchoring puts the resolve between the root and
    // its requirements, so the graph must be at least 2 deep.
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let doc = run_scan(dir.path());
    let depth = max_depth_from_root(&doc);
    assert!(
        depth >= 2,
        "expected depth >= 2 (root -> resolve -> requirement); got {depth}",
    );
}

#[test]
fn t017_a_package_in_two_resolves_is_reachable_via_each() {
    // FR-002b / FR-006 / SC-006a. `waybill-fixture-shared` is locked in
    // both resolves. Reaching it once is not enough — the path through
    // EACH resolve must exist, or a consumer filtering to one resolve
    // loses it.
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let doc = run_scan(dir.path());

    let refs = ref_by_purl(&doc);
    let shared = refs
        .get("pkg:pypi/waybill-fixture-shared@2.0.0")
        .expect("shared package must be emitted once")
        .clone();

    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for d in doc["dependencies"].as_array().into_iter().flatten() {
        let from = d["ref"].as_str().unwrap_or_default().to_string();
        edges.entry(from).or_default().extend(
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(String::from)),
        );
    }

    // Reachability from each resolve component independently.
    for resolve_purl in ["pkg:generic/app-runtime", "pkg:generic/lint-tools"] {
        let start = refs.get(resolve_purl).expect("resolve component").clone();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue = VecDeque::from(vec![start.clone()]);
        seen.insert(start);
        while let Some(n) = queue.pop_front() {
            for next in edges.get(&n).into_iter().flatten() {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
        assert!(
            seen.contains(&shared),
            "the shared package must be reachable from {resolve_purl}; a \
             consumer filtering to that resolve would otherwise lose it",
        );
    }
}

#[test]
fn t012_anchoring_adds_resolve_components_and_invents_no_packages() {
    // FR-008 / SC-006 / contract A-6. Component count rises by exactly the
    // number of DECLARED resolves; the package set is untouched.
    let declared = tempfile::tempdir().unwrap();
    two_resolve_repo(declared.path());
    let with_resolves = run_scan(declared.path());

    // The same two lockfiles, discovered by glob, declared by nothing.
    let undeclared = tempfile::tempdir().unwrap();
    write_repo(
        undeclared.path(),
        &[
            (
                "pyproject.toml",
                br#"
[project]
name = "waybill-fixture-app"
version = "0.1.0"
dependencies = ["waybill-fixture-direct"]
"#,
            ),
            ("pants.toml", b"[python]\n"),
            (
                "3rdparty/python/app.lock",
                &synth_lockfile(
                    &[
                        (
                            "waybill-fixture-web",
                            "1.0.0",
                            &["waybill-fixture-shared>=2.0"],
                        ),
                        ("waybill-fixture-shared", "2.0.0", &[]),
                    ],
                    &["waybill-fixture-web~=1.0"],
                ),
            ),
            (
                "3rdparty/python/lint.lock",
                &synth_lockfile(
                    &[
                        (
                            "waybill-fixture-linter",
                            "3.0.0",
                            &["waybill-fixture-shared>=2.0"],
                        ),
                        ("waybill-fixture-shared", "2.0.0", &[]),
                    ],
                    &["waybill-fixture-linter~=3.0"],
                ),
            ),
        ],
    );
    let without_resolves = run_scan(undeclared.path());

    let pkgs = |doc: &Value| -> Vec<String> {
        let mut v: Vec<String> = purls(doc)
            .into_iter()
            .filter(|p| p.starts_with("pkg:pypi/"))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    assert_eq!(
        pkgs(&with_resolves),
        pkgs(&without_resolves),
        "declaring a resolve must not change which PACKAGES exist",
    );

    let resolve_components: Vec<String> = purls(&with_resolves)
        .into_iter()
        .filter(|p| p.starts_with("pkg:generic/"))
        .collect();
    assert_eq!(
        resolve_components.len(),
        2,
        "expected exactly one resolve component per declared resolve; got \
         {resolve_components:#?}",
    );
    assert!(
        !purls(&without_resolves)
            .iter()
            .any(|p| p.starts_with("pkg:generic/")),
        "a project declaring no resolve must emit no resolve component",
    );
}
