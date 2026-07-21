//! Milestone 108 — end-to-end gated integration test for the external
//! fingerprint corpus fetch + scan path.
//!
//! Gated behind `WAYBILL_FINGERPRINTS_NETWORK_TESTS=1`. When the env
//! var is unset (the default CI lane), the test exits zero with a
//! `println!("skipped: ...")` message — preserves the offline-by-default
//! posture of `cargo +stable test --workspace`.
//!
//! When enabled, the test:
//!
//! 1. Points `WAYBILL_FINGERPRINTS_CACHE_DIR` at a fresh tempdir so we
//!    exercise the real cache-miss → fetch → cache-populate path.
//! 2. Invokes `waybill sbom scan --fingerprints-corpus --path <fixture>`
//!    against a synthetic empty fixture (we don't need real
//!    statically-linked binaries to prove plumbing — the unit tests
//!    cover matching; this test proves fetch + cache mechanics).
//! 3. Asserts the cache directory now contains the build-time-embedded
//!    corpus SHA's `corpus/index.json` (proves the fetch landed bytes).
//! 4. Asserts the scan succeeded with a parseable CDX SBOM output.
//!
//! Run locally:
//!
//! ```sh
//! WAYBILL_FINGERPRINTS_NETWORK_TESTS=1 \
//!     cargo +stable test -p waybill \
//!         --test scan_fingerprint_corpus_external
//! ```

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

fn network_tests_enabled() -> bool {
    std::env::var("WAYBILL_FINGERPRINTS_NETWORK_TESTS").ok().as_deref() == Some("1")
}

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Read the build-time-embedded corpus SHA from the same env var
/// the production binary's `CorpusSha::build_time_embedded()` uses.
/// This couples the test to the build-time pin exactly the way the
/// production code is coupled — no second source of truth.
fn embedded_sha() -> &'static str {
    env!("WAYBILL_FINGERPRINTS_CORPUS_SHA")
}

#[test]
fn external_corpus_fetch_populates_cache_and_scan_succeeds() {
    if !network_tests_enabled() {
        println!(
            "skipped: WAYBILL_FINGERPRINTS_NETWORK_TESTS not set (offline CI lane)"
        );
        return;
    }

    let cache_dir = tempfile::tempdir().expect("tempdir for cache");
    let scan_target = tempfile::tempdir().expect("tempdir for scan input");
    let out_file = scan_target.path().join("out.cdx.json");

    // Write a tiny placeholder file so the path resolver has something
    // to walk. The actual content doesn't matter — we're testing the
    // fetch + scan plumbing, not the matcher.
    std::fs::write(scan_target.path().join("placeholder.txt"), b"placeholder")
        .expect("write placeholder");

    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", cache_dir.path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target.path())
        .arg("--output")
        .arg(&out_file)
        .arg("--fingerprints-corpus")
        .arg("--no-deep-hash")
        .output()
        .expect("failed to invoke waybill");
    assert!(
        output.status.success(),
        "waybill sbom scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );

    // SBOM should be parseable JSON regardless of whether any matches
    // happened (none expected on a placeholder.txt fixture).
    let sbom_bytes = std::fs::read(&out_file).expect("SBOM not written");
    let _: serde_json::Value =
        serde_json::from_slice(&sbom_bytes).expect("invalid SBOM JSON");

    // The fetch should have populated the cache at the build-time SHA.
    let expected_index_path: PathBuf = cache_dir
        .path()
        .join(embedded_sha())
        .join("corpus")
        .join("index.json");
    assert!(
        expected_index_path.is_file(),
        "cache populate failed: expected {} to exist after --fingerprints-corpus scan",
        expected_index_path.display(),
    );
}
