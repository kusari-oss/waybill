//! npm transitive-parity regression test — milestone 083 (issue #111).
//!
//! Fixture: expressjs/express @ 4.21.0 (commit `7e562c6`). Manifest +
//! lockfile only per spec FR-002 + Q1. The lockfile was generated via
//! `npm install --package-lock-only` since express (a library) doesn't
//! commit its own lockfile.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "npm";

// Baseline was 150 at alpha.24.
//
// Shift 150 → 147 (issue #262 / PR #263, alpha.38): 3 edges from
// dev-scope parents that previously resolved via bare-name
// last-write-wins to hoisted runtime targets now correctly resolve
// to their nested dev-scope targets and emit as `DEV_DEPENDENCY_OF`
// (not counted by `extract_edges_spdx_2_3`).
//
// Shift 147 → 155 (npm walk-up resolution follow-up, alpha.40):
// the lockfile parser + main-module builder now do FULL walk-up
// node_modules resolution (mirroring npm's resolver algorithm).
// Bare-name fallback effectively never fires, so every dep with
// an actual install in node_modules emits a version-pinned PURL
// reference. Recovers 8 edges that were previously routed via
// last-write-wins to wrong-version targets (dev-scope nested ones,
// which then routed as `DEV_DEPENDENCY_OF` rather than
// `DEPENDS_ON`). Net effect: more edges land in `DEPENDS_ON`
// (counted) because their now-correctly-pinned targets are Runtime
// instead of the wrong Dev version. Confirms reachability invariant
// on the molcajete corpus (66 orphans → 0 post-fix).
const EXPECTED_WAYBILL_EDGE_COUNT: usize = 155;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Confirmed in waybill output — accepts pulls in mime-types.
    ("pkg:npm/accepts", "pkg:npm/mime-types"),
    // accepts also pulls in negotiator.
    ("pkg:npm/accepts", "pkg:npm/negotiator"),
    // body-parser pulls in bytes.
    ("pkg:npm/body-parser", "pkg:npm/bytes"),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("package.json").exists(), "missing package.json at {}", f.display());
    assert!(f.join("package-lock.json").exists(), "missing package-lock.json at {}", f.display());
}

#[test]
fn transitive_edges_match_baseline() {
    let mikebom_edges = run_mikebom(&fixture());
    assert_eq!(
        mikebom_edges.len(),
        EXPECTED_WAYBILL_EDGE_COUNT,
        "waybill edge count drifted from the alpha.24 baseline."
    );
    let edge_set: std::collections::HashSet<(String, String)> = mikebom_edges
        .iter()
        .map(|e| (strip_version(&e.from).to_string(), strip_version(&e.to).to_string()))
        .collect();
    for (from_prefix, to_prefix) in EXPECTED_REPRESENTATIVE_EDGES {
        assert!(
            edge_set.contains(&(from_prefix.to_string(), to_prefix.to_string())),
            "expected representative edge missing: {from_prefix} → {to_prefix}"
        );
    }
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_npm::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let waybill = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&waybill, &trivy, &syft);
    eprintln!("\n=== npm audit (expressjs/express @ 4.21.0) ===");
    eprintln!(
        "edge counts: waybill={} trivy={} syft={}",
        waybill.len(),
        trivy.len(),
        syft.len()
    );
    eprintln!("diff:\n{}", format_edge_diff(&diff));
}

fn strip_version(purl: &str) -> &str {
    match purl.rfind('@') {
        Some(i) => &purl[..i],
        None => purl,
    }
}
