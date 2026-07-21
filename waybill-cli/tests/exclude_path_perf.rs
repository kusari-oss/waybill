//! Milestone 118 (#343 / FR-011) — opt-in perf benchmark asserting that
//! `--exclude-path` overhead stays within the SC-006 budget (≤1.10× the
//! no-flag baseline) on a polyglot fixture.
//!
//! Per data-model.md § Entity 4 + contracts/perf-bench.md:
//! - `#[ignore]`d in default `cargo test` invocation
//! - Opt-in via `cargo +stable test --test exclude_path_perf -- --ignored`
//! - Median-of-5 sampling per condition
//! - Linux: strict assertion; macOS: measurement-only print (milestone-094
//!   thermal-noise rationale)
//!
//! The spec clarification Q1 named `kusari-cli` as the target fixture;
//! the milestone-090 cache shipped to local hosts ships `polyglot-monorepo`
//! (npm + pip, multi-ecosystem) as the polyglot equivalent. Either works
//! — what matters is multi-ecosystem walker exercise + a deterministic
//! `**/testdata`-style exclusion target. We use `polyglot-monorepo` per
//! what's actually cached on every dev/CI host.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("WAYBILL_FIXTURES_DIR")).join("polyglot-monorepo")
}

fn time_scan(fixture: &Path, exclude_paths: &[&str]) -> Duration {
    let bin = env!("CARGO_BIN_EXE_mikebom");
    let mut cmd = Command::new(bin);
    cmd.arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(fixture)
        .arg("--no-deep-hash")
        .arg("--output")
        .arg("/dev/null")
        .env("WAYBILL_FIXED_TIMESTAMP", "2026-01-01T00:00:00Z")
        .env("RUST_LOG", "warn")
        .env_remove("WAYBILL_EXCLUDE_PATH");
    for entry in exclude_paths {
        cmd.arg("--exclude-path").arg(entry);
    }
    let start = Instant::now();
    let output = cmd.output().expect("waybill should run");
    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    elapsed
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "wall-clock perf test — opt in via `cargo test -- --ignored`; runs strict on Linux per milestone-094"]
fn exclude_path_does_not_exceed_1_10x_baseline() {
    let fixture = fixture_root();
    assert!(
        fixture.is_dir(),
        "polyglot-monorepo fixture not present at {} — milestone-090 fixture cache may need refresh",
        fixture.display()
    );

    // Warm-up: one untimed scan so the OS page cache + linker dynamic
    // loader state aren't measured as part of the first sample.
    let _ = time_scan(&fixture, &[]);

    let baseline_samples: Vec<Duration> =
        (0..5).map(|_| time_scan(&fixture, &[])).collect();
    let excluded_samples: Vec<Duration> = (0..5)
        .map(|_| time_scan(&fixture, &["**/testdata"]))
        .collect();
    let baseline_median = median(baseline_samples);
    let excluded_median = median(excluded_samples);

    let ratio = excluded_median.as_secs_f64() / baseline_median.as_secs_f64();
    eprintln!(
        "exclude_path_perf measurement: baseline_median={:?} excluded_median={:?} ratio={:.3}",
        baseline_median, excluded_median, ratio,
    );

    if cfg!(target_os = "macos") {
        eprintln!(
            "(macOS lane: strict assertion skipped per milestone-094 thermal-noise rationale)"
        );
        return;
    }

    let max_allowed = baseline_median.mul_f64(1.10);
    assert!(
        excluded_median <= max_allowed,
        "perf: --exclude-path scan ({:?}) exceeded 1.10× baseline ({:?}); ratio={:.3}",
        excluded_median,
        baseline_median,
        ratio,
    );
}
