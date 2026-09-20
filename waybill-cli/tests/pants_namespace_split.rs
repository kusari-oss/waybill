//! Issue #919 (m922) — `--split=resolve` must not merge resolves that share a
//! name across Pants language namespaces.
//!
//! `[python.resolves]` and `[jvm.resolves]` are separate namespaces, so a
//! repository can legitimately declare `default` in both. The split grouped on
//! the bare name, so the two merged into one document whose contents were the
//! union of a Python resolve and a JVM resolve — and a consumer asking for
//! "the production Python resolve" silently received another namespace's
//! packages.

use std::collections::BTreeSet;
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
    warned_not_partitionable: bool,
}

fn split(name: &str) -> Split {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
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
        .output()
        .expect("run waybill");
    assert!(out.status.success(), "scan failed: {}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let warned = stderr.contains("no partitionable Pants resolves");

    let mut docs = Vec::new();
    let mut manifest = None;
    for e in std::fs::read_dir(dir.path()).expect("read outdir") {
        let p = e.expect("entry").path();
        let f = p.file_name().expect("name").to_string_lossy().to_string();
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&p).expect("read")).expect("parse");
        if f == "split-manifest.json" {
            manifest = Some(v);
        } else if f.ends_with(".cdx.json") {
            docs.push((f, v));
        }
    }
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    Split { _dir: dir, docs, manifest, warned_not_partitionable: warned }
}

fn purls(doc: &serde_json::Value) -> BTreeSet<String> {
    doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["purl"].as_str().map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------
// US1 — two resolves sharing a name produce two documents
// ---------------------------------------------------------------

/// SC-001 + SC-002. The headline. Today this fixture yields ONE `default.*`
/// document containing a Maven jar and a PyPI wheel.
#[test]
fn same_name_in_two_namespaces_yields_two_documents() {
    let s = split("pants_namespace_collision");

    let defaults: Vec<&(String, serde_json::Value)> = s
        .docs
        .iter()
        .filter(|(n, _)| n.contains("default"))
        .collect();
    assert_eq!(
        defaults.len(),
        2,
        "expected one document per `default` resolve, got {:?}",
        s.docs.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    // SC-002: disjoint. The whole defect is that these were one set.
    let a = purls(&defaults[0].1);
    let b = purls(&defaults[1].1);
    assert!(
        a.is_disjoint(&b),
        "the two `default` documents share components: {:?} vs {:?}",
        a,
        b
    );

    // And each holds only its own namespace's package.
    let all: BTreeSet<&String> = a.union(&b).collect();
    assert!(
        all.iter().any(|p| p.starts_with("pkg:maven/")),
        "the JVM resolve's package is missing entirely"
    );
    assert!(
        all.iter().any(|p| p.starts_with("pkg:pypi/")),
        "the Python resolve's package is missing entirely"
    );
    for (name, doc) in defaults {
        let set = purls(doc);
        let maven = set.iter().filter(|p| p.starts_with("pkg:maven/")).count();
        let pypi = set.iter().filter(|p| p.starts_with("pkg:pypi/")).count();
        assert!(
            maven == 0 || pypi == 0,
            "{name} contains BOTH a Maven and a PyPI package — the namespaces are \
             still merged: {set:?}"
        );
    }
}

/// SC-004a / FR-002a. The two documents must be distinguishable on disk and in
/// the manifest. Note the existing filename-collision fallback cannot do this:
/// it hashes the root's source directory and a resolve projection's synthetic
/// root has an empty one, so both colliding resolves hash identically.
#[test]
fn colliding_documents_have_distinct_filenames_and_manifest_entries() {
    let s = split("pants_namespace_collision");

    // Guard against a vacuous pass. Before the fix there is only ONE `default`
    // document, so "no duplicate filenames" is trivially true and this test
    // proves nothing. Assert the collision is actually present first.
    assert_eq!(
        s.docs.iter().filter(|(n, _)| n.contains("default")).count(),
        2,
        "only one `default` document — the collision is still merged, so the \
         filename assertions below would pass vacuously"
    );

    let names: Vec<&String> = s.docs.iter().map(|(n, _)| n).collect();
    let unique: BTreeSet<&&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "duplicate filenames: {names:?}");

    // Distinct is not enough: BOTH colliding documents must be qualified.
    // The declared side has an m868 anchor, and taking the naming root from
    // that anchor left it as bare `default.generic.cdx.json` while only the
    // unanchored side became `jvm-default.…`. Distinct, asymmetric, and the
    // bare one is indistinguishable from a repository with no collision.
    for (n, _) in s.docs.iter().filter(|(n, _)| n.contains("default")) {
        assert!(
            n.starts_with("python-default.") || n.starts_with("jvm-default."),
            "{n} is not namespace-qualified; only one side of the collision was"
        );
    }

    let m = s.manifest.expect("split-manifest.json");
    let ids: Vec<&str> = m["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["subproject_id"].as_str())
        .collect();
    let roots: Vec<&str> = m["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["root_purl"].as_str())
        .collect();
    assert_eq!(
        ids.iter().collect::<BTreeSet<_>>().len(),
        ids.len(),
        "duplicate manifest subproject_id: {ids:?}"
    );
    assert_eq!(
        roots.iter().collect::<BTreeSet<_>>().len(),
        roots.len(),
        "duplicate manifest root_purl: {roots:?}"
    );
}

// ---------------------------------------------------------------
// US3 — a repository whose only resolves collide still splits
// ---------------------------------------------------------------

/// SC-004 / FR-003. Today: the split sees one group, declares the repository
/// not partitionable, and emits NO split at all — the simplest reproduction,
/// swallowed behind a warning that reads like correct behaviour.
#[test]
fn a_collision_only_repository_still_splits() {
    let s = split("pants_namespace_collision_only");
    assert!(
        !s.warned_not_partitionable,
        "the not-partitionable fallback fired: two colliding resolves were counted \
         as one group"
    );
    assert_eq!(
        s.docs.len(),
        2,
        "expected two documents, got {:?}",
        s.docs.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
}

/// FR-009. Only the miscount is being fixed. A repository with genuinely ONE
/// resolve is still degenerate and must still fall back to a single SBOM.
///
/// Without this, "make the collision-only fixture split" has an obvious wrong
/// implementation: drop the `groups.len() <= 1` check entirely. That passes
/// US3 and silently turns every single-resolve repository into a one-document
/// "split" it never asked for.
#[test]
fn a_genuinely_single_resolve_repository_still_falls_back() {
    let s = split("pants_coursier_jvm/minimal_jvm");
    assert!(
        s.warned_not_partitionable,
        "a repository with one resolve must still hit the fallback; got documents {:?}",
        s.docs.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    assert!(
        s.docs.is_empty(),
        "the fallback emits a single unsplit SBOM, not split documents"
    );
}
