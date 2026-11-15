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
use common::git_fixture::GitFixture;
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

/// Run `waybill sbom scan` (CycloneDX) against `tree`, normalized as
/// [`scan_fixture_cdx`] does.
fn scan_tree_cdx(fx: &GitFixture, tree: &Path, out_path: &Path) -> Vec<u8> {
    let mut cmd = Command::new(bin());
    apply_fake_home_env(&mut cmd, &fx.home());
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .args(["--offline", "sbom", "scan", "--path"])
        .arg(tree)
        .args(["--format", "cyclonedx-json", "--output"])
        .arg(out_path);
    let status = cmd.status().expect("spawn waybill");
    assert!(status.success(), "waybill scan failed: {status:?}");
    let raw = std::fs::read_to_string(out_path).expect("read emitted SBOM");
    normalize_cdx_for_golden(&raw, &workspace_root()).into_bytes()
}

/// The `dedup_collision` fixture made a real repository: its files plus a
/// gitlink at `third_party/abseil`, which the in-tree copy cannot carry.
fn collision_repo(fx: &GitFixture) -> PathBuf {
    fx.repo("collision");
    let src = dedup_collision_fixture();
    for f in [".gitmodules", "CMakeLists.txt", "conanfile.txt"] {
        std::fs::copy(src.join(f), fx.path("collision").join(f)).expect("copy fixture file");
    }
    let tree = fx.path("collision");
    fx.git(&tree, &["add", ".gitmodules", "CMakeLists.txt", "conanfile.txt"]);
    fx.add_gitlink("collision", "third_party/abseil", ABSEIL_COMMIT);
    tree
}

const ABSEIL_COMMIT: &str = "4447c7562e3bc702ade25105912dce503f0c4010";

/// Spec 1530 FR-009 (#1515). Milestone 105 expected the Conan `abseil` and
/// the `abseil-cpp` submodule to merge after an unspecified
/// "canonicalization". There is none: a package-manager name and a
/// repository are different identities, so both components are emitted,
/// each with its own `waybill:source-mechanism`. C56 (also-detected-via) is
/// not asserted; it is not yet emitted (#1558).
#[test]
fn dedup_collision_conan_and_submodule_stay_two_components() {
    let fx = GitFixture::new();
    let tree = collision_repo(&fx);
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let out = workdir.path().join("collision.cdx.json");
    let bytes = scan_tree_cdx(&fx, &tree, &out);
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse CDX JSON");
    let mech = |c: &serde_json::Value| {
        c["properties"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["name"] == "waybill:source-mechanism"))
            .and_then(|p| p["value"].as_str())
            .map(str::to_string)
    };
    let mut abseil: Vec<(String, Option<String>)> = json["components"]
        .as_array()
        .expect("components")
        .iter()
        .filter(|c| {
            let p = c["purl"].as_str().unwrap_or("");
            p.starts_with("pkg:conan/abseil@") || p.starts_with("pkg:github/abseil/")
        })
        .map(|c| (c["purl"].as_str().unwrap_or("").to_string(), mech(c)))
        .collect();
    abseil.sort();
    assert_eq!(
        abseil,
        vec![
            ("pkg:conan/abseil@20240722.0".to_string(), Some("conan-recipe".to_string())),
            (format!("pkg:github/abseil/abseil-cpp@{ABSEIL_COMMIT}"), Some("git-submodule".to_string())),
        ]
    );
}

/// SC-005: ten scans of the collision repository are byte-identical.
#[test]
fn dedup_collision_repository_scans_deterministically() {
    let fx = GitFixture::new();
    let tree = collision_repo(&fx);
    let workdir = tempfile::tempdir().expect("workdir tempdir");
    let first = scan_tree_cdx(&fx, &tree, &workdir.path().join("run-0.cdx.json"));
    for i in 1..10 {
        let bytes = scan_tree_cdx(&fx, &tree, &workdir.path().join(format!("run-{i}.cdx.json")));
        assert!(first == bytes, "run {i} differs from run 0");
    }
}
