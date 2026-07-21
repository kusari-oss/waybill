//! Integration tests for the root-component PURL control flags
//! (`--root-purl-type <TYPE>` and `--no-root-purl`).
//!
//! Both flags extend the existing milestone-077 `RootComponentOverride`
//! surface — `--root-name` / `--root-version` already cover name and
//! version; the new flags add producer-side control of the PURL itself.
//!
//! Coverage:
//!
//! - `--root-purl-type` × CDX + SPDX 2.3 + SPDX 3 × representative name
//!   shapes (plain identifier, slashed Go module path, scoped npm
//!   `@scope/name`, Maven `group/artifact`).
//! - `--no-root-purl` × CDX + SPDX 2.3 + SPDX 3 — verifies the PURL
//!   slot is ABSENT (not null, not empty) in every format.
//! - Mutual exclusion + co-presence regressions: clap rejects both
//!   flags simultaneously, and rejects either without `--root-name`.
//! - Invalid type-token regressions: uppercase / whitespace inputs are
//!   rejected at parse time with a flag-named error.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::path::Path;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

// ---------------------------------------------------------------------
// Helpers (cloned from identifiers_root_component_override.rs)
// ---------------------------------------------------------------------

fn run_scan_returning_json(
    fake_home: &Path,
    scan_target: &Path,
    extra_args: &[&str],
    out_format: &str,
    out_filename: &str,
) -> serde_json::Value {
    let out_dir = tempfile::tempdir().unwrap();
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
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    assert!(
        out.status.success(),
        "scan failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    drop(out_dir);
    parsed
}

fn run_scan_expecting_failure(
    fake_home: &Path,
    scan_target: &Path,
    extra_args: &[&str],
) -> (i32, String) {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.to_string_lossy()))
        .arg("--no-deep-hash");
    for a in extra_args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("scan runs");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    drop(out_dir);
    (code, stderr)
}

fn make_arbitrary_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new().prefix(name).tempdir().unwrap()
}

/// Find the root `software_Package` element in an SPDX 3 `@graph`.
fn spdx3_root_package(spdx3: &serde_json::Value) -> &serde_json::Value {
    let root_iri = spdx3
        .get("@graph")
        .and_then(|g| g.as_array())
        .and_then(|arr| {
            arr.iter().find(|e| {
                e.get("type").and_then(|v| v.as_str()) == Some("SpdxDocument")
            })
        })
        .and_then(|doc| doc.get("rootElement"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .expect("SPDX 3 rootElement IRI present");
    spdx3
        .get("@graph")
        .and_then(|g| g.as_array())
        .and_then(|arr| {
            arr.iter().find(|e| {
                e.get("spdxId").and_then(|v| v.as_str()) == Some(root_iri)
            })
        })
        .expect("SPDX 3 root package present in @graph")
}

// =====================================================================
// `--root-purl-type` × CDX
// =====================================================================

#[test]
fn root_purl_type_golang_with_slashed_name_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "github.com/example/svc",
            "--root-version",
            "v1.0.0",
            "--root-purl-type",
            "golang",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["name"].as_str(), Some("github.com/example/svc"));
    assert_eq!(comp["version"].as_str(), Some("v1.0.0"));
    // The `/` in the name is percent-encoded as `%2F` per RFC 3986 in
    // the percent_encode_purl_name helper.
    assert_eq!(
        comp["purl"].as_str(),
        Some("pkg:golang/github.com%2Fexample%2Fsvc@v1.0.0")
    );
}

#[test]
fn root_purl_type_npm_scoped_name_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("pkg");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "@scope/pkg",
            "--root-version",
            "1.0.0",
            "--root-purl-type",
            "npm",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    // `@` → `%40`, `/` → `%2F` per percent_encode_purl_name.
    assert_eq!(
        comp["purl"].as_str(),
        Some("pkg:npm/%40scope%2Fpkg@1.0.0")
    );
}

#[test]
fn root_purl_type_maven_groupartifact_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("artifact");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "com.example/artifact",
            "--root-version",
            "1.0.0",
            "--root-purl-type",
            "maven",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(
        comp["purl"].as_str(),
        Some("pkg:maven/com.example%2Fartifact@1.0.0")
    );
}

