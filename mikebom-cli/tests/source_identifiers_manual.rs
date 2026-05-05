//! Milestone 073 T015 — manual `--with-source` flag emission test
//! (US2 happy paths + override + dedup + error semantics).
//!
//! Coverage per tasks.md T015 (a)-(f):
//!
//! - (a) manual `--with-source repo:...` on a non-git tempdir →
//!   identifier appears in the standards-native VCS slot per format.
//! - (b) two user-defined `--with-source` flags →
//!   `mikebom:source-identifiers` annotation carries both, sorted lex.
//! - (c) git checkout + `--with-source repo:<different>` →
//!   manual override wins (auto-detected entry dropped).
//! - (d) duplicate `--with-source repo:<same>` twice → deduplicated.
//! - (e1) `--with-source repo:` (empty value) → clap parse error
//!   citing `IdentifierError::EmptyValue`, exit non-zero.
//! - (e2) `--with-source NOT_VALID:value` (uppercase scheme) → clap
//!   parse error citing `IdentifierError::InvalidSchemeName`, exit
//!   non-zero. Different message from (e1).
//! - (f) `--with-source repo:obviously_invalid` → soft-fail to opaque,
//!   identifier appears under `mikebom:source-identifiers` not the
//!   VCS slot.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn write_minimal_cargo_project(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        b"[package]\nname = \"src-id-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.lock"),
        b"version = 3\n\n[[package]]\nname = \"src-id-test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
}

/// Run a CDX scan, returning the parsed JSON output. `extra_args`
/// can include `--with-source <id>` flags.
fn run_scan_cdx(path: &Path, fake_home: &Path, extra_args: &[&str]) -> serde_json::Value {
    let out_path = path.join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Run a CDX scan that's expected to FAIL at parse time. Returns the
/// stderr output for assertion.
fn run_scan_cdx_expect_failure(
    path: &Path,
    fake_home: &Path,
    extra_args: &[&str],
) -> String {
    let out_path = path.join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        !out.status.success(),
        "scan should have failed but succeeded; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn cdx_vcs_urls(doc: &serde_json::Value) -> Vec<String> {
    let refs = match doc["metadata"]["component"]
        .get("externalReferences")
        .and_then(|v| v.as_array())
    {
        Some(r) => r,
        None => return Vec::new(),
    };
    refs.iter()
        .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
        .filter_map(|r| r.get("url").and_then(|v| v.as_str()).map(String::from))
        .collect()
}

fn cdx_user_defined_payload(doc: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    let props = doc["metadata"].get("properties")?.as_array()?;
    let entry = props.iter().find(|p| {
        p.get("name").and_then(|v| v.as_str()) == Some("mikebom:source-identifiers")
    })?;
    let raw = entry["value"].as_str()?;
    serde_json::from_str(raw).ok()
}

#[test]
fn manual_with_source_emits_in_vcs_slot_no_git() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    // No git init — auto-detect can't fire.

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &["--with-source", "repo:git@github.com:acme/foo.git"],
    );
    let urls = cdx_vcs_urls(&cdx);
    assert_eq!(urls, vec!["git@github.com:acme/foo.git".to_string()]);

    // The comment field for a manual flag is "manual --with-source".
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .unwrap();
    let vcs_entry = refs
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
        .unwrap();
    assert_eq!(vcs_entry["comment"].as_str(), Some("manual --with-source"));
}

#[test]
fn user_defined_identifiers_ride_mikebom_annotation_sorted_lex() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &[
            "--with-source",
            "internal_ticket:PROJ-456",
            "--with-source",
            "acme_corp_id:abc123",
        ],
    );
    let payload = cdx_user_defined_payload(&cdx).expect("user-defined payload present");
    assert_eq!(payload.len(), 2);
    // Sorted lex by (scheme, value): acme_corp_id < internal_ticket.
    assert_eq!(payload[0]["scheme"].as_str(), Some("acme_corp_id"));
    assert_eq!(payload[0]["value"].as_str(), Some("abc123"));
    assert_eq!(payload[1]["scheme"].as_str(), Some("internal_ticket"));
    assert_eq!(payload[1]["value"].as_str(), Some("PROJ-456"));
}

#[test]
fn manual_override_drops_auto_detected_entry() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    // git init + origin = the auto-detected URL
    let _ = Command::new("git")
        .args(["-C", td.path().to_str().unwrap(), "init", "-q"])
        .status()
        .unwrap();
    let _ = Command::new("git")
        .args([
            "-C",
            td.path().to_str().unwrap(),
            "remote",
            "add",
            "origin",
            "git@github.com:auto-detected/foo.git",
        ])
        .status()
        .unwrap();

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &["--with-source", "repo:git@github.com:manual/foo.git"],
    );
    let urls = cdx_vcs_urls(&cdx);
    // Only the manual entry; the auto-detected one was dropped.
    assert_eq!(urls, vec!["git@github.com:manual/foo.git".to_string()]);
}

#[test]
fn duplicate_with_source_dedupes() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &[
            "--with-source",
            "repo:git@example.com:dup/foo.git",
            "--with-source",
            "repo:git@example.com:dup/foo.git",
        ],
    );
    let urls = cdx_vcs_urls(&cdx);
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0], "git@example.com:dup/foo.git");
}

#[test]
fn empty_value_clap_parse_error() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let stderr = run_scan_cdx_expect_failure(
        td.path(),
        fake_home.path(),
        &["--with-source", "repo:"],
    );
    // `IdentifierError::EmptyValue` formats as "identifier value is
    // empty" per data-model.md.
    assert!(
        stderr.contains("identifier value is empty"),
        "expected EmptyValue error in stderr; got: {stderr}"
    );
}

#[test]
fn malformed_scheme_clap_parse_error() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let stderr = run_scan_cdx_expect_failure(
        td.path(),
        fake_home.path(),
        &["--with-source", "NOT_VALID:value"],
    );
    // `IdentifierError::InvalidSchemeName` formats as "scheme ...
    // fails regex `^[a-z][a-z0-9_-]*$`".
    assert!(
        stderr.contains("fails regex"),
        "expected InvalidSchemeName error in stderr; got: {stderr}"
    );
    // Verify the two error-kinds emit DIFFERENT messages — the
    // EmptyValue case does NOT mention the regex.
    assert!(
        !stderr.contains("identifier value is empty"),
        "InvalidSchemeName error message should NOT contain EmptyValue text; got: {stderr}"
    );
}

#[test]
fn malformed_builtin_value_softfails_to_user_defined() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &["--with-source", "repo:obviously_invalid_not_a_url"],
    );
    // The identifier should NOT appear in the VCS slot.
    let urls = cdx_vcs_urls(&cdx);
    assert!(
        urls.is_empty(),
        "soft-failed built-in identifier MUST NOT ride the VCS slot; got urls={urls:?}"
    );
    // It should appear under `mikebom:source-identifiers` instead.
    let payload = cdx_user_defined_payload(&cdx)
        .expect("payload present after soft-fail downgrade");
    let found = payload
        .iter()
        .any(|e| e["scheme"].as_str() == Some("repo"));
    assert!(
        found,
        "soft-failed built-in identifier must emit under mikebom:source-identifiers; got {payload:?}"
    );
}
