//! Milestone 665 US3 T026 — cross-format annotation parity for the
//! `waybill:binary-scan-suppressed` C153 catalog row.
//!
//! **What this test covers**
//!
//! - SC-006 (spec.md): scanning the same tree twice — once with
//!   `--no-binary-scan=go`, once without — produces the same
//!   suppression-annotation shape across all three formats (CDX 1.6,
//!   SPDX 2.3, SPDX 3.0.1). Value must be `"go"` in the flagged run;
//!   absent from the default run.
//! - Contract C2 (contracts/cli-flag.md): document-scope annotation
//!   emitted iff `Some(_)`, elided iff `None` — verified for every
//!   emitter.
//! - m071 parity catalog C153: value equality across formats
//!   (extractor semantics from T025's `cdx_anno!`/`spdx23_anno!`/
//!   `spdx3_anno!` triple, already validated by `holistic_parity`).
//!
//! Fixture reuse — same rationale as T012 (SC-005): the existing m003
//! `go/binaries/` fixture is functionally identical to the one T016
//! proposed. No sibling-repo push required.

use std::path::PathBuf;
use std::process::Command;

mod common;

#[derive(Debug)]
struct EmittedSboms {
    cdx: serde_json::Value,
    spdx23: serde_json::Value,
    spdx3: serde_json::Value,
}

fn scan_all_formats(path: &std::path::Path, extra: &[&str]) -> EmittedSboms {
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let cdx_out = tmp.path().join("out.cdx.json");
    let spdx23_out = tmp.path().join("out.spdx23.json");
    let spdx3_out = tmp.path().join("out.spdx3.json");

    let mut cmd = Command::new(bin);
    // Mirror T012 / scan_go.rs conventions so the delta between runs
    // is exactly the added `--no-binary-scan=go` flag.
    cmd.env("WAYBILL_NO_GO_MOD_WHY", "1");
    // waybill's multi-format emission requires `--output <fmt>=<path>`
    // when more than one `--format` is passed. Route each format's
    // output through its own target so the three files coexist.
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--format")
        .arg("spdx-3-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", cdx_out.display()))
        .arg("--output")
        .arg(format!("spdx-2.3-json={}", spdx23_out.display()))
        .arg("--output")
        .arg(format!("spdx-3-json={}", spdx3_out.display()));
    for a in extra {
        cmd.arg(a);
    }
    let output = cmd.output().expect("waybill should run");
    assert!(
        output.status.success(),
        "scan failed with extra args {extra:?}: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    EmittedSboms {
        cdx: serde_json::from_str(
            &std::fs::read_to_string(&cdx_out).expect("read cdx"),
        )
        .expect("valid CDX JSON"),
        spdx23: serde_json::from_str(
            &std::fs::read_to_string(&spdx23_out).expect("read spdx23"),
        )
        .expect("valid SPDX 2.3 JSON"),
        spdx3: serde_json::from_str(
            &std::fs::read_to_string(&spdx3_out).expect("read SPDX 3 JSON"),
        )
        .expect("valid SPDX 3 JSON"),
    }
}

/// Extract the CDX `metadata.properties[].value` for a given property
/// name. Returns `None` when the property is absent (byte-identity
/// default path). Returns `Some(String)` when present.
fn cdx_property_value(sbom: &serde_json::Value, name: &str) -> Option<String> {
    sbom["metadata"]["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"].as_str() == Some(name))
        .and_then(|p| p["value"].as_str().map(|s| s.to_string()))
}

