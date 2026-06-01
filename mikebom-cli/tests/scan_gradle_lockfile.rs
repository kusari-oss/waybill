//! Integration test for the Gradle lockfile reader (milestone 106 US3, issue #277).
//!
//! Companion to the unit tests in `scan_fs::package_db::gradle::lockfile::tests`
//! (which exercise `read_gradle_lockfile` directly). This test invokes the
//! `mikebom sbom scan --path <fixture>` binary against the in-repo
//! `gradle_lockfile/runtime_only/` and `gradle_lockfile/buildscript_classpath/`
//! fixtures to verify the dispatcher integration — `gradle::read` is called
//! from `read_all`, the project-roots walker finds both lockfile filenames,
//! and the emitted SBOM contains the expected `pkg:maven/...` components
//! plus the build-lifecycle scope on the buildscript fixture.

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
        .join("gradle_lockfile")
        .join(subdir)
}

fn run_scan(path: &std::path::Path) -> serde_json::Value {
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
        "gradle scan unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

#[test]
fn gradle_lockfile_runtime_only_emits_maven_components() {
    let path = fixture("runtime_only");
    assert!(path.is_dir(), "fixture missing at {}", path.display());
    let json = run_scan(&path);
    let components = json["components"].as_array().expect("components array");
    let maven_purls: Vec<&str> = components
        .iter()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:maven/"))
        .collect();
    assert!(
        maven_purls.contains(&"pkg:maven/com.google.guava/guava@32.1.3-jre"),
        "expected guava in output; got: {maven_purls:?}",
    );
    assert!(
        maven_purls
            .contains(&"pkg:maven/org.jetbrains.kotlin/kotlin-stdlib@1.9.20"),
        "expected kotlin-stdlib in output; got: {maven_purls:?}",
    );
}

#[test]
fn gradle_lockfile_buildscript_tags_build_scope() {
    let path = fixture("buildscript_classpath");
    assert!(path.is_dir(), "fixture missing at {}", path.display());
    let json = run_scan(&path);
    let components = json["components"].as_array().expect("components array");
    let springboot = components
        .iter()
        .find(|c| {
            c["purl"]
                .as_str()
                .map(|p| {
                    p == "pkg:maven/org.springframework.boot/org.springframework.boot.gradle.plugin@3.2.0"
                })
                .unwrap_or(false)
        })
        .expect("springboot plugin component present");
    // Milestone 052 emission path maps LifecycleScope::Build -> CDX "excluded".
    assert_eq!(
        springboot["scope"].as_str(),
        Some("excluded"),
        "buildscript entry should be tagged scope=excluded; got: {springboot}",
    );
}
