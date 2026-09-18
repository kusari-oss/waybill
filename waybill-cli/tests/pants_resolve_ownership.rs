//! Issue #911 (#902 item 3) — the document says which resolves were declared
//! and which were found by filename convention.
//!
//! **Why it matters.** Milestone 868 declined to anchor a lockfile found by
//! glob: its name comes from a filename stem, which is a convention rather
//! than a declaration of ownership. That reasoning stands. The consequence
//! was that a repository relying on the `3rdparty/python/*.lock` convention —
//! a large share of real Pants repositories — produced components carrying
//! resolve names with nothing to walk from, and nothing in the document said
//! which situation it was in.
//!
//! The doc-scope annotation counted unanchored lockfiles. A count tells a
//! consumer HOW MANY resolves it cannot walk from; it cannot tell them WHICH,
//! so it cannot tell them whether the repository is partitionable at all.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn scan(name: &str) -> serde_json::Value {
    let out = tempfile::tempdir().expect("tempdir");
    let path = out.path().join("actual.cdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture(name).to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            path.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");
    serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse")
}

/// The decoded `waybill:resolve-ownership` object.
fn ownership(doc: &serde_json::Value) -> serde_json::Value {
    let raw = doc["metadata"]["properties"]
        .as_array()
        .expect("metadata properties")
        .iter()
        .find(|p| p["name"].as_str() == Some("waybill:resolve-ownership"))
        .map(|p| p["value"].as_str().unwrap_or_default().to_string())
        .expect("no waybill:resolve-ownership annotation");
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("resolve-ownership is not JSON: {raw} ({e})"))
}

fn names(v: &serde_json::Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not an array in {v}"))
        .iter()
        .filter_map(|x| x.as_str())
        .map(str::to_string)
        .collect()
}

/// Every resolve named by any component's membership.
fn resolves_on_components(doc: &serde_json::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for c in doc["components"].as_array().into_iter().flatten() {
        for p in c["properties"].as_array().into_iter().flatten() {
            if p["name"].as_str() != Some("waybill:pants-resolve") {
                continue;
            }
            let raw = p["value"].as_str().unwrap_or_default();
            if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(raw) {
                out.extend(items.iter().filter_map(|x| x.as_str()).map(str::to_string));
            }
        }
    }
    out
}

/// FR-007 — a declaring repository names its declared resolves.
#[test]
fn a_declaring_repository_names_its_declared_resolves() {
    let o = ownership(&scan("pants_resolve_edges"));
    assert_eq!(names(&o, "declared"), vec!["app", "tools"]);
    assert!(
        names(&o, "discovered").is_empty(),
        "nothing here was found by convention; got {o}"
    );
}

/// FR-007 — a convention-only repository names its discovered resolves.
/// The count form could only say "2", which does not tell a consumer which
/// two, nor whether it can partition.
#[test]
fn a_convention_only_repository_names_its_discovered_resolves() {
    let o = ownership(&scan("pants_discovered_resolves"));
    assert_eq!(names(&o, "discovered"), vec!["default", "lint"]);
    assert!(
        names(&o, "declared").is_empty(),
        "nothing here was declared; got {o}"
    );
}

/// SC-006 — the two lists together account for every resolve named on any
/// component. A resolve appearing in membership but in neither list is a
/// defect, not a third category.
#[test]
fn the_two_lists_account_for_every_resolve_on_any_component() {
    for f in ["pants_resolve_edges", "pants_discovered_resolves"] {
        let doc = scan(f);
        let o = ownership(&doc);
        let listed: BTreeSet<String> = names(&o, "declared")
            .into_iter()
            .chain(names(&o, "discovered"))
            .collect();
        let on_components = resolves_on_components(&doc);
        assert_eq!(
            on_components, listed,
            "{f}: resolves named on components and resolves listed at document \
             scope must be the same set"
        );
    }
}

/// FR-008 — answerable from the document alone. The two lists are disjoint,
/// so "which category is this resolve in" has exactly one answer.
#[test]
fn a_resolve_appears_in_exactly_one_category() {
    for f in ["pants_resolve_edges", "pants_discovered_resolves"] {
        let o = ownership(&scan(f));
        let declared: BTreeSet<String> = names(&o, "declared").into_iter().collect();
        let discovered: BTreeSet<String> = names(&o, "discovered").into_iter().collect();
        let both: Vec<&String> = declared.intersection(&discovered).collect();
        assert!(both.is_empty(), "{f}: resolve in both categories: {both:?}");
    }
}

/// T027 / FR-009 — naming a discovered resolve is information, not an
/// ownership claim. It still gets no anchor component. If this regresses,
/// m868's distinction has been softened by the back door.
#[test]
fn a_discovered_resolve_is_named_but_still_unanchored() {
    let doc = scan("pants_discovered_resolves");
    let o = ownership(&doc);
    assert_eq!(names(&o, "discovered"), vec!["default", "lint"]);

    let anchors: Vec<&str> = doc["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:generic/"))
        .collect();
    assert!(
        anchors.is_empty(),
        "a resolve found by filename convention must not gain an anchor — \
         naming it is information, not a declaration of ownership. Found: \
         {anchors:?}"
    );
}

/// The annotation must survive as valid JSON in every format, not only CDX.
#[test]
fn the_value_is_json_not_the_pre_911_key_value_string() {
    let o = ownership(&scan("pants_resolve_edges"));
    assert!(o.is_object(), "expected a JSON object, got {o}");
    for key in ["declared", "discovered", "weak_classification", "unanchored_lockfiles"] {
        assert!(o.get(key).is_some(), "missing key {key} in {o}");
    }
}
