//! Milestone 235 US1 subprocess resolver — end-to-end integration test.
//!
//! Companion to the unit tests in
//! `scan_fs::package_db::gradle::subprocess::tests` (which exercise the
//! ASCII-tree parser + PURL construction directly). This test invokes
//! the `waybill sbom scan --path <fixture> --gradle-resolve` binary
//! against the `wrapper_single_subproject/` fixture and asserts the
//! emitted CDX contains:
//!
//! - A component for the direct dep (`waybillfixture:direct@1.0.0`)
//! - A component for the mock's transitive dep
//!   (`waybillfixture:transitive@0.5.0`)
//! - A component for the test-scope dep
//!   (`waybillfixture:test-only@2.0.0`)
//! - A `dependencies[]` edge from direct → transitive
//!
//! The fixture's `./gradlew` is a bash mock (no JDK required); it
//! emits canned ASCII-tree output for the configurations the m235
//! subprocess resolver requests by default (runtimeClasspath +
//! testRuntimeClasspath per clarify Q1).
//!
//! Golden fixture pinning (T019) is deferred to a follow-on PR;
//! structural assertions here cover SC-001's "transitive edge
//! present" acceptance criterion.

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
        .join("gradle")
        .join("wrapper_single_subproject")
}

fn run_scan_with_gradle_resolve() -> serde_json::Value {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();
    assert!(
        fixture_path.is_dir(),
        "fixture missing at {}",
        fixture_path.display()
    );

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--gradle-resolve",
        "--gradle-timeout-secs",
        "30",
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "gradle-resolve scan failed:\n  stdout={}\n  stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    serde_json::from_slice(&bytes).expect("parse JSON")
}

/// Extract all `pkg:maven/*` PURLs from an SBOM's `components[]` array
/// AND the top-level metadata.component (waybill's main-module
/// placement varies by ecosystem).
fn maven_purls(json: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = json["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["purl"].as_str())
                .filter(|p| p.starts_with("pkg:maven/"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(main) = json["metadata"]["component"]["purl"].as_str() {
        if main.starts_with("pkg:maven/") {
            out.push(main.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Return `true` iff there exists an edge with `ref == source_purl`
/// AND `target_purl` present in the `dependsOn` array.
fn has_edge(json: &serde_json::Value, source_purl: &str, target_purl: &str) -> bool {
    let Some(deps) = json["dependencies"].as_array() else {
        return false;
    };
    for dep in deps {
        if dep["ref"].as_str() == Some(source_purl) {
            let Some(depends_on) = dep["dependsOn"].as_array() else {
                continue;
            };
            for t in depends_on {
                if t.as_str() == Some(target_purl) {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract the value of a CDX document-scope `metadata.properties[]`
/// entry with the given name. Returns `None` if the property is absent.
fn doc_scope_property(json: &serde_json::Value, name: &str) -> Option<String> {
    json["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(str::to_string))
}

// -----------------------------------------------------------
// SC-001 — US1 fixture emits transitive edge in CDX
// -----------------------------------------------------------

#[test]
fn us1_wrapper_single_subproject_transitive_edge() {
    let json = run_scan_with_gradle_resolve();

    let purls = maven_purls(&json);
    let direct = "pkg:maven/com.example.waybillfixture/direct@1.0.0";
    let transitive = "pkg:maven/com.example.waybillfixture/transitive@0.5.0";
    let test_only = "pkg:maven/com.example.waybillfixture/test-only@2.0.0";

    assert!(
        purls.iter().any(|p| p == direct),
        "expected direct dep component; got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == transitive),
        "expected transitive dep component (proves ASCII-tree parser recorded child of direct); got: {purls:?}"
    );
    assert!(
        purls.iter().any(|p| p == test_only),
        "expected testRuntimeClasspath dep component (Clarifications Q1 default set); got: {purls:?}"
    );

    // SC-001 acceptance — the transitive edge is emitted in
    // `dependencies[]`. This is the m235 US1 promise: subprocess
    // tier surfaces parent → child relationships that m106 lockfile
    // reading cannot.
    assert!(
        has_edge(&json, direct, transitive),
        "expected dependencies[] edge {direct} -> {transitive}; got: {}",
        serde_json::to_string_pretty(&json["dependencies"]).unwrap_or_default()
    );
}

// -----------------------------------------------------------
// FR-009 non-regression — scan WITHOUT --gradle-resolve emits
// zero m235 components (the fixture has no gradle.lockfile).
// -----------------------------------------------------------

#[test]
fn without_gradle_resolve_the_fixture_produces_no_maven_components() {
    // Verifies the m106-only path: without --gradle-resolve, waybill
    // looks for `gradle.lockfile` (absent in this fixture) and finds
    // nothing. FR-009 non-regression.
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let out_path = workdir.path().join("sbom.cdx.json");
    let fixture_path = fixture();

    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture_path.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
        "--no-deep-hash",
    ]);
    let output = cmd.output().expect("spawn waybill");
    assert!(
        output.status.success(),
        "no-flag scan failed unexpectedly:\n  stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    let bytes = std::fs::read(&out_path).expect("read emitted SBOM");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse JSON");
    let purls = maven_purls(&json);
    let fixture_maven: Vec<&String> = purls
        .iter()
        .filter(|p| p.contains("waybillfixture"))
        .collect();
    assert!(
        fixture_maven.is_empty(),
        "expected zero m235-fixture components without --gradle-resolve; got: {fixture_maven:?}"
    );
}

// -----------------------------------------------------------
// FR-006 / SC-004 — US4 tier annotation appears on Gradle-touching
// scans (subprocess variant).
// -----------------------------------------------------------

#[test]
fn us4_tier_annotation_present_on_subprocess_scan() {
    let json = run_scan_with_gradle_resolve();
    // FR-006: every scan touching a Gradle project MUST carry the
    // `waybill:gradle-resolution-tier` document-scope annotation.
    // With `--gradle-resolve` set and the mock wrapper emitting real
    // dependency output, the tier MUST be `subprocess`.
    let tier = doc_scope_property(&json, "waybill:gradle-resolution-tier");
    assert_eq!(
        tier.as_deref(),
        Some("subprocess"),
        "expected doc-scope waybill:gradle-resolution-tier=subprocess; got {tier:?}"
    );
}
