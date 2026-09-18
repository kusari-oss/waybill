//! Issue #914 (m912) — a per-resolve split document says which resolve it is.
//!
//! Before this, two documents from a convention-only repository were
//! indistinguishable on every document-scope field mentioning resolves: both
//! named the repository in `metadata.component`, and both carried a
//! byte-identical `waybill:resolve-ownership` describing the repository
//! rather than the document. A reader holding one file saw two resolve names
//! and nothing saying which file it had.
//!
//! **The identity is namespace-qualified** (`python:default`). `[python.resolves]`
//! and `[jvm.resolves]` are separate namespaces in `pants.toml`, so one
//! repository can declare `default` in both, and a bare name would let a
//! document claim to be `default` without saying which (FR-001a).

use std::path::PathBuf;
use std::process::Command;

const IDENTITY: &str = "waybill:document-resolve";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

struct Split {
    _dir: tempfile::TempDir,
    dir: PathBuf,
    docs: Vec<(String, serde_json::Value)>,
}

/// Run a split in `mode` over `fixture`, in one format.
fn split_mode(name: &str, mode: &str, format: &str, ext: &str) -> Split {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture(name).to_str().expect("fixture"),
            "--offline",
            "--format",
            format,
            &format!("--split={mode}"),
            "--output-dir",
            dir.path().to_str().expect("outdir"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");

    let mut docs = Vec::new();
    for e in std::fs::read_dir(dir.path()).expect("read outdir") {
        let p = e.expect("entry").path();
        let fname = p.file_name().expect("name").to_string_lossy().to_string();
        if fname == "split-manifest.json" || !fname.ends_with(ext) {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&p).expect("read")).expect("parse");
        docs.push((fname, v));
    }
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    let path = dir.path().to_path_buf();
    Split { _dir: dir, dir: path, docs }
}

fn split(name: &str) -> Split {
    split_mode(name, "resolve", "cyclonedx-json", ".cdx.json")
}

/// The DECODED identity. CycloneDX spec'es `properties[].value` as a string,
/// so the array arrives as JSON-in-string here while SPDX carries a real
/// array. The decoded value is the contract, not the bytes — the same split
/// C143 and C161 already make, and the trap m911 fell into.
fn identity_cdx(doc: &serde_json::Value) -> Option<Vec<String>> {
    let raw = doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(IDENTITY))?["value"]
        .as_str()?;
    serde_json::from_str(raw).ok()
}

fn ownership_cdx(doc: &serde_json::Value) -> Option<String> {
    doc["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(IDENTITY.replace("document-resolve", "resolve-ownership").as_str()))
        .and_then(|p| p["value"].as_str())
        .map(str::to_string)
}

fn root_name(doc: &serde_json::Value) -> Option<String> {
    doc["metadata"]["component"]["name"].as_str().map(str::to_string)
}

// ---------------------------------------------------------------
// US1 — a document identifies itself
// ---------------------------------------------------------------

/// SC-001 + SC-002. The headline: each document from a convention-only
/// repository names its OWN resolve, from its own bytes.
#[test]
fn each_discovered_resolve_document_states_its_own_resolve() {
    let s = split("pants_discovered_resolves");
    assert_eq!(s.docs.len(), 2, "expected one document per resolve");

    let ids: Vec<Option<Vec<String>>> =
        s.docs.iter().map(|(_, d)| identity_cdx(d)).collect();
    assert_eq!(ids[0].as_deref(), Some(&["python:default".to_string()][..]));
    assert_eq!(ids[1].as_deref(), Some(&["python:lint".to_string()][..]));

    // SC-002: and they differ, which is the whole complaint. Before m912 every
    // doc-scope field mentioning resolves was byte-identical across these two.
    assert_ne!(ids[0], ids[1]);
}

/// The documents still name the REPOSITORY in their root, and that is fine —
/// FR-006 refuses to invent an owning component for a discovered resolve, so
/// the identity is what closes the gap, not a fabricated root. If this ever
/// starts naming the resolve, someone has synthesised the anchor m868
/// declined to emit.
#[test]
fn a_discovered_resolve_document_still_has_no_invented_root() {
    let s = split("pants_discovered_resolves");
    for (name, doc) in &s.docs {
        assert_eq!(
            root_name(doc).as_deref(),
            Some("pants_discovered_resolves"),
            "{name}: root should still name the repository (FR-006)"
        );
    }
}

/// SC-003 / FR-003. The answer lives in the content. A file renamed, attached
/// to a ticket, or ingested by a scanner keeps it — which is the entire
/// scenario the feature exists for, since the manifest and the filename slug
/// both answer it only while they travel with the file.
#[test]
fn renaming_a_document_does_not_change_the_answer() {
    let s = split("pants_discovered_resolves");
    let (orig_name, doc) = &s.docs[0];
    let before = identity_cdx(doc).expect("identity present");

    let renamed = s.dir.join("something-else-entirely.json");
    std::fs::copy(s.dir.join(orig_name), &renamed).expect("copy");
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&renamed).expect("read")).expect("parse");

    assert_eq!(identity_cdx(&after), Some(before));
}

