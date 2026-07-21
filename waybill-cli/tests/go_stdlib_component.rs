//! Issue #364 integration test — Go SBOMs include a `stdlib`
//! component carrying the Go toolchain version from the project's
//! go.mod. Closes the vulnerability-scanning gap (e.g. CVE-2024-34156
//! big.Int overflow) on the same shape syft v1.42.3 produces:
//!
//!   PURL: pkg:golang/stdlib@v<go-version>
//!   CPE:  cpe:2.3:a:golang:go:<go-version>:*:*:*:*:*:*:*

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join(sub)
}

fn run_scan(path: &Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--file-inventory=off")
        .arg("--no-deep-hash")
        .status()
        .expect("waybill should run");
    assert!(status.success(), "scan failed");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn find_stdlib(sbom: &serde_json::Value) -> Option<&serde_json::Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["name"].as_str() == Some("stdlib"))
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn go_source_scan_emits_stdlib_component_with_purl_and_cpe() {
    let path = fixture("go/simple-module");
    let sbom = run_scan(&path);
    let stdlib = find_stdlib(&sbom).expect("stdlib component must be emitted on a Go scan");

    let purl = stdlib["purl"].as_str().unwrap_or("");
    assert!(
        purl.starts_with("pkg:golang/stdlib@v"),
        "stdlib PURL must follow `pkg:golang/stdlib@v<version>`; got {purl:?}"
    );

    let cpe = stdlib["cpe"].as_str().unwrap_or("");
    assert!(
        cpe.starts_with("cpe:2.3:a:golang:go:"),
        "stdlib CPE must use NVD's `golang:go` vendor/product slug; got {cpe:?}"
    );
    // The CPE version segment must match the PURL version with the
    // `v`-prefix stripped (NVD's bare-version convention).
    let purl_version = purl.trim_start_matches("pkg:golang/stdlib@v");
    let expected_cpe_prefix = format!("cpe:2.3:a:golang:go:{purl_version}:");
    assert!(
        cpe.starts_with(&expected_cpe_prefix),
        "stdlib CPE version segment {cpe:?} must match the PURL bare version {purl_version:?}"
    );

    assert_eq!(
        stdlib["type"].as_str(),
        Some("library"),
        "stdlib CDX type must be `library`"
    );

    // Build-inclusion misclassification regression: stdlib must NOT
    // carry `waybill:build-inclusion = "not-needed"` (false positive
    // from `go mod why stdlib` returning "package not in import
    // graph"). See `apply_go_mod_why_verdicts` stdlib skip.
    let props = stdlib["properties"].as_array().cloned().unwrap_or_default();
    for p in &props {
        if p["name"].as_str() == Some("waybill:build-inclusion") {
            assert_ne!(
                p["value"].as_str(),
                Some("not-needed"),
                "stdlib MUST NOT be classified `not-needed` (#364 regression)"
            );
        }
    }
}