/// Extract the m071 envelope's `value` field from the first SPDX 2.3
/// document-scope annotation whose parsed comment names the given
/// `field`. Returns `None` when no annotation matches.
fn spdx23_envelope_value(
    sbom: &serde_json::Value,
    field: &str,
) -> Option<String> {
    sbom["annotations"]
        .as_array()?
        .iter()
        .find_map(|anno| {
            let comment = anno["comment"].as_str()?;
            let env: serde_json::Value = serde_json::from_str(comment).ok()?;
            if env["field"].as_str() == Some(field) {
                env["value"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
}

/// Extract the m071 envelope's `value` field from the first SPDX 3
/// `@graph` Annotation whose parsed statement names the given `field`.
/// Returns `None` when no annotation matches.
fn spdx3_envelope_value(
    sbom: &serde_json::Value,
    field: &str,
) -> Option<String> {
    sbom["@graph"]
        .as_array()?
        .iter()
        .find_map(|node| {
            if node["type"].as_str() != Some("Annotation") {
                return None;
            }
            let statement = node["statement"].as_str()?;
            let env: serde_json::Value = serde_json::from_str(statement).ok()?;
            if env["field"].as_str() == Some(field) {
                env["value"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
}

/// SC-006 anchor. The delta between the two runs is exactly the
/// `--no-binary-scan=go` flag; the parity of the resulting annotation
/// across CDX / SPDX 2.3 / SPDX 3 is the whole test.
#[test]
fn binary_scan_suppressed_annotation_parity_across_formats() {
    let path: PathBuf = common::fixture_path("go/binaries");

    // ---- Baseline: flag unset ----
    let baseline = scan_all_formats(&path, &[]);
    let baseline_cdx = cdx_property_value(&baseline.cdx, "waybill:binary-scan-suppressed");
    let baseline_spdx23 =
        spdx23_envelope_value(&baseline.spdx23, "waybill:binary-scan-suppressed");
    let baseline_spdx3 = spdx3_envelope_value(&baseline.spdx3, "waybill:binary-scan-suppressed");
    assert_eq!(
        baseline_cdx, None,
        "FR-003 byte-identity: annotation must be absent from CDX \
         when the flag is unset, got: {:?}",
        baseline_cdx,
    );
    assert_eq!(
        baseline_spdx23, None,
        "FR-003 byte-identity: annotation must be absent from SPDX 2.3 \
         when the flag is unset, got: {:?}",
        baseline_spdx23,
    );
    assert_eq!(
        baseline_spdx3, None,
        "FR-003 byte-identity: annotation must be absent from SPDX 3 \
         when the flag is unset, got: {:?}",
        baseline_spdx3,
    );

    // ---- Flagged: --no-binary-scan=go ----
    let suppressed = scan_all_formats(&path, &["--no-binary-scan=go"]);
    let sup_cdx = cdx_property_value(&suppressed.cdx, "waybill:binary-scan-suppressed");
    let sup_spdx23 =
        spdx23_envelope_value(&suppressed.spdx23, "waybill:binary-scan-suppressed");
    let sup_spdx3 = spdx3_envelope_value(&suppressed.spdx3, "waybill:binary-scan-suppressed");

    assert_eq!(
        sup_cdx.as_deref(),
        Some("go"),
        "SC-006: CDX must carry the suppression annotation with value \
         \"go\" when the flag is set, got: {:?}",
        sup_cdx,
    );
    assert_eq!(
        sup_spdx23.as_deref(),
        Some("go"),
        "SC-006: SPDX 2.3 must carry the suppression annotation with value \
         \"go\" when the flag is set, got: {:?}",
        sup_spdx23,
    );
    assert_eq!(
        sup_spdx3.as_deref(),
        Some("go"),
        "SC-006: SPDX 3 must carry the suppression annotation with value \
         \"go\" when the flag is set, got: {:?}",
        sup_spdx3,
    );

    // Cross-format equality — the m071 SymmetricEqual guarantee.
    // Redundant with the three per-format asserts above (all equal "go")
    // but codifies the parity intent explicitly for future readers.
    assert_eq!(
        sup_cdx, sup_spdx23,
        "m071 C153 SymmetricEqual violation: CDX vs SPDX 2.3 values \
         disagree",
    );
    assert_eq!(
        sup_spdx23, sup_spdx3,
        "m071 C153 SymmetricEqual violation: SPDX 2.3 vs SPDX 3 values \
         disagree",
    );
}