/// R3 — anchoring is Pex-only, so a JVM Pants repository has NO anchors and
/// every one of its documents is in the failing case, declared or not. A
/// Python-only fixture would let a Python-only implementation look complete
/// while this entire ecosystem stayed broken.
#[test]
fn a_jvm_repository_identifies_its_resolves_too() {
    let s = split("pants_coursier_jvm/multi_resolve");
    assert_eq!(s.docs.len(), 3, "three JVM resolves");
    let ids: Vec<Vec<String>> = s
        .docs
        .iter()
        .map(|(n, d)| identity_cdx(d).unwrap_or_else(|| panic!("{n}: no identity")))
        .collect();
    assert_eq!(ids[0], ["jvm:default"]);
    assert_eq!(ids[1], ["jvm:junit"]);
    assert_eq!(ids[2], ["jvm:scalatest"]);

    // Every one of them names the repository in its root — the failing case,
    // across the board, which is what makes this fixture necessary.
    for (n, d) in &s.docs {
        assert_eq!(root_name(d).as_deref(), Some("multi_resolve"), "{n}");
    }
}

/// SC-002a / FR-001a / C-2 — the one thing a bare name cannot do.
///
/// Also the #919 reproducer. The split groups on the BARE resolve name, so
/// `python:default` and `jvm:default` collapse into one document today. That
/// document represents both, so stating either alone would be false and it
/// states both (C-6). Self-correcting: once #919 lands the state cannot arise
/// and this expectation becomes two documents with one identity each.
#[test]
fn same_name_in_two_namespaces_is_distinguishable() {
    let s = split("pants_namespace_collision");
    let merged = s
        .docs
        .iter()
        .find(|(n, _)| n.starts_with("default."))
        .expect("a document for `default`");
    assert_eq!(
        identity_cdx(&merged.1),
        Some(vec!["jvm:default".to_string(), "python:default".to_string()]),
        "a document representing two resolves must name both (C-6)"
    );

    let lint = s
        .docs
        .iter()
        .find(|(n, _)| n.starts_with("lint."))
        .expect("a document for `lint`");
    assert_eq!(identity_cdx(&lint.1), Some(vec!["python:lint".to_string()]));
}

/// The collision is INVISIBLE without a third resolve, and that is worth
/// pinning. With only the two same-named resolves the split sees one group,
/// decides the repository is not partitionable, and emits a single SBOM with
/// no split at all — so #919 hides behind the degenerate-split fallback.
#[test]
fn the_collision_fixture_needs_its_third_resolve_to_split_at_all() {
    let s = split("pants_namespace_collision");
    assert_eq!(
        s.docs.len(),
        2,
        "two groups: the merged `default` and the uncollided `lint`. \
         Drop `lint` and the split degenerates to one document, hiding #919."
    );
}

// ---------------------------------------------------------------
// FR-007 / SC-006 / SC-007 — what must NOT change
// ---------------------------------------------------------------

