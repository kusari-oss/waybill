//! Milestone 108 US4 — `waybill fingerprints list` integration test.
//! Fully offline; uses `tempfile::TempDir` + the
//! `WAYBILL_FINGERPRINTS_CACHE_DIR` env override to point at a
//! synthetic cache without touching the operator's real cache.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

const SHA_A: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";
const SHA_B: &str = "0123456789abcdef0123456789abcdef01234567";

fn seed_cache_entry(cache_root: &std::path::Path, sha: &str, record_count: usize) {
    let corpus_dir = cache_root.join(sha).join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    let mut entries = Vec::new();
    for i in 0..record_count {
        entries.push(format!(r#"{{"library":"lib{i}","path":"lib{i}.json"}}"#));
    }
    let index_json = format!(
        r#"{{"version":1,"entries":[{}]}}"#,
        entries.join(",")
    );
    std::fs::write(corpus_dir.join("index.json"), index_json).unwrap();
}

#[test]
fn list_empty_cache_exits_zero_with_no_output() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "", "expected no output; got: {stdout:?}");
}

#[test]
fn list_two_cached_shas_prints_alphabetically_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    seed_cache_entry(tmp.path(), SHA_A, 7);
    seed_cache_entry(tmp.path(), SHA_B, 12);
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    // Alphabetical: `0123...` precedes `fff3...`.
    assert!(
        lines[0].starts_with(SHA_B),
        "expected SHA_B first; got: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with(SHA_A),
        "expected SHA_A second; got: {}",
        lines[1]
    );
    // Counts present in the right columns.
    assert!(lines[0].contains("12"), "expected count=12 in {:?}", lines[0]);
    assert!(lines[1].contains("7"), "expected count=7 in {:?}", lines[1]);
}

#[test]
fn list_skips_non_sha_directories() {
    let tmp = tempfile::tempdir().unwrap();
    seed_cache_entry(tmp.path(), SHA_A, 3);
    // Leftover `.tmp-<uuid>/` staging dir from a crashed fetcher.
    std::fs::create_dir_all(tmp.path().join(".tmp-abc123")).unwrap();
    let output = Command::new(binary_path())
        .env("WAYBILL_FINGERPRINTS_CACHE_DIR", tmp.path())
        .arg("fingerprints")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 1, "expected exactly 1 line; got: {stdout:?}");
    assert!(
        stdout.starts_with(SHA_A),
        "expected the real SHA only; got: {stdout:?}"
    );
}
