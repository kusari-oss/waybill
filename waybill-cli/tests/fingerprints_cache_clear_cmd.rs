//! Milestone 108 US4 — `mikebom fingerprints cache-clear` integration
//! test. Fully offline; uses `tempfile::TempDir` + the
//! `MIKEBOM_FINGERPRINTS_CACHE_DIR` env override.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

const SHA_A: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";
const SHA_B: &str = "0123456789abcdef0123456789abcdef01234567";

fn seed_cache_entry(cache_root: &std::path::Path, sha: &str) {
    std::fs::create_dir_all(cache_root.join(sha).join("corpus")).unwrap();
}

#[test]
fn cache_clear_removes_all_directories_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    seed_cache_entry(tmp.path(), SHA_A);
    seed_cache_entry(tmp.path(), SHA_B);
    let output = Command::new(binary_path())
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("cache-clear")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 2, "expected 2 `removed:` lines; got: {stdout:?}");
    assert!(!tmp.path().join(SHA_A).exists());
    assert!(!tmp.path().join(SHA_B).exists());
}

#[test]
fn cache_clear_with_keep_rev_preserves_the_named_sha() {
    let tmp = tempfile::tempdir().unwrap();
    seed_cache_entry(tmp.path(), SHA_A);
    seed_cache_entry(tmp.path(), SHA_B);
    let output = Command::new(binary_path())
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("cache-clear")
        .arg("--keep-rev")
        .arg(SHA_A)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 1, "expected exactly 1 `removed:` line; got: {stdout:?}");
    assert!(stdout.contains(SHA_B), "expected SHA_B in removed list; got: {stdout:?}");
    assert!(tmp.path().join(SHA_A).exists(), "SHA_A must be preserved");
    assert!(!tmp.path().join(SHA_B).exists());
}

#[test]
fn cache_clear_idempotent_on_empty_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("cache-clear")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "", "expected no output on empty cache; got: {stdout:?}");
}

#[test]
fn cache_clear_rejects_malformed_keep_rev_with_exit_1() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("cache-clear")
        .arg("--keep-rev")
        .arg("not-a-sha")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1), "expected exit-code 1 for malformed SHA");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid SHA"),
        "expected `invalid SHA` in stderr; got: {stderr:?}"
    );
}
