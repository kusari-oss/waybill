//! Integration test for the uv.lock reader (milestone 106 US1, issue #276).
//!
//! Companion to the unit tests in `scan_fs::package_db::pip::uv_lock::tests`
//! (which exercise `parse_uv_lock` directly). This test invokes the
//! `mikebom sbom scan --path <fixture>` binary against the in-repo
//! `uv_lock/basic/` fixture to verify the dispatcher integration —
//! `pip::read` actually calls `uv_lock::read_uv_lock`, the emitted
//! SBOM contains the expected `pkg:pypi/...` components, and the
//! dependency edges from `[[package.dependencies]]` appear in the
//! relationship graph.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::bin;
use common::normalize::apply_fake_home_env;

/// Path to the in-repo uv_lock/basic fixture. Per research R6,
/// milestone-106 fixtures live in the local repo (not the external
/// mikebom-test-fixtures repo) because they're small + tightly
/// coupled to the per-reader implementation.
fn basic_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("uv_lock")
        .join("basic")
}

#[test]
fn uv_lock_basic_fixture_emits_pypi_components() {
    let fixture = basic_fixture();
    assert!(
        fixture.is_dir(),
        "uv_lock/basic fixture missing at {}",
        fixture.display()
    );
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
        fixture.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let output = cmd.output().expect("spawn mikebom");
    assert!(
        output.status.success(),
        "uv.lock scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");

    let components = json["components"]
        .as_array()
        .expect("components array present");

    // The basic fixture has 4 PyPI packages in uv.lock: httpx, anyio,
    // certifi, pydantic. All four MUST emerge as pkg:pypi/... components.
    let pypi_components: Vec<&str> = components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:pypi/"))
        .collect();

    for expected_purl in [
        "pkg:pypi/httpx@0.27.2",
        "pkg:pypi/anyio@4.4.0",
        "pkg:pypi/certifi@2024.8.30",
        "pkg:pypi/pydantic@2.9.2",
    ] {
        assert!(
            pypi_components.contains(&expected_purl),
            "expected {expected_purl} in output; got pypi components: {pypi_components:?}",
        );
    }

    // Verify dependency edges from [[package.dependencies]] surface in
    // the CDX dependency graph. httpx depends on anyio + certifi per
    // the fixture's uv.lock.
    let dependencies = json["dependencies"]
        .as_array()
        .expect("dependencies array present");
    let httpx_deps = dependencies
        .iter()
        .find(|d| d["ref"].as_str() == Some("pkg:pypi/httpx@0.27.2"))
        .expect("httpx dependency-graph entry present")
        ["dependsOn"]
        .as_array()
        .expect("httpx dependsOn array present")
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(
        httpx_deps
            .iter()
            .any(|d| d == &"pkg:pypi/anyio@4.4.0"),
        "httpx MUST have dependsOn edge to anyio; got: {httpx_deps:?}",
    );
    assert!(
        httpx_deps
            .iter()
            .any(|d| d == &"pkg:pypi/certifi@2024.8.30"),
        "httpx MUST have dependsOn edge to certifi; got: {httpx_deps:?}",
    );
}
