//! End-to-end integration test for the v2 fingerprint corpus pipeline
//! (milestone 110 Phase 4 Slice B-3).
//!
//! Verifies that when a v2 corpus record lives in mikebom's fingerprint
//! cache and an operator scans a binary whose extracted indicators match
//! the record, mikebom emits a PackageDbEntry with the v2 record's
//! canonical PURL (versioned, ecosystem-specific) and the numeric
//! `mikebom:fingerprint-confidence` annotation derived from the matcher's
//! fusion algorithm.
//!
//! Test design:
//! - Use `tempfile::tempdir` for an isolated fingerprint cache.
//! - Override `MIKEBOM_FINGERPRINTS_CACHE_DIR` for the spawned mikebom
//!   subprocess to point at the temp cache.
//! - Override `MIKEBOM_FINGERPRINTS_REV` with a synthetic SHA so the
//!   cache lookup hits our fixture dir (independent of the build-time-
//!   embedded sibling-repo SHA).
//! - Author a v2 record whose `purl.name()` is `mikebom-110-zlib-detector`
//!   (distinct from `zlib` so the `by_library` slot is vacant + the v2
//!   path can emit alongside the existing v1 zlib emission).
//! - Run `mikebom sbom scan` against the cmake-demo project root (which
//!   carries `build/crc-demo`, a Mach-O binary exporting zlib's public
//!   API symbols).
//! - Assert the SBOM contains a component with the v2 PURL +
//!   `mikebom:fingerprint-confidence` ≥ "0.70".
//!
//! Skips gracefully when the cmake-demo isn't pre-built on the test host
//! (same skip pattern as the milestone-109 binary_source_binding tests).

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_mikebom")
}

/// Locate the cmake-demo project root (contains `build/crc-demo`, a
/// zlib-statically-linked binary).
fn find_cmake_demo_root() -> Option<PathBuf> {
    let candidates = ["../mikebom-cmake-demo", "../../mikebom-cmake-demo"];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.is_dir() && p.join("build/crc-demo").is_file() {
            return p.canonicalize().ok();
        }
    }
    None
}

/// Synthetic SHA used as the cache key for the fixture cache. Distinct
/// from the milestone-108 build-time-embedded SHA so the matcher's cache
/// lookup hits our temp dir.
const FIXTURE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// v2 corpus record JSON. PURL name `mikebom-110-zlib-detector` is
/// intentionally distinct from `zlib` so the by_library merge doesn't
/// collide with the existing v1 / source-binding zlib emission.
/// confidence_baseline 0.70 + min_match 5 means 5-of-10 zlib symbol
/// matches → `Medium` bucket → emits.
fn fixture_v2_record() -> &'static str {
    r#"{
      "id": "mikebom-110-zlib-detector-test-fixture",
      "purl": "pkg:test/mikebom-fixture/mikebom-110-zlib-detector@v2-0.1.0",
      "version_range": "v2-0.1.0",
      "indicators": {
        "exported_symbols": {
          "type": "symbol-set",
          "required": [
            "adler32",
            "compress",
            "compress2",
            "crc32",
            "deflate",
            "deflateInit_",
            "inflate",
            "inflateInit_",
            "uncompress",
            "zlibVersion"
          ],
          "min_match": 5,
          "confidence_baseline": 0.70
        }
      },
      "provenance": {
        "tier": "manual-curation",
        "extracted_from": "https://example.com/mikebom-110-fixture",
        "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "extraction_toolchain": "mikebom-110-fixture-test",
        "extracted_at": "2026-06-05T00:00:00Z"
      },
      "schema_version": 2
    }"#
}

/// Set up a fixture fingerprint cache under `cache_root` containing a
/// single v2 record at `<FIXTURE_SHA>/corpus/`.
fn populate_fixture_cache(cache_root: &Path) {
    let corpus_dir = cache_root.join(FIXTURE_SHA).join("corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    std::fs::write(
        corpus_dir.join("mikebom-110-zlib-detector.json"),
        fixture_v2_record(),
    )
    .unwrap();
    // index.json — points at the v2 record file. Note: the existing v1
    // loader also reads this file, peeks at the entries, attempts to
    // load each as a v1 FingerprintRecord, and skips those that fail
    // (since our v2 record's shape doesn't deserialize as v1). Both
    // loaders share the index but dispatch per-record by shape.
    std::fs::write(
        corpus_dir.join("index.json"),
        r#"{"version":1,"entries":[{"library":"mikebom-110-zlib-detector","path":"mikebom-110-zlib-detector.json"}]}"#,
    )
    .unwrap();
}

