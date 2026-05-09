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
/// `$GOMODCACHE` is empty. Mikebom's go reader has a 5-step ladder per
/// milestones 055 + 091 (`go mod graph` / `$GOMODCACHE` / proxy /
/// **go.sum flat fallback** / no-edges-fallback). With `--offline` and
/// an empty cache, step 5 (the milestone-091 go.sum-driven flat
/// fallback) claims every go.sum module steps 1–3 missed and augments
/// the main-module's `depends` list with flat root → transitive edges.
/// This recovers ~78 transitive edges that were dropped pre-091 (count
/// rose from 31 → 109 on the cri-tools fixture).
///
/// Real-world output on a developer's box with a populated module
/// cache will be 260+ edges (full per-transitive parent-child topology
/// from step 2); we pin the 109-edge offline-cache-empty count because
/// that's what CI sees and what `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1`
/// must reproduce.
///
/// Closed by milestone 091 (go.sum-fallback step 5):
/// - Pre-091: 31 edges (direct-deps only — main-module → ~24 direct
///   deps from go.mod's non-`// indirect` require lines + ~7
///   inter-transitive cache hits).
/// - Post-091: 109 edges (~24 direct deps + ~85 root → transitive
///   edges synthesized from go.sum's flat closure via step 5).
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 109;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Direct deps from go.mod `require` block — synthesized into edges
    // from the main-module PURL by `build_main_module_entry`.
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
    // Milestone 091 invariant — step-5 go.sum-fallback edge: a
    // transitive dep that was NOT a direct dep in cri-tools' go.mod
    // and was previously dropped in offline+cache-empty mode.
    // beorn7/perks is a transitive of prometheus libraries, not a
    // direct cri-tools dep — it's only reachable via go.sum.
    // Pre-091 mikebom emitted no edge to this component; post-091
    // step 5 augments main-module's depends list.
    (
        "pkg:golang/sigs.k8s.io/cri-tools",
        "pkg:golang/github.com/beorn7/perks",
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
