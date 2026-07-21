//! Milestone 128 US4 — `.bbappend` provenance tracking.

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
) -> (serde_json::Value, String) {
    let out_dir = tempfile::Builder::new()
        .prefix("mb128-us4-")
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
    let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (json, stderr)
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

#[test]
fn us4_cross_layer_bbappend_matches_base_recipe() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let (cdx, _stderr) = run_scan(
        fake_home.path(),
        &fixture("multi_layer_polyglot"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // foo_1.0.bb is in layer-a; foo_%.bbappend is in layer-b. The
    // wildcard `%` matches foo@1.0 → foo carries waybill:bbappend-applied
    // listing the layer-b append path.
    let foo = cdx_component(&cdx, "foo");
    let applied = cdx_property(foo, "waybill:bbappend-applied")
        .expect("waybill:bbappend-applied property present on foo");
    assert!(
        applied.contains("foo_%.bbappend"),
        "FR-008: applied appends list MUST contain the matching .bbappend filename, got: {applied}"
    );
    // Layer-a's bar and layer-b's baz have NO matching appends —
    // they MUST NOT carry the annotation.
    let bar = cdx_component(&cdx, "bar");
    assert!(
        cdx_property(bar, "waybill:bbappend-applied").is_none(),
        "bar has no matching .bbappend; annotation must be absent"
    );
}

#[test]
fn us4_orphan_bbappend_warn_but_no_phantom() {
    let fake_home = tempfile::Builder::new()
        .prefix("mb128-home-")
        .tempdir()
        .unwrap();
    let (cdx, stderr) = run_scan(
        fake_home.path(),
        &fixture("orphan_bbappend"),
        "cyclonedx-json",
        "out.cdx.json",
    );
    // The recipe-z component IS emitted.
    let _z = cdx_component(&cdx, "recipe-z");
    // The orphan `nonexistent-recipe_%.bbappend` MUST NOT produce
    // any component (Constitution VIII completeness).
    let comps = cdx.pointer("/components").and_then(|c| c.as_array()).unwrap();
    let phantom = comps
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some("nonexistent-recipe"));
    assert!(
        phantom.is_none(),
        "FR-008: orphan .bbappend MUST NOT synthesize phantom components"
    );
    // The warn log MUST surface the orphan path on stderr.
    assert!(
        stderr.contains("orphan .bbappend")
            || stderr.contains("nonexistent-recipe_%.bbappend"),
        "FR-008: orphan .bbappend MUST emit a warn log naming the file; stderr was: {stderr}"
    );
}
