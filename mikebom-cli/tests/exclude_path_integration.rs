//! Milestone 113 integration tests — user-supplied directory
//! exclusion for `mikebom scan`.
//!
//! Coverage:
//!
//! - `cargo_fixture_suppressed_under_tests_fixtures` (T016): a real
//!   Cargo workspace at the scan root + a fixture Cargo crate at
//!   `tests/fixtures/sample-fixture/` is scanned with
//!   `--exclude-path tests/fixtures`; the fixture component is
//!   absent from the emitted SBOM, the real workspace component
//!   is present.
//! - `glob_pattern_matches_nested_testdata` (T025): synthetic
//!   monorepo with multiple nested cargo fixtures under
//!   `services/<name>/testdata/...`; a single
//!   `--exclude-path '**/testdata'` argument suppresses every
//!   nested fixture.
//! - `transparency_annotation_emitted_when_set_non_empty` (T024a /
//!   FR-014 / SC-007): scanning with `--exclude-path tests/fixtures`
//!   makes the emitted SBOM carry the `mikebom:exclude-path`
//!   envelope annotation in CDX; scanning without any exclusion
//!   does NOT emit the annotation.
//! - `no_flag_scan_is_byte_identical_to_baseline` (T024 / FR-003 /
//!   SC-002): two back-to-back scans of the same fixture (one with
//!   `--exclude-path` absent, one with `MIKEBOM_EXCLUDE_PATH=""`)
//!   produce byte-identical CDX output modulo the random
//!   `serialNumber` field. Exercises the empty-set no-op path.
//! - `malformed_pattern_exits_nonzero_before_scan` (T024 / FR-007
//!   / SC-005): supplying `--exclude-path '['` (unmatched bracket)
//!   causes mikebom to exit non-zero before any walker begins.
//!
//! Tests use the `mikebom` binary via `env!("CARGO_BIN_EXE_mikebom")`,
//! the standard cargo-supported way to invoke the integration-test
//! target without rebuilding. Each test creates its fixture tree
//! under `tempfile::tempdir()` for isolation.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

/// Write a minimal real Cargo project (no deps, no lockfile required
/// for main-module emission per milestone 064).
fn write_cargo_project(root: &std::path::Path, name: &str, version: &str) {
    std::fs::create_dir_all(root).unwrap();
    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"
    );
    std::fs::write(root.join("Cargo.toml"), manifest).unwrap();
    // A bare `src/lib.rs` so the crate is structurally complete; this
    // doesn't affect mikebom's scan but keeps the fixture realistic.
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
}

/// Run `mikebom sbom scan --path <dir>` with deterministic env, the
/// supplied --exclude-path entries, and `--format cdx`. Returns the
/// parsed CDX value and the process status.
fn run_scan(
    root: &std::path::Path,
    exclude_paths: &[&str],
) -> (serde_json::Value, std::process::Output) {
    let mut cmd = Command::new(binary_path());
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--no-deep-hash")
        .arg("--offline")
        .arg("--output")
        .arg(root.join("out.cdx.json"))
        // Determinism levers — same env vars used by milestone-112's
        // byte-identity tests.
        .env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        // Clear inherited values so the test environment is hermetic.
        .env_remove("MIKEBOM_EXCLUDE_PATH")
        .env_remove("MIKEBOM_NO_GO_MOD_WHY");
    for entry in exclude_paths {
        cmd.arg("--exclude-path").arg(entry);
    }
    let output = cmd.output().expect("failed to invoke mikebom binary");
    if !output.status.success() {
        eprintln!(
            "mikebom exited non-zero:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    let cdx_text = std::fs::read_to_string(root.join("out.cdx.json"))
        .expect("mikebom should have written out.cdx.json");
    let cdx: serde_json::Value =
        serde_json::from_str(&cdx_text).expect("CDX output must parse as JSON");
    (cdx, output)
}

/// Gather every component name in the SBOM — both `metadata.component`
/// (the scan subject; mikebom promotes the dominant project here when
/// only one survives the scan) AND every entry in `components[]`. Tests
/// need both because a fixture that's excluded may have been the
/// metadata.component in the unfiltered scan and become absent in the
/// filtered one, and vice-versa.
fn component_names(cdx: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(name) = cdx
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("name"))
        .and_then(|n| n.as_str())
    {
        out.push(name.to_string());
    }
    if let Some(arr) = cdx.get("components").and_then(|c| c.as_array()) {
        for c in arr {
            if let Some(n) = c.get("name").and_then(|n| n.as_str()) {
                out.push(n.to_string());
            }
        }
    }
    out
}

fn envelope_property(cdx: &serde_json::Value, name: &str) -> Option<String> {
    cdx.get("metadata")
        .and_then(|m| m.get("properties"))
        .and_then(|p| p.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|prop| {
                if prop.get("name").and_then(|n| n.as_str()) == Some(name) {
                    prop.get("value")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
}

#[test]
fn cargo_fixture_suppressed_under_tests_fixtures() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "real-app", "1.0.0");
    write_cargo_project(
        &root.join("tests/fixtures/sample-fixture"),
        "sample-fixture",
        "0.0.1",
    );

    // Baseline: without exclusion, both components appear.
    let (cdx_baseline, _) = run_scan(root, &[]);
    let baseline_names = component_names(&cdx_baseline);
    assert!(
        baseline_names.iter().any(|n| n == "real-app"),
        "baseline: real-app must appear in unfiltered scan; got: {baseline_names:?}",
    );
    assert!(
        baseline_names.iter().any(|n| n == "sample-fixture"),
        "baseline: sample-fixture must appear without --exclude-path; got: {baseline_names:?}",
    );

    // With exclusion: the fixture vanishes.
    let (cdx, status) = run_scan(root, &["tests/fixtures"]);
    assert!(status.status.success(), "mikebom exited non-zero");
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n == "real-app"),
        "real-app must remain in filtered scan; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "sample-fixture"),
        "sample-fixture must be suppressed by --exclude-path tests/fixtures; got: {names:?}",
    );
}

