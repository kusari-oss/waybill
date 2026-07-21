//! Milestone 108 US4 — end-to-end air-gapped roundtrip test.
//!
//! Models the documented Scenario 2 (operator pre-fetches on
//! internet-connected machine → tars cache → ships to air-gapped
//! destination → runs scan under `--offline`).
//!
//! Stages:
//!   1. `waybill fingerprints fetch` against tempdir A's cache root.
//!   2. `tar czf cache.tgz` over tempdir A's contents.
//!   3. `tar xzf` into tempdir B.
//!   4. `waybill sbom scan --offline --fingerprints-corpus` against
//!      tempdir B's cache root + a placeholder fixture.
//!   5. Assert the scan succeeded + emitted a parseable SBOM
//!      (`--offline` MUST NOT abort when the cache is populated).
//!
//! Uses the build-time-embedded SHA throughout — no `--fingerprints-rev`
//! override (that's a Phase 7 / US5 feature). The roundtrip semantics
//! (cache is portable; `--offline` + populated cache = no network) are
//! fully covered by the single-pin scenario.
//!
//! Gated behind `WAYBILL_FINGERPRINTS_NETWORK_TESTS=1` because stage 1
//! is the only one that requires real network access. The rest is
//! purely local file shuffling.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn embedded_sha() -> &'static str {
    env!("WAYBILL_FINGERPRINTS_CORPUS_SHA")
}

fn network_tests_enabled() -> bool {
    std::env::var("WAYBILL_FINGERPRINTS_NETWORK_TESTS").ok().as_deref() == Some("1")
}

#[test]
fn airgap_roundtrip_fetch_tar_untar_offline_scan() {
    if !network_tests_enabled() {
        println!(
            "skipped: WAYBILL_FINGERPRINTS_NETWORK_TESTS not set (offline CI lane)"
        );
        return;
    }

    let connected_cache = tempfile::tempdir().expect("connected-cache tempdir");
    let airgap_cache = tempfile::tempdir().expect("airgap-cache tempdir");
    let scan_target = tempfile::tempdir().expect("scan-target tempdir");
    let tarball_dir = tempfile::tempdir().expect("tarball tempdir");
    let tarball_path = tarball_dir.path().join("cache.tgz");

    // ──────────────────────────────────────────────────────────────
    // Stage 1: fetch on the "internet-connected" machine.
    // ──────────────────────────────────────────────────────────────
    let stage1 = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", connected_cache.path())
        .arg("fingerprints")
        .arg("fetch")
        .output()
        .unwrap();
    assert!(
        stage1.status.success(),
        "stage 1 fetch failed: {}",
        String::from_utf8_lossy(&stage1.stderr),
    );
    let sha = embedded_sha();
    assert!(
        connected_cache
            .path()
            .join(sha)
            .join("corpus")
            .join("index.json")
            .is_file(),
        "stage 1 didn't populate the cache",
    );

    // ──────────────────────────────────────────────────────────────
    // Stage 2: tar the populated cache.
    // ──────────────────────────────────────────────────────────────
    let stage2 = Command::new("tar")
        .arg("-czf")
        .arg(&tarball_path)
        .arg("-C")
        .arg(connected_cache.path())
        .arg(".")
        .output()
        .unwrap();
    assert!(
        stage2.status.success(),
        "stage 2 tar failed: {}",
        String::from_utf8_lossy(&stage2.stderr),
    );
    assert!(tarball_path.is_file(), "tarball not created");

    // ──────────────────────────────────────────────────────────────
    // Stage 3: untar into the "air-gapped" cache dir.
    // ──────────────────────────────────────────────────────────────
    let stage3 = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball_path)
        .arg("-C")
        .arg(airgap_cache.path())
        .output()
        .unwrap();
    assert!(
        stage3.status.success(),
        "stage 3 untar failed: {}",
        String::from_utf8_lossy(&stage3.stderr),
    );
    let airgap_index_path: &Path = &airgap_cache
        .path()
        .join(sha)
        .join("corpus")
        .join("index.json");
    assert!(
        airgap_index_path.is_file(),
        "stage 3 didn't reproduce the cache at {}",
        airgap_index_path.display(),
    );

    // ──────────────────────────────────────────────────────────────
    // Stage 4: scan under --offline against the air-gapped cache.
    // ──────────────────────────────────────────────────────────────
    std::fs::write(scan_target.path().join("placeholder.txt"), b"placeholder").unwrap();
    let out_file = scan_target.path().join("out.cdx.json");
    let stage4 = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", airgap_cache.path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target.path())
        .arg("--output")
        .arg(&out_file)
        .arg("--fingerprints-corpus")
        .arg("--no-deep-hash")
        .output()
        .unwrap();
    assert!(
        stage4.status.success(),
        "stage 4 offline scan failed: {}",
        String::from_utf8_lossy(&stage4.stderr),
    );

    // ──────────────────────────────────────────────────────────────
    // Stage 5: SBOM is parseable JSON.
    // ──────────────────────────────────────────────────────────────
    let sbom_bytes = std::fs::read(&out_file).expect("SBOM not written");
    let sbom: serde_json::Value =
        serde_json::from_slice(&sbom_bytes).expect("invalid SBOM JSON");
    // Sanity: top-level CDX structure present (don't over-assert; the
    // roundtrip is what we're testing, not SBOM contents).
    assert_eq!(
        sbom["bomFormat"].as_str(),
        Some("CycloneDX"),
        "SBOM missing CycloneDX bomFormat",
    );
}
