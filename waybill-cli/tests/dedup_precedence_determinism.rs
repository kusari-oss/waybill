//! Dedup precedence determinism — SC-010 cornerstone integration test
//! (milestone 105 T024+T025).
//!
//! Companion to the unit-level `dedup_is_input_order_invariant` test
//! in `scan_fs::dedup::tests`. This file exercises the END-TO-END
//! determinism guarantee: a real fixture, real `waybill sbom scan`
//! invocations, real SBOM bytes compared across N runs.
//!
//! The fixture at `tests/fixtures/golden_inputs/dedup_collision/`
//! contains both a `conanfile.txt` AND a `.gitmodules` declaring the
//! same library (`abseil`). Once milestone 105's US6 git-submodule
//! reader lands, this fixture produces a real cross-reader collision
//! and the `waybill:also-detected-via` annotation MUST be emitted
//! deterministically (lexicographic ordering of losing source-
//! mechanism strings per FR-015).
//!
//! Two test functions:
//!
//! 1. `dedup_collision_scans_deterministically_today` — active. Runs
//!    10 sequential scans against the same fixture, asserts byte-
//!    identical SBOM output across all 10. Catches the most common
//!    determinism bugs (HashMap-iteration-order, time-dependent
//!    fields, file-discovery ordering) under the readers wired today
//!    (only conan-recipe fires on this fixture pre-US6).
//!
//! 2. `dedup_collision_emits_also_detected_via_after_us6` — gated
//!    `#[ignore]`. The full SC-010 test. Activated when the
//!    git-submodule reader is wired AND the dispatcher feeds
//!    DetectionRecords through the dedup pipeline. Asserts the
//!    `waybill:also-detected-via` annotation appears with the
//!    expected payload across multiple runs.

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::normalize::{apply_fake_home_env, normalize_cdx_for_golden};
use common::{bin, workspace_root};

/// Path to the local dedup_collision fixture. The milestone-090
/// `fixture_path()` helper points at the EXTERNAL fixtures repo;
/// this fixture lives in-tree (under `waybill-cli/tests/fixtures/`)
/// because it's small + tightly coupled to the milestone-105 dedup
/// pipeline code. Reference by `CARGO_MANIFEST_DIR` directly.
fn dedup_collision_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden_inputs")
        .join("dedup_collision")
}

/// Run `waybill sbom scan` against the dedup_collision fixture,
/// returning the NORMALIZED CDX-JSON output. Pins HOME and the
/// emission timestamp env var; runs the produced output through
/// `normalize_cdx_for_golden` to mask the spec-mandated volatile
/// `serialNumber` (v4 UUID per CDX 1.6) and `metadata.timestamp`
/// fields. After normalization, two sequential runs of the
/// deterministic-readers path produce byte-identical strings.
fn scan_fixture_cdx(out_path: &Path) -> Vec<u8> {
    let fake_home = tempfile::tempdir().expect("fake-home tempdir");
    let fixture = dedup_collision_fixture();
    assert!(
        fixture.is_dir(),
        "dedup_collision fixture missing at {}",
        fixture.display()
    );
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, fake_home.path());
    cmd.env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z");
    cmd.args([
        "--offline",
        "sbom",
        "scan",
        "--path",
        fixture.to_str().unwrap(),
        "--format",
        "cyclonedx-json",
        "--output",
        out_path.to_str().unwrap(),
    ]);
    let status = cmd.status().expect("spawn waybill");
    assert!(status.success(), "waybill scan failed: {status:?}");
    let raw = std::fs::read_to_string(out_path).expect("read emitted SBOM");
    normalize_cdx_for_golden(&raw, &workspace_root()).into_bytes()
}

