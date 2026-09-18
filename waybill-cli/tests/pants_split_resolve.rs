//! Issue #911 (#902 item 4) — `--split=resolve` produces one SBOM per Pants
//! resolve.
//!
//! **Selection is a membership filter, not a graph walk.** The other split
//! modes BFS from a main-module seed. A resolve anchor is not a main-module,
//! and a resolve found by filename convention has no anchor component at all,
//! so a walk cannot reach it. One filter path serves both; two strategies
//! that must agree would disagree rarely and data-dependently.

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

struct Split {
    _dir: tempfile::TempDir,
    docs: Vec<(String, serde_json::Value)>,
    manifest: Option<serde_json::Value>,
}

fn split(name: &str) -> Split {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture(name).to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--split=resolve",
            "--output-dir",
            dir.path().to_str().expect("outdir"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");

    let mut docs = Vec::new();
    let mut manifest = None;
    for e in std::fs::read_dir(dir.path()).expect("read outdir") {
        let p = e.expect("entry").path();
        let fname = p.file_name().expect("name").to_string_lossy().to_string();
        let bytes = std::fs::read(&p).expect("read");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
        if fname == "split-manifest.json" {
            manifest = Some(v);
        } else {
            docs.push((fname, v));
        }
    }
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    Split { _dir: dir, docs, manifest }
}

fn membership(doc: &serde_json::Value, purl: &str) -> Option<Vec<String>> {
    let raw = doc["components"].as_array()?.iter().find_map(|c| {
        if c["purl"].as_str()? != purl {
            return None;
        }
        c["properties"]
            .as_array()?
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:pants-resolve"))
            .and_then(|p| p["value"].as_str())
            .map(str::to_string)
    })?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(
        v.as_array()?
            .iter()
            .filter_map(|x| x.as_str())
            .map(str::to_string)
            .collect(),
    )
}

fn edges(doc: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dep in doc["dependencies"].as_array().into_iter().flatten() {
        let from = dep["ref"].as_str().unwrap_or_default().to_string();
        for to in dep["dependsOn"].as_array().into_iter().flatten() {
            out.push((from.clone(), to.as_str().unwrap_or_default().to_string()));
        }
    }
    out.sort();
    out
}

const COMMON: &str = "pkg:pypi/waybill-fixture-common@1.0.0";
const SHARED_1: &str = "pkg:pypi/waybill-fixture-shared@1.0.0";
const SHARED_2: &str = "pkg:pypi/waybill-fixture-shared@2.0.0";

/// SC-007 — one document per resolve.
#[test]
fn one_document_per_resolve() {
    let s = split("pants_resolve_edges");
    let names: Vec<&str> = s.docs.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["app.generic.cdx.json", "tools.generic.cdx.json"]);
}

/// SC-007 / FR-011 — a shared package appears in every document whose resolve
/// pins it.
#[test]
fn a_shared_package_appears_in_every_document_that_pins_it() {
    let s = split("pants_resolve_edges");
    for (name, doc) in &s.docs {
        assert!(
            membership(doc, COMMON).is_some(),
            "{name} should contain the package both resolves pin"
        );
    }
}

/// FR-011a — membership is NOT narrowed to the containing document.
///
/// Narrowing would recreate the under-reporting this feature fixes, moved
/// from component scope to document scope and unrecoverable without the
/// unsplit document. A consumer triaging one resolve's SBOM must be able to
/// see that the same fix lands in another.
#[test]
fn a_shared_package_keeps_its_full_membership_in_every_document() {
    let s = split("pants_resolve_edges");
    for (name, doc) in &s.docs {
        assert_eq!(
            membership(doc, COMMON).as_deref(),
            Some(&["app".to_string(), "tools".to_string()][..]),
            "{name}: membership was narrowed to this document's own resolve"
        );
    }
}

/// SC-007a / FR-011c — edge separation, which needs no edge-level tagging:
/// an edge belongs to resolve R when both endpoints name R.
///
/// The unsplit document carries BOTH edges from the two-resolve component
/// (FR-011b); each per-resolve document must carry exactly the one that
/// belongs to it.
#[test]
fn each_document_carries_only_its_own_resolves_edge() {
    let s = split("pants_resolve_edges");
    for (name, doc) in &s.docs {
        let e = edges(doc);
        let (want, unwanted) = if name.starts_with("app") {
            (SHARED_1, SHARED_2)
        } else {
            (SHARED_2, SHARED_1)
        };
        assert!(
            e.contains(&(COMMON.to_string(), want.to_string())),
            "{name}: missing the edge belonging to this resolve. Edges:\n{e:#?}"
        );
        assert!(
            !e.contains(&(COMMON.to_string(), unwanted.to_string())),
            "{name}: carries an edge belonging to the OTHER resolve — the \
             filter is not separating them. Edges:\n{e:#?}"
        );
    }
}

/// C-5a — each document names its own resolve, not the repository.
///
/// `metadata.component` is chosen at emit time by m127's root-selector, which
/// looks for the single main-module in the projection. A resolve projection
/// has none until the anchor is promoted, and without that promotion every
/// sub-SBOM claims to be the whole monorepo — the failure m215 hit where 23
/// of 25 sub-SBOMs named the repository instead of themselves.
#[test]
fn each_document_names_its_own_resolve_when_the_resolve_is_declared() {
    let s = split("pants_resolve_edges");
    for (name, doc) in &s.docs {
        let root = doc["metadata"]["component"]["name"]
            .as_str()
            .unwrap_or_default();
        let expected = name.split('.').next().expect("slug");
        assert_eq!(
            root, expected,
            "{name}: root should name this resolve, got {root}"
        );
    }
}

/// SC-006a — the assumption the clarify decision rests on: a repository whose
/// resolves are ALL discovered by filename convention, and which therefore has
/// no anchors anywhere, still partitions.
///
/// If this fails, the decision not to anchor discovered resolves has to be
/// reopened rather than worked around.
#[test]
fn a_repository_with_no_anchors_still_partitions() {
    let s = split("pants_discovered_resolves");
    let names: Vec<&str> = s.docs.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["default.generic.cdx.json", "lint.generic.cdx.json"],
        "a convention-only repository must still partition — the filter needs \
         no anchor to start from"
    );
    // And each document holds that resolve's packages, not the other's.
    let (_, default_doc) = &s.docs[0];
    assert!(membership(default_doc, "pkg:pypi/waybill-fixture-alpha@1.0.0").is_some());
    assert!(membership(default_doc, "pkg:pypi/waybill-fixture-gamma@1.0.0").is_none());
}

/// The manifest names the resolve behind each file, which is what lets a
/// consumer map a document back to its resolve without opening it.
#[test]
fn the_manifest_identifies_each_document_by_resolve() {
    let s = split("pants_discovered_resolves");
    let m = s.manifest.expect("split-manifest.json");
    let ids: Vec<&str> = m["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["root_purl"].as_str())
        .collect();
    assert_eq!(ids, vec!["pkg:generic/default", "pkg:generic/lint"]);
}

/// C-5b / FR-012 — a repository with no Pants resolves gets a stated outcome
/// and the single-document fallback, never an empty directory and exit zero.
#[test]
fn a_repository_with_no_resolves_falls_back_rather_than_emitting_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture("bun_lock/minimal_repro").to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--split=resolve",
            "--output-dir",
            dir.path().to_str().expect("outdir"),
        ])
        .output()
        .expect("run waybill");
    assert!(out.status.success(), "scan should not fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no partitionable Pants resolves detected"),
        "the operator must be told what happened. stderr:\n{stderr}"
    );
}
