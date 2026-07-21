//! Milestone 108 US4 — `waybill fingerprints fetch` integration test.
//!
//! Two-tier coverage:
//!
//! - **Offline half** (always runs): cache-hit short-circuit. Seeds
//!   a synthetic cache entry, runs `fingerprints fetch`, asserts the
//!   command prints `cache hit:` + exits 0 without touching the
//!   network.
//! - **Network-gated half** (`WAYBILL_FINGERPRINTS_NETWORK_TESTS=1`):
//!   real fetch from the sibling repo at the build-time-embedded
//!   SHA. Asserts the fetched-message + cache-populated invariants.
//!
//! Also exercises the malformed-SHA exit-code-1 path.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

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
fn fetch_short_circuits_on_cache_hit() {
    let tmp = tempfile::tempdir().unwrap();
    let sha = embedded_sha();
    // Seed the cache to simulate a hit at the build-time-embedded SHA.
    let corpus_dir = tmp.path().join(sha).join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    std::fs::write(
        corpus_dir.join("index.json"),
        r#"{"version":1,"entries":[]}"#,
    )
    .unwrap();
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("fetch")
        .output()
        .unwrap();
    assert!(output.status.success(), "exit non-zero: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("cache hit:"),
        "expected `cache hit:` prefix; got: {stdout:?}"
    );
    assert!(stdout.contains(sha));
}

#[test]
fn fetch_rejects_malformed_corpus_rev_with_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("fetch")
        .arg("--corpus-rev")
        .arg("not-a-sha")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid SHA"),
        "expected `invalid SHA` in stderr; got: {stderr:?}"
    );
}

#[test]
fn fetch_populates_cache_and_prints_fetched_message() {
    if !network_tests_enabled() {
        println!(
            "skipped: WAYBILL_FINGERPRINTS_NETWORK_TESTS not set (offline CI lane)"
        );
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let sha = embedded_sha();
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("fetch")
        .output()
        .unwrap();
    assert!(output.status.success(), "fetch failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("fetched:"),
        "expected `fetched:` prefix; got: {stdout:?}"
    );
    assert!(stdout.contains(sha));
    // Cache now populated.
    assert!(
        tmp.path()
            .join(sha)
            .join("corpus")
            .join("index.json")
            .is_file(),
        "index.json missing after fetch"
    );
}
