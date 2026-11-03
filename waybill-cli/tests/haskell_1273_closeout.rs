//! Milestone 1273 closeout — the two cross-cutting contracts, asserted as
//! their own checks rather than as side effects of a story's tests.
//!
//! **C1 / FR-021 / FR-022** (the CLI surface, and no host dependence) are
//! verified outside the test suite, by diffing `waybill sbom scan --help`
//! against `main` and grepping the milestone's diff for `Command`, `reqwest`
//! and `PATH` reads. Both are recorded in the closeout commit; a test cannot
//! diff itself against another revision.
//!
//! What remains testable here is **C23** and **SC-010**.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/haskell_hpack")
        .join(name)
}

struct Scan {
    doc: Value,
    stderr: String,
}

fn scan(root: &Path, offline: bool) -> Scan {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("o.cdx.json");
    let mut cmd = Command::new(binary_path());
    if offline {
        cmd.arg("--offline");
    }
    let result = cmd
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", path.display()))
        .env("WAYBILL_NIXPKGS_CACHE", out.path().join("__no_cache__"))
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    Scan {
        doc: serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

fn doc_prop(doc: &Value, key: &str) -> Option<String> {
    doc.pointer("/metadata/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

// ───────────────────────────────────────────────────────────────────────
// T083 / C23 — every signal readable from the DOCUMENT alone
// ───────────────────────────────────────────────────────────────────────

/// SC-009: *"observable by reading the emitted document alone, with no access
/// to the scan log."*
///
/// Asserted by parsing the emitted JSON and **never touching stderr**, which
/// is the whole point: a log-only diagnostic leaves the artifact exactly as
/// dishonest as the defect this milestone fixes. The log half is FR-020 and
/// is asserted separately, in US2's suite.
#[test]
fn m1273_every_signal_is_readable_from_the_document_alone() {
    // An unreadable manifest: C8's reason, and C8's completeness verdict.
    let s = scan(&fixture("malformed"), true);
    let outcome: Value = serde_json::from_str(
        &doc_prop(&s.doc, "waybill:hpack-read-outcome")
            .expect("C200 — the hpack read outcome is in the document"),
    )
    .unwrap();
    assert_eq!(
        outcome["manifests-unread"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        1,
        "FR-003 — the unread manifest is named in the document"
    );
    assert_eq!(
        outcome["manifests-unread"][0]["reason"].as_str(),
        Some("malformed-yaml"),
        "with a closed reason code, not prose"
    );
    assert_ne!(
        doc_prop(&s.doc, "waybill:graph-completeness").as_deref(),
        Some("complete"),
        "FR-005 — and completeness is not asserted over it"
    );

    // An unmodelled construct: C9.
    let d = scan(&fixture("defaults"), true);
    let o: Value = serde_json::from_str(
        &doc_prop(&d.doc, "waybill:hpack-read-outcome").unwrap(),
    )
    .unwrap();
    assert_eq!(
        o["manifests-partial"][0]["unmodelled-construct"].as_str(),
        Some("defaults"),
        "FR-001b — the construct is named in the document"
    );

    // A suppressed sibling: C7's record of which file was read.
    let g = scan(&fixture("manifest_and_generated_cabal"), true);
    let og: Value = serde_json::from_str(
        &doc_prop(&g.doc, "waybill:hpack-read-outcome").unwrap(),
    )
    .unwrap();
    assert_eq!(
        og["generated-cabal-suppressed"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        1,
        "FR-002 — the document records that a generated sibling was suppressed"
    );
    assert_eq!(
        og["manifests-read"].as_array().map(Vec::len).unwrap_or(0),
        1,
        "and which manifest was read instead"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T073 / SC-010 — `--offline` is honoured, and so is its absence
// ───────────────────────────────────────────────────────────────────────

/// SC-010 requires the `defaults:` case be verified **two ways**: by
/// asserting on the document, and by running under `--offline`.
///
/// One alone is not enough. A pass that silently skipped itself offline would
/// satisfy the `--offline` run; a pass that fetched successfully would
/// satisfy the document assertion. So this runs the same fixture **with and
/// without** the flag and requires the hpack outcome to be identical — if
/// `defaults:` were ever resolved over the network, the two would differ.
#[test]
fn m1273_defaults_behaves_identically_with_and_without_offline() {
    let off = scan(&fixture("defaults"), true);
    let on = scan(&fixture("defaults"), false);

    let outcome = |s: &Scan| -> Value {
        serde_json::from_str(&doc_prop(&s.doc, "waybill:hpack-read-outcome").unwrap())
            .unwrap()
    };
    assert_eq!(
        outcome(&off),
        outcome(&on),
        "a `defaults:` manifest must read identically offline and online — \
         any difference means the construct was resolved over the network, \
         which FR-001c forbids"
    );
    // And nothing reached for the URL hpack itself would have fetched.
    for s in [&off, &on] {
        assert!(
            !s.stderr.contains("raw.githubusercontent.com"),
            "stderr:\n{}",
            s.stderr
        );
    }
}
