//! Milestone 191 US2 (issue #558) — integration tests for the
//! standalone design-tier component versionless-PURL emission shape.
//!
//! Scenario: an npm project declares a dependency in `package.json`
//! that has NO corresponding entry in `package-lock.json` (e.g., an
//! `optionalDependencies` entry that failed to install, or a
//! freshly-added dep before lockfile refresh). Pre-m191 mikebom
//! emitted `pkg:npm/optional-dep@` (trailing `@`) with `.version: ""`.
//! Post-m191 the same component emits `pkg:npm/optional-dep` (no `@`)
//! with the `.version` field omitted from the CDX JSON entirely,
//! `versionInfo: "NOASSERTION"` in SPDX 2.3, and
//! `software_packageVersion` omitted in SPDX 3.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;
use common::workspace_root;

fn scan(dir: &Path, format: &str) -> Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_ext = match format {
        "cyclonedx-json" => "cdx.json",
        "spdx-2.3-json" => "spdx.json",
        "spdx-3-json" => "spdx3.json",
        _ => panic!("unknown format {format}"),
    };
    let out_path = workdir.path().join(format!("out.{out_ext}"));

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        dir.to_str().unwrap(),
        "--format",
        format,
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");
    assert!(
        output.status.success(),
        "scan failed: format={format} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).expect("read output");
    serde_json::from_slice(&bytes).expect("parse output json")
}

/// Build a synthetic npm project that declares `optional-dep: "^1.0.0"`
/// in `package.json` but has NO lockfile (the manifest-only design-tier
/// path — mikebom's npm reader emits a design-tier component for the
/// declaration, carrying `mikebom:sbom-tier: design` +
/// `mikebom:requirement-range: ^1.0.0`).
///
/// This is the primary US2 test vector: a real-world case where a
/// user runs mikebom against a repo that hasn't yet run `npm install`
/// (fresh checkout, CI pre-install phase, or a repo where the lockfile
/// is intentionally gitignored per team policy).
fn build_optional_missing_project(dir: &Path) -> PathBuf {
    let project = dir.join("m191-us2-project");
    std::fs::create_dir_all(&project).expect("mkdir project");

    let package_json = r#"{
  "name": "m191-us2-project",
  "version": "0.1.0",
  "dependencies": {
    "optional-dep": "^1.0.0"
  }
}
"#;
    std::fs::write(project.join("package.json"), package_json).expect("write package.json");

    project
}

fn find_component_by_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
}

fn find_spdx23_package_by_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc["packages"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
}

fn find_spdx3_software_package_by_name<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc["@graph"]
        .as_array()?
        .iter()
        .find(|e| {
            e["type"].as_str() == Some("software_Package") && e["name"].as_str() == Some(name)
        })
}

// ── FR-009: CDX 1.6 versionless PURL ──────────────────────────────

#[test]
fn us2_cdx_optional_missing_emits_versionless_purl_no_trailing_at() {
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());
    let doc = scan(&project, "cyclonedx-json");

    let component = find_component_by_name(&doc, "optional-dep")
        .expect("optional-dep component must be emitted (design-tier)");
    let purl = component["purl"].as_str().expect("component has purl");
    assert_eq!(
        purl, "pkg:npm/optional-dep",
        "purl must be versionless (no trailing @); got: {purl}"
    );
    // Regression grep: literally reject any component whose purl
    // ends with `@`.
    for c in doc["components"].as_array().unwrap() {
        if let Some(p) = c["purl"].as_str() {
            assert!(
                !p.ends_with('@'),
                "component `{}` has trailing-@ PURL: {p}",
                c["name"]
            );
        }
    }
}

#[test]
fn us2_cdx_optional_missing_omits_version_field_entirely() {
    // FR-010: the CDX `.version` field MUST be omitted from the JSON
    // (not emitted as `""`) for versionless design-tier components.
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());
    let doc = scan(&project, "cyclonedx-json");

    let component = find_component_by_name(&doc, "optional-dep")
        .expect("optional-dep component present");
    assert!(
        component.get("version").is_none(),
        "component.version must be OMITTED entirely; found: {:?}",
        component.get("version")
    );
}

#[test]
fn us2_cdx_optional_missing_bom_ref_matches_versionless_purl() {
    // FR-013 / Q3: bom-ref is the versionless PURL as-is.
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());
    let doc = scan(&project, "cyclonedx-json");

    let component = find_component_by_name(&doc, "optional-dep")
        .expect("optional-dep component present");
    let bom_ref = component["bom-ref"].as_str().expect("bom-ref present");
    assert_eq!(
        bom_ref, "pkg:npm/optional-dep",
        "bom-ref must equal the versionless PURL; got: {bom_ref}"
    );
}

