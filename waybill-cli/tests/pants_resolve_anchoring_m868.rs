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

/// Run a scan emitting all three formats, returning (cdx, spdx23, spdx3).
/// US2 asserts the resolve marker survives every emitter, so the assertion
/// has to see every emitter.
fn run_scan_all_formats(root: &Path) -> (Value, Value, Value) {
    let out = tempfile::tempdir().unwrap();
    let cdx = out.path().join("o.cdx.json");
    let s23 = out.path().join("o.spdx.json");
    let s3 = out.path().join("o.spdx3.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json,spdx-2.3-json,spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", s23.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", s3.display()))
        .arg("--no-deep-hash")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&result.stderr),
    );
    let read =
        |p: &std::path::PathBuf| -> Value { serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap() };
    (read(&cdx), read(&s23), read(&s3))
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

// -------------------------------------------------------------------
// User Story 2 — the anchor says what it is.
// -------------------------------------------------------------------

/// Purls of components carrying the resolve-nature marker, per format.
fn cdx_marked_resolves(doc: &Value) -> Vec<String> {
    let mut v: Vec<String> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["properties"].as_array().into_iter().flatten().any(|p| {
                p["name"].as_str() == Some("waybill:component-kind")
                    && p["value"].as_str() == Some("lockfile-resolve")
            })
        })
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    v.sort();
    v
}

fn envelope_hits(text_iter: impl Iterator<Item = String>) -> usize {
    text_iter
        .filter(|c| c.contains("waybill:component-kind") && c.contains("lockfile-resolve"))
        .count()
}

