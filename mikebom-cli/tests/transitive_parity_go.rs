//! Go transitive-parity regression test — milestone 083 (issue #111).
//!
//! Fixture: kubernetes-sigs/cri-tools @ v1.32.0 (commit `b5cf674`).
//! Manifest + lockfile only per spec FR-002 + Q1. go.mod + go.sum
//! committed at the tagged release.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "go";

/// **Cache-empty baseline** — pinned at the CI-reproducible state where
/// `$GOMODCACHE` is empty. Mikebom's go reader has a 4-step ladder per
/// milestone-055 research §3 (`go mod graph` / `$GOMODCACHE` / proxy /
/// no-edges fallback); with `--offline` and an empty cache, only the
/// no-edges-fallback step (synthesized direct edges from go.mod
/// `require` block) fires. Real-world output on a developer's box with
/// a populated module cache will be 260+ edges; we pin the 31-edge
/// cache-empty count because that's what CI sees and what `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1`
/// must reproduce.
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 31;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Direct deps from go.mod `require` block — synthesized by the
    // ladder's no-edges-fallback step into edges from the main-module
    // PURL to each direct dep.
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/distribution/reference",
    ),
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/google/uuid",
    ),
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/onsi/ginkgo/v2",
    ),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("go.mod").exists(), "missing go.mod at {}", f.display());
    assert!(f.join("go.sum").exists(), "missing go.sum at {}", f.display());
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
        eprintln!("transitive_parity_go::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== go audit (kubernetes-sigs/cri-tools @ v1.32.0) ===");
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
