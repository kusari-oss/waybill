//! Milestone 219 — integration tests for `--split=<mode>` extensibility.
//!
//! Covers US1 (directory-mode grouping) + US2 (extensibility gate +
//! invalid-mode error + INFO log substring) + SC-005 (bare `--split`
//! vs `--split=workspace` byte-identity).

use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn fixture_two_dir_polyglot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/split_modes/two_dir_polyglot")
}

fn waybill_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_waybill"))
}

/// Invoke waybill with the given split-mode arg (None = flag absent;
/// Some("") = bare `--split`; Some("workspace") = `--split=workspace`;
/// etc.) against `fixture` writing to a fresh output dir. Returns the
/// output dir path + captured stderr for log assertions.
fn run_split(fixture: &PathBuf, mode: Option<&str>) -> (tempfile::TempDir, String) {
    let out = tempdir().expect("output tempdir");
    let home = tempdir().expect("home tempdir");
    let mut cmd = Command::new(waybill_bin());
    // Isolated HOME per m217 goroot_skip.rs pattern.
    cmd.env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env(
            "WAYBILL_FIXTURES_DIR",
            env!("WAYBILL_FIXTURES_DIR"),
        )
        .env("RUST_LOG", "info")
        // Disable ANSI color escapes so log-substring assertions
        // can match literal text like `mode=directory` without
        // stripping color codes.
        .env("NO_COLOR", "1")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output-dir")
        .arg(out.path())
        .arg("--no-deep-hash");
    match mode {
        None => {} // no --split flag
        Some("") => {
            cmd.arg("--split");
        }
        Some(value) => {
            cmd.arg(format!("--split={value}"));
        }
    }
    let output = cmd.output().expect("waybill invokes");
    assert!(
        output.status.success(),
        "waybill failed (mode={:?}): stderr={}",
        mode,
        String::from_utf8_lossy(&output.stderr)
    );
    // Concatenate stdout + stderr — some tracing subscriber
    // configurations write to stdout, others to stderr; be robust.
    let mut combined = String::from_utf8_lossy(&output.stderr).to_string();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    (out, combined)
}

fn count_cdx_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .expect("read out dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".cdx.json")
        })
        .count()
}

// -------- US1: directory-mode grouping (SC-001, SC-003, SC-004) --------

#[test]
fn us1_split_directory_emits_one_sbom_per_dir_on_polyglot_fixture() {
    let (out, _) = run_split(&fixture_two_dir_polyglot(), Some("directory"));
    // 3 main-modules (cargo + npm at services/api, golang at services/worker)
    // → 2 sub-SBOMs (one per dir). Delivers SC-001.
    let count = count_cdx_files(out.path());
    assert_eq!(
        count, 2,
        "expected 2 sub-SBOMs under --split=directory, got {count}"
    );
    // split-manifest.json exists.
    assert!(
        out.path().join("split-manifest.json").exists(),
        "split-manifest.json MUST exist"
    );
}

#[test]
fn us1_multi_member_group_entry_carries_members_field() {
    let (out, _) = run_split(&fixture_two_dir_polyglot(), Some("directory"));
    let manifest_bytes =
        std::fs::read(out.path().join("split-manifest.json")).expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    let entries = manifest["entries"].as_array().expect("entries[]");
    // Find the services/api entry (multi-member).
    let api_entry = entries
        .iter()
        .find(|e| {
            e["source_dir"]
                .as_str()
                .map(|s| s.ends_with("services/api"))
                .unwrap_or(false)
        })
        .expect("services/api entry present");
    let members = api_entry["members"].as_array().expect("members[]");
    assert_eq!(members.len(), 2, "members[] must have 2 entries (cargo + npm)");
    // Sorted lex by purl: cargo < npm.
    assert_eq!(
        members[0]["purl"].as_str().expect("purl"),
        "pkg:cargo/m219-api@0.1.0"
    );
    assert_eq!(
        members[1]["purl"].as_str().expect("purl"),
        "pkg:npm/m219-api@0.1.0"
    );
    // Delivers SC-003.
}

