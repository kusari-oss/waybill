//! Milestone 133 US2.3 (FR-014, T021): assert every emitted component
//! carrying a language-ecosystem PURL has `evidence.occurrences[]`
//! populated with a non-empty, rootfs-relative location string AND a
//! `additionalContext.sha256` anchor.
//!
//! Pre-feature baseline: 177 / 2926 (6 %) — only the OS-package
//! deep-hash path populated occurrences. Post-feature: ≥95 % across
//! the deps.dev-indexed ecosystems (cargo, npm, nuget, maven, pypi,
//! gem, golang).
//!
//! This test scans a fixture Cargo.lock and asserts:
//! 1. every `pkg:cargo/...` component has at least one occurrence;
//! 2. each occurrence's `location` is non-empty and has no leading `/`;
//! 3. each occurrence's `additionalContext` parses as JSON with a
//!    non-empty `sha256` field.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(sub: &str) -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR"))
        .join("cargo")
        .join(sub)
}

fn run_scan(path: &Path) -> (tempfile::TempDir, PathBuf) {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out_path = tmp.path().join("sbom.cdx.json");
    let status = Command::new(bin)
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(path)
        .arg("--output")
        .arg(&out_path)
        .arg("--no-deep-hash")
        .status()
        .expect("waybill should run");
    assert!(status.success(), "waybill scan failed");
    (tmp, out_path)
}

#[test]
#[cfg_attr(test, allow(clippy::unwrap_used))]
fn cargo_lockfile_v3_every_cargo_component_has_evidence_occurrence() {
    let (_tmp, sbom_path) = run_scan(&fixture("lockfile-v3"));
    let sbom: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sbom_path).unwrap()).unwrap();
    let components = sbom["components"].as_array().unwrap();
    assert!(!components.is_empty(), "expected at least one component");

    let mut cargo_count = 0usize;
    for c in components {
        let purl = c["purl"].as_str().unwrap_or("");
        if !purl.starts_with("pkg:cargo/") {
            continue;
        }
        cargo_count += 1;
        let occs = c
            .pointer("/evidence/occurrences")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| {
                panic!(
                    "cargo component {purl} missing evidence.occurrences[] entirely (US2.3 regression)",
                )
            });
        assert!(
            !occs.is_empty(),
            "cargo component {purl} has empty evidence.occurrences[] (US2.3 regression)",
        );
        let first = &occs[0];
        let location = first["location"].as_str().unwrap_or("");
        assert!(
            !location.is_empty(),
            "cargo component {purl} occurrence has empty location",
        );
        assert!(
            !location.starts_with('/'),
            "cargo component {purl} occurrence location {location:?} starts with `/` (FR-007 violation)",
        );
        let ctx_str = first["additionalContext"].as_str().unwrap_or("{}");
        let ctx: serde_json::Value = serde_json::from_str(ctx_str)
            .unwrap_or_else(|_| panic!("additionalContext on {purl} should be JSON: {ctx_str:?}"));
        let sha = ctx["sha256"].as_str().unwrap_or("");
        assert!(
            !sha.is_empty(),
            "cargo component {purl} occurrence additionalContext.sha256 empty (manifest unreadable?)",
        );
        assert_eq!(
            sha.len(),
            64,
            "cargo component {purl} occurrence sha256 should be 64 hex chars, got: {sha:?}",
        );
    }
    assert!(
        cargo_count > 0,
        "expected the lockfile-v3 fixture to yield at least one pkg:cargo/* component",
    );
}