/// FR-007 + SC-006. The repository-wide ownership statement stays
/// repository-wide: identical across a repository's documents, describing what
/// else exists.
///
/// This is the test that rejects the obvious wrong fix for the whole feature —
/// narrowing C161 to the document's own resolve instead of adding C163. That
/// would make the documents differ, at the cost of the reader's view of what
/// else the repository contains.
#[test]
fn the_repository_wide_ownership_statement_stays_repository_wide() {
    let s = split("pants_discovered_resolves");
    let a = ownership_cdx(&s.docs[0].1).expect("ownership on doc 0");
    let b = ownership_cdx(&s.docs[1].1).expect("ownership on doc 1");
    assert_eq!(a, b, "C161 must remain identical across split documents");
    assert!(
        a.contains("\"discovered\"") && a.contains("default") && a.contains("lint"),
        "C161 must still name every resolve in the repository, got: {a}"
    );
}

/// SC-007 / FR-006 / C-5 — **the guard on the constraint the whole feature is
/// downstream of.**
///
/// The easy wrong implementation synthesises the anchor m868 refused, which
/// would make the identity trivial to derive and would pass every other test
/// in this file. Component counts are what catches it. Baseline measured
/// pre-change: default=2, lint=1.
#[test]
fn no_document_gains_a_component() {
    let s = split("pants_discovered_resolves");
    let counts: Vec<usize> = s
        .docs
        .iter()
        .map(|(_, d)| d["components"].as_array().map_or(0, |a| a.len()))
        .collect();
    assert_eq!(
        counts,
        vec![2, 1],
        "component counts must match the pre-change baseline; a change here \
         means an owning component was invented for a discovered resolve"
    );

    // And nothing anywhere claims to BE the resolve.
    for (name, doc) in &s.docs {
        for c in doc["components"].as_array().into_iter().flatten() {
            assert_ne!(
                c["purl"].as_str(),
                Some("pkg:generic/default"),
                "{name}: a synthetic anchor appeared for a discovered resolve"
            );
        }
    }
}

// ---------------------------------------------------------------
// US2 — every split document answers the same way
// ---------------------------------------------------------------

/// FR-005 / SC-005 / C-3. A declared Python resolve's document states its
/// resolve TWICE — once in the root component, which m868's anchor promotion
/// already provided, and once in the identity. Two fields stating one fact
/// drift; this is the test that stops it.
///
/// This session has already produced two instances of that class: the C143
/// mis-parse risk and the C161 double-encoding.
#[test]
fn where_a_root_also_names_the_resolve_the_two_agree() {
    let s = split("pants_resolve_edges");
    assert_eq!(s.docs.len(), 2, "two declared resolves");

    for (name, doc) in &s.docs {
        let root = root_name(doc).unwrap_or_else(|| panic!("{name}: no root"));
        let id = identity_cdx(doc).unwrap_or_else(|| panic!("{name}: no identity"));
        assert_eq!(id.len(), 1, "{name}: a declared resolve is singular");

        // The identity is namespace-qualified; the root is not. Agreement is
        // on the resolve NAME, which is the fact both are stating.
        let bare = id[0].split_once(':').expect("qualified identity").1;
        assert_eq!(
            bare, root,
            "{name}: identity {id:?} and root component disagree"
        );
    }
}

/// FR-004 / SC-004. **One procedure, no branch on provenance.**
///
/// The same expression reads the identity from a discovered-resolve document,
/// a declared-resolve document, and a JVM document. Without this a consumer
/// must first determine whether a resolve was declared in order to know where
/// to read its identity — which is the question it came to ask.
#[test]
fn one_reading_procedure_answers_for_every_provenance() {
    // Discovered (no anchor), declared (anchor promoted to root), and JVM
    // (no anchoring mechanism at all — R3).
    let cases = [
        ("pants_discovered_resolves", "python:default"),
        ("pants_resolve_edges", "python:app"),
        ("pants_coursier_jvm/multi_resolve", "jvm:default"),
    ];

    for (fixture_name, expected_first) in cases {
        let s = split(fixture_name);
        // The identical expression in all three cases.
        let ids: Vec<Vec<String>> = s
            .docs
            .iter()
            .map(|(n, d)| {
                identity_cdx(d).unwrap_or_else(|| {
                    panic!("{fixture_name}/{n}: no identity — a consumer would have \
                            to branch on provenance here")
                })
            })
            .collect();
        assert!(!ids.is_empty(), "{fixture_name}: no documents");
        assert_eq!(
            ids[0][0], expected_first,
            "{fixture_name}: first document's identity"
        );
    }
}
