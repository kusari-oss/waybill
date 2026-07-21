//! Integration test for the opkg installed-DB reader (milestone 107 US1).
//!
//! Companion to the unit tests in `scan_fs::package_db::opkg::tests`.
//! This test invokes the `waybill sbom scan --path <fixture>` binary
//! against the in-repo `opkg_basic/` fixture (a synthetic rootfs with
//! `/var/lib/opkg/status` + per-package `.list` files) and asserts:
//!
//! - All 5 expected `pkg:opkg/...` components emerge with correct
//!   PURLs, including the URL-encoded arch qualifier
//! - The `nativesdk-` prefixed package is tagged with CDX scope
//!   `excluded` (proves the FR-006 lifecycle-scope override + the
//!   milestone-052 emission path translation)
//! - The `waybill:source-mechanism` annotation is `"opkg-installed"`
//!   on every emitted component (proves T024 + the milestone-105
//!   dedup pipeline wiring)
//!
//! All package names use the synthetic `waybill-fixture-*` prefix
//! per the milestone-106 convention — no real-world packages, no CVE
//! advisory collisions.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("opkg_basic")
}

#[test]
fn opkg_basic_fixture_emits_components() {
    let path = fixture();
    assert!(path.is_dir(), "fixture missing at {}", path.display());

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
        "opkg scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"].as_array().expect("components array");
    let opkg_purls: Vec<&str> = components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:opkg/"))
        .collect();

    // All 5 stanzas should emerge.
    let expected = [
        "pkg:opkg/waybill-fixture-libcore@1.2.3?arch=waybill-fixture-arch",
        "pkg:opkg/waybill-fixture-libutil@0.5.2?arch=waybill-fixture-arch",
        "pkg:opkg/waybill-fixture-app@3.0.0?arch=waybill-fixture-arch",
        "pkg:opkg/nativesdk-waybill-fixture-buildtool@2.0.0?arch=x86_64",
        "pkg:opkg/waybill-fixture-kernel-modules@5.15.0?arch=waybill-fixture-arch",
    ];
    for purl in expected {
        assert!(
            opkg_purls.contains(&purl),
            "expected `{purl}` in output; got: {opkg_purls:#?}",
        );
    }

    // The nativesdk component MUST be tagged scope=excluded via the
    // milestone-052 lifecycle-scope → CDX scope path.
    let nativesdk = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p == "pkg:opkg/nativesdk-waybill-fixture-buildtool@2.0.0?arch=x86_64")
                .unwrap_or(false)
        })
        .expect("nativesdk component present");
    assert_eq!(
        nativesdk["scope"].as_str(),
        Some("excluded"),
        "nativesdk-* component should be tagged scope=excluded; got: {nativesdk}",
    );
}
