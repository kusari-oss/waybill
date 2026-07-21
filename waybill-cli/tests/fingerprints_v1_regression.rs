//! Milestone 110 US3 — v1 backward-compat regression gate.
//!
//! Verifies the spec's SC-002 + FR-005 contract: when the operator runs
//! `waybill sbom scan --fingerprints-corpus` against a binary that triggers
//! a milestone-108-style symbol-fingerprint match, the emitted SBOM contains
//! the same component list as before milestone 110 PLUS a single new
//! `waybill:fingerprint-confidence: "0.70"` annotation alongside the existing
//! `waybill:fingerprint-corpus-sha` annotation per the 2026-06-03
//! /speckit-clarify Q3 mapping (v1 records map to the design doc §7
//! "threshold-met exported symbols" baseline 0.70).
//!
//! Strategy:
//!   - Reuse the milestone-109 `binary_source_binding_cmake` test fixture
//!     (zlib-exporting binary; same opt-in scan path).
//!   - Run waybill + assert the new annotation is present.
//!   - The OSS-regression CI lane (.github/workflows/ci.yml — T022) runs
//!     this test with no extra WAYBILL_FINGERPRINTS_SOURCES env, ensuring
//!     the milestone-108 default-source path is the one being exercised.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Locate the cmake-demo project root (contains both source tree AND
/// the built `build/crc-demo` binary that exports zlib's full API).
/// `waybill sbom scan --path` requires a directory, so we point at the
/// project root rather than the binary directly.
fn find_cmake_demo_root() -> Option<PathBuf> {
    let candidates = ["../waybill-cmake-demo", "../../waybill-cmake-demo"];
    for c in candidates {
        let p = PathBuf::from(c);
        // Need both the project root AND a built binary present.
        if p.is_dir() && p.join("build/crc-demo").is_file() {
            return p.canonicalize().ok();
        }
    }
    None
}

/// Scan with `--fingerprints-corpus` (opt-in) — this is the path that
/// stamps the new `waybill:fingerprint-confidence` annotation alongside the existing
/// `waybill:fingerprint-corpus-sha`.
fn scan_with_corpus(project_root: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let out_file = out.path().join("sbom.cdx.json");
    let result = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .arg("--fingerprints-corpus")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_file).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// US3 acceptance scenario 3: every fingerprint-derived component emitted
/// when opted in MUST carry the new `waybill:fingerprint-confidence` annotation with
/// the v1-baseline value `"0.70"` (numeric, per the 2026-06-03 design
/// revision favouring numeric over bucket-name).
#[test]
fn opt_in_emits_numeric_confidence_annotation_on_fingerprint_matches() {
    let Some(project_root) = find_cmake_demo_root() else {
        println!(
            "skipped: no zlib-exporting binary available \
             (build waybill-cmake-demo first: \
             `cd ../waybill-cmake-demo && cmake -S . -B build -G Ninja && ninja -C build`)"
        );
        return;
    };

    let sbom = scan_with_corpus(&project_root);
    let components = sbom["components"].as_array().unwrap();

    // Find every component carrying a `waybill:fingerprint-corpus-sha`
    // annotation — those are the fingerprint-derived ones.
    let fingerprint_derived: Vec<&Value> = components
        .iter()
        .filter(|c| {
            c.get("properties")
                .and_then(|p| p.as_array())
                .map(|props| {
                    props
                        .iter()
                        .any(|p| p["name"].as_str() == Some("waybill:fingerprint-corpus-sha"))
                })
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !fingerprint_derived.is_empty(),
        "expected at least one fingerprint-derived component (a component carrying \
         waybill:fingerprint-corpus-sha); got 0. SBOM components: {components:#?}"
    );

    // Every fingerprint-derived component MUST carry the new
    // `waybill:fingerprint-confidence: "0.70"` annotation per FR-005 + FR-017.
    for component in &fingerprint_derived {
        let props = component["properties"].as_array().unwrap();
        let confidence_prop = props
            .iter()
            .find(|p| p["name"].as_str() == Some("waybill:fingerprint-confidence"));
        assert!(
            confidence_prop.is_some(),
            "fingerprint-derived component {:?} is missing waybill:fingerprint-confidence annotation per \
             milestone-110 FR-017; got properties: {:#?}",
            component["name"],
            props
        );
        let confidence_value = confidence_prop.unwrap()["value"].as_str().unwrap();
        assert_eq!(
            confidence_value, "0.70",
            "v1-record-derived component {:?} MUST carry confidence \"0.70\" (the design-doc §7 \
             threshold-met-exported-symbols baseline); got {:?}",
            component["name"], confidence_value
        );
    }
}

/// US3 acceptance scenario 2 negative case: when NOT opted in to the
/// corpus, the new annotation MUST NOT appear (matches existing gating
/// pattern for `waybill:fingerprint-corpus-sha`). Preserves the 33
/// pre-milestone-110 byte-identity goldens by avoiding any annotation
/// emission on the non-opt-in path.
#[test]
fn no_opt_in_does_not_emit_confidence_annotation() {
    let Some(project_root) = find_cmake_demo_root() else {
        println!("skipped: no zlib-exporting binary available");
        return;
    };

    let out = tempfile::tempdir().unwrap();
    let out_file = out.path().join("sbom.cdx.json");
    // NOTE: no `--fingerprints-corpus` flag, no WAYBILL_FINGERPRINTS_CORPUS env.
    let result = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(&project_root)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .env_remove("WAYBILL_FINGERPRINTS_CORPUS")
        .output()
        .unwrap();
    assert!(result.status.success());

    let bytes = std::fs::read(&out_file).unwrap();
    let sbom: Value = serde_json::from_slice(&bytes).unwrap();
    let components = sbom["components"].as_array().unwrap();

    // No component should carry `waybill:fingerprint-confidence` when non-opt-in.
    for component in components {
        if let Some(props) = component.get("properties").and_then(|p| p.as_array()) {
            for prop in props {
                assert_ne!(
                    prop["name"].as_str(),
                    Some("waybill:fingerprint-confidence"),
                    "non-opt-in scan must NOT emit waybill:fingerprint-confidence; found one on component \
                     {:?}",
                    component["name"]
                );
            }
        }
    }
}
