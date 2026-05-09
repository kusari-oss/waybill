//! Transitive-parity audit harness — milestone 083 (issue #111).
//!
//! Shared infrastructure for the per-ecosystem `transitive_parity_*`
//! regression tests. Runs 3 SBOM tools (mikebom + trivy + syft) plus
//! a source-format direct-read tiebreaker against a vendored fixture,
//! computes a set-theoretic edge diff, and emits an `AuditRow` so the
//! per-ecosystem test files stay thin.
//!
//! Per spec/083 FR-006: pinned tool versions are trivy 0.69.3 + syft
//! 1.27.0. Per research §5: the harness gracefully skips when an
//! external tool isn't on PATH, unless `MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1`
//! is set (CI's strict mode mirroring milestone-078's pattern).
//!
//! Cross-tool comparison uses SPDX 2.3 as the lingua franca: all
//! three tools can emit it, and the `relationships[]` array's
//! `DEPENDS_ON` entries (resolved to PURLs via the `packages[]`
//! lookup) are the comparison key. This sidesteps SPDX 3 vs CDX
//! shape differences across tools.

#![allow(dead_code)] // Per-ecosystem files use a subset of these helpers.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

// Reuse the same fake-home env helper that all other integration
// tests use (CDX/SPDX regression, holistic parity, etc.) for
// cross-test consistency on subprocess env setup.
#[path = "../common/normalize.rs"]
mod fake_home_normalize;
use fake_home_normalize::apply_fake_home_env;

/// A directed dependency edge between two PURL-identified components.
/// Cross-tool comparison key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

impl Edge {
    pub fn new<F: Into<String>, T: Into<String>>(from: F, to: T) -> Self {
        Self { from: from.into(), to: to.into() }
    }
}

/// Set-theoretic comparison across the 3 SBOM tools.
#[derive(Debug, Clone, Default)]
pub struct EdgeDiff {
    pub agreement: HashSet<Edge>,
    pub mikebom_only: HashSet<Edge>,
    pub trivy_only: HashSet<Edge>,
    pub syft_only: HashSet<Edge>,
    pub mikebom_trivy: HashSet<Edge>,
    pub mikebom_syft: HashSet<Edge>,
    pub trivy_syft: HashSet<Edge>,
}

impl EdgeDiff {
    pub fn is_unanimous(&self) -> bool {
        self.mikebom_only.is_empty()
            && self.trivy_only.is_empty()
            && self.syft_only.is_empty()
            && self.mikebom_trivy.is_empty()
            && self.mikebom_syft.is_empty()
            && self.trivy_syft.is_empty()
    }