#[test]
fn root_purl_type_plain_name_replaces_generic_default_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("widget");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "widget-svc",
            "--root-version",
            "1.2.3",
            "--root-purl-type",
            "oci",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["purl"].as_str(), Some("pkg:oci/widget-svc@1.2.3"));
}

// =====================================================================
// `--root-purl-type` × SPDX 2.3
// =====================================================================

#[test]
fn root_purl_type_golang_spdx23() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let spdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "github.com/example/svc",
            "--root-version",
            "v1.0.0",
            "--root-purl-type",
            "golang",
        ],
        "spdx-2.3-json",
        "out.spdx23.json",
    );
    // SPDX 2.3 root Package is packages[0] (synthesized root); the PURL
    // rides externalRefs[].referenceLocator with referenceType="purl".
    let root_pkg = &spdx["packages"][0];
    assert_eq!(
        root_pkg["name"].as_str(),
        Some("github.com/example/svc")
    );
    let refs = root_pkg["externalRefs"].as_array().unwrap();
    let purl_entry = refs
        .iter()
        .find(|r| r["referenceType"].as_str() == Some("purl"))
        .expect("root Package has a purl externalRef");
    assert_eq!(
        purl_entry["referenceLocator"].as_str(),
        Some("pkg:golang/github.com%2Fexample%2Fsvc@v1.0.0")
    );
}

// =====================================================================
// `--root-purl-type` × SPDX 3
// =====================================================================

#[test]
fn root_purl_type_golang_spdx3() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let spdx3 = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "github.com/example/svc",
            "--root-version",
            "v1.0.0",
            "--root-purl-type",
            "golang",
        ],
        "spdx-3-json",
        "out.spdx3.json",
    );
    let root = spdx3_root_package(&spdx3);
    let expected = "pkg:golang/github.com%2Fexample%2Fsvc@v1.0.0";
    // Both emission slots: software_packageUrl AND externalIdentifier[packageUrl].
    assert_eq!(
        root["software_packageUrl"].as_str(),
        Some(expected)
    );
    let ext_ids = root["externalIdentifier"].as_array().unwrap();
    let purl_id = ext_ids
        .iter()
        .find(|i| i["externalIdentifierType"].as_str() == Some("packageUrl"))
        .expect("SPDX 3 root has externalIdentifier[packageUrl]");
    assert_eq!(purl_id["identifier"].as_str(), Some(expected));
}

// =====================================================================
// `--no-root-purl` × CDX
// =====================================================================

#[test]
fn no_root_purl_omits_field_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "my-svc",
            "--root-version",
            "1.0.0",
            "--no-root-purl",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    // Name + version still present.
    assert_eq!(comp["name"].as_str(), Some("my-svc"));
    assert_eq!(comp["version"].as_str(), Some("1.0.0"));
    // `purl` field ABSENT (not null, not empty).
    assert!(
        comp.get("purl").is_none(),
        "metadata.component.purl must be ABSENT when --no-root-purl is set; got {:?}",
        comp.get("purl")
    );
}

#[test]
fn no_root_purl_with_full_registry_path_roundtrips_cdx() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "767xxxxxxxxx.dkr.ecr.us-east-1.amazonaws.com/pico-server",
            "--root-version",
            "1.0.0",
            "--no-root-purl",
        ],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    // Slashed identifier round-trips byte-for-byte — no PURL encoding involved.
    assert_eq!(
        comp["name"].as_str(),
        Some("767xxxxxxxxx.dkr.ecr.us-east-1.amazonaws.com/pico-server")
    );
    assert!(comp.get("purl").is_none());
}

// =====================================================================
// `--no-root-purl` × SPDX 2.3
// =====================================================================

