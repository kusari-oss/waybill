//! Issue #914 (m912) — where the resolve identity must be ABSENT.
//!
//! FR-008: a document that is not a per-resolve split has no resolve to name.
//! FR-009 / Principle III: absent, not present-and-empty — "this is not a
//! per-resolve document" and "a resolve with no name" are different claims,
//! and a consumer reading a document produced before this feature must be
//! able to tell the identity is missing rather than blank.
//!
//! The `--split=workspace` case is checked against a fixture that actually
//! PRODUCES workspace documents. An earlier version of this check ran against
//! a Pants fixture with no workspace boundaries, where the split emitted
//! nothing at all and the assertion passed vacuously.

use std::path::PathBuf;
use std::process::Command;

const IDENTITY: &str = "waybill:document-resolve";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn has_identity(doc: &serde_json::Value) -> bool {
    doc["metadata"]["properties"]
        .as_array()
        .map(|a| a.iter().any(|p| p["name"].as_str() == Some(IDENTITY)))
        .unwrap_or(false)
}

fn split_docs(name: &str, mode: &str) -> Vec<(String, serde_json::Value)> {
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
            &format!("--split={mode}"),
            "--output-dir",
            dir.path().to_str().expect("outdir"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success(), "scan failed: {status}");

    let mut out = Vec::new();
    for e in std::fs::read_dir(dir.path()).expect("read outdir") {
        let p = e.expect("entry").path();
        let f = p.file_name().expect("name").to_string_lossy().to_string();
        if f == "split-manifest.json" || !f.ends_with(".cdx.json") {
            continue;
        }
        out.push((
            f,
            serde_json::from_slice(&std::fs::read(&p).expect("read")).expect("parse"),
        ));
    }
    out
}

/// SC-008, `--split=workspace`. Guarded against the vacuous pass: the fixture
/// must actually produce documents, or this proves nothing.
#[test]
fn a_workspace_split_document_carries_no_resolve_identity() {
    let docs = split_docs("pants_go", "workspace");
    assert!(
        !docs.is_empty(),
        "fixture produced no workspace documents — the assertion below would \
         pass vacuously"
    );
    for (name, doc) in &docs {
        assert!(!has_identity(doc), "{name}: identity must be absent (FR-008)");
    }
}

/// SC-008, `--split=directory`.
#[test]
fn a_directory_split_document_carries_no_resolve_identity() {
    let docs = split_docs("pants_go", "directory");
    assert!(!docs.is_empty(), "fixture produced no directory documents");
    for (name, doc) in &docs {
        assert!(!has_identity(doc), "{name}: identity must be absent (FR-008)");
    }
}

/// SC-008, unsplit. An unsplit document represents EVERY resolve rather than
/// one, so it has no single identity to state.
#[test]
fn an_unsplit_document_carries_no_resolve_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("u.cdx.json");
    let status = Command::new(env!("CARGO_BIN_EXE_waybill"))
        .args([
            "sbom",
            "scan",
            "--path",
            fixture("pants_discovered_resolves").to_str().expect("fixture"),
            "--offline",
            "--format",
            "cyclonedx-json",
            "--output",
            out.to_str().expect("out"),
        ])
        .status()
        .expect("run waybill");
    assert!(status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).expect("read")).expect("parse");

    assert!(!has_identity(&doc), "an unsplit document has no single resolve");

    // But it DOES still carry the repository-wide ownership statement — the
    // two are separately readable (SC-009), and removing one leaves the other
    // intact and meaningful.
    let has_ownership = doc["metadata"]["properties"]
        .as_array()
        .map(|a| {
            a.iter()
                .any(|p| p["name"].as_str() == Some("waybill:resolve-ownership"))
        })
        .unwrap_or(false);
    assert!(has_ownership, "C161 must survive independently of C163 (SC-009)");
}
