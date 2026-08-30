//! Milestone 669 T026 — CLI flag-parsing contract tests.
//!
//! Asserts that each `BenchArgs` flag wired for US1 parses under clap
//! and that `--per-fixture-timeout-sec` respects the [60, 3600] range
//! per contract xtask-bench-cli.md.
//!
//! Uses a test-local `Parser` wrapper because `main.rs`'s `Cli` isn't
//! re-exported through `xtask::bench` (it's binary-scope). The wrapper
//! `#[command(flatten)]`s `BenchArgs` so we exercise the same clap
//! derive-metadata the shipped binary uses.

use clap::Parser;
use std::path::PathBuf;

use xtask::bench::BenchArgs;

#[derive(Parser, Debug)]
#[command(name = "bench-flag-test", disable_help_flag = true)]
struct TestCli {
    #[command(flatten)]
    args: BenchArgs,
}

fn parse(argv: &[&str]) -> Result<TestCli, clap::Error> {
    // clap wants argv[0] to be the program name; prepend one.
    let mut full = Vec::with_capacity(argv.len() + 1);
    full.push("bench-flag-test");
    full.extend_from_slice(argv);
    TestCli::try_parse_from(full)
}

#[test]
fn no_flags_parses_with_all_optional_none() {
    let cli = parse(&[]).unwrap();
    assert!(cli.args.filter.is_empty());
    assert!(cli.args.output.is_none());
    assert!(cli.args.fixtures_dir.is_none());
    assert!(cli.args.per_fixture_timeout_sec.is_none());
    assert!(cli.args.baseline.is_none());
    assert!(cli.args.threshold.is_none());
    assert!(!cli.args.update_baseline);
    assert!(!cli.args.preflight_check);
}

#[test]
fn single_filter_parses() {
    let cli = parse(&["--filter", "cargo-*"]).unwrap();
    assert_eq!(cli.args.filter, vec!["cargo-*".to_string()]);
}

#[test]
fn multiple_filters_accumulate() {
    let cli = parse(&[
        "--filter",
        "cargo-*",
        "--filter",
        "debian-*",
        "--filter",
        "*-medium",
    ])
    .unwrap();
    assert_eq!(cli.args.filter.len(), 3);
    assert_eq!(cli.args.filter[0], "cargo-*");
    assert_eq!(cli.args.filter[1], "debian-*");
    assert_eq!(cli.args.filter[2], "*-medium");
}

#[test]
fn output_flag_parses_to_pathbuf() {
    let cli = parse(&["--output", "/tmp/run.json"]).unwrap();
    assert_eq!(cli.args.output, Some(PathBuf::from("/tmp/run.json")));
}

#[test]
fn fixtures_dir_flag_parses_to_pathbuf() {
    let cli = parse(&["--fixtures-dir", "/x/fixtures"]).unwrap();
    assert_eq!(cli.args.fixtures_dir, Some(PathBuf::from("/x/fixtures")));
}

#[test]
fn per_fixture_timeout_sec_accepts_minimum_60() {
    let cli = parse(&["--per-fixture-timeout-sec", "60"]).unwrap();
    assert_eq!(cli.args.per_fixture_timeout_sec, Some(60));
}

#[test]
fn per_fixture_timeout_sec_accepts_maximum_3600() {
    let cli = parse(&["--per-fixture-timeout-sec", "3600"]).unwrap();
    assert_eq!(cli.args.per_fixture_timeout_sec, Some(3600));
}

#[test]
fn per_fixture_timeout_sec_accepts_middle_value() {
    let cli = parse(&["--per-fixture-timeout-sec", "300"]).unwrap();
    assert_eq!(cli.args.per_fixture_timeout_sec, Some(300));
}

#[test]
fn per_fixture_timeout_sec_rejects_zero() {
    // Contract: range [60, 3600]. 0 is below floor.
    let err = parse(&["--per-fixture-timeout-sec", "0"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn per_fixture_timeout_sec_rejects_below_60() {
    let err = parse(&["--per-fixture-timeout-sec", "59"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn per_fixture_timeout_sec_rejects_above_3600() {
    let err = parse(&["--per-fixture-timeout-sec", "3601"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
}

#[test]
fn per_fixture_timeout_sec_rejects_non_integer() {
    let err = parse(&["--per-fixture-timeout-sec", "abc"]).unwrap_err();
    // Non-numeric hits either ValueValidation or InvalidValue depending
    // on clap version; both are the "user-visible rejection" contract.
    let kind = err.kind();
    assert!(
        matches!(
            kind,
            clap::error::ErrorKind::ValueValidation | clap::error::ErrorKind::InvalidValue
        ),
        "unexpected error kind: {kind:?}"
    );
}

// ─── US2/US4 flag surface (declared but unhandled in US1) ────────

#[test]
fn baseline_flag_parses_but_not_yet_wired() {
    // T024 explicitly declares --baseline for surface stability;
    // its logic lands in T031. Parsing must succeed here so callers
    // depending on the CLI surface don't break between US1 and US2.
    let cli = parse(&["--baseline", "docs/perf/baseline.json"]).unwrap();
    assert_eq!(
        cli.args.baseline,
        Some(PathBuf::from("docs/perf/baseline.json"))
    );
}

#[test]
fn threshold_requires_baseline() {
    // Contract xtask-bench-cli.md: --threshold is only accepted with --baseline.
    let err = parse(&["--threshold", "0.4"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn threshold_with_baseline_parses() {
    let cli = parse(&["--baseline", "b.json", "--threshold", "0.4"]).unwrap();
    assert_eq!(cli.args.threshold, Some(0.4));
}

#[test]
fn update_baseline_and_baseline_are_mutually_exclusive() {
    // Contract xtask-bench-cli.md: --update-baseline overrides --output,
    // and there's no need to also read a comparison baseline in the
    // same invocation. Enforce with clap `conflicts_with`.
    let err = parse(&["--update-baseline", "--baseline", "b.json"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn preflight_check_and_update_baseline_are_mutually_exclusive() {
    let err = parse(&["--preflight-check", "--update-baseline"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn preflight_check_and_baseline_are_mutually_exclusive() {
    let err = parse(&["--preflight-check", "--baseline", "b.json"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn preflight_check_alone_parses() {
    let cli = parse(&["--preflight-check"]).unwrap();
    assert!(cli.args.preflight_check);
}

#[test]
fn all_us1_wired_flags_together_parse() {
    // Kitchen-sink: every US1-wired flag in one invocation.
    let cli = parse(&[
        "--filter",
        "cargo-*",
        "--filter",
        "*-medium",
        "--output",
        "/tmp/run.json",
        "--fixtures-dir",
        "/x/fixtures",
        "--per-fixture-timeout-sec",
        "120",
    ])
    .unwrap();
    assert_eq!(cli.args.filter.len(), 2);
    assert_eq!(cli.args.output, Some(PathBuf::from("/tmp/run.json")));
    assert_eq!(cli.args.fixtures_dir, Some(PathBuf::from("/x/fixtures")));
    assert_eq!(cli.args.per_fixture_timeout_sec, Some(120));
}
