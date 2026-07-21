//! Issue #363 integration test — `--conclude-licenses` operator-
//! assertion flag.
//!
//! Default (flag absent): components with declared licenses leave
//! `licenseConcluded` as `NOASSERTION` (pre-feature byte-identity
//! preserved). When the flag is passed, the operator asserts that the
//! declared licenses have been reviewed and may be promoted to the
//! concluded slot; the per-component
//! `waybill:license-concluded-source = "operator-asserted"`
//! annotation records the assertion provenance.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join(sub)
}

fn run_scan(path: &Path, extra_args: &[&str]) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .arg("--file-inventory=off");
    for a in extra_args {
        cmd.arg(a);
    }
    let status = cmd.status().expect("waybill should run");
    assert!(status.success(), "scan failed: {extra_args:?}");
    let raw = std::fs::read(&out_path).expect("read sbom");
    serde_json::from_slice(&raw).expect("valid JSON")
}

fn count_concluded_licenses(sbom: &serde_json::Value) -> usize {
    sbom["components"]
        .as_array()
        .map(|comps| {
            comps
                .iter()
                .filter(|c| {
                    let Some(licenses) = c["licenses"].as_array() else {
                        return false;
                    };
                    licenses.iter().any(|l| {
                        l["license"]["acknowledgement"].as_str() == Some("concluded")
                    })
                })
                .count()
        })
        .unwrap_or(0)
}

fn count_operator_asserted_annotations(sbom: &serde_json::Value) -> usize {
    sbom["components"]
        .as_array()
        .map(|comps| {
            comps
                .iter()
                .filter(|c| {
                    let Some(props) = c["properties"].as_array() else {
                        return false;
                    };
                    props.iter().any(|p| {
                        p["name"].as_str() == Some("waybill:license-concluded-source")
                            && p["value"].as_str() == Some("operator-asserted")
                    })
                })
                .count()
        })
        .unwrap_or(0)
}

fn count_components_with_declared(sbom: &serde_json::Value) -> usize {
    sbom["components"]
        .as_array()
        .map(|comps| {
            comps
                .iter()
                .filter(|c| {
                    let Some(licenses) = c["licenses"].as_array() else {
                        return false;
                    };
                    licenses.iter().any(|l| {
                        l["license"]["acknowledgement"].as_str() == Some("declared")
                            || l["license"]["acknowledgement"].is_null()
                    })
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn default_mode_leaves_concluded_empty_on_pip_fixture() {
    // pip fixture has 7 components, all with declared licenses
    // (PEP 621 metadata in dist-info/METADATA).
    let path = fixture("python/simple-venv");
    let sbom = run_scan(&path, &[]);
    let declared = count_components_with_declared(&sbom);
    let concluded = count_concluded_licenses(&sbom);
    let asserted = count_operator_asserted_annotations(&sbom);
    assert!(
        declared >= 7,
        "expected pip fixture to populate ≥7 declared licenses; got {declared}"
    );
    assert_eq!(
        concluded, 0,
        "default mode must NOT populate licenseConcluded (issue #363 byte-identity contract); got {concluded}"
    );
    assert_eq!(
        asserted, 0,
        "default mode must NOT emit the operator-asserted annotation; got {asserted}"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn conclude_flag_promotes_declared_to_concluded_with_provenance_on_pip() {
    let path = fixture("python/simple-venv");
    let sbom = run_scan(&path, &["--conclude-licenses"]);
    let declared = count_components_with_declared(&sbom);
    let concluded = count_concluded_licenses(&sbom);
    let asserted = count_operator_asserted_annotations(&sbom);
    assert!(
        declared >= 7,
        "expected pip fixture to populate ≥7 declared licenses; got {declared}"
    );
    assert_eq!(
        concluded, declared,
        "with --conclude-licenses, concluded count must equal declared count; got concluded={concluded} declared={declared}"
    );
    assert_eq!(
        asserted, concluded,
        "with --conclude-licenses, every promoted component must carry waybill:license-concluded-source=operator-asserted; got concluded={concluded} annotated={asserted}"
    );
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn conclude_flag_no_op_when_declared_empty() {
    // Cargo fixture has no declared licenses on the lockfile-resolved
    // crates — the flag becomes a no-op (no annotation, no concluded).
    let path = fixture("cargo/lockfile-v3");
    let sbom = run_scan(&path, &["--conclude-licenses"]);
    let concluded = count_concluded_licenses(&sbom);
    let asserted = count_operator_asserted_annotations(&sbom);
    assert_eq!(
        concluded, 0,
        "no declared licenses → no concluded promotion; got {concluded}"
    );
    assert_eq!(
        asserted, 0,
        "no concluded promotion → no operator-asserted annotation; got {asserted}"
    );
}
