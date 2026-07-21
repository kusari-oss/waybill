//! Milestone 128 US3 + US5 + Phase 9 — layer attribution, layer-root
//! BOM subject, DEPENDS edges, CPE-name normalization.
//!
//! Verifies:
//! - Every recipe carries `waybill:yocto-layer` annotation pointing
//!   at its nearest-ancestor `conf/layer.conf`'s `BBFILE_COLLECTIONS`.
//! - The BOM subject identifies a layer-collection name via the
//!   milestone-127 root-selector ladder.
//! - `DEPENDS_ON` relationship edges connect recipes within the scan.
//! - Unresolved DEPENDS entries surface in `waybill:depends-unresolved`.
//! - `waybill:cpe-candidates` array includes the openembedded-core
//!   normalized CPE product name (e.g., `linux_kernel`) when
//!   FR-017's table fires.

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
        .prefix("mb128-us3-")
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

fn cdx_property<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .pointer("/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
}

fn cdx_property_array(
    component: &serde_json::Value,
    name: &str,
) -> Option<Vec<String>> {
    let s = cdx_property(component, name)?;
    let parsed: serde_json::Value = serde_json::from_str(s).ok()?;
    parsed
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
}

#[test]
fn us3_each_recipe_attributed_to_nearest_ancestor_layer() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("multi_layer_polyglot"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // foo + bar in layer-a; baz in layer-b.
    let foo = cdx_component(&cdx, "foo");
    let bar = cdx_component(&cdx, "bar");
    let baz = cdx_component(&cdx, "baz");
    assert_eq!(cdx_property(foo, "waybill:yocto-layer"), Some("layer-a"));
    assert_eq!(cdx_property(bar, "waybill:yocto-layer"), Some("layer-a"));
    assert_eq!(cdx_property(baz, "waybill:yocto-layer"), Some("layer-b"));
}

#[test]
fn us3_layer_root_bom_subject_is_collection_not_directory_basename() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("multi_layer_polyglot"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // metadata.component should be one of the layer collections,
    // not `pkg:generic/multi_layer_polyglot@0.0.0` (the pre-128 fallback).
    let subject_purl = cdx
        .pointer("/metadata/component/purl")
        .and_then(|v| v.as_str())
        .expect("metadata.component.purl present");
    assert!(
        subject_purl.starts_with("pkg:generic/layer-a@") || subject_purl.starts_with("pkg:generic/layer-b@"),
        "BOM subject MUST be a layer-collection PURL per FR-007 + milestone-127 root selector, got: {subject_purl}"
    );
    // It MUST NOT be a `multi_layer_polyglot@0.0.0` placeholder.
    assert!(
        !subject_purl.contains("multi_layer_polyglot"),
        "FR-007: BOM subject must NOT be the directory-basename placeholder"
    );
}

#[test]
fn us5_depends_resolves_to_relationships_for_in_scope_recipes() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let cdx = run_scan(
        fake_home.path(),
        &fixture("multi_layer_polyglot"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // foo depends on bar (resolvable in scope) AND openssl-dev (unresolvable).
    let foo = cdx_component(&cdx, "foo");

    // FR-009: unresolved entry appears under waybill:depends-unresolved.
    let unresolved = cdx_property_array(foo, "waybill:depends-unresolved")
        .expect("waybill:depends-unresolved property present");
    assert!(
        unresolved.iter().any(|s| s == "openssl-dev"),
        "openssl-dev MUST appear in waybill:depends-unresolved (no openssl-dev recipe in scan), got: {unresolved:?}"
    );

    // foo → bar DEPENDS_ON edge MUST exist in CDX dependencies[].
    let foo_ref = foo.get("bom-ref").and_then(|v| v.as_str()).expect("foo bom-ref");
    let bar = cdx_component(&cdx, "bar");
    let bar_ref = bar.get("bom-ref").and_then(|v| v.as_str()).expect("bar bom-ref");
    let deps = cdx
        .pointer("/dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies array");
    let foo_deps = deps
        .iter()
        .find(|d| d.get("ref").and_then(|v| v.as_str()) == Some(foo_ref))
        .expect("foo entry in dependencies[]");
    let depends_on: Vec<String> = foo_deps
        .get("dependsOn")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        depends_on.iter().any(|s| s == bar_ref),
        "foo MUST DEPENDS_ON bar; foo's dependsOn = {depends_on:?}, bar's bom-ref = {bar_ref}"
    );
}

#[test]
fn phase9_cpe_candidates_includes_openembedded_normalized_name() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let tmp_fixture = tempfile::Builder::new()
        .prefix("mb128-cpe-")
        .tempdir()
        .unwrap();
    let root = tmp_fixture.path();
    // Synthesize a layer with a linux-kernel recipe in-place.
    std::fs::create_dir_all(root.join("conf")).unwrap();
    std::fs::write(
        root.join("conf").join("layer.conf"),
        r#"BBFILE_COLLECTIONS += "fixture-cpe"
LAYERVERSION_fixture-cpe = "1"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("recipes-kernel")).unwrap();
    std::fs::write(
        root.join("recipes-kernel").join("linux-kernel_6.18.bb"),
        r#"LICENSE = "GPL-2.0-only"
SUMMARY = "Linux kernel recipe"
"#,
    )
    .unwrap();
    let cdx = run_scan(fake_home.path(), root, "cyclonedx-json", "out.cdx.json");
    let kernel = cdx_component(&cdx, "linux-kernel");
    let candidates = cdx_property_array(kernel, "waybill:cpe-candidates")
        .expect("waybill:cpe-candidates present");
    assert!(
        candidates.iter().any(|s| s == "linux-kernel"),
        "raw recipe name MUST be in cpe-candidates, got: {candidates:?}"
    );
    assert!(
        candidates.iter().any(|s| s == "linux_kernel"),
        "FR-017: cpe_name_map MUST translate `linux-kernel` → `linux_kernel`, got: {candidates:?}"
    );
}