fn scan_with_fixture_cache(project_root: &Path, cache_root: &Path) -> Value {
    let out = tempfile::tempdir().unwrap();
    let out_file = out.path().join("sbom.cdx.json");
    let result = Command::new(binary_path())
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(project_root)
        .arg("--output")
        .arg(&out_file)
        .arg("--no-deep-hash")
        .arg("--fingerprints-corpus")
        .env("MIKEBOM_FINGERPRINTS_CACHE_DIR", cache_root)
        .env("MIKEBOM_FINGERPRINTS_REV", FIXTURE_SHA)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "mikebom sbom scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_file).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Find the component with a matching `purl` substring (lets the test
/// tolerate URL-encoding variations in the PURL).
fn find_component_by_purl_contains<'a>(sbom: &'a Value, needle: &str) -> Option<&'a Value> {
    sbom["components"]
        .as_array()?
        .iter()
        .find(|c| c["purl"].as_str().is_some_and(|p| p.contains(needle)))
}

/// US1 acceptance: when a v2 corpus record matches a binary's
/// exported-symbol set, mikebom emits a component with the record's
/// canonical (versioned) PURL.
#[test]
fn v2_record_emits_canonical_purl_when_indicators_match() {
    let Some(project_root) = find_cmake_demo_root() else {
        println!(
            "skipped: no cmake-demo project available \
             (build mikebom-cmake-demo first: \
             `cd ../mikebom-cmake-demo && cmake -S . -B build -G Ninja && ninja -C build`)"
        );
        return;
    };

    let cache = tempfile::tempdir().unwrap();
    populate_fixture_cache(cache.path());

    let sbom = scan_with_fixture_cache(&project_root, cache.path());

    let v2_component = find_component_by_purl_contains(
        &sbom,
        "pkg:test/mikebom-fixture/mikebom-110-zlib-detector",
    );
    assert!(
        v2_component.is_some(),
        "expected a v2-derived component with the fixture PURL; got components: {:#?}",
        sbom["components"]
            .as_array()
            .map(|a| a.iter().map(|c| &c["purl"]).collect::<Vec<_>>())
    );
    let v2_component = v2_component.unwrap();
    assert!(
        v2_component["purl"]
            .as_str()
            .is_some_and(|p| p.contains("@v2-0.1.0")),
        "v2 PURL MUST carry the version segment; got {:?}",
        v2_component["purl"]
    );
}

/// US1 acceptance scenario 1 + FR-017: every v2-derived component MUST
/// carry a `mikebom:fingerprint-confidence` annotation whose value is a
/// numeric "X.XX" string ≥ "0.70" (the medium-bucket floor).
#[test]
fn v2_record_emits_numeric_confidence_annotation() {
    let Some(project_root) = find_cmake_demo_root() else {
        println!("skipped: no cmake-demo project available");
        return;
    };

    let cache = tempfile::tempdir().unwrap();
    populate_fixture_cache(cache.path());

    let sbom = scan_with_fixture_cache(&project_root, cache.path());
    let v2_component =
        find_component_by_purl_contains(&sbom, "mikebom-110-zlib-detector")
            .expect("v2 component must emit");

    let props = v2_component["properties"]
        .as_array()
        .expect("v2 component MUST carry properties[]");
    let confidence = props
        .iter()
        .find(|p| p["name"].as_str() == Some("mikebom:fingerprint-confidence"))
        .expect("v2 component MUST carry mikebom:fingerprint-confidence per FR-017");
    let value = confidence["value"].as_str().expect("annotation value is a string");
    // Format check: "X.XX" + parses as a float ≥ 0.70.
    let parsed: f64 = value
        .parse()
        .unwrap_or_else(|_| panic!("mikebom:fingerprint-confidence MUST parse as a float; got {value:?}"));
    assert!(
        parsed >= 0.70,
        "v2 fingerprint-confidence MUST be >= 0.70 (the Medium bucket floor); got {parsed}"
    );
}

/// US3 invariant: the existing v1 zlib emission survives unchanged when
/// the v2 path also fires. Both components coexist in the SBOM — the
/// v1 zlib at its source-tier PURL (from milestone-109 source-binding)
/// AND the v2 fixture component at its canonical v2 PURL.
#[test]
fn v1_zlib_emission_survives_alongside_v2_emission() {
    let Some(project_root) = find_cmake_demo_root() else {
        println!("skipped: no cmake-demo project available");
        return;
    };

    let cache = tempfile::tempdir().unwrap();
    populate_fixture_cache(cache.path());

    let sbom = scan_with_fixture_cache(&project_root, cache.path());

    // v1 zlib (via cmake source-binding) MUST still emit.
    let v1_zlib =
        find_component_by_purl_contains(&sbom, "pkg:github/madler/zlib@v1.3.1");
    assert!(
        v1_zlib.is_some(),
        "v1 zlib emission MUST survive — by_library['zlib'] is populated by the v1 path \
         + source-binding, and v2's `entry().or_insert_with(...)` gate must not override it"
    );

    // v2 fixture detector MUST emit alongside.
    let v2_detector =
        find_component_by_purl_contains(&sbom, "mikebom-110-zlib-detector");
    assert!(v2_detector.is_some(), "v2 fixture component must emit alongside v1");
}
