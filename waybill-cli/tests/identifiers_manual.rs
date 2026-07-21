//! Milestone 073 T015 — manual identifier flag emission test
//! (US2 happy paths + override + dedup + error semantics).
//!
//! Coverage per tasks.md T015 (a)-(f), updated for the post-073
//! CLI refactor (dedicated `--repo` / `--git-ref` / `--image-id` /
//! `--attestation` / `--id <scheme>=<value>` flags):
//!
//! - (a) manual `--repo <url>` on a non-git tempdir →
//!   identifier appears in the standards-native VCS slot per format.
//! - (b) two user-defined `--id <scheme>=<value>` flags →
//!   `waybill:identifiers` annotation carries both, sorted lex.
//! - (c) git checkout + `--repo <different>` →
//!   manual override wins (auto-detected entry dropped).
//! - (d) duplicate `--repo <same>` is *not* possible at the CLI
//!   level (the flag is `Option<String>`, not repeatable). The
//!   resolution-pipeline dedup against an auto-detected entry is
//!   exercised by (c) + the unit-test coverage in
//!   `cli/scan_cmd.rs::tests::resolve_*`.
//! - (e1) `--id acme=` (empty value) → clap parse error
//!   citing `IdentifierError::EmptyValue`, exit non-zero. (The
//!   former `--with-source repo:` empty-value path applied to the
//!   freeform `<scheme>:<value>` parser; the new `--id` parser
//!   carries the same error case via the `=` split.)
//! - (e2) `--id NOT_VALID=value` (uppercase scheme) → clap
//!   parse error citing `IdentifierError::InvalidSchemeName`, exit
//!   non-zero. Different message from (e1).
//! - (e3) `--id repo=foo` (built-in scheme on `--id`) → clap parse
//!   error pointing at the dedicated `--repo` flag. NEW post-073-refactor
//!   case (the old `--with-source repo:foo` was a valid form; the
//!   refactor splits it into `--repo foo`).
//! - (f) `--repo obviously_invalid` → soft-fail to opaque,
//!   identifier appears under `waybill:identifiers` not the
//!   VCS slot. (Built-in value validators behind dedicated flags
//!   share the same soft-fail path.)

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
/// can include any of the new identifier flags (`--repo`, `--git-ref`,
/// `--image-id`, `--attestation`, `--id`).
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
        p.get("name").and_then(|v| v.as_str()) == Some("waybill:identifiers")
    })?;
    let raw = entry["value"].as_str()?;
    serde_json::from_str(raw).ok()
}

#[test]
fn manual_repo_emits_in_vcs_slot_no_git() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    // No git init — auto-detect can't fire.

    let cdx = run_scan_cdx(
        td.path(),
        fake_home.path(),
        &["--repo", "git@github.com:acme/foo.git"],
    );
    let urls = cdx_vcs_urls(&cdx);
    assert_eq!(urls, vec!["git@github.com:acme/foo.git".to_string()]);

    // The comment field for a manual flag is "manual identifier flag"
    // (post-073-refactor wording).
    let refs = cdx["metadata"]["component"]["externalReferences"]
        .as_array()
        .unwrap();
    let vcs_entry = refs
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("vcs"))
        .unwrap();
    assert_eq!(vcs_entry["comment"].as_str(), Some("manual identifier flag"));
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
            "--id",
            "internal_ticket=PROJ-456",
            "--id",
            "acme_corp_id=abc123",
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
        &["--repo", "git@github.com:manual/foo.git"],
    );
    let urls = cdx_vcs_urls(&cdx);
    // Only the manual entry; the auto-detected one was dropped.
    assert_eq!(urls, vec!["git@github.com:manual/foo.git".to_string()]);
}

#[test]
fn empty_id_value_clap_parse_error() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let stderr = run_scan_cdx_expect_failure(
        td.path(),
        fake_home.path(),
        &["--id", "acme_corp_id="],
    );
    // `IdentifierError::EmptyValue` formats as "identifier value is
    // empty" per data-model.md.
    assert!(
        stderr.contains("identifier value is empty"),
        "expected EmptyValue error in stderr; got: {stderr}"
    );
}

#[test]
fn malformed_id_scheme_clap_parse_error() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let stderr = run_scan_cdx_expect_failure(
        td.path(),
        fake_home.path(),
        &["--id", "NOT_VALID=value"],
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
fn id_built_in_scheme_clap_parse_error_points_at_dedicated_flag() {
    // NEW post-073-refactor case: `--id repo=foo` (and likewise for
    // git/image/attestation) MUST clap-error with a message pointing
    // at the dedicated flag (--repo / --git-ref / --image-id /
    // --attestation).
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());

    let stderr = run_scan_cdx_expect_failure(
        td.path(),
        fake_home.path(),
        &["--id", "repo=foo"],
    );
    assert!(
        stderr.contains("--id rejects the built-in scheme `repo`"),
        "expected built-in-rejection error citing scheme; got: {stderr}"
    );
    assert!(
        stderr.contains("--repo"),
        "error MUST point operator at the dedicated --repo flag; got: {stderr}"
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
        &["--repo", "obviously_invalid_not_a_url"],
    );
    // The identifier should NOT appear in the VCS slot.
    let urls = cdx_vcs_urls(&cdx);
    assert!(
        urls.is_empty(),
        "soft-failed built-in identifier MUST NOT ride the VCS slot; got urls={urls:?}"
    );
    // It should appear under `waybill:identifiers` instead.
    let payload = cdx_user_defined_payload(&cdx)
        .expect("payload present after soft-fail downgrade");
    let found = payload
        .iter()
        .any(|e| e["scheme"].as_str() == Some("repo"));
    assert!(
        found,
        "soft-failed built-in identifier must emit under waybill:identifiers; got {payload:?}"
    );
}
