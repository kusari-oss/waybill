//! End-to-end integration test for the Conan recipe reader
//! (milestone 102 US3 / spec FR-008 / Contract 8). Scans the in-repo
//! `tests/fixtures/conan/` directory and asserts the emitted CDX SBOM
//! contains the expected `pkg:conan/...` components with the right
//! scope on `[tool_requires]` entries.

use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/conan")
}

fn scan_fixture(exclude_dev_test_build: bool) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let mut cmd = Command::new(bin);
    cmd.arg("--offline");
    if exclude_dev_test_build {
        cmd.arg("--exclude-scope").arg("dev,build,test");
    }
    cmd.arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("mikebom should run");
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

/// Contract 8 (FR-008): `[requires]` lines emit components with NO scope.
#[test]
fn conan_txt_requires_emit_runtime_scope() {
    let sbom = scan_fixture(false);
    let zlib = components_by_prefix(&sbom, "pkg:conan/zlib");
    assert!(
        zlib.iter().any(|c| {
            c["purl"].as_str() == Some("pkg:conan/zlib@1.2.13")
        }),
        "expected pkg:conan/zlib@1.2.13 from [requires]; got {zlib:?}"
    );
    let openssl = components_by_prefix(&sbom, "pkg:conan/openssl");
    assert!(openssl.iter().any(|c| {
        c["purl"].as_str() == Some("pkg:conan/openssl@3.0.0")
    }));
}

/// Contract 8 (FR-008): `[tool_requires]` lines emit components with
/// CDX `scope = "excluded"` (the existing milestone-052 mapping for
/// `LifecycleScope::Build` → CDX scope per Constitution Principle V).
#[test]
fn conan_txt_tool_requires_emit_build_scope_then_excluded_by_default() {
    let sbom = scan_fixture(true);
    let cmake = components_by_prefix(&sbom, "pkg:conan/cmake");
    // With `--exclude-scope dev,build,test`, the cmake/3.27.0 entry
    // from [tool_requires] (which is Build scope) MUST be filtered out.
    assert_eq!(
        cmake.len(),
        0,
        "tool_requires-scoped cmake should be filtered by --exclude-scope=build; got {cmake:?}"
    );
}

#[test]
fn conan_txt_tool_requires_present_by_default() {
    let sbom = scan_fixture(false);
    let cmake = components_by_prefix(&sbom, "pkg:conan/cmake");
    assert!(
        cmake.iter().any(|c| {
            c["purl"].as_str() == Some("pkg:conan/cmake@3.27.0")
        }),
        "expected pkg:conan/cmake@3.27.0 from [tool_requires] when no scope-filter set; got {cmake:?}"
    );
}
