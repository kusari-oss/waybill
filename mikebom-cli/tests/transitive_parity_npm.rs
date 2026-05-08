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

const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 150;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Confirmed in mikebom output — accepts pulls in mime-types.
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
        EXPECTED_MIKEBOM_EDGE_COUNT,
        "mikebom edge count drifted from the alpha.24 baseline."
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
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== npm audit (expressjs/express @ 4.21.0) ===");
    eprintln!(
        "edge counts: mikebom={} trivy={} syft={}",
        mikebom.len(),
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