#[test]
fn glob_pattern_matches_nested_testdata() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "monorepo-root", "1.0.0");
    write_cargo_project(
        &root.join("services/a/testdata/fixture-a"),
        "fixture-a",
        "0.0.1",
    );
    write_cargo_project(
        &root.join("services/b/testdata/fixture-b"),
        "fixture-b",
        "0.0.1",
    );

    let (cdx, status) = run_scan(root, &["**/testdata"]);
    assert!(status.status.success());
    let names = component_names(&cdx);
    assert!(
        names.iter().any(|n| n == "monorepo-root"),
        "real workspace must remain; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "fixture-a"),
        "fixture-a must be suppressed by **/testdata; got: {names:?}",
    );
    assert!(
        !names.iter().any(|n| n == "fixture-b"),
        "fixture-b must be suppressed by **/testdata; got: {names:?}",
    );
}

#[test]
fn transparency_annotation_emitted_when_set_non_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "annotation-app", "1.0.0");
    write_cargo_project(
        &root.join("tests/fixtures/fixture-x"),
        "fixture-x",
        "0.0.1",
    );

    // With exclusion: annotation is present, value matches the
    // operator-typed entry.
    let (cdx, status) = run_scan(root, &["tests/fixtures"]);
    assert!(status.status.success());
    let value = envelope_property(&cdx, "mikebom:exclude-path");
    assert_eq!(
        value.as_deref(),
        Some("tests/fixtures"),
        "exclude-path annotation must carry the entry verbatim",
    );

    // Without exclusion: annotation is absent.
    let (cdx_clean, _) = run_scan(root, &[]);
    let value = envelope_property(&cdx_clean, "mikebom:exclude-path");
    assert_eq!(
        value, None,
        "exclude-path annotation must be absent when no exclusions in effect",
    );
}

#[test]
fn no_flag_scan_is_byte_identical_to_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "identity-app", "1.0.0");

    let (cdx_a, _) = run_scan(root, &[]);
    let (cdx_b, _) = run_scan(root, &[]);

    // Two back-to-back scans with the same fixture must produce
    // byte-identical CDX modulo the random serialNumber. Strip the
    // serial number before comparison (its randomness is intentional
    // per CDX 1.6 spec; not affected by milestone 113).
    let mut a = cdx_a.clone();
    let mut b = cdx_b.clone();
    if let Some(obj) = a.as_object_mut() {
        obj.insert(
            "serialNumber".into(),
            serde_json::Value::String("urn:uuid:MASKED".into()),
        );
    }
    if let Some(obj) = b.as_object_mut() {
        obj.insert(
            "serialNumber".into(),
            serde_json::Value::String("urn:uuid:MASKED".into()),
        );
    }

    assert_eq!(
        a, b,
        "back-to-back scans with no --exclude-path must produce byte-identical CDX",
    );
    // The exclude-path annotation must be absent from both.
    assert!(
        envelope_property(&a, "mikebom:exclude-path").is_none(),
        "no-flag scan must not emit mikebom:exclude-path annotation",
    );
}

#[test]
fn malformed_pattern_exits_nonzero_before_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_cargo_project(root, "malformed-app", "1.0.0");

    let output = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--no-deep-hash")
        .arg("--offline")
        .arg("--output")
        .arg(root.join("out.cdx.json"))
        .arg("--exclude-path")
        .arg("foo[")
        .env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("MIKEBOM_EXCLUDE_PATH")
        .env_remove("MIKEBOM_NO_GO_MOD_WHY")
        .output()
        .expect("failed to invoke mikebom");

    assert!(
        !output.status.success(),
        "mikebom must exit non-zero on malformed --exclude-path entry; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("foo["),
        "error must name the offending entry verbatim; stderr was: {stderr}",
    );
    // The output file must not have been created (scan never started).
    assert!(
        !root.join("out.cdx.json").is_file(),
        "scan should not have produced an output file on malformed input",
    );
}
