//! Integration test for the BitBake recipe walker (milestone 107 US4).
//!
//! Companion to the unit tests in `scan_fs::package_db::yocto::recipe::tests`.
//! Invokes the `mikebom sbom scan` binary against the in-repo
//! `yocto_recipe_layer/` fixture (a synthetic `meta-mikebom-fixture/`
//! Yocto layer with 4 recipe files) and asserts:
//!
//! - 3 `pkg:generic/...` components emerge (the well-formed recipes +
//!   the no-version recipe; the `${PN}_${PV}.bb` is silently skipped
//!   per FR-008)
//! - Each emitted component carries the `?layer=meta-mikebom-fixture`
//!   qualifier (proves the layer-root detection walked up correctly)
//! - The no-version recipe emerges with `version: "unknown"` + a
//!   `mikebom:version-status: "missing"` annotation
//! - Every emitted component carries
//!   `mikebom:source-mechanism: "bitbake-recipe"`
//!
//! All fixture recipe names use the synthetic `mikebom-fixture-*`
//! prefix per the milestone-106 convention.

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
        .join("yocto_recipe_layer")
}

#[test]
fn yocto_recipe_layer_emits_bitbake_components() {
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
        "recipe scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let components = json["components"].as_array().expect("components array");
    // Milestone 128 (FR-001): bitbake recipes now emit
    // `pkg:generic/<name>@<ver>?...openembedded=true`. Filter on the
    // `openembedded=true` qualifier to isolate recipe components from
    // any other `pkg:generic/` synthesized component (e.g. layer-roots).
    let bitbake_components: Vec<&serde_json::Value> = components
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.starts_with("pkg:generic/") && p.contains("openembedded=true"))
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(
        bitbake_components.len(),
        3,
        "expected 3 bitbake components (lib + app + noversion; \
         the ${{PN}}_${{PV}}.bb is silently skipped); got {}: \
         {bitbake_components:#?}",
        bitbake_components.len()
    );

    let bitbake_purls: Vec<&str> = bitbake_components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .collect();

    let expected = [
        "pkg:generic/mikebom-fixture-lib@1.2.3?layer=meta-mikebom-fixture&openembedded=true",
        // `+` in version becomes `%2B` per the package-url spec via encode_purl_segment.
        "pkg:generic/mikebom-fixture-app@2.0%2Bgit1234abcd?layer=meta-mikebom-fixture&openembedded=true",
        "pkg:generic/mikebom-fixture-noversion@unknown?layer=meta-mikebom-fixture&openembedded=true",
    ];
    for purl in expected {
        assert!(
            bitbake_purls.contains(&purl),
            "expected `{purl}` in output; got: {bitbake_purls:#?}",
        );
    }

    // Every emitted component carries the source-mechanism annotation.
    for component in &bitbake_components {
        let mechanism = find_property(component, "mikebom:source-mechanism");
        assert_eq!(
            mechanism.as_deref(),
            Some("bitbake-recipe"),
            "component {component} missing source-mechanism annotation",
        );
    }

    // The no-version recipe carries version-status: "missing".
    let noversion = bitbake_components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p.contains("mikebom-fixture-noversion"))
                .unwrap_or(false)
        })
        .expect("noversion component present");
    let status = find_property(noversion, "mikebom:version-status");
    assert_eq!(
        status.as_deref(),
        Some("missing"),
        "noversion component should have version-status=missing; got: {noversion}"
    );
}

/// CDX serializes per-component `mikebom:*` annotations as
/// `properties[]` entries with `name` + `value` keys. This helper
/// returns the first matching property's `value` as a String.
fn find_property(component: &serde_json::Value, name: &str) -> Option<String> {
    component["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(str::to_string))
}