#[test]
fn t019_a_resolve_is_identifiable_as_a_resolve_in_every_format() {
    // FR-002a / contract A-3. A resolve has no upstream, nothing to fetch
    // and no vulnerability surface. A consumer that cannot tell it from a
    // package will try to resolve it against an index and fail.
    //
    // `pkg:generic/` alone does not carry that — real, fetchable things use
    // the same type — so the nature is asserted from the explicit marker,
    // in all three formats. A signal that survives only CycloneDX is not a
    // signal a consumer can rely on.
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let (cdx, spdx23, spdx3) = run_scan_all_formats(dir.path());

    assert_eq!(
        cdx_marked_resolves(&cdx),
        vec![
            "pkg:generic/app-runtime".to_string(),
            "pkg:generic/lint-tools".to_string()
        ],
        "CDX: exactly the resolve components carry the marker",
    );

    // No PACKAGE may carry it — the marker would mean nothing if one did.
    let marked = cdx_marked_resolves(&cdx);
    for c in cdx["components"].as_array().into_iter().flatten() {
        let purl = c["purl"].as_str().unwrap_or_default();
        if purl.starts_with("pkg:pypi/") {
            assert!(
                !marked.iter().any(|m| m == purl),
                "{purl} is a package and must not be marked a resolve",
            );
        }
    }

    // SPDX 2.3 — per-Package `annotations[].comment` envelope.
    let s23 = envelope_hits(
        spdx23["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| p["annotations"].as_array().into_iter().flatten())
            .filter_map(|a| a["comment"].as_str().map(String::from)),
    );
    assert_eq!(s23, 2, "SPDX 2.3 must carry the marker on both resolves");

    // SPDX 3 — `Annotation.statement` envelope.
    let s3 = envelope_hits(
        spdx3["@graph"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["statement"].as_str().map(String::from)),
    );
    assert!(
        s3 >= 2,
        "SPDX 3 must carry the marker on both resolves; got {s3}",
    );
}

#[test]
fn t019b_the_root_to_resolve_edge_is_distinguishable_by_its_target() {
    // FR-004. The anchor edge is an ordinary dependency edge on purpose —
    // consumers traverse it without special handling. What makes it
    // distinguishable is its TARGET being marked a resolve, which is why
    // there is no per-edge annotation to look for.
    //
    // Asserted as a property of the pair so it fails if either half
    // regresses: the root's out-edges split into exactly the anchors and a
    // non-empty remainder, and the split is computable from the marker
    // alone.
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let doc = run_scan(dir.path());

    let marked: HashSet<String> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["properties"].as_array().into_iter().flatten().any(|p| {
                p["name"].as_str() == Some("waybill:component-kind")
                    && p["value"].as_str() == Some("lockfile-resolve")
            })
        })
        .filter_map(|c| c["bom-ref"].as_str().map(String::from))
        .collect();

    let root = doc["metadata"]["component"]["bom-ref"].as_str().unwrap();
    let root_targets: Vec<String> = doc["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["ref"].as_str() == Some(root))
        .flat_map(|d| {
            d["dependsOn"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .collect();

    let (anchors, plain): (Vec<&String>, Vec<&String>) =
        root_targets.iter().partition(|t| marked.contains(*t));
    assert_eq!(
        anchors.len(),
        2,
        "expected the two anchor edges among the root's out-edges; got {root_targets:#?}",
    );
    assert!(
        !plain.is_empty(),
        "the fixture must also carry a NON-anchor root edge, or this test \
         cannot show the two are distinguishable",
    );
}

// -------------------------------------------------------------------
// User Story 3 — say how much of the classification was guessed.
// -------------------------------------------------------------------

/// The doc-scope `waybill:resolve-ownership` value, per format. `None` means
/// the annotation is absent, which is a different claim from a zeroed value
/// and is asserted as such.
fn resolve_ownership(cdx: &Value, spdx23: &Value, spdx3: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let c = cdx["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some("waybill:resolve-ownership"))
        .and_then(|p| p["value"].as_str().map(String::from));
    let extract = |s: &str| -> Option<String> {
        let v: Value = serde_json::from_str(s).ok()?;
        (v["field"].as_str()? == "waybill:resolve-ownership")
            .then(|| v["value"].as_str().map(String::from))?
    };
    let s2 = spdx23["annotations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["comment"].as_str())
        .find_map(extract);
    let s3 = spdx3["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["statement"].as_str())
        .find_map(extract);
    (c, s2, s3)
}

#[test]
fn t026_the_counts_are_emitted_even_when_both_are_zero() {
    // FR-003c. A project whose every resolve is declared needed no guessing
    // at all — and that is a finding, not an absence. If the annotation were
    // omitted at zero, a consumer could not tell it apart from a document
    // that never ran the classifier.
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
app-runtime = "locks/app.lock"

[app-runtime]
install_from_resolve = "app-runtime"
"#,
            ),
            (
                "locks/app.lock",
                &synth_lockfile(
                    &[("waybill-fixture-alpha", "1.0.0", &[])],
                    &["waybill-fixture-alpha"],
                ),
            ),
        ],
    );
    let (cdx, spdx23, spdx3) = run_scan_all_formats(dir.path());
    let expected = Some("weak-classification=0;unanchored-lockfiles=0".to_string());
    assert_eq!(
        resolve_ownership(&cdx, &spdx23, &spdx3),
        (expected.clone(), expected.clone(), expected),
        "both counts must be present at zero, in all three formats",
    );
}

#[test]
fn t026b_the_annotation_is_absent_when_no_lockfile_was_found() {
    // Contract A-7 / SC-004. The flip side of t026: a repo with no Pex
    // lockfile learned nothing about ownership, so it reports nothing.
    // Emitting two honest zeroes here would change every non-Pants document
    // in the corpus.
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[(
            "pyproject.toml",
            br#"
[project]
name = "waybill-fixture-app"
version = "0.1.0"
dependencies = ["waybill-fixture-direct"]
"#,
        )],
    );
    let (cdx, spdx23, spdx3) = run_scan_all_formats(dir.path());
    assert_eq!(
        resolve_ownership(&cdx, &spdx23, &spdx3),
        (None, None, None),
        "a project with no Pex lockfile must emit no ownership annotation",
    );
}

#[test]
fn t034_a_glob_discovered_lockfile_nobody_declares_is_counted_unanchored() {
    // FR-003. The lockfile is read and its packages emitted, but no resolve
    // component and no anchor edge are created — a filename stem is a
    // convention, not a declaration of ownership. The count is how that
    // silence is made visible.
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[
            ("pants.toml", b"[python]\n"),
            (
                "3rdparty/python/undeclared.lock",
                &synth_lockfile(
                    &[("waybill-fixture-alpha", "1.0.0", &[])],
                    &["waybill-fixture-alpha"],
                ),
            ),
        ],
    );
    let (cdx, spdx23, spdx3) = run_scan_all_formats(dir.path());

    let expected = Some("weak-classification=0;unanchored-lockfiles=1".to_string());
    assert_eq!(
        resolve_ownership(&cdx, &spdx23, &spdx3),
        (expected.clone(), expected.clone(), expected),
    );

    // No resolve component, and therefore no anchor edge.
    assert!(
        cdx_marked_resolves(&cdx).is_empty(),
        "an undeclared lockfile owns nothing",
    );
    // The packages themselves are still read.
    assert!(
        purls(&cdx)
            .iter()
            .any(|p| p.starts_with("pkg:pypi/waybill-fixture-alpha")),
        "the lockfile's packages are unaffected by the ownership question",
    );
}

#[test]
fn t030_undeclared_resolves_are_counted_as_weakly_classified() {
    // FR-003c again, from the other side: the two-resolve fixture declares
    // neither resolve via a tool section, so both fall to the name allowlist
    // and both are counted.
    let dir = tempfile::tempdir().unwrap();
    two_resolve_repo(dir.path());
    let (cdx, spdx23, spdx3) = run_scan_all_formats(dir.path());
    let expected = Some("weak-classification=2;unanchored-lockfiles=0".to_string());
    assert_eq!(
        resolve_ownership(&cdx, &spdx23, &spdx3),
        (expected.clone(), expected.clone(), expected),
    );
}

// -------------------------------------------------------------------
// Issue #894 — regressions found by the #890 golden refresh.
// -------------------------------------------------------------------

#[test]
fn m894_anchoring_does_not_strand_components_the_fallback_was_covering() {
    // Defect 2. CycloneDX's primary-dependency fallback fires only when the
    // root has no outgoing edges. Milestone 868 gave the root one genuine
    // edge (root -> the resolve component), which switched the fallback off
    // — and on a project whose other components are NOT lockfile entries,
    // everything the fallback had been covering was stranded.
    //
    // Observed on `pants-example-django`: reachability fell from 46 of 46 to
    // 34 of 47. A resolve anchor is synthetic ownership, not a dependency
    // the project declared, so it must not count toward "the root has edges".
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
app-runtime = "locks/app.lock"
"#,
            ),
            (
                "locks/app.lock",
                &synth_lockfile(
                    &[("waybill-fixture-locked", "1.0.0", &[])],
                    &["waybill-fixture-locked"],
                ),
            ),
            // Declared in a requirements file, absent from the lockfile —
            // the design-tier shape the fallback used to carry. No
            // `pyproject.toml`, so the root has no dependency edge of its
            // own: exactly the condition that made the fallback load-bearing.
            (
                "requirements.txt",
                b"waybill-fixture-requirements-only\n",
            ),
        ],
    );
    let doc = run_scan(dir.path());
    let refs = ref_by_purl(&doc);
    let reached = reachable_from_root(&doc);

    let stranded: Vec<&String> = refs
        .iter()
        .filter(|(_, r)| !reached.contains(*r))
        .map(|(p, _)| p)
        .collect();
    assert!(
        stranded.is_empty(),
        "anchoring must not strand components the fallback was covering; \
         unreachable: {stranded:#?}",
    );

    // And the anchor itself is still there — the fix restores the fallback,
    // it does not remove the feature.
    assert_eq!(
        cdx_marked_resolves(&doc),
        vec!["pkg:generic/app-runtime".to_string()],
    );
}

