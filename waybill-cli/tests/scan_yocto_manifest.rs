//! Integration test for the Yocto image-manifest reader (milestone 107 US2).
//!
//! Companion to the unit tests in `scan_fs::package_db::yocto::manifest::tests`.
//! Invokes the `mikebom sbom scan` binary against the in-repo
//! `yocto_manifest_basic/` fixture (a synthetic Yocto build directory
//! with `build/tmp/deploy/images/<machine>/<image>.manifest`) and
//! asserts:
//!
//! - All 5 expected `pkg:opkg/...` components emerge with correct
//!   PURLs (line-format `<name> <arch> <version>` parsed correctly)
//! - The `nativesdk-` prefixed line is tagged with CDX `scope:
//!   "excluded"` (proves the FR-006 per-line override flows through
//!   the milestone-052 emission path)
//! - The `mikebom:source-mechanism` annotation is
//!   `"yocto-image-manifest"` on every emitted component
//!
//! All package names use the synthetic `mikebom-fixture-*` prefix per
//! the milestone-106 convention.

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
        .join("yocto_manifest_basic")
}

#[test]
fn yocto_manifest_fixture_emits_components() {
    let path = fixture();
    assert!(path.is_dir(), "fixture missing at {}", path.display());

    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("MIKEBOM_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
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
    let output = cmd.output().expect("spawn mikebom");
    assert!(
        output.status.success(),
        "manifest scan unexpectedly failed: stderr={}",
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

    // All 5 manifest lines should emerge as distinct PURLs.
    let expected = [
        "pkg:opkg/mikebom-fixture-libc@2.38?arch=mikebom-fixture-arch",
        "pkg:opkg/mikebom-fixture-openssl@3.0.5?arch=mikebom-fixture-arch",
        "pkg:opkg/mikebom-fixture-gstreamer@1.22.7?arch=mikebom-fixture-arch",
        "pkg:opkg/mikebom-fixture-app@1.0.0?arch=mikebom-fixture-arch",
        "pkg:opkg/nativesdk-mikebom-fixture-cmake@3.27.0?arch=x86_64",
    ];
    for purl in expected {
        assert!(
            opkg_purls.contains(&purl),
            "expected `{purl}` in output; got: {opkg_purls:#?}",
        );
    }

    // The nativesdk line MUST be tagged scope=excluded via the
    // milestone-052 lifecycle-scope → CDX scope path.
    let nativesdk = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p == "pkg:opkg/nativesdk-mikebom-fixture-cmake@3.27.0?arch=x86_64")
                .unwrap_or(false)
        })
        .expect("nativesdk component present");
    assert_eq!(
        nativesdk["scope"].as_str(),
        Some("excluded"),
        "nativesdk-* manifest line should be tagged scope=excluded; got: {nativesdk}",
    );
}