// ── FR-011: SPDX 2.3 versionInfo == NOASSERTION ───────────────────

#[test]
fn us2_spdx23_optional_missing_uses_noassertion_version_info() {
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());
    let doc = scan(&project, "spdx-2.3-json");

    let package = find_spdx23_package_by_name(&doc, "optional-dep")
        .expect("optional-dep package present in SPDX 2.3 output");
    let version_info = package["versionInfo"].as_str().expect("versionInfo present");
    assert_eq!(
        version_info, "NOASSERTION",
        "versionInfo must be NOASSERTION; got: {version_info}"
    );

    // externalRefs[purl].referenceLocator must be the versionless PURL.
    let refs = package["externalRefs"].as_array().expect("externalRefs");
    let purl_ref = refs
        .iter()
        .find(|r| r["referenceType"].as_str() == Some("purl"))
        .expect("purl externalRef present");
    assert_eq!(
        purl_ref["referenceLocator"].as_str(),
        Some("pkg:npm/optional-dep")
    );
}

// ── FR-012: SPDX 3 software_packageVersion omitted ────────────────

#[test]
fn us2_spdx3_optional_missing_omits_software_package_version() {
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());
    let doc = scan(&project, "spdx-3-json");

    let package = find_spdx3_software_package_by_name(&doc, "optional-dep")
        .expect("optional-dep software_Package present in SPDX 3 output");
    assert!(
        package.get("software_packageVersion").is_none(),
        "software_packageVersion must be OMITTED entirely; found: {:?}",
        package.get("software_packageVersion")
    );
    assert_eq!(
        package["software_packageUrl"].as_str(),
        Some("pkg:npm/optional-dep"),
        "software_packageUrl must be the versionless PURL"
    );
}

// ── FR-015: cross-format PURL parity ──────────────────────────────

#[test]
fn us2_cross_format_versionless_purl_parity() {
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());

    let cdx = scan(&project, "cyclonedx-json");
    let spdx23 = scan(&project, "spdx-2.3-json");
    let spdx3 = scan(&project, "spdx-3-json");

    let cdx_purl = find_component_by_name(&cdx, "optional-dep").expect("cdx")["purl"]
        .as_str()
        .expect("cdx purl")
        .to_string();
    let spdx23_purl = find_spdx23_package_by_name(&spdx23, "optional-dep").expect("spdx23")
        ["externalRefs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["referenceType"].as_str() == Some("purl"))
        .unwrap()["referenceLocator"]
        .as_str()
        .unwrap()
        .to_string();
    let spdx3_purl = find_spdx3_software_package_by_name(&spdx3, "optional-dep").expect("spdx3")
        ["software_packageUrl"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(cdx_purl, "pkg:npm/optional-dep");
    assert_eq!(cdx_purl, spdx23_purl);
    assert_eq!(cdx_purl, spdx3_purl);
}

// ── FR-007 / SC-004: spdx3-validate conformance ───────────────────

fn spdx3_validate_or_skip(spdx3_path: &Path) {
    let bin_path = workspace_root().join(".venv/spdx3-validate/bin/spdx3-validate");
    if !bin_path.exists() {
        let require =
            std::env::var("MIKEBOM_REQUIRE_SPDX3_VALIDATOR").ok().as_deref() == Some("1");
        if require {
            panic!(
                "spdx3-validate not found at {} and MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 is set",
                bin_path.display()
            );
        }
        eprintln!(
            "[design_tier_versionless_purl] WARN: spdx3-validate not found at {}; skipping conformance gate",
            bin_path.display()
        );
        return;
    }
    let output = Command::new(&bin_path)
        .arg("--quiet")
        .arg("-j")
        .arg(spdx3_path)
        .output()
        .expect("spdx3-validate must run when binary exists");
    let combined_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && !combined_text.contains("Violation of type"),
        "spdx3-validate reported violations for {}:\n{}",
        spdx3_path.display(),
        combined_text
    );
}

#[test]
fn us2_spdx3_validate_accepts_versionless_design_tier() {
    let dir = tempfile::tempdir().unwrap();
    let project = build_optional_missing_project(dir.path());

    let workdir = tempfile::tempdir().unwrap();
    let fake_home = tempfile::tempdir().unwrap();
    let out_path = workdir.path().join("out.spdx3.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        project.to_str().unwrap(),
        "--format",
        "spdx-3-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");
    assert!(output.status.success(), "spdx-3 emission failed");

    spdx3_validate_or_skip(&out_path);
}
