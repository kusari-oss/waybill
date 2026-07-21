//! Milestone 164 SC-010 optional real-testbed audit — gated behind
//! `WAYBILL_PNPM_MULTIVER_AUDIT=1`. If a cached copy of
//! `podman-desktop` is available (via `WAYBILL_FIXTURES_DIR`), assert
//! multi-version orphan count ≤ 30 and BFS reachability ≥ 93%. NOT
//! blocking for the PR — matches milestone-160 T033 + milestone-161
//! T040 + milestone-162 T034 + milestone-163 T037 pattern.
//!
//! Empirical baseline (2026-07-05, live podman-desktop):
//!   Pre-164: multi-version orphans = 435, BFS reachability = 77.4%
//!   Post-164: multi-version orphans = 12, BFS reachability = 99.6%
//! Assertion thresholds intentionally loose (≤30 and ≥93%) to survive
//! reasonable upstream drift.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

fn should_run() -> Option<PathBuf> {
    if std::env::var("WAYBILL_PNPM_MULTIVER_AUDIT").is_err() {
        return None;
    }
    let fixtures_dir = std::env::var("WAYBILL_FIXTURES_DIR").ok()?;
    let candidate = PathBuf::from(fixtures_dir).join("podman-desktop");
    if !candidate.join("pnpm-lock.yaml").is_file() {
        return None;
    }
    Some(candidate)
}

fn scan(path: &std::path::Path) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let out = tempfile::NamedTempFile::new()
        .expect("tempfile")
        .path()
        .to_path_buf();
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out)
        .arg("--no-deep-hash")
        .output()
        .expect("run waybill");
    assert!(
        status.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&status.stderr)
    );
    let raw = std::fs::read_to_string(&out).expect("read sbom");
    serde_json::from_str(&raw).expect("parse sbom")
}

#[test]
fn t020_podman_desktop_multi_version_orphans_and_bfs_within_thresholds() {
    let Some(path) = should_run() else {
        eprintln!(
            "t020 SKIP: set WAYBILL_PNPM_MULTIVER_AUDIT=1 + WAYBILL_FIXTURES_DIR to a \
             directory containing a `podman-desktop/` subdir with pnpm-lock.yaml"
        );
        return;
    };
    let sbom = scan(&path);
    let components = sbom
        .get("components")
        .and_then(|c| c.as_array())
        .expect("components array");
    let deps = sbom
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies array");
    let root = sbom
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("purl"))
        .and_then(|p| p.as_str())
        .expect("metadata.component.purl");

    // BFS.
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for node in deps {
        let from = node
            .get("ref")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();
        let tgts: Vec<String> = node
            .get("dependsOn")
            .and_then(|d| d.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        adj.insert(from, tgts);
    }
    let mut visited: HashSet<String> = HashSet::new();
    let mut q = vec![root.to_string()];
    while let Some(cur) = q.pop() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        if let Some(tgts) = adj.get(&cur) {
            for t in tgts {
                if !visited.contains(t) {
                    q.push(t.clone());
                }
            }
        }
    }

    let npm_purls: Vec<String> = components
        .iter()
        .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(String::from))
        .filter(|p| p.starts_with("pkg:npm/"))
        .collect();

    // Multi-version orphan classification.
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    for p in &npm_purls {
        let base = p[8..].rsplit_once('@').map(|(n, _)| n.to_string()).unwrap_or_default();
        by_name.entry(base).or_default().push(p.clone());
    }
    let orphans: Vec<&String> = npm_purls.iter().filter(|p| !visited.contains(*p)).collect();
    let multi_version_orphans: usize = orphans
        .iter()
        .filter(|o| {
            let base = o[8..].rsplit_once('@').map(|(n, _)| n).unwrap_or("");
            by_name
                .get(base)
                .map(|siblings| siblings.iter().any(|s| visited.contains(s)))
                .unwrap_or(false)
        })
        .count();

    let bfs_pct = visited
        .iter()
        .filter(|p| p.starts_with("pkg:npm/"))
        .count() as f64
        / npm_purls.len() as f64
        * 100.0;

    // SC-001: multi-version orphans ≤ 30 (94% reduction from 435).
    assert!(
        multi_version_orphans <= 30,
        "SC-001 violated: multi-version orphans = {multi_version_orphans} (expected ≤ 30). \
         Pre-164 baseline: 435. Post-164 target: ≤ 30."
    );
    // SC-002: BFS reachability ≥ 93% (+15pp from 77.4%).
    assert!(
        bfs_pct >= 93.0,
        "SC-002 violated: BFS reachability = {bfs_pct:.1}% (expected ≥ 93%). \
         Pre-164 baseline: 77.4%. Post-164 target: ≥ 93%."
    );

    eprintln!(
        "t020 PASS: multi-version orphans = {multi_version_orphans} (target ≤ 30), \
         BFS reachability = {bfs_pct:.1}% (target ≥ 93%)"
    );
}