    pub fn requires_tiebreaker(&self) -> bool {
        !self.is_unanimous()
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AuditClassification {
    /// All 3 SBOM tools agree on the edge set.
    MatchesExpected,
    /// <5% per-edge divergence; documented in regression test.
    MinorDifferences,
    /// Material divergence; follow-up issue filed.
    GapSurfaced,
    /// Tool support N/A (e.g., syft for some OS package cases).
    NotApplicable,
}

#[derive(Debug)]
pub struct AuditRow {
    pub ecosystem: &'static str,
    pub fixture_path: PathBuf,
    pub fixture_source_url: &'static str,
    pub fixture_commit_sha: &'static str,
    pub mikebom_count: usize,
    pub trivy_count: usize,
    pub syft_count: usize,
    pub source_truth_count: Option<usize>,
    pub diff: EdgeDiff,
    pub classification: AuditClassification,
    pub follow_up_issue: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct TransitiveParityFixture {
    pub ecosystem: &'static str,
    pub fixture_subpath: &'static str,
    pub expected_edge_count: usize,
    pub expected_representative_edges: &'static [(&'static str, &'static str)],
    pub required_tools: &'static [&'static str],
}

// ============================================================
// Skip / strict-mode logic — mirrors milestone-078
// ============================================================

/// Returns Some(reason) when the test should skip; None to proceed.
/// Strict-mode (`MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1`) flips skip to
/// hard fail by panicking with the same reason string.
pub fn maybe_skip(required_tools: &[&str]) -> Option<String> {
    let strict = std::env::var("MIKEBOM_REQUIRE_TRANSITIVE_PARITY")
        .ok()
        .as_deref()
        == Some("1");
    let missing: Vec<&str> = required_tools
        .iter()
        .copied()
        .filter(|t| !is_on_path(t))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let reason = format!(
        "missing required tools: {} (set MIKEBOM_REQUIRE_TRANSITIVE_PARITY=1 to make this fail instead of skip)",
        missing.join(", ")
    );
    if strict {
        panic!("transitive-parity strict mode: {reason}");
    }
    Some(reason)
}

/// Shell-out detection so we don't need the `which` crate as a
/// new dev-dep — `command -v` works on every POSIX shell mikebom
/// CI hits (Linux + macOS).
fn is_on_path(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// macOS unconditionally skips OS-package fixtures (dpkg/rpm/apk
/// don't exist there per research §5).
pub fn skip_on_macos_for_os_package(ecosystem: &str) -> Option<String> {
    if cfg!(target_os = "macos")
        && matches!(ecosystem, "dpkg" | "rpm" | "apk")
    {
        return Some(format!(
            "{ecosystem} fixture skipped on macOS — Linux-only per research §5"
        ));
    }
    None
}

// ============================================================
// Per-tool invocations
// ============================================================

/// Run mikebom against a fixture and return the dep edges as `Edge`
/// tuples. Uses SPDX 2.3 emission as the lingua franca with the
/// other two tools.
pub fn run_mikebom(fixture_path: &Path) -> Vec<Edge> {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("mikebom.spdx.json");
    let fake_home = tempfile::tempdir().expect("fake-home");
    let mut cmd = Command::new(bin);
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture_path)
        .arg("--format")
        .arg("spdx-2.3-json")
        .arg("--output")
        .arg(&out)
        .arg("--no-deep-hash");
    let output = cmd.output().expect("mikebom invokes");
    assert!(
        output.status.success(),
        "mikebom failed for {}: {}",
        fixture_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out).expect("read mikebom output");
    let doc: serde_json::Value =
        serde_json::from_slice(&bytes).expect("parse mikebom SPDX 2.3");
    extract_edges_spdx_2_3(&doc)
}

/// Run trivy against a fixture; returns dep edges. Empty Vec if
/// trivy isn't on PATH (caller should have already checked via
/// `maybe_skip`).
pub fn run_trivy(fixture_path: &Path) -> Vec<Edge> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("trivy.spdx.json");
    let output = Command::new("trivy")
        .arg("--quiet")
        .arg("fs")
        .arg("--format")
        .arg("spdx-json")
        .arg("--output")
        .arg(&out_path)
        .arg(fixture_path)
        .output()
        .expect("trivy invokes");
    assert!(
        output.status.success(),
        "trivy failed for {}: {}",
        fixture_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap_or_default();
    if bytes.is_empty() {
        return Vec::new();
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("parse trivy SPDX 2.3");
    extract_edges_spdx_2_3(&doc)
}

/// Run syft against a fixture; returns dep edges.
pub fn run_syft(fixture_path: &Path) -> Vec<Edge> {
    let output = Command::new("syft")
        .arg(fixture_path)
        .arg("-o")
        .arg("spdx-json")
        .arg("-q")
        .output()
        .expect("syft invokes");
    assert!(
        output.status.success(),
        "syft failed for {}: {}",
        fixture_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.stdout.is_empty() {
        return Vec::new();
    }
    let doc: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse syft SPDX 2.3");
    extract_edges_spdx_2_3(&doc)
}

// ============================================================
// SPDX 2.3 → Vec<Edge> extraction
// ============================================================

/// Walk an SPDX 2.3 document's `relationships[]` for `DEPENDS_ON`
/// entries, resolve SPDX-IDs to PURLs via the `packages[]` lookup,
/// and emit normalized `Edge` tuples. Drops edges where either
/// endpoint has no PURL — those aren't comparable across tools.
pub fn extract_edges_spdx_2_3(doc: &serde_json::Value) -> Vec<Edge> {
    let mut purl_by_spdxid: BTreeMap<String, String> = BTreeMap::new();
    if let Some(packages) = doc.get("packages").and_then(|v| v.as_array()) {
        for p in packages {
            let id = match p.get("SPDXID").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let purl = p
                .get("externalRefs")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|r| {
                        if r.get("referenceType").and_then(|v| v.as_str()) == Some("purl") {
                            r.get("referenceLocator")
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        } else {
                            None
                        }
                    })
                });
            if let Some(p) = purl {
                purl_by_spdxid.insert(id, normalize_purl(&p));
            }
        }
    }

    let mut edges: Vec<Edge> = Vec::new();
    if let Some(rels) = doc.get("relationships").and_then(|v| v.as_array()) {
        for r in rels {
            // SPDX 2.3 §11.1 has both DEPENDS_ON (forward) and
            // DEPENDENCY_OF (reverse) relationship types. Different
            // SBOM tools choose different representations for the
            // same logical edge: trivy + mikebom emit DEPENDS_ON,
            // syft emits DEPENDENCY_OF. The audit normalizes both
            // to a forward-direction `Edge` so cross-tool
            // comparison sees the same set.
            let rel_type = r.get("relationshipType").and_then(|v| v.as_str());
            let (from_field, to_field) = match rel_type {
                Some("DEPENDS_ON") => ("spdxElementId", "relatedSpdxElement"),
                Some("DEPENDENCY_OF") => ("relatedSpdxElement", "spdxElementId"),
                _ => continue,
            };
            let from_id = r.get(from_field).and_then(|v| v.as_str());
            let to_id = r.get(to_field).and_then(|v| v.as_str());
            let (Some(f), Some(t)) = (from_id, to_id) else {
                continue;
            };
            let (Some(from_purl), Some(to_purl)) =
                (purl_by_spdxid.get(f), purl_by_spdxid.get(t))
            else {
                continue;
            };
            edges.push(Edge::new(from_purl.clone(), to_purl.clone()));
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

// ============================================================
// PURL normalization (per VR-083-004)
// ============================================================

/// Normalize a PURL for cross-tool comparison: lowercase the package
/// type, strip qualifiers (the `?repository_url=...` suffix some
/// tools add and others omit). Sort + dedup is handled by the
/// caller.
pub fn normalize_purl(purl: &str) -> String {
    // Strip qualifiers (?...)
    let no_qualifiers = match purl.find('?') {
        Some(idx) => &purl[..idx],
        None => purl,
    };
    // Lowercase the package type (between `pkg:` and the first `/`).
    if let Some(after_pkg) = no_qualifiers.strip_prefix("pkg:") {
        if let Some(slash_idx) = after_pkg.find('/') {
            let (pkg_type, rest) = after_pkg.split_at(slash_idx);
            return format!("pkg:{}{}", pkg_type.to_lowercase(), rest);
        }
    }
    no_qualifiers.to_string()
}

// ============================================================
// Edge-set diff
// ============================================================

pub fn compute_edge_diff(
    mikebom: &[Edge],
    trivy: &[Edge],
    syft: &[Edge],
) -> EdgeDiff {
    let m: HashSet<Edge> = mikebom.iter().cloned().collect();
    let t: HashSet<Edge> = trivy.iter().cloned().collect();
    let s: HashSet<Edge> = syft.iter().cloned().collect();

    let agreement: HashSet<Edge> = m.intersection(&t).cloned().collect::<HashSet<_>>()
        .intersection(&s)
        .cloned()
        .collect();

    EdgeDiff {
        agreement: agreement.clone(),
        mikebom_only: m
            .difference(&t)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&s)
            .cloned()
            .collect(),
        trivy_only: t
            .difference(&m)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&s)
            .cloned()
            .collect(),
        syft_only: s
            .difference(&m)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&t)
            .cloned()
            .collect(),
        mikebom_trivy: m
            .intersection(&t)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&s)
            .cloned()
            .collect(),
        mikebom_syft: m
            .intersection(&s)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&t)
            .cloned()
            .collect(),
        trivy_syft: t
            .intersection(&s)
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&m)
            .cloned()
            .collect(),
    }
}

