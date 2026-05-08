//! pip-poetry transitive-parity regression test — milestone 083.
//!
//! Fixture: python-poetry/poetry @ 1.8.4 (commit `6a071c1`).
//! Manifest + lockfile only per spec FR-002 + Q1. Self-hosting case
//! per research §2 (poetry's own pyproject.toml + poetry.lock).

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "pip_poetry";

const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 62;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // build → packaging.
    ("pkg:pypi/build", "pkg:pypi/packaging"),
    // build → tomli.
    ("pkg:pypi/build", "pkg:pypi/tomli"),
    // build → pyproject-hooks.
    ("pkg:pypi/build", "pkg:pypi/pyproject-hooks"),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("pyproject.toml").exists(), "missing pyproject.toml at {}", f.display());
    assert!(f.join("poetry.lock").exists(), "missing poetry.lock at {}", f.display());
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
        eprintln!("transitive_parity_pip_poetry::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== pip-poetry audit (python-poetry/poetry @ 1.8.4) ===");
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