#[test]
fn m894_a_tool_installing_from_the_default_resolve_stays_runtime() {
    // Defect 1. `install_from_resolve` says a tool is installed FROM a
    // resolve, which is evidence the resolve CONTAINS the tool — not that it
    // EXISTS FOR it. When the named resolve is the application's own
    // default, treating the back-reference as a build-time declaration marks
    // every runtime dependency `scope: "excluded"` and hides them from a
    // consumer filtering for runtime risk. Contract A-5 forbids exactly this
    // direction of error.
    //
    // Observed on `pants-example-django`: 34 of 47 components excluded.
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python]
resolves = { python-default = "locks/python-default.lock" }

[pytest]
install_from_resolve = "python-default"
[mypy]
install_from_resolve = "python-default"
"#,
            ),
            (
                "locks/python-default.lock",
                &synth_lockfile(
                    &[("waybill-fixture-app-dep", "1.0.0", &[])],
                    &["waybill-fixture-app-dep"],
                ),
            ),
        ],
    );
    let doc = run_scan(dir.path());

    let excluded: Vec<String> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["scope"].as_str() == Some("excluded"))
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    assert!(
        excluded.is_empty(),
        "a tool installing FROM the default resolve must not push the \
         application's dependencies out of runtime scope; excluded: {excluded:#?}",
    );

    // The resolve is classified, and classified by the weaker evidence —
    // which is the honest answer here, and is counted as such.
    let resolve = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["purl"].as_str() == Some("pkg:generic/python-default"))
        .expect("resolve component");
    let source = resolve["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some("waybill:resolve-classification-source"))
        .and_then(|p| p["value"].as_str());
    assert_eq!(
        source,
        Some("heuristic-or-default"),
        "the back-reference is not a declaration about this resolve, so the \
         classification rests on the weaker evidence and must say so",
    );
}

