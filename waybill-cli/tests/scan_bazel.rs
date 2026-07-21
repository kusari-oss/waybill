//! End-to-end integration test for the Bazel source-tree reader
//! (milestone 102 US1 / milestone 103 / Contracts 1+2+3). Scans the
//! sibling-repo `bazel/` fixture and asserts the emitted CDX SBOM
//! contains the expected `pkg:bazel/...` components with correct
//! PURLs, annotations, and standards-native scope mapping.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("bazel")
}

fn scan_fixture() -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let output = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .output()
        .expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = std::fs::read_to_string(&out_path).expect("read sbom");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn components_by_prefix<'a>(
    sbom: &'a serde_json::Value,
    prefix: &str,
) -> Vec<&'a serde_json::Value> {
    sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .filter(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with(prefix))
        })
        .collect()
}

fn component_property<'a>(c: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    c["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str())
}

/// Contract 1: MODULE.bazel `bazel_dep` emits `pkg:bazel/<name>@<version>`.
/// Standards-native CDX `scope` field MUST reflect the dev_dependency
/// flag per Principle V (FR-002 / FR-012).
#[test]
fn bazel_module_emits_pkg_bazel_purls_with_native_scope() {
    let sbom = scan_fixture();

    let abseil = components_by_prefix(&sbom, "pkg:bazel/abseil-cpp");
    assert_eq!(abseil.len(), 1);
    assert_eq!(
        abseil[0]["purl"].as_str(),
        Some("pkg:bazel/abseil-cpp@20240722.0")
    );

    let googletest = components_by_prefix(&sbom, "pkg:bazel/googletest");
    assert_eq!(googletest.len(), 1);
    assert_eq!(
        googletest[0]["purl"].as_str(),
        Some("pkg:bazel/googletest@1.14.0")
    );
    // dev_dependency = True → LifecycleScope::Development, which
    // emits as `waybill:lifecycle-scope = "development"` per the
    // existing milestone-052 mapping.
    let scope_prop = component_property(googletest[0], "waybill:lifecycle-scope");
    assert_eq!(
        scope_prop,
        Some("development"),
        "dev_dependency = True must set CDX scope per Principle V; got {scope_prop:?}"
    );
}

/// Contract 2: WORKSPACE.bazel http_archive emits URL + sha256 + archive-name.
#[test]
fn bazel_workspace_http_archive_emits_url_and_sha() {
    let sbom = scan_fixture();
    let rules_python = components_by_prefix(&sbom, "pkg:bazel/rules_python");
    assert_eq!(rules_python.len(), 1);
    assert_eq!(
        component_property(rules_python[0], "waybill:download-url"),
        Some("https://github.com/bazelbuild/rules_python/archive/0.30.0.tar.gz")
    );
    assert_eq!(
        component_property(rules_python[0], "waybill:bazel-archive-name"),
        Some("rules_python")
    );
    // SHA-256 must be in component.hashes[] (CDX-native), not a property.
    let hashes = rules_python[0]["hashes"]
        .as_array()
        .expect("hashes array");
    assert!(
        hashes.iter().any(|h| h["alg"].as_str() == Some("SHA-256")),
        "expected SHA-256 in hashes[]; got {hashes:?}"
    );
}

/// Contract 3: git_repository commit truncates to short-SHA in PURL.
#[test]
fn bazel_workspace_git_repository_short_sha() {
    let sbom = scan_fixture();
    let rules_foo = components_by_prefix(&sbom, "pkg:bazel/rules_foo");
    assert_eq!(rules_foo.len(), 1);
    assert_eq!(
        rules_foo[0]["purl"].as_str(),
        Some("pkg:bazel/rules_foo@deadbee"),
        "git commit SHA must truncate to first 7 chars; got {:?}",
        rules_foo[0]["purl"].as_str()
    );
    assert_eq!(
        component_property(rules_foo[0], "waybill:download-url"),
        Some("https://github.com/foo/rules_foo.git")
    );
}

/// FR-010: every Bazel component carries `waybill:source-files`.
#[test]
fn bazel_components_carry_source_files() {
    let sbom = scan_fixture();
    let bazel_components = components_by_prefix(&sbom, "pkg:bazel/");
    assert!(
        !bazel_components.is_empty(),
        "expected ≥1 pkg:bazel/ component"
    );
    for c in &bazel_components {
        let source = component_property(c, "waybill:source-files")
            .expect("every bazel component must carry waybill:source-files (FR-010)");
        assert!(
            source.contains("MODULE.bazel") || source.contains("WORKSPACE.bazel"),
            "source-files must point at a Bazel manifest; got {source:?}"
        );
    }
}
