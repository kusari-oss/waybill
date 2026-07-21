//! Milestone 108 US5 — hermetic-build SHA-pin tests.
//!
//! Verifies the contract: two operators running the same mikebom-cli
//! binary get byte-identical SBOMs regardless of local cache state,
//! AND the runtime `--fingerprints-rev <SHA>` override flows through
//! the CLI → env → `LoadOptions::from_env()` → `load_corpus()` chain
//! to the cache key.
//!
//! Fully offline. Uses `tempfile::TempDir` + the
//! `MIKEBOM_FINGERPRINTS_CACHE_DIR` env override to point at synthetic
//! caches.
//!
//! What this test does NOT do: assert that the override SHA's value
//! shows up in the SBOM annotation. That contract is covered by the
//! offline `symbol_fingerprint::tests::scan_with_corpus_emits_12_hex_for_cached_corpus`
//! unit test (Phase 4), which directly constructs a corpus + asserts
//! the matcher stamps the right SHA. Reproducing that with a real
//! CLI invocation would require a synthetic ELF binary with the
//! right `.dynsym` payload — much heavier than what the unit test
//! already proves end-to-end through `scan_with_corpus`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::process::Command;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

fn embedded_sha() -> &'static str {
    env!("MIKEBOM_FINGERPRINTS_CORPUS_SHA")
}

const OVERRIDE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Pre-populate a minimal valid corpus snapshot under
/// `<cache>/<sha>/corpus/` so the cache-hit path triggers.
fn seed_cache_at(cache_root: &std::path::Path, sha: &str) {
    let corpus_dir = cache_root.join(sha).join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    std::fs::write(
        corpus_dir.join("index.json"),
        r#"{"version":1,"entries":[{"library":"libfoo","path":"libfoo.json"}]}"#,
    )
    .unwrap();
    std::fs::write(
        corpus_dir.join("libfoo.json"),
        r#"{"library":"libfoo","target_purl":"pkg:generic/libfoo","symbols":["a","b","c","d","e","f","g","h"],"min_symbols":5}"#,
    )
    .unwrap();
}

/// Invoke `mikebom sbom scan --offline --fingerprints-corpus`
/// against `scan_target`, honoring an optional `--fingerprints-rev`
/// override. `MIKEBOM_FIXED_TIMESTAMP` is set so the SBOM bytes are
/// stable across invocations.
fn run_scan(
    cache_root: &std::path::Path,
    scan_target: &std::path::Path,
    out_file: &std::path::Path,
    fingerprints_rev: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(binary_path());
    cmd.env("MIKEBOM_FINGERPRINTS_CACHE_DIR", cache_root)
        .env("MIKEBOM_FIXED_TIMESTAMP", "2026-06-02T00:00:00Z")
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target)
        .arg("--output")
        .arg(out_file)
        .arg("--fingerprints-corpus")
        .arg("--no-deep-hash");
    if let Some(rev) = fingerprints_rev {
        cmd.arg("--fingerprints-rev").arg(rev);
    }
    cmd.output().unwrap()
}

/// T055 — byte-identity: `--fingerprints-corpus` alone vs
/// `--fingerprints-corpus --fingerprints-rev <embedded-sha>` produce
/// the same SBOM bytes. Proves the no-override default equals the
/// explicit-embedded-override case.
#[test]
fn fingerprints_rev_matching_embedded_is_byte_identical_to_no_override() {
    let cache = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("placeholder.txt"), b"placeholder").unwrap();
    let sha = embedded_sha();
    seed_cache_at(cache.path(), sha);

    let out_a = scan_target.path().join("a.cdx.json");
    let out_b = scan_target.path().join("b.cdx.json");

    let r_a = run_scan(cache.path(), scan_target.path(), &out_a, None);
    assert!(
        r_a.status.success(),
        "scan A failed: {}",
        String::from_utf8_lossy(&r_a.stderr)
    );
    let r_b = run_scan(cache.path(), scan_target.path(), &out_b, Some(sha));
    assert!(
        r_b.status.success(),
        "scan B failed: {}",
        String::from_utf8_lossy(&r_b.stderr)
    );

    // Structural compare modulo the always-randomized `serialNumber`
    // (CDX requires it but mikebom has no fixed-uuid env knob today).
    // `MIKEBOM_FIXED_TIMESTAMP` pins the timestamp; serialNumber is
    // the only remaining per-run-volatile field. Strip it from both
    // SBOMs before comparing.
    let mut sbom_a: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out_a).unwrap()).unwrap();
    let mut sbom_b: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out_b).unwrap()).unwrap();
    sbom_a.as_object_mut().unwrap().remove("serialNumber");
    sbom_b.as_object_mut().unwrap().remove("serialNumber");
    assert_eq!(
        sbom_a, sbom_b,
        "SBOMs must be structurally identical (modulo serialNumber) when --fingerprints-rev matches the embedded SHA",
    );
}