/// SC-010 active test: same fixture, 10 sequential scans, all
/// byte-identical. Exercises the readers currently wired against
/// this fixture (conan-recipe today; git-submodule post-US6).
///
/// The number of components emitted will depend on which readers
/// fire. Today: 1 (the conan recipe). After US6: 1 deduped
/// component with `waybill:also-detected-via` annotation. The test
/// makes NO assertion about component count — only that the bytes
/// are stable across runs.
#[test]
fn dedup_collision_scans_deterministically_today() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let mut runs: Vec<Vec<u8>> = Vec::with_capacity(10);
    for i in 0..10 {
        let out_path = workdir.path().join(format!("run-{i}.cdx.json"));
        runs.push(scan_fixture_cdx(&out_path));
    }
    let first = &runs[0];
    for (i, bytes) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            first.len(),
            bytes.len(),
            "run {i} produced different output length than run 0 ({} vs {} bytes)",
            first.len(),
            bytes.len()
        );
        assert!(
            first == bytes,
            "run {i} bytes differ from run 0 — non-determinism in waybill output. First mismatch byte: {}",
            first.iter().zip(bytes.iter()).position(|(a, b)| a != b).unwrap_or(usize::MAX)
        );
    }
}

/// SC-010 cornerstone test (gated until US6 wires the
/// `git-submodule` reader through the dedup pipeline). Asserts that
/// the dedup_collision fixture produces:
///
/// - Exactly ONE component for `abseil` (collision deduplicated).
/// - `waybill:source-mechanism: "conan-recipe"` on that component
///   (manifest-mode tier outranks filesystem-derived per FR-015).
/// - `waybill:also-detected-via` containing `["git-submodule"]`
///   (the losing reader, sorted lex order — single entry here).
/// - Byte-identical output across multiple runs.
///
/// When activated, this test does the heavy lifting of SC-010. The
/// `#[ignore]` gate is removed in the US6 PR (T085-T095) once the
/// reader + dispatcher wiring is complete.
#[test]
#[ignore = "pending US6: git-submodule reader + dispatcher wiring (T085-T095)"]
fn dedup_collision_emits_also_detected_via_after_us6() {
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let out_path = workdir.path().join("collision.cdx.json");
    let _bytes = scan_fixture_cdx(&out_path);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&out_path).expect("read"))
        .expect("parse CDX JSON");
    // Find the abseil component.
    let components = json["components"]
        .as_array()
        .expect("components array present");
    let abseil = components
        .iter()
        .find(|c| {
            c["name"].as_str().unwrap_or("") == "abseil"
                || c["purl"].as_str().unwrap_or("").contains("abseil")
        })
        .expect("abseil component present");
    // Source-mechanism: conan-recipe (manifest-mode wins over git-submodule).
    let sm = abseil["properties"]
        .as_array()
        .and_then(|props| {
            props.iter().find_map(|p| {
                if p["name"].as_str()? == "waybill:source-mechanism" {
                    p["value"].as_str().map(str::to_string)
                } else {
                    None
                }
            })
        })
        .expect("waybill:source-mechanism property present");
    assert_eq!(
        sm, "conan-recipe",
        "expected conan-recipe to win the manifest-mode tier; got {sm:?}"
    );
    // also-detected-via lives natively in evidence.identity[].methods[]
    // on the CDX side per research R1. The C56 parity row reads
    // `evidence.identity[0].methods[*].waybill-source-mechanism`
    // (skipping the winner).
    let methods = abseil["evidence"]["identity"][0]["methods"]
        .as_array()
        .expect("evidence.identity[0].methods[] present");
    let losers: Vec<&str> = methods
        .iter()
        .skip(1)
        .filter_map(|m| m["waybill-source-mechanism"].as_str())
        .collect();
    assert_eq!(
        losers,
        vec!["git-submodule"],
        "expected git-submodule as the sole loser; got {losers:?}"
    );

    // Determinism: re-scan, byte-identical (after normalization)
    // to first run.
    let second_path = workdir.path().join("collision-2.cdx.json");
    let second = scan_fixture_cdx(&second_path);
    let first = scan_fixture_cdx(&out_path);
    assert_eq!(first, second, "non-deterministic SBOM bytes across runs");
}
