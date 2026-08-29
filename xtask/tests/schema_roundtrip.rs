//! Milestone 669 T011 — contract test per json-schema.md T1.
//!
//! Round-trips a hand-crafted `BenchRun` through serialize + deserialize;
//! asserts field-preservation AND that every field name appears verbatim
//! in the emitted JSON. Locks the wire representation externally — a PR
//! that adds `#[serde(rename)]` to any field will trip this test even if
//! the same-file unit tests happen to pass.

use xtask::bench::schema::{
    BenchResult, BenchRun, ExitStatus, Fixture, FixtureKind, Mode, NoiseClass,
    RunMetadata, ScanClass,
};

fn a_full_bench_run() -> BenchRun {
    BenchRun {
        schema_version: BenchRun::schema_version(),
        metadata: RunMetadata {
            waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            runner_uname: "Linux ci-runner 6.5.0-generic x86_64".into(),
            noise_class: NoiseClass::Reference,
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:15:00Z".into(),
            total_duration_sec: 900,
        },
        results: vec![
            BenchResult {
                fixture_name: "cargo-workspace-medium".into(),
                mode: Mode::Default,
                median_wall_clock_ms: 1523,
                max_rss_kb: 47280,
                output_bytes: 82734,
                component_count: 234,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [1500, 1510, 1523, 1540, 1600],
            },
            BenchResult {
                fixture_name: "debian-slim".into(),
                mode: Mode::TripleFormat,
                median_wall_clock_ms: 3421,
                max_rss_kb: 61230,
                output_bytes: 158420,
                component_count: 892,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "0000000000000000000000000000000000000000".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [3390, 3410, 3421, 3450, 3510],
            },
        ],
    }
}

#[test]
fn full_bench_run_survives_json_round_trip() {
    let run = a_full_bench_run();
    let json = serde_json::to_string(&run).expect("serialize");
    let back: BenchRun = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(run, back);
}

/// Locks every BenchRun / RunMetadata / BenchResult field name to its
/// wire representation. Any accidental `#[serde(rename)]` addition (or
/// field rename) trips this test even if the round-trip still passes.
#[test]
fn every_field_name_appears_verbatim_in_the_json() {
    let run = a_full_bench_run();
    let json = serde_json::to_string(&run).expect("serialize");

    // BenchRun root
    for name in ["schema_version", "metadata", "results"] {
        assert!(
            json.contains(&format!("\"{name}\"")),
            "BenchRun root missing wire-field name {name}. JSON was:\n{json}",
        );
    }

    // RunMetadata
    for name in [
        "waybill_commit_sha",
        "fixture_sha",
        "runner_uname",
        "noise_class",
        "started_at",
        "finished_at",
        "total_duration_sec",
    ] {
        assert!(
            json.contains(&format!("\"{name}\"")),
            "RunMetadata missing wire-field name {name}",
        );
    }

    // BenchResult
    for name in [
        "fixture_name",
        "mode",
        "median_wall_clock_ms",
        "max_rss_kb",
        "output_bytes",
        "component_count",
        "exit_status",
        "raw_samples_ms",
    ] {
        assert!(
            json.contains(&format!("\"{name}\"")),
            "BenchResult missing wire-field name {name}",
        );
    }
}

#[test]
fn nested_enum_wire_shapes_are_stable() {
    let run = a_full_bench_run();
    let json = serde_json::to_string(&run).expect("serialize");

    // Enum kebab-case wire shape spot-check
    assert!(json.contains("\"reference\""), "NoiseClass::Reference");
    assert!(json.contains("\"default\""), "Mode::Default");
    assert!(json.contains("\"triple-format\""), "Mode::TripleFormat");
    assert!(json.contains("\"success\""), "ExitStatus::Success");
}

#[test]
fn fixture_manifest_round_trips_too() {
    // Not part of BenchRun, but still schema-critical: Fixture is
    // read from the manifest.json in the fixtures repo.
    let f = Fixture {
        name: "cargo-workspace-medium".into(),
        path: "benchmark/source-tier/cargo-workspace-medium".into(),
        kind: FixtureKind::SourceTree,
        ecosystem: Some("cargo".into()),
        supported_modes: vec![
            Mode::Default,
            Mode::NoDeepHash,
            Mode::TripleFormat,
            Mode::NoDeepHashPlusTripleFormat,
        ],
        expected_scan_class: ScanClass::Medium,
    };
    let json = serde_json::to_string(&f).expect("serialize");
    let back: Fixture = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(f, back);

    // Wire-field names on Fixture
    for name in [
        "name",
        "path",
        "kind",
        "ecosystem",
        "supported_modes",
        "expected_scan_class",
    ] {
        assert!(
            json.contains(&format!("\"{name}\"")),
            "Fixture missing wire-field name {name}",
        );
    }
}