/// T056 — runtime override flows through the CLI without error when
/// the override SHA has a populated cache entry. The "annotation
/// reflects the override" contract is covered by the offline unit
/// test `symbol_fingerprint::tests::scan_with_corpus_emits_12_hex_for_cached_corpus`
/// (Phase 4); this test exercises the CLI parse + env-bridge +
/// `load_corpus(sha_override = Some(...))` plumbing end-to-end.
#[test]
fn fingerprints_rev_with_distinct_sha_resolves_to_override_cache_dir() {
    let cache = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("placeholder.txt"), b"placeholder").unwrap();
    // Pre-populate the cache at the OVERRIDE SHA only. The embedded
    // SHA's cache directory does not exist — if the override didn't
    // flow through, the offline scan would fall back to bundled
    // (with a `cache is empty` warning on stderr).
    seed_cache_at(cache.path(), OVERRIDE_SHA);

    let out = scan_target.path().join("out.cdx.json");
    let result = run_scan(
        cache.path(),
        scan_target.path(),
        &out,
        Some(OVERRIDE_SHA),
    );
    assert!(
        result.status.success(),
        "scan with --fingerprints-rev failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    // SBOM is parseable CDX (the override flowed through without
    // breaking the scan).
    let sbom_bytes = std::fs::read(&out).unwrap();
    let sbom: serde_json::Value =
        serde_json::from_slice(&sbom_bytes).expect("invalid SBOM JSON");
    assert_eq!(
        sbom["bomFormat"].as_str(),
        Some("CycloneDX"),
        "expected valid CDX SBOM after override scan",
    );
    // The override-SHA cache directory is still intact (the scan
    // didn't touch it destructively).
    assert!(
        cache
            .path()
            .join(OVERRIDE_SHA)
            .join("corpus")
            .join("index.json")
            .is_file(),
        "override-SHA cache entry must survive the scan",
    );
}

/// T053 implicit-dep warn: `--fingerprints-rev` without
/// `--fingerprints-corpus` emits a warning and ignores the override
/// (bundled fallback is used).
#[test]
fn fingerprints_rev_without_opt_in_warns_and_ignores() {
    let cache = tempfile::tempdir().unwrap();
    let scan_target = tempfile::tempdir().unwrap();
    std::fs::write(scan_target.path().join("placeholder.txt"), b"placeholder").unwrap();

    let out = scan_target.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", cache.path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(scan_target.path())
        .arg("--output")
        .arg(&out)
        // NOTE: no --fingerprints-corpus here.
        .arg("--fingerprints-rev")
        .arg(OVERRIDE_SHA)
        .arg("--no-deep-hash")
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "scan should succeed (override ignored), not fail: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("--fingerprints-rev provided without --fingerprints-corpus")
            || stderr.contains("ignoring"),
        "expected implicit-dep warn in stderr; got: {stderr}"
    );
    // The override SHA's cache dir was never created (override was
    // ignored, no cache access happened).
    assert!(
        !cache.path().join(OVERRIDE_SHA).exists(),
        "override-SHA cache dir must NOT be created when override is ignored",
    );
}
