//! Milestone 669 T038 — Contract test for `xtask bench-docs`
//! determinism (contract xtask-bench-cli.md § C-7).
//!
//! Runs `generate_markdown` twice against the same baseline and
//! asserts byte-identical output. Enforces the "pure function of
//! `baseline`" contract — no wall-clock, no environment reads, no
//! timestamps sourced outside the input.
//!
//! Also runs the full `run(args)` path via `--dry-run` twice with
//! the disk-baseline load path, redirecting stdout via a shell
//! pipe, to lock the CLI-integration surface too (not just the
//! function).

use xtask::bench::docs::{generate_markdown, run, BenchDocsArgs};
use xtask::bench::schema::{BenchResult, BenchRun, ExitStatus, Mode, NoiseClass, RunMetadata};

fn stub_baseline() -> BenchRun {
    BenchRun {
        schema_version: BenchRun::schema_version(),
        metadata: RunMetadata {
            waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            runner_uname: "Linux ci 6.5.0 x86_64".into(),
            noise_class: NoiseClass::Reference,
            started_at: "2026-08-29T00:00:00Z".into(),
            finished_at: "2026-08-29T00:15:00Z".into(),
            total_duration_sec: 900,
        },
        results: vec![
            BenchResult {
                fixture_name: "cargo-workspace-medium".into(),
                mode: Mode::Default,
                median_wall_clock_ms: 325,
                max_rss_kb: 18944,
                output_bytes: 13729,
                component_count: 6,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [325, 325, 325, 325, 325],
            },
            BenchResult {
                fixture_name: "cargo-workspace-medium".into(),
                mode: Mode::TripleFormat,
                median_wall_clock_ms: 328,
                max_rss_kb: 18960,
                output_bytes: 70122,
                component_count: 6,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [328, 328, 328, 328, 328],
            },
            BenchResult {
                fixture_name: "npm-monorepo-medium".into(),
                mode: Mode::Default,
                median_wall_clock_ms: 412,
                max_rss_kb: 21000,
                output_bytes: 15211,
                component_count: 9,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [412, 412, 412, 412, 412],
            },
        ],
    }
}

#[test]
fn generate_markdown_byte_identical_across_invocations() {
    let b = stub_baseline();
    let a = generate_markdown(&b);
    let a2 = generate_markdown(&b);
    let a3 = generate_markdown(&b);
    assert_eq!(a, a2);
    assert_eq!(a, a3);
}

#[test]
fn generate_markdown_output_is_deterministic_after_serde_roundtrip() {
    // Round-trip the baseline through JSON before rendering — the
    // Serialize/Deserialize path could reorder Vec entries if fields
    // used HashMap. Locks that regression away.
    let b = stub_baseline();
    let a = generate_markdown(&b);

    let json = serde_json::to_string(&b).unwrap();
    let b2: BenchRun = serde_json::from_str(&json).unwrap();
    let a2 = generate_markdown(&b2);

    assert_eq!(a, a2);
}

#[test]
fn generate_markdown_output_is_stable_when_results_are_reordered() {
    // The Markdown grouping uses a BTreeMap keyed by fixture_name
    // and sorts modes by their Debug string, so the output is
    // invariant to the order of `baseline.results` — verify.
    let mut b1 = stub_baseline();
    let mut b2 = stub_baseline();
    b2.results.reverse(); // Vec order flipped; grouping should absorb.
    // Add a duplicate-mode-different-fixture entry in both, differently
    // ordered, just for extra churn.
    b1.results.push(BenchResult {
        fixture_name: "aardvark-fixture".into(),
        mode: Mode::Default,
        median_wall_clock_ms: 100,
        max_rss_kb: 10_000,
        output_bytes: 500,
        component_count: 1,
        exit_status: ExitStatus::Success,
        waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
        fixture_sha: "1111111111111111111111111111111111111111".into(),
        raw_samples_ms: [100, 100, 100, 100, 100],
    });
    b2.results.insert(
        0,
        BenchResult {
            fixture_name: "aardvark-fixture".into(),
            mode: Mode::Default,
            median_wall_clock_ms: 100,
            max_rss_kb: 10_000,
            output_bytes: 500,
            component_count: 1,
            exit_status: ExitStatus::Success,
            waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            raw_samples_ms: [100, 100, 100, 100, 100],
        },
    );

    let a = generate_markdown(&b1);
    let a2 = generate_markdown(&b2);
    assert_eq!(a, a2);
}

#[test]
fn run_writes_deterministic_bytes_to_disk_across_two_invocations() {
    // Full CLI path: write the baseline to disk, invoke `run()`
    // twice with `--output <path>`, assert the two written files
    // are byte-identical.
    let tmp = tempfile::tempdir().unwrap();
    let baseline_path = tmp.path().join("baseline.json");
    let b = stub_baseline();
    std::fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&b).unwrap(),
    )
    .unwrap();

    let out1 = tmp.path().join("numbers-1.md");
    let out2 = tmp.path().join("numbers-2.md");
    run(BenchDocsArgs {
        baseline: Some(baseline_path.clone()),
        output: Some(out1.clone()),
        dry_run: false,
    })
    .unwrap();
    run(BenchDocsArgs {
        baseline: Some(baseline_path),
        output: Some(out2.clone()),
        dry_run: false,
    })
    .unwrap();

    let a = std::fs::read_to_string(&out1).unwrap();
    let b_ = std::fs::read_to_string(&out2).unwrap();
    assert_eq!(a, b_);
}
