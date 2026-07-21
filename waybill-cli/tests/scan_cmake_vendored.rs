//! Integration test for `--include-vendored` CLI flag runtime behavior
//! (milestone 102 US3 / milestone 103 / Contract 8). Verifies:
//! - default OFF → zero vendored components
//! - flag ON (via env var) → vendored components emit with `waybill:vendored = true`
//! - path-prefix gate rejects first-party `add_subdirectory(src)`

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::fixture_path;

fn fixture() -> PathBuf {
    fixture_path("cmake")
}

fn scan_fixture(include_vendored: bool) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out_path = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let mut cmd = Command::new(bin);
    if include_vendored {
        cmd.env("WAYBILL_INCLUDE_VENDORED", "1");
    } else {
        // Explicitly clear so a host-level env doesn't leak into the test.
        cmd.env_remove("WAYBILL_INCLUDE_VENDORED");
    }
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("waybill should run");
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

#[test]
fn vendored_zero_by_default() {
    let sbom = scan_fixture(false);
    let foo = components_by_prefix(&sbom, "pkg:generic/foo");
    assert!(
        foo.is_empty(),
        "without --include-vendored, third_party/foo MUST NOT emit; got {foo:?}"
    );
}

#[test]
fn vendored_emitted_with_flag() {
    let sbom = scan_fixture(true);
    let foo = components_by_prefix(&sbom, "pkg:generic/foo");
    assert_eq!(
        foo.len(),
        1,
        "with WAYBILL_INCLUDE_VENDORED=1, third_party/foo MUST emit one component"
    );
    assert_eq!(
        foo[0]["purl"].as_str(),
        Some("pkg:generic/foo@1.2.3"),
        "version must be backfilled from third_party/foo/version.txt"
    );
    let vendored = component_property(foo[0], "waybill:vendored");
    assert_eq!(
        vendored,
        Some("true"),
        "waybill:vendored MUST be JSON boolean true (serializes as the string \"true\" when CDX property values are stringified); got {vendored:?}"
    );
}

#[test]
fn vendored_path_prefix_gate_rejects_first_party() {
    let sbom = scan_fixture(true);
    let src = components_by_prefix(&sbom, "pkg:generic/src");
    let tests = components_by_prefix(&sbom, "pkg:generic/tests");
    assert!(
        src.is_empty() && tests.is_empty(),
        "first-party add_subdirectory(src)/(tests) MUST NOT emit even with --include-vendored; got src={src:?}, tests={tests:?}"
    );
}