#[test]
fn m892_reported_completeness_agrees_with_the_emitted_graph() {
    // Issue #892. The completeness annotation is what consumers read to
    // decide whether to trust the graph, so it must not contradict the
    // document carrying it.
    //
    // The classifier mirrors the emitter's primary-dependency fallback
    // internally. Milestone 868's resolve anchor made the classifier's gate
    // think the root already had edges, so its mirror declined to fire while
    // the emitter's did — and it reported `partial` with 12 unreachable
    // components over a `pants-example-django` document that reached every
    // one of them.
    //
    // Asserted as agreement rather than as a fixed verdict: the point is
    // that the two halves see one graph, not that this fixture is complete.
    let dir = tempfile::tempdir().unwrap();
    write_repo(
        dir.path(),
        &[
            (
                "pants.toml",
                br#"
[python.resolves]
app-runtime = "locks/app.lock"
"#,
            ),
            (
                "locks/app.lock",
                &synth_lockfile(
                    &[("waybill-fixture-locked", "1.0.0", &[])],
                    &["waybill-fixture-locked"],
                ),
            ),
            // Not in the lockfile, and the root declares no dependency of
            // its own — the shape that makes the fallback load-bearing.
            ("requirements.txt", b"waybill-fixture-requirements-only\n"),
        ],
    );
    let doc = run_scan(dir.path());

    let reached = reachable_from_root(&doc);
    let tiered: Vec<&Value> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| {
            c["properties"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| p["name"].as_str() == Some("waybill:sbom-tier"))
        })
        .collect();
    let walked_orphans = tiered
        .iter()
        .filter(|c| !reached.contains(c["bom-ref"].as_str().unwrap_or_default()))
        .count();

    let reported = doc["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some("waybill:graph-completeness"))
        .and_then(|p| p["value"].as_str())
        .expect("completeness annotation");
    let reason = doc["metadata"]["properties"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some("waybill:graph-completeness-reason"))
        .and_then(|p| p["value"].as_str())
        .unwrap_or("");

    assert_eq!(
        walked_orphans, 0,
        "precondition: this fixture's graph reaches everything",
    );
    // `partial` is legitimate here for an unrelated reason — a design-tier
    // pypi component with no resolved version makes transitive edges
    // unresolvable — so the verdict is not what this asserts. The ORPHAN
    // claim is: the annotation must not say components are unreachable when
    // the document reaches all of them.
    assert!(
        !reason.contains("orphaned-components-detected"),
        "the annotation claims orphans the emitted graph does not have; \
         walked orphans = {walked_orphans}, verdict = {reported:?}, \
         reason = {reason:?}",
    );
}
