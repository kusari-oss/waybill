//! Gem transitive-parity regression test — milestone 083.
//!
//! Fixture: fastlane/fastlane @ 2.224.0. Manifest + lockfile only
//! per spec FR-002 + Q1. fastlane commits its Gemfile.lock at HEAD,
//! sidestepping the bundle-lock-needs-Ruby-3+ issue we hit with
//! rubocop on the macOS dev box.

mod transitive_parity_common;

use std::path::PathBuf;
use transitive_parity_common::*;

const FIXTURE_SUBPATH: &str = "gem";

// Issue #236 bumped the baseline from 196 → 217: fastlane's fixture
// has only Gemfile + Gemfile.lock (no top-level `.gemspec`), so per
// milestone 069's FR-002 it doesn't get a main-module annotation,
// and the SPDX 2.3 emitter falls through to `synthesize_root`. The
// issue-#236 fix adds `synth-root → graph-root` DEPENDS_ON edges
// from the synthesized root to every component nothing else depends
// on (mirrors CDX's primary-dependency fallback). 21 graph-root
// gems × 1 edge each = 21 new edges; 196 + 21 = 217.
//
// Milestone 162 (#496) bumped baseline from 217 → 218: a Gemfile.lock
// spec declares `bundler (>= 1.12.0, < 3.0.0)` as a dep, but
// `bundler` is a Ruby toolchain-provided built-in gem NOT in the
// GEM/specs section. Pre-162 mikebom silently dropped that edge;
// post-162 mikebom emits a synthetic `pkg:gem/bundler` (versionless)
// component + preserves the edge from source to synthetic. 1 new
// edge; 217 + 1 = 218.
const EXPECTED_MIKEBOM_EDGE_COUNT: usize = 218;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Confirmed in mikebom output — fastlane's main module pulls in CFPropertyList.
    ("pkg:gem/fastlane", "pkg:gem/CFPropertyList"),
    // fastlane → addressable.
    ("pkg:gem/fastlane", "pkg:gem/addressable"),
    // fastlane → aws-sdk-s3.
    ("pkg:gem/fastlane", "pkg:gem/aws-sdk-s3"),
];

fn fixture() -> PathBuf {
    fixture_path(FIXTURE_SUBPATH)
}

#[test]
fn fixture_present() {
    let f = fixture();
    assert!(f.join("Gemfile").exists(), "missing Gemfile at {}", f.display());
    assert!(f.join("Gemfile.lock").exists(), "missing Gemfile.lock at {}", f.display());
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
        eprintln!("transitive_parity_gem::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let mikebom = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&mikebom, &trivy, &syft);
    eprintln!("\n=== gem audit (fastlane/fastlane @ 2.224.0) ===");
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
