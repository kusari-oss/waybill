//! Milestone 128 US2 — source-pinned upstream provenance end-to-end.
//!
//! Verifies SRC_URI extraction (FR-002), SRCREV (FR-003), the FR-002a
//! host-typed PURL emission for github/gitlab/bitbucket/codeberg
//! SRC_URI, and the FR-018 version derivation when PV is literally
//! `"git"` or contains `AUTOINC`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_scan(
    fake_home: &Path,
    scan_target: &Path,
    out_format: &str,
    out_filename: &str,
) -> serde_json::Value {
    let out_dir = tempfile::Builder::new()
        .prefix("mb128-us2-")
        .tempdir()
        .unwrap();
    let out_path = out_dir.path().join(out_filename);
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg(out_format)
        .arg("--output")
        .arg(format!("{out_format}={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = fs::read_to_string(&out_path).expect("read output");
    serde_json::from_str(&body).expect("valid JSON")
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("tests")
        .join("fixtures")
        .join("yocto_recipe_enrich")
        .join(name)
}

fn find_component_by_purl_prefix<'a>(
    cdx: &'a serde_json::Value,
    prefix: &str,
) -> Option<&'a serde_json::Value> {
    cdx.pointer("/components")?
        .as_array()?
        .iter()
        .find(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .map(|p| p.starts_with(prefix))
                .unwrap_or(false)
        })
}

fn cdx_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .pointer("/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

#[test]
fn us2_fr002a_github_git_uri_emits_host_typed_purl() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("git_srcuri_srcrev"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // FR-002a: SRC_URI = git://github.com/example/widget.git → host-typed PURL.
    // Version derived from SRCREV first 12 hex chars (lowercased) per FR-018-style.
    let widget = find_component_by_purl_prefix(&cdx, "pkg:github/example/widget@")
        .expect("expected pkg:github/example/widget@... component");
    let purl = widget.get("purl").and_then(|p| p.as_str()).unwrap();
    assert_eq!(
        purl, "pkg:github/example/widget@abc123def456",
        "FR-002a host-typed PURL MUST use SRCREV 12-hex prefix as version"
    );

    // Recipe-identity provenance preserved in annotations per FR-002a.
    assert_eq!(
        cdx_property(widget, "mikebom:yocto-recipe-name"),
        Some("widget")
    );
    assert_eq!(
        cdx_property(widget, "mikebom:yocto-recipe-version"),
        Some("1.0")
    );

    // mikebom:srcrev annotation carries the full 40-hex SHA per FR-003.
    assert_eq!(
        cdx_property(widget, "mikebom:srcrev"),
        Some("abc123def456abc123def456abc123def456abcd")
    );

    // mikebom:src-uri preserves the raw SRC_URI entries (JSON-encoded array).
    let src_uri = cdx_property(widget, "mikebom:src-uri").expect("mikebom:src-uri present");
    assert!(
        src_uri.contains("github.com/example/widget.git"),
        "mikebom:src-uri must carry the git URI verbatim, got: {src_uri}"
    );
}

#[test]
fn us2_fr018_autoinc_version_derives_from_srcrev() {
    // gadget_0.0.4.AUTOINC+f597fb026637.bb has AUTOINC in PV.
    // FR-018: derive version from SRCREV first 12 hex chars.
    // SRC_URI is not set on this recipe → FR-011 fallback (pkg:generic/).
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("autoinc_version"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // PURL should NOT contain "AUTOINC" — the version must be SRCREV-derived.
    let comps = cdx.pointer("/components").and_then(|c| c.as_array()).unwrap();
    let bad: Vec<_> = comps
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .map(|p| p.contains("AUTOINC"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        bad.is_empty(),
        "FR-018: no component PURL may contain literal 'AUTOINC', got: {bad:?}"
    );
    // Specifically: gadget version should be the SRCREV first 12 hex (lowercased).
    let gadget = comps
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("gadget"))
        .expect("gadget component");
    let version = gadget.get("version").and_then(|v| v.as_str()).unwrap();
    assert_eq!(version, "f597fb026637");
}

#[test]
fn us2_fr018_literal_git_version_derives_from_srcrev() {
    // gadget-git_git.bb has PV literal = "git". FR-018 rejects the
    // "version: git" anti-pattern; derives from SRCREV.
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("autoinc_version"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comps = cdx.pointer("/components").and_then(|c| c.as_array()).unwrap();
    // No component may have version == "git".
    let bad: Vec<_> = comps
        .iter()
        .filter(|c| c.get("version").and_then(|v| v.as_str()) == Some("git"))
        .collect();
    assert!(
        bad.is_empty(),
        "FR-018: no component version may be literal 'git', got: {bad:?}"
    );
    // Specifically: gadget-git version should be SRCREV first 12 hex.
    let gadget_git = comps
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("gadget-git"))
        .expect("gadget-git component");
    let version = gadget_git.get("version").and_then(|v| v.as_str()).unwrap();
    assert_eq!(version, "1234567890ab");
}
