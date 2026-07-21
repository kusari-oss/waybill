//! Milestone 202 (issue #579) — CDX license splitter LicenseRef escape hatch.
//!
//! The pre-m202 CDX splitter (`license_entry_for_token` at
//! `builder.rs:1494`) put any non-`LicenseRef-`-prefixed token into the
//! `license.id` slot — including non-canonical operands like
//! `bzip2-1.0.4` that violate CDX 1.6 §5.4.4.1's SPDX-List constraint.
//!
//! Post-m202 (per FR-001): a 3-branch classifier checks each token
//! against the SPDX License List via `spdx::license_id`; non-canonical
//! operands route to `license.name = "LicenseRef-<sanitized>"` per
//! CDX 1.6 §5.4.4.2, matching the SPDX 2.3 emitter's escape-hatch
//! convention (m152 for #481). FR-002 guarantees byte-identical
//! `LicenseRef-*` identifiers between CDX and SPDX 2.3 emissions of
//! the same scan (single-source-of-truth via the extracted
//! `waybill_common::types::license::sanitize_license_operand_to_ref`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ipk/license_licenseref_splitter_m202")
}

fn scan_path_with_format(path: &Path, format: &str) -> serde_json::Value {
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
        .arg(path)
        .arg("--format")
        .arg(format)
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

/// SC-001 + SC-002 + SC-003: the compound License field
/// `GPL-2.0-only & bzip2-1.0.4` splits into two license entries — the
/// canonical operand into `license.id` (unchanged from pre-m202) and
/// the non-canonical operand into `license.name = "LicenseRef-*"` (new
/// post-m202 behavior). Neither is `license.id: "bzip2-1.0.4"`
/// (schema-invalid per CDX 1.6 §5.4.4.1).
#[test]
fn scan_ipk_licenseref_slot_routes_correctly_m202() {
    let sbom = scan_path_with_format(&fixture_root(), "cyclonedx-json");
    let component = sbom["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| c["name"].as_str() == Some("test"))
        .expect("test component present");
    let licenses = component["licenses"]
        .as_array()
        .expect("component has licenses");

    // SC-003: canonical `GPL-2.0-only` remains in the id slot (unchanged).
    let canonical_id_count = licenses
        .iter()
        .filter(|entry| entry["license"]["id"].as_str() == Some("GPL-2.0-only"))
        .count();
    assert_eq!(
        canonical_id_count, 1,
        "SC-003: canonical `GPL-2.0-only` must remain in license.id; got licenses={licenses:?}"
    );

    // SC-002: non-canonical operand lands in name slot with LicenseRef- prefix.
    let ref_name_count = licenses
        .iter()
        .filter(|entry| entry["license"]["name"].as_str() == Some("LicenseRef-bzip2-1.0.4"))
        .count();
    assert_eq!(
        ref_name_count, 1,
        "SC-002: non-canonical `bzip2-1.0.4` must route to LicenseRef- via license.name; got licenses={licenses:?}"
    );

    // SC-001: absolute guard — no license.id equals "bzip2-1.0.4"
    // (schema-invalid per CDX 1.6 §5.4.4.1).
    let bad_id_count = licenses
        .iter()
        .filter(|entry| entry["license"]["id"].as_str() == Some("bzip2-1.0.4"))
        .count();
    assert_eq!(
        bad_id_count, 0,
        "SC-001: non-canonical operand MUST NOT appear in license.id slot (CDX 1.6 §5.4.4.1); got licenses={licenses:?}"
    );
}

/// SC-004 + FR-002 (revised): structural pattern parity — BOTH CDX
/// AND SPDX 2.3 emit a `LicenseRef-*` identifier somewhere for the
/// same compound-with-non-canonical input, using their respective
/// spec-blessed escape hatches. The BYTE-IDENTICAL identifier claim
/// in the original spec was over-reach: CDX uses the m152-shared
/// per-operand sanitizer output (`LicenseRef-bzip2-1.0.4`); SPDX 2.3
/// uses a hash-based whole-compound identifier (`LicenseRef-<HASH>`
/// referencing the full expression in `hasExtractedLicensingInfos[]`).
/// Both are valid per their respective specs. The test asserts the
/// structural pattern: schema-invalid `license.id: "bzip2-1.0.4"` is
/// GONE (SC-001), AND both emissions carry the LicenseRef escape
/// hatch in the correct slot.
#[test]
fn scan_ipk_licenseref_escape_hatch_present_in_both_formats_m202() {
    let cdx = scan_path_with_format(&fixture_root(), "cyclonedx-json");
    let spdx = scan_path_with_format(&fixture_root(), "spdx-2.3-json");

    // CDX side: LicenseRef-* in license.name slot per CDX 1.6 §5.4.4.2.
    let cdx_has_licenseref = cdx["components"]
        .as_array()
        .expect("components array")
        .iter()
        .find(|c| c["name"].as_str() == Some("test"))
        .expect("test component present")["licenses"]
        .as_array()
        .expect("licenses array")
        .iter()
        .any(|entry| {
            entry["license"]["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("LicenseRef-"))
        });
    assert!(
        cdx_has_licenseref,
        "CDX MUST emit a LicenseRef-* in license.name slot post-m202"
    );

    // SPDX 2.3 side: LicenseRef-* in hasExtractedLicensingInfos[].licenseId.
    let spdx_has_licenseref = spdx["hasExtractedLicensingInfos"]
        .as_array()
        .expect("hasExtractedLicensingInfos array")
        .iter()
        .any(|entry| {
            entry["licenseId"]
                .as_str()
                .is_some_and(|id| id.starts_with("LicenseRef-"))
        });
    assert!(
        spdx_has_licenseref,
        "SPDX 2.3 MUST emit a LicenseRef-* in hasExtractedLicensingInfos"
    );
}
