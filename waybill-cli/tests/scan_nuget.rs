//! Integration test for the NuGet reader (milestone 106 US4, issue #275).
//!
//! Companion to the unit tests in `scan_fs::package_db::nuget::*`. This
//! test invokes the `waybill sbom scan --path <fixture>` binary against
//! four in-repo fixtures and asserts the emitted CDX contains the
//! expected `pkg:nuget/...` components with correct version resolution,
//! CPM walk-up, PrivateAssets-driven build-only tagging, and
//! packages.lock.json transitive emission.
//!
//! All fixture package names use the synthetic `MikebomFixture.*`
//! prefix so the fixtures never collide with real-world CVE advisories
//! (lessons learned from PR #285).

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
        .join("nuget")
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
        "nuget scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

fn nuget_purls(json: &serde_json::Value) -> Vec<String> {
    json["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:nuget/"))
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn csproj_legacy_emits_nuget_components() {
    let path = fixture("csproj_legacy");
    let json = run_scan(&path);
    let purls = nuget_purls(&json);
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.SampleLib@1.2.3".to_string()),
        "expected SampleLib in output; got: {purls:?}",
    );
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.OtherLib@2.3.4".to_string()),
        "expected OtherLib in output; got: {purls:?}",
    );
}

#[test]
fn csproj_cpm_resolves_via_directory_packages_props() {
    let path = fixture("csproj_cpm");
    let json = run_scan(&path);
    let purls = nuget_purls(&json);
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.CpmLib@9.0.1".to_string()),
        "expected CpmLib at CPM-resolved version 9.0.1; got: {purls:?}",
    );
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.CpmOther@2.0.0".to_string()),
        "expected CpmOther at 2.0.0; got: {purls:?}",
    );
}

#[test]
fn private_assets_all_tags_build_scope() {
    let path = fixture("private_assets_all");
    let json = run_scan(&path);
    let components = json["components"].as_array().expect("components");
    let runtime = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p == "pkg:nuget/MikebomFixture.RuntimeLib@1.0.0")
                .unwrap_or(false)
        })
        .expect("RuntimeLib present");
    // Runtime entry must NOT carry scope=excluded.
    assert_ne!(runtime["scope"].as_str(), Some("excluded"));

    let sourcelink = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p == "pkg:nuget/MikebomFixture.SourceLink@1.0.0")
                .unwrap_or(false)
        })
        .expect("SourceLink present");
    assert_eq!(sourcelink["scope"].as_str(), Some("excluded"));

    let analyzer = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| p == "pkg:nuget/MikebomFixture.Analyzer@1.0.0")
                .unwrap_or(false)
        })
        .expect("Analyzer present");
    assert_eq!(analyzer["scope"].as_str(), Some("excluded"));
}

#[test]
fn packages_lock_overrides_csproj_and_emits_transitives() {
    let path = fixture("packages_lock_present");
    let json = run_scan(&path);
    let purls = nuget_purls(&json);
    // Lockfile's "resolved": "1.2.4" wins over csproj's "[1.2.3, )".
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.SampleLib@1.2.4".to_string()),
        "expected lockfile-resolved 1.2.4; got: {purls:?}",
    );
    // Transitive must appear from the lockfile.
    assert!(
        purls.contains(&"pkg:nuget/MikebomFixture.SubDep@0.5.0".to_string()),
        "expected transitive SubDep@0.5.0; got: {purls:?}",
    );
    // The csproj's range-only ([1.2.3, )) must NOT emit as a separate
    // component — lockfile took precedence.
    assert!(
        !purls.iter().any(|p| p.contains("SampleLib@%5B1.2.3")),
        "range string should not survive into emitted purls; got: {purls:?}",
    );
}
