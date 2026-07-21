//! Integration test for the yarn.lock reader (milestone 106 US5, issue #274).
//!
//! Companion to the unit tests in `scan_fs::package_db::npm::yarn_lock::tests`.
//! This test invokes the `waybill sbom scan --path <fixture>` binary against
//! TWO in-repo fixtures (one yarn-v1, one Berry) to verify both formats
//! are auto-detected and parsed end-to-end through to emitted CDX.
//!
//! All fixture package names use the synthetic `waybill-fixture-*` prefix
//! (and `@waybill-fixture/*` scope) so they never collide with real-world
//! CVE advisories — applying the lesson from PR #285's lodash flagging.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn fixture(subdir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("yarn_lock")
        .join(subdir)
}

fn run_scan(path: &std::path::Path) -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "yarn scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

fn npm_purls(json: &serde_json::Value) -> Vec<String> {
    json["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:npm/"))
        .map(String::from)
        .collect()
}

#[test]
fn yarn_v1_basic_fixture_emits_npm_components() {
    let path = fixture("v1_basic");
    let json = run_scan(&path);
    let purls = npm_purls(&json);
    assert!(
        purls.contains(&"pkg:npm/waybill-fixture-lib@1.2.3".to_string()),
        "expected waybill-fixture-lib@1.2.3 in output; got: {purls:?}",
    );
    assert!(
        purls.contains(&"pkg:npm/%40mikebom-fixture/types-pkg@4.5.6".to_string()),
        "expected URL-encoded @waybill-fixture/types-pkg@4.5.6 in output; got: {purls:?}",
    );
}

#[test]
fn yarn_berry_basic_fixture_emits_npm_components() {
    let path = fixture("berry_basic");
    let json = run_scan(&path);
    let purls = npm_purls(&json);
    assert!(
        purls.contains(&"pkg:npm/waybill-fixture-lib@1.2.3".to_string()),
        "expected waybill-fixture-lib@1.2.3 in output; got: {purls:?}",
    );
    assert!(
        purls.contains(&"pkg:npm/%40mikebom-fixture/types-pkg@4.5.6".to_string()),
        "expected URL-encoded @waybill-fixture/types-pkg@4.5.6 in output; got: {purls:?}",
    );
}
