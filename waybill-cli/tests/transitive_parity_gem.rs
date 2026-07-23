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
// GEM/specs section. Pre-162 waybill silently dropped that edge;
// post-162 waybill emits a synthetic `pkg:gem/bundler` (versionless)
// component + preserves the edge from source to synthetic. 1 new
// edge; 217 + 1 = 218.
//
// Milestone 216 REDUCED baseline from 218 → 197 (net −21): the m216
// Gemfile-only main-module emission fires on this fixture (no
// top-level `.gemspec`), producing a real `pkg:generic/gem@0.0.0-unknown`
// main-module and suppressing the issue-#236 `synthesize_root` fallback
// (that fallback was gated on "no main-module exists" — the m216
// condition change eliminates it). Loss: the 21 synth-root → graph-root
// edges are no longer emitted. The m216 main-module DOES declare
// direct-deps in its `depends[]` field (from Gemfile.lock's
// DEPENDENCIES block), but scan_fs::mod.rs's dep-name resolver is
// same-ecosystem-scoped (keys on `(ecosystem, name)`), so a
// pkg:generic/ main-module cannot cross-link to pkg:gem/ deps.
// Follow-up work: waybill#633 (m218) bridges cross-ecosystem lookup
// for pkg:generic/ main-modules. Net delta: 218 − 21 = 197.
const EXPECTED_WAYBILL_EDGE_COUNT: usize = 197;

// Milestone 218 (waybill#633) INCREASES the flag-on baseline from
// 197 → 224 (+27): with the FR-000
// `--experimental-cross-ecosystem-edges` flag enabled, the resolver
// falls back to iterating every non-generic ecosystem in the
// resolver index when a `pkg:generic/`-source main-module lookup
// misses. fastlane's Gemfile.lock DEPENDENCIES block has 27 entries,
// all of which resolve to registered `pkg:gem/` components (no
// unresolved names on this fixture). Every recovered edge emits a
// per-edge `waybill:cross-ecosystem-inference` annotation with the
// `{from_eco:"generic", lookup_via:"gemfile-lock-dependencies",
// target_purl, to_eco:"gem"}` payload. Flag-off baseline (197)
// unchanged per SC-009 byte-identity contract.
const EXPECTED_WAYBILL_EDGE_COUNT_FLAG_ON: usize = 224;

const EXPECTED_REPRESENTATIVE_EDGES: &[(&str, &str)] = &[
    // Confirmed in waybill output — fastlane's main module pulls in CFPropertyList.
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

/// Milestone 218 (waybill#633) US1 SC-001 + SC-002: with the
/// FR-000 experimental flag enabled, the pkg:generic/ main-module
/// gains outgoing DEPENDS_ON edges to every DEPENDENCIES-declared
/// gem (27 for fastlane's Gemfile.lock). Total edge count moves
/// from 197 (flag-off baseline) → 224 (flag-on baseline).
///
/// SC-009 (byte-identity flag-off) is enforced by the
/// `transitive_edges_match_baseline` test above (unchanged from
/// pre-m218).
#[test]
fn m218_flag_on_recovers_edges_from_pkg_generic_main_module() {
    use std::process::Command;
    let bin = env!("CARGO_BIN_EXE_waybill");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("waybill.spdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let mut cmd = Command::new(bin);
    // Same fake-HOME isolation as run_mikebom.
    cmd.env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("HOME", fake_home.path())
        .env(
            "WAYBILL_FIXTURES_DIR",
            env!("WAYBILL_FIXTURES_DIR"),
        )
        // m218: enable the experimental flag via env var.
        .env("WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES", "1")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture())
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(&out)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("waybill invokes");
    assert!(
        output.status.success(),
        "waybill flag-on failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out).expect("read waybill output");
    let doc: serde_json::Value =
        serde_json::from_slice(&bytes).expect("parse SPDX 2.3");
    // Count DEPENDS_ON edges (matches the flag-off baseline path).
    let edges: Vec<_> = doc
        .get("relationships")
        .and_then(|v| v.as_array())
        .expect("relationships[]")
        .iter()
        .filter(|r| {
            r.get("relationshipType").and_then(|v| v.as_str())
                == Some("DEPENDS_ON")
        })
        .collect();
    assert_eq!(
        edges.len(),
        EXPECTED_WAYBILL_EDGE_COUNT_FLAG_ON,
        "flag-on baseline drift: expected {}, got {}",
        EXPECTED_WAYBILL_EDGE_COUNT_FLAG_ON,
        edges.len()
    );
    // SC-001: count of pkg:generic/-source DEPENDS_ON edges MUST
    // equal the DEPENDENCIES block size (27 for fastlane).
    let generic_source_spdxid: String = doc
        .get("packages")
        .and_then(|v| v.as_array())
        .expect("packages[]")
        .iter()
        .find(|p| {
            p.get("externalRefs")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        r.get("referenceLocator")
                            .and_then(|v| v.as_str())
                            .map(|s| s.starts_with("pkg:generic/"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .and_then(|p| {
            p.get("SPDXID").and_then(|v| v.as_str()).map(String::from)
        })
        .expect("pkg:generic/ main-module Package MUST exist");
    let generic_edges = edges
        .iter()
        .filter(|r| {
            r.get("spdxElementId").and_then(|v| v.as_str())
                == Some(&generic_source_spdxid)
        })
        .count();
    assert_eq!(
        generic_edges, 27,
        "expected 27 pkg:generic/-source DEPENDS_ON edges (all 27 \
         DEPENDENCIES gems resolve on fastlane), got {generic_edges}"
    );
}

#[test]
fn cross_tool_parity_check() {
    if let Some(reason) = maybe_skip(&["trivy", "syft"]) {
        eprintln!("transitive_parity_gem::cross_tool_parity_check skipped: {reason}");
        return;
    }
    let waybill = run_mikebom(&fixture());
    let trivy = run_trivy(&fixture());
    let syft = run_syft(&fixture());
    let diff = compute_edge_diff(&waybill, &trivy, &syft);
    eprintln!("\n=== gem audit (fastlane/fastlane @ 2.224.0) ===");
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
