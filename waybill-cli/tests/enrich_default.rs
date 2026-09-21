//! Issue #927 (m923) — T009 / FR-008 / FR-009 / SC-004.
//!
//! **The default flip must not reach a scan that makes no network calls.**
//!
//! Enrichment is where the batched/per-component choice lives, and `--offline`
//! disables enrichment entirely. So under `--offline` the choice must be
//! invisible: the same document, whichever path the flags would have selected.
//! If it is not, the flip has leaked into a code path that has no business
//! knowing about it.
//!
//! **Why this compares two runs of the current binary rather than one run
//! against the preserved pre-change binary.** The pre-change binary is a
//! local artifact of the implementing session; it does not exist in CI, so a
//! test written against it would silently skip there — which is the shape of
//! guard that reports success having compared nothing. Comparing the two flag
//! settings tests the same property and keeps testing it.
//!
//! **Why the comparison is masked.** `serialNumber` and `metadata.timestamp`
//! differ between any two runs by design. Measured on the pair this test
//! replaces: those two fields were the *only* differences out of a 4.2 MB
//! document, so masking them costs nothing and comparing without masking
//! would fail always.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Blank the two per-run fields. Anything else that differs is a real
/// difference and must fail the test.
fn mask(v: &mut serde_json::Value) {
    if let Some(obj) = v.as_object_mut() {
        if obj.contains_key("serialNumber") {
            obj.insert("serialNumber".into(), serde_json::Value::String("<masked>".into()));
        }
        if let Some(meta) = obj.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            if meta.contains_key("timestamp") {
                meta.insert("timestamp".into(), serde_json::Value::String("<masked>".into()));
            }
        }
    }
}

/// Build the fixture once. Each scan must run against the *same* directory:
/// the scan root's path appears throughout the emitted document, so a fresh
/// tempdir per run makes every comparison fail for a reason that has nothing
/// to do with enrichment. (Found the hard way — the first version of this
/// test did exactly that and reported an FR-008 violation that was its own.)
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        b"[package]\nname = \"waybill-fixture-enrich-default\"\nversion = \"0.1.0\"\n\
          edition = \"2021\"\n\n[dependencies]\nwaybill-fixture-dep = \"1.0.0\"\n",
    )
    .unwrap();
    std::fs::write(src.join("lib.rs"), b"// fixture\n").unwrap();
    dir
}

fn scan_offline(root: &std::path::Path, tag: &str, extra: &[&str]) -> serde_json::Value {
    let out = root.join(format!("sbom-{tag}.cdx.json"));

    let mut args: Vec<&str> = vec![
        "sbom", "scan",
        "--path", root.to_str().unwrap(),
        "--offline",
        "--output", out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);

    let status = Command::new(binary_path())
        .args(&args)
        .status()
        .expect("scan must run");
    assert!(status.success(), "offline scan failed with {status:?}, args {args:?}");

    let mut v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    mask(&mut v);
    v
}

/// FR-008 / SC-004. An enrichment-disabled scan produces the same document
/// whichever enrichment path the flags select, because neither runs.
#[test]
fn offline_output_is_unaffected_by_the_enrichment_path_m923() {
    let dir = fixture();
    let root = dir.path();
    let default_path = scan_offline(root, "default", &[]);
    let opt_out_path = scan_offline(root, "optout", &["--no-enrich-batch"]);

    assert_eq!(
        default_path, opt_out_path,
        "--offline output differs depending on the enrichment path. Enrichment \
         does not run offline, so the flip has reached a code path that makes no \
         network calls — which FR-008 forbids",
    );

    // And the legacy no-op flag must not perturb it either (FR-004).
    let legacy = scan_offline(root, "legacy", &["--enrich-batch"]);
    assert_eq!(
        default_path, legacy,
        "the legacy --enrich-batch flag changed offline output; as a no-op it \
         must change nothing",
    );
}

/// Guard on the guard: if the fixture ever stops producing components, the
/// comparison above would pass by comparing two empty documents.
#[test]
fn the_offline_fixture_actually_produces_components_m923() {
    let dir = fixture();
    let doc = scan_offline(dir.path(), "count", &[]);
    let n = doc
        .get("components")
        .and_then(|c| c.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        n > 0,
        "the offline fixture produced {n} components; the equality test above \
         would then be comparing two empty documents and passing for the wrong \
         reason",
    );
}
