//! Milestone 156 SC-011 integration — third_party/ opt-in flag
//! off-by-default + on-when-set.
//!
//! Fixture: `third_party/somedep/cmake/deps.cmake` (depth-3 within
//! third_party/) contains `find_package(VendoredDepDep)`. Milestone-156
//! FR-019 default: NOT walked. With `--cmake-third-party-recursive`:
//! walked + emitted.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cmake-walker-depth/third-party-opt-in")
}

fn run_scan(project_root: &std::path::Path, recursive: bool) -> Value {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let mut cmd = Command::new(binary_path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()));
    if recursive {
        cmd.arg("--cmake-third-party-recursive");
    }
    let result = cmd.output().unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr),
    );
    let bytes = std::fs::read(&out_path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn count_vendoreddepdep(doc: &Value) -> usize {
    doc.get("components")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .iter()
        .filter(|c| {
            c.get("purl")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.starts_with("pkg:generic/vendoreddepdep"))
        })
        .count()
}

#[test]
fn third_party_opt_in_off_by_default() {
    let doc = run_scan(&fixture_root(), false);
    assert_eq!(
        count_vendoreddepdep(&doc),
        0,
        "milestone 156 FR-019 default: third_party/somedep/cmake/deps.cmake at depth-3 MUST NOT be walked without --cmake-third-party-recursive"
    );
}

#[test]
fn third_party_opt_in_flag_enables_recursion() {
    let doc = run_scan(&fixture_root(), true);
    assert_eq!(
        count_vendoreddepdep(&doc),
        1,
        "milestone 156 FR-019 opt-in: --cmake-third-party-recursive MUST enable depth-3 discovery under third_party/"
    );
}
