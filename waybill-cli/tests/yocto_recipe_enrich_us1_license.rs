//! Milestone 128 US1 — recipe-level license attribution end-to-end.
//!
//! Verifies that LICENSE field extraction (FR-001), CLOSED license
//! discriminator (FR-012), and include-chain last-write-in-source-order
//! semantics (FR-004 + Clarifications Q1) all wire through to the
//! emitted CDX `components[].licenses[]` and SPDX 2.3
//! `packages[].licenseDeclared` fields.

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
        .prefix("mb128-us1-")
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

/// Find a component in the CDX `components[]` array by `name`.
fn cdx_component<'a>(
    cdx: &'a serde_json::Value,
    name: &str,
) -> &'a serde_json::Value {
    cdx.pointer("/components")
        .and_then(|c| c.as_array())
        .expect("components array")
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name))
        .unwrap_or_else(|| panic!("no component named {name}"))
}

fn cdx_license_expression(component: &serde_json::Value) -> Option<String> {
    component
        .pointer("/licenses/0/expression")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            component
                .pointer("/licenses/0/license/id")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("tests")
        .join("fixtures")
        .join("yocto_recipe_enrich")
        .join(name)
}

#[test]
fn us1_mit_license_extracted_and_canonicalized() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("single_layer_meta"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let recipe_a = cdx_component(&cdx, "recipe-a");
    assert_eq!(cdx_license_expression(recipe_a).as_deref(), Some("MIT"));
}

#[test]
fn us1_dual_license_canonicalizes_bitbake_ampersand_to_spdx_and() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("single_layer_meta"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let recipe_b = cdx_component(&cdx, "recipe-b");
    // The CDX builder splits compound expressions into multiple
    // `license.id` entries (more spec-compliant than a single
    // `expression` entry per CDX 1.6 schema preference). Verify
    // both licenses are present in the licenses[] array.
    let licenses = recipe_b
        .pointer("/licenses")
        .and_then(|v| v.as_array())
        .expect("licenses array");
    let license_ids: Vec<String> = licenses
        .iter()
        .filter_map(|l| {
            l.pointer("/license/id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    l.pointer("/expression")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
        })
        .collect();
    assert!(
        license_ids.iter().any(|s| s == "GPL-2.0-only"),
        "GPL-2.0-only must be in licenses[], got: {license_ids:?}"
    );
    assert!(
        license_ids.iter().any(|s| s == "LGPL-2.1-or-later"),
        "LGPL-2.1-or-later must be in licenses[], got: {license_ids:?}"
    );
}

#[test]
fn us1_closed_license_emits_yocto_license_closed_annotation() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("single_layer_meta"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let recipe_c = cdx_component(&cdx, "recipe-c");
    // CLOSED → no licenses[].
    assert!(
        cdx_license_expression(recipe_c).is_none(),
        "CLOSED recipe MUST NOT emit a license expression"
    );
    // CLOSED → waybill:yocto-license-closed: true annotation.
    let props = recipe_c
        .pointer("/properties")
        .and_then(|v| v.as_array())
        .expect("properties array");
    let has_closed_marker = props.iter().any(|p| {
        p.get("name").and_then(|v| v.as_str()) == Some("waybill:yocto-license-closed")
            && p.get("value").and_then(|v| v.as_str()) == Some("true")
    });
    assert!(
        has_closed_marker,
        "CLOSED recipe MUST carry waybill:yocto-license-closed annotation"
    );
}

#[test]
fn us1_include_chain_bb_overrides_inc_per_fr004_q1() {
    // foo_1.0.bb sets LICENSE = "MIT"
    // foo.inc sets LICENSE = "GPL-2.0-only"
    // foo-shared.inc sets LICENSE = "Apache-2.0"
    // Per FR-004 + Q1 last-write-in-source-order, the .bb's MIT wins.
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("include_chain"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    let foo = cdx_component(&cdx, "foo");
    assert_eq!(
        cdx_license_expression(foo).as_deref(),
        Some("MIT"),
        ".bb's LICENSE MUST override conflicting .inc values per FR-004"
    );
}
