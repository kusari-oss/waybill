//! Milestone 073 T011 — auto-detected `repo:` identifier
//! emission test (US1 happy path + 3-step fallback).
//!
//! Synthesizes a tempdir + `git init` + `git remote add origin …`
//! fixture, runs `mikebom sbom scan --path <tempdir>` for each of the
//! three formats, and asserts the emitted SBOM carries the
//! auto-detected `repo:` identifier in the per-format standards-native
//! carrier per `contracts/identifiers-annotation.md` C-1.
//!
//! Coverage:
//! - origin-only → `repo:<url>` with comment `auto-detected from git remote
//!   `origin``
//! - upstream-only → uses `upstream`
//! - third-remote-only → uses first-listed (alphabetical) with the
//!   "(origin/upstream absent; first-listed)" suffix in the comment
//! - no-git → no `repo:` identifier emitted, scan still succeeds
//!
//! Tests use `--offline` to avoid any network enrichment and a
//! minimal Cargo.toml fixture so the scan has something to scan.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

mod common;
use common::normalize::apply_fake_home_env;
use common::bin;

/// Spawn `git` against `dir` with the given args, asserting success.
fn git(dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    for a in args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("git available");
    assert!(status.success(), "git {args:?} in {dir:?} failed");
}

/// Create a minimal scannable cargo project at `dir`. Cargo is the
/// cheapest ecosystem to scan — tiny lockfile, no network.
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

/// Run `mikebom sbom scan --path` once, returning the parsed CDX
/// JSON output. Uses `--offline` for hermeticity.
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
    let bytes = std::fs::read(&out_path).expect("read CDX");
    serde_json::from_slice(&bytes).expect("parse CDX")
}

/// Run `mikebom sbom scan --path` for SPDX 2.3, returning parsed JSON.
fn run_scan_spdx23(path: &Path, fake_home: &Path) -> serde_json::Value {
    let out_path = path.join("out.spdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("scan runs");
    assert!(out.status.success(), "scan failed");
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Run `mikebom sbom scan --path` for SPDX 3, returning parsed JSON.
fn run_scan_spdx3(path: &Path, fake_home: &Path) -> serde_json::Value {
    let out_path = path.join("out.spdx3.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("spdx-3-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("scan runs");
    assert!(out.status.success(), "scan failed");
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Find a `metadata.component.externalReferences[]` entry of the
/// given `type`. Returns the entry's URL + comment (or panics if no
/// entry matches).
fn cdx_external_ref_url_comment(
    doc: &serde_json::Value,
    ref_type: &str,
) -> (String, String) {
    let refs = doc["metadata"]["component"]["externalReferences"]
        .as_array()
        .expect("metadata.component.externalReferences is an array");
    let entry = refs
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some(ref_type))
        .expect("entry of expected type");
    let url = entry["url"].as_str().expect("url present").to_string();
    let comment = entry["comment"]
        .as_str()
        .expect("comment present")
        .to_string();
    (url, comment)
}

#[test]
fn auto_detect_origin_emits_repo_identifier_in_all_three_formats() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    git(td.path(), &["init", "-q"]);
    git(
        td.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:test/foo.git",
        ],
    );

    // CDX
    let cdx = run_scan_cdx(td.path(), fake_home.path(), &[]);
    let (url, comment) = cdx_external_ref_url_comment(&cdx, "vcs");
    assert_eq!(url, "git@github.com:test/foo.git");
    assert_eq!(comment, "auto-detected from git remote `origin`");

    // SPDX 2.3 — check creationInfo.creators redundant text
    let spdx23 = run_scan_spdx23(td.path(), fake_home.path());
    let creators = spdx23["creationInfo"]["creators"]
        .as_array()
        .expect("creators array");
    let found_redundant_text = creators.iter().any(|c| {
        c.as_str()
            .map(|s| s.contains("source: repo:git@github.com:test/foo.git"))
            .unwrap_or(false)
    });
    assert!(
        found_redundant_text,
        "SPDX 2.3 creationInfo.creators must carry redundant `Tool: ... source: <id>` line for built-in identifier; got creators={creators:?}"
    );

    // SPDX 3 — check Element.externalIdentifier[]. Per milestone
    // 079, the `repo:` mikebom scheme maps to the SPDX 3
    // controlled-vocab value `other` with the original scheme name
    // preserved on the `comment` field as `original-scheme: repo`.
    let spdx3 = run_scan_spdx3(td.path(), fake_home.path());
    let graph = spdx3["@graph"].as_array().expect("graph");
    let doc_el = graph
        .iter()
        .find(|el| el.get("type").and_then(|v| v.as_str()) == Some("SpdxDocument"))
        .expect("SpdxDocument element");
    let idents = doc_el["externalIdentifier"]
        .as_array()
        .expect("externalIdentifier[]");
    let repo_entry = idents
        .iter()
        .find(|e| {
            e.get("externalIdentifierType").and_then(|v| v.as_str()) == Some("other")
                && e.get("comment").and_then(|v| v.as_str())
                    == Some("original-scheme: repo")
        })
        .expect("repo entry mapped to (other, comment=original-scheme: repo)");
    assert_eq!(
        repo_entry["identifier"].as_str(),
        Some("git@github.com:test/foo.git")
    );
}

#[test]
fn auto_detect_upstream_only_uses_upstream() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    git(td.path(), &["init", "-q"]);
    git(
        td.path(),
        &["remote", "add", "upstream", "git@github.com:acme/foo.git"],
    );

    let cdx = run_scan_cdx(td.path(), fake_home.path(), &[]);
    let (url, comment) = cdx_external_ref_url_comment(&cdx, "vcs");
    assert_eq!(url, "git@github.com:acme/foo.git");
    assert_eq!(comment, "auto-detected from git remote `upstream`");
}

#[test]
fn auto_detect_third_remote_only_uses_first_alphabetical() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    git(td.path(), &["init", "-q"]);
    // Add zebra first so we can verify alphabetical sort picks alpha.
    git(td.path(), &["remote", "add", "zebra", "git@example.com:z/foo.git"]);
    git(td.path(), &["remote", "add", "alpha", "git@example.com:a/foo.git"]);

    let cdx = run_scan_cdx(td.path(), fake_home.path(), &[]);
    let (url, comment) = cdx_external_ref_url_comment(&cdx, "vcs");
    assert_eq!(url, "git@example.com:a/foo.git");
    assert_eq!(
        comment,
        "auto-detected from git remote `alpha` (origin/upstream absent; first-listed)"
    );
}

#[test]
fn no_git_dir_emits_no_repo_identifier() {
    let td = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    write_minimal_cargo_project(td.path());
    // No `git init` — the scan root is not a git checkout.

    let cdx = run_scan_cdx(td.path(), fake_home.path(), &[]);
    // Either externalReferences[] is absent OR no `vcs` entry exists.
    if let Some(refs) =
        cdx["metadata"]["component"].get("externalReferences").and_then(|v| v.as_array())
    {
        let has_vcs = refs.iter().any(|r| {
            r.get("type").and_then(|v| v.as_str()) == Some("vcs")
        });
        assert!(
            !has_vcs,
            "non-git scan must not emit a vcs externalReference; got {refs:?}"
        );
    }
}