#[test]
fn us1_directory_mode_no_component_overlap_between_groups() {
    let (out, _) = run_split(&fixture_two_dir_polyglot(), Some("directory"));
    let api = std::fs::read(out.path().join("services-api.multi.cdx.json"))
        .expect("read services-api sub-SBOM");
    let worker = std::fs::read(out.path().join("m219-worker.golang.cdx.json"))
        .expect("read worker sub-SBOM");
    let api_doc: serde_json::Value = serde_json::from_slice(&api).expect("parse");
    let worker_doc: serde_json::Value = serde_json::from_slice(&worker).expect("parse");
    let api_purls: std::collections::HashSet<String> = api_doc["components"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    let worker_purls: std::collections::HashSet<String> = worker_doc["components"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|c| c["purl"].as_str().map(String::from))
        .collect();
    let overlap: Vec<&String> = api_purls.intersection(&worker_purls).collect();
    assert!(
        overlap.is_empty(),
        "SC-004: services/api and services/worker sub-SBOMs MUST share no PURLs; found: {overlap:?}"
    );
}

// -------- US2: extensibility + INFO log (SC-006, SC-007) --------

#[test]
fn us2_invalid_mode_value_fails_cli_parse() {
    let out = tempdir().expect("out tempdir");
    let home = tempdir().expect("home tempdir");
    let output = Command::new(waybill_bin())
        .env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", home.path())
        .env(
            "WAYBILL_FIXTURES_DIR",
            env!("WAYBILL_FIXTURES_DIR"),
        )
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_two_dir_polyglot())
        .arg("--split=nonexistent-mode")
        .arg("--output-dir")
        .arg(out.path())
        .output()
        .expect("waybill invokes");
    assert!(!output.status.success(), "invalid mode MUST fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nonexistent-mode"),
        "stderr must name the invalid mode; got: {stderr}"
    );
    assert!(
        stderr.contains("workspace") && stderr.contains("directory"),
        "stderr must list accepted values; got: {stderr}"
    );
    // Delivers SC-006.
}

#[test]
fn us2_info_log_carries_mode_field() {
    let (_out, combined) = run_split(&fixture_two_dir_polyglot(), Some("directory"));
    assert!(
        combined.contains("mode=directory"),
        "FR-010 INFO log MUST contain `mode=directory` (lowercase Display, not Debug)"
    );
    // Delivers SC-007.
}

// -------- US1 (E1 remediation): zero-boundaries fallback under directory mode --------

#[test]
fn us1_directory_mode_zero_boundaries_falls_back() {
    // Fixture with NO main-modules: an empty dir.
    let empty = tempdir().expect("empty fixture tempdir");
    let empty_path = empty.path().to_path_buf();
    let (out, stderr) = run_split(&empty_path, Some("directory"));
    // Fallback path: 1 SBOM emitted at output dir; NO split-manifest.
    let cdx_count = count_cdx_files(out.path());
    assert!(
        cdx_count <= 1,
        "zero-boundaries fallback: expected ≤1 sub-SBOM, got {cdx_count}"
    );
    assert!(
        !out.path().join("split-manifest.json").exists(),
        "zero-boundaries fallback: split-manifest.json MUST NOT be emitted"
    );
    assert!(
        stderr.contains("no workspace boundaries detected"),
        "FR-009 fallback WARN log missing; got: {stderr}"
    );
}

// -------- Phase 5: SC-002 + SC-005 --------

#[test]
fn sc002_split_workspace_emits_one_sbom_per_main_module() {
    let (out, _) = run_split(&fixture_two_dir_polyglot(), Some("workspace"));
    // 3 main-modules → 3 sub-SBOMs under workspace mode.
    let count = count_cdx_files(out.path());
    assert_eq!(count, 3, "expected 3 sub-SBOMs under --split=workspace, got {count}");
    // Delivers SC-002.
}

#[test]
fn sc005_bare_split_produces_same_filenames_as_workspace_mode() {
    // Bare --split and --split=workspace MUST emit identical FILE
    // LISTS (byte-identity per-file is separately verified against
    // alpha.67 via the m215 test suite; this test proves the bare
    // and explicit forms agree with each other).
    let (bare_out, _) = run_split(&fixture_two_dir_polyglot(), Some(""));
    let (explicit_out, _) = run_split(&fixture_two_dir_polyglot(), Some("workspace"));
    let mut bare_files: Vec<String> = std::fs::read_dir(bare_out.path())
        .expect("read bare out")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let mut explicit_files: Vec<String> = std::fs::read_dir(explicit_out.path())
        .expect("read explicit out")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    bare_files.sort();
    explicit_files.sort();
    assert_eq!(
        bare_files, explicit_files,
        "SC-005: bare --split and --split=workspace MUST emit identical file lists"
    );
}