/// Format an edge diff for human inspection — shows up to 5 edges
/// per category so test failures stay within terminal width.
pub fn format_edge_diff(diff: &EdgeDiff) -> String {
    fn sample(set: &HashSet<Edge>, label: &str) -> String {
        if set.is_empty() {
            return format!("  {label}: (empty)\n");
        }
        let mut sorted: Vec<&Edge> = set.iter().collect();
        sorted.sort();
        let n = sorted.len();
        let head: Vec<String> = sorted
            .iter()
            .take(5)
            .map(|e| format!("    {} -> {}", e.from, e.to))
            .collect();
        let suffix = if n > 5 {
            format!("    ... and {} more\n", n - 5)
        } else {
            String::new()
        };
        format!(
            "  {label} ({n} edge{s}):\n{}\n{suffix}",
            head.join("\n"),
            s = if n == 1 { "" } else { "s" }
        )
    }
    let mut out = String::new();
    out.push_str(&sample(&diff.agreement, "agreement (all 3 tools)"));
    out.push_str(&sample(&diff.mikebom_only, "mikebom-only"));
    out.push_str(&sample(&diff.trivy_only, "trivy-only"));
    out.push_str(&sample(&diff.syft_only, "syft-only"));
    out.push_str(&sample(&diff.mikebom_trivy, "mikebom+trivy (not syft)"));
    out.push_str(&sample(&diff.mikebom_syft, "mikebom+syft (not trivy)"));
    out.push_str(&sample(&diff.trivy_syft, "trivy+syft (not mikebom)"));
    out
}

// ============================================================
// Source-format direct-read tiebreaker (per Q2 + research §4)
// ============================================================

/// Per-ecosystem dispatch for the source-format tiebreaker. Returns
/// None when no tiebreaker is implemented for the ecosystem (e.g.,
/// the harness assumes the per-ecosystem regression test will inline
/// its own when needed).
pub fn run_source_format_direct(_fixture_path: &Path, _ecosystem: &str) -> Option<Vec<Edge>> {
    // Scaffolded but per-ecosystem implementations land alongside
    // each ecosystem's regression test — keeps this common module
    // free of every parser dependency. The contracts/audit-harness.md
    // dispatch table is the source of truth for what each ecosystem
    // tiebreaker is expected to do.
    None
}

// ============================================================
// Workspace path helper
// ============================================================

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

pub fn fixture_path(eco_subpath: &str) -> PathBuf {
    // Milestone 090: transitive_parity fixtures moved to the
    // `mikebom-test-fixtures` repo. Resolve via build.rs's
    // MIKEBOM_FIXTURES_DIR rather than against workspace_root.
    PathBuf::from(env!("MIKEBOM_FIXTURES_DIR"))
        .join("transitive_parity")
        .join(eco_subpath)
}

// (sanity tests for this module live in
// `mikebom-cli/tests/transitive_parity_common_sanity.rs` — required
// by cargo's integration-test sharing pattern, where this `mod.rs`
// is included via `mod transitive_parity_common;` from each
// per-ecosystem regression test.)