#[test]
fn no_root_purl_omits_externalref_spdx23() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let spdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "my-svc",
            "--root-version",
            "1.0.0",
            "--no-root-purl",
        ],
        "spdx-2.3-json",
        "out.spdx23.json",
    );
    let root_pkg = &spdx["packages"][0];
    let refs = root_pkg["externalRefs"].as_array().unwrap();
    let has_purl = refs
        .iter()
        .any(|r| r["referenceType"].as_str() == Some("purl"));
    assert!(
        !has_purl,
        "root Package externalRefs[] must NOT contain a purl entry; got {refs:?}"
    );
    // CPE entry should still be present.
    let has_cpe = refs
        .iter()
        .any(|r| r["referenceType"].as_str() == Some("cpe23Type"));
    assert!(has_cpe, "CPE externalRef should still emit");
}

// =====================================================================
// `--no-root-purl` × SPDX 3
// =====================================================================

#[test]
fn no_root_purl_omits_software_packageurl_and_externalidentifier_spdx3() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let spdx3 = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "my-svc",
            "--root-version",
            "1.0.0",
            "--no-root-purl",
        ],
        "spdx-3-json",
        "out.spdx3.json",
    );
    let root = spdx3_root_package(&spdx3);
    // `software_packageUrl` field ABSENT.
    assert!(
        root.get("software_packageUrl").is_none(),
        "software_packageUrl must be ABSENT; got {:?}",
        root.get("software_packageUrl")
    );
    // externalIdentifier[] still present (CPE), but NO packageUrl entry.
    let ext_ids = root["externalIdentifier"].as_array().unwrap();
    let has_purl = ext_ids
        .iter()
        .any(|i| i["externalIdentifierType"].as_str() == Some("packageUrl"));
    assert!(
        !has_purl,
        "externalIdentifier[] must NOT contain a packageUrl entry; got {ext_ids:?}"
    );
}

// =====================================================================
// Mutual-exclusion + co-presence + invalid-type regressions
// =====================================================================

#[test]
fn mutex_root_purl_type_and_no_root_purl() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "svc",
            "--root-purl-type",
            "golang",
            "--no-root-purl",
        ],
    );
    assert_ne!(code, 0, "expected non-zero exit; got 0");
    assert!(
        stderr.contains("conflicts with") || stderr.contains("cannot be used with"),
        "stderr should explain the mutex; got: {stderr}"
    );
}

#[test]
fn root_purl_type_requires_root_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--root-purl-type", "golang"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--root-name") || stderr.contains("root-name") || stderr.contains("requires"),
        "stderr should mention --root-name requirement; got: {stderr}"
    );
}

#[test]
fn no_root_purl_requires_root_name() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &["--no-root-purl"],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--root-name") || stderr.contains("root-name") || stderr.contains("requires"),
        "stderr should mention --root-name requirement; got: {stderr}"
    );
}

#[test]
fn invalid_root_purl_type_uppercase() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "svc",
            "--root-purl-type",
            "FOO",
        ],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--root-purl-type")
            || stderr.contains("root-purl-type")
            || stderr.contains("valid purl-spec type token"),
        "stderr should name the invalid flag value; got: {stderr}"
    );
}

#[test]
fn invalid_root_purl_type_whitespace() {
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let (code, stderr) = run_scan_expecting_failure(
        fake_home.path(),
        scan_target.path(),
        &[
            "--root-name",
            "svc",
            "--root-purl-type",
            "has space",
        ],
    );
    assert_ne!(code, 0);
    assert!(
        stderr.contains("--root-purl-type")
            || stderr.contains("root-purl-type")
            || stderr.contains("valid purl-spec type token"),
        "stderr should name the invalid flag value; got: {stderr}"
    );
}

// =====================================================================
// Back-compat: without the new flags, --root-name still defaults to generic
// =====================================================================

#[test]
fn root_name_without_new_flags_still_produces_pkg_generic() {
    // Regression: this test mirrors the existing milestone-077 default
    // behavior. The new flags must NOT change the bytes when absent.
    let fake_home = tempfile::tempdir().unwrap();
    let scan_target = make_arbitrary_dir("svc");
    let cdx = run_scan_returning_json(
        fake_home.path(),
        scan_target.path(),
        &["--root-name", "widget-svc", "--root-version", "1.2.3"],
        "cyclonedx-json",
        "out.cdx.json",
    );
    let comp = &cdx["metadata"]["component"];
    assert_eq!(comp["purl"].as_str(), Some("pkg:generic/widget-svc@1.2.3"));
}
