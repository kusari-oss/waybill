//! Integration test for the Yocto SDK sysroot context detection
//! (milestone 107 US3).
//!
//! Scans the `yocto_sysroot/sdk-root/sysroots/<arch>/` directory of the
//! in-repo fixture. The SDK-root parent dir carries an
//! `environment-setup-*` script (the primary signal) so every emitted
//! opkg component MUST be tagged `LifecycleScope::Build` →
//! CDX `scope: "excluded"`.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

fn sysroot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("yocto_sysroot")
        .join("sdk-root")
        .join("sysroots")
        .join("mikebom-fixture-target")
}

#[test]
fn sdk_sysroot_scan_tags_every_component_with_build_scope() {
    let path = sysroot_path();
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
        "sysroot scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"].as_array().expect("components array");

    let opkg_components: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:opkg/"))
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        opkg_components.len(),
        3,
        "expected 3 opkg components from the sysroot fixture; got {}: {opkg_components:#?}",
        opkg_components.len()
    );

    // Primary signal (env-script in the grandparent) fires; the
    // synthetic sysroot has no /usr/include/ or /etc/init.d/ in the
    // scan target itself, so the secondary signal is also absent —
    // but that's NOT an ambiguity (primary alone is sufficient). Every
    // emitted component MUST carry CDX scope=excluded.
    for component in &opkg_components {
        let purl = component["purl"].as_str().unwrap_or("(none)");
        assert_eq!(
            component["scope"].as_str(),
            Some("excluded"),
            "sysroot component `{purl}` should be tagged scope=excluded \
             (primary env-script signal fires from the SDK parent dir); got: {component}",
        );
    }

    // No `mikebom:scan-ambiguity` annotation on metadata.properties — the
    // fixture is unambiguous (primary fires; secondary is absent but not
    // conflicting).
    if let Some(props) = json["metadata"]["properties"].as_array() {
        for prop in props {
            assert_ne!(
                prop["name"].as_str(),
                Some("mikebom:scan-ambiguity"),
                "unexpected scan-ambiguity annotation on a clean sysroot fixture",
            );
        }
    }
}
