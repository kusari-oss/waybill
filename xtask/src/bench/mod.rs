// milestone 669 - see specs/669-bench-harness/plan.md
//
// Entry points for the `xtask bench` and `xtask bench-docs` subcommands.
//
// T024: BenchArgs clap struct — 8 flags declared per contract
//       xtask-bench-cli.md. Only 4 are wired for US1
//       (--filter/--output/--fixtures-dir/--per-fixture-timeout-sec);
//       the other 4 (--baseline/--threshold/--update-baseline/
//       --preflight-check) are declared-but-ignored until US2/US4.
// T025: run() stitches enumerate → run_matrix → JSON emit → Markdown
//       stdout summary. Atomic write via tempfile::NamedTempFile.

use std::error::Error;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;

use crate::bench::compare::compare;
use crate::bench::run::{RunConfig, read_fixture_pin_sha, read_waybill_commit_sha, run_matrix};
use crate::bench::schema::{BenchRun, ExitStatus, Fixture, RegressionDiff};

pub mod preflight;

pub mod compare;
pub mod docs;
pub mod matrix;
pub mod measure;
pub mod run;
pub mod schema;

/// CLI flags for `xtask bench`. Wire shape per contract
/// specs/669-bench-harness/contracts/xtask-bench-cli.md.
#[derive(Args, Debug, Clone, Default)]
pub struct BenchArgs {
    /// Glob-match fixture names (FR-006). Multiple flags = union.
    /// Empty match set is legal (exit 0).
    #[arg(long, value_name = "PATTERN", action = clap::ArgAction::Append)]
    pub filter: Vec<String>,

    /// Override the default `target/bench/run-<git-sha>.json` output.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Override the fixtures-cache location. Defaults to
    /// `$WAYBILL_FIXTURES_DIR` if set, else
    /// `~/.cache/waybill/fixtures/<pinned-sha>/`.
    #[arg(long, value_name = "PATH")]
    pub fixtures_dir: Option<PathBuf>,

    /// Per-fixture wall-clock cap in seconds. Range [60, 3600].
    /// Q3 default 300s (5 min).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u64).range(60..=3600))]
    pub per_fixture_timeout_sec: Option<u64>,

    // ─── US2 flags (declared for surface stability; wired in T031/T032) ───
    /// Compare against a baseline JSON; exit 1 on regression. Wired in T031.
    #[arg(long, value_name = "PATH")]
    pub baseline: Option<PathBuf>,

    /// Regression threshold as fraction (default 0.25). Wired in T031.
    #[arg(long, value_name = "FRACTION", requires = "baseline")]
    pub threshold: Option<f64>,

    /// Write to `docs/perf/baseline.json` instead of the default. Wired in T031.
    #[arg(long, conflicts_with_all = ["preflight_check", "baseline"])]
    pub update_baseline: bool,

    // ─── US4 flag (declared; wired in T041) ───
    /// Verify the committed baseline is not stale vs HEAD. Wired in T041.
    #[arg(long, conflicts_with_all = ["update_baseline", "baseline"])]
    pub preflight_check: bool,
}

/// Entry point for `cargo run -p xtask -- bench [...]`.
///
/// US1 slice (T025): enumerate matrix → run → write JSON → print
/// Markdown table → exit 0.
/// US2 slice (T031/T032): if `--baseline` is set, load the baseline
/// after the run, compute a `RegressionDiff` via
/// `bench::compare::compare`, write it to
/// `target/bench/regression-diff-<sha>.json`, print a Markdown table,
/// exit 1 if `regressions` is non-empty. `--update-baseline`
/// overrides the default output path to `docs/perf/baseline.json`.
/// US4 slice (T041): `--preflight-check` short-circuits the entire
/// bench-run path — reads the committed baseline, runs the R7 git-diff
/// staleness algorithm, exits 1 with a recovery-command diagnostic if
/// waybill-runtime code changed since baseline; exits 0 silently
/// otherwise.
pub fn run(args: BenchArgs) -> Result<(), Box<dyn Error>> {
    // T041: --preflight-check short-circuits everything else.
    // Mutually exclusive with --update-baseline + --baseline (enforced
    // by clap `conflicts_with_all` — no double-check needed here).
    if args.preflight_check {
        return handle_preflight_check();
    }

    let workspace_root = workspace_root_from_manifest();
    let waybill_sha = read_waybill_commit_sha()?;
    let fixture_sha = read_fixture_pin_sha(&workspace_root)?;

    let fixtures_dir = resolve_fixtures_dir(&args, &fixture_sha)?;
    let manifest_path = fixtures_dir.join("benchmark/manifest.json");

    let per_fixture_timeout = Duration::from_secs(args.per_fixture_timeout_sec.unwrap_or(300));

    let matrix = matrix::enumerate(&manifest_path, filter_arg(&args))?;
    if matrix.is_empty() {
        // Per contract C-2: empty match set is legal, not an error.
        eprintln!(
            "note: filter matched zero fixture-mode combinations — nothing to run."
        );
    }

    let cfg = RunConfig {
        fixtures_dir: fixtures_dir.clone(),
        waybill_bin: default_waybill_bin(&workspace_root),
        per_fixture_timeout,
        fingerprints_corpus: resolve_fingerprints_corpus(),
        waybill_sha: waybill_sha.clone(),
        fixture_sha,
    };

    let run_result = run_matrix(matrix, &cfg)?;

    // V5 (against-manifest) — the runner only self-validates (V1+V6);
    // cross-check the emitted results against the manifest here.
    let fixtures = Fixture::all_from_manifest(&manifest_path)?;
    run_result.validate_against_manifest(&fixtures)?;

    // T031: --update-baseline overrides --output to
    // <workspace-root>/docs/perf/baseline.json. Mutually exclusive with
    // --baseline (enforced by clap).
    let output_path = if args.update_baseline {
        workspace_root.join("docs/perf/baseline.json")
    } else {
        args.output
            .clone()
            .unwrap_or_else(|| default_output_path(&workspace_root, &waybill_sha))
    };
    write_run_atomically(&run_result, &output_path)?;

    println!("{}", render_markdown_table(&run_result));
    println!("\nWrote {}", output_path.display());

    // T032: --baseline drives regression detection. Load baseline,
    // compare, write RegressionDiff, print, exit 1 if regressions.
    if let Some(baseline_path) = &args.baseline {
        let threshold = args.threshold.unwrap_or(0.25);
        let baseline = load_baseline(baseline_path)?;
        let diff = compare(&run_result, &baseline, threshold);

        let diff_path = workspace_root
            .join("target/bench")
            .join(format!(
                "regression-diff-{}.json",
                waybill_sha.get(..12).unwrap_or(&waybill_sha)
            ));
        write_regression_diff_atomically(&diff, &diff_path)?;

        println!("\n{}", render_regression_diff_table(&diff));
        println!("\nWrote {}", diff_path.display());

        if !diff.regressions.is_empty() {
            eprintln!(
                "regression detected: {} dimension(s) crossed the {:.0}% threshold",
                diff.regressions.len(),
                diff.threshold * 100.0,
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

/// The `filter` field is a `Vec<String>` even when unused; convert
/// empty vec → None for the matrix enumerator.
fn filter_arg(args: &BenchArgs) -> Option<&Vec<String>> {
    if args.filter.is_empty() {
        None
    } else {
        Some(&args.filter)
    }
}

/// Resolve the fixtures-repo cache path per xtask-bench-cli.md flag
/// contract: `--fixtures-dir` > `$WAYBILL_FIXTURES_DIR` >
/// `$WAYBILL_FIXTURE_CACHE/<sha>/` > `~/.cache/waybill/fixtures/<sha>/`.
fn resolve_fixtures_dir(
    args: &BenchArgs,
    fixture_sha: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(explicit) = &args.fixtures_dir {
        return Ok(explicit.clone());
    }
    if let Ok(env) = std::env::var("WAYBILL_FIXTURES_DIR") {
        if !env.is_empty() {
            return Ok(PathBuf::from(env));
        }
    }
    let base = if let Ok(cache) = std::env::var("WAYBILL_FIXTURE_CACHE") {
        PathBuf::from(cache)
    } else {
        let home = std::env::var("HOME").map_err(|_| "HOME not set")?;
        PathBuf::from(home).join(".cache/waybill/fixtures")
    };
    Ok(base.join(fixture_sha))
}

/// Resolve the fingerprints-corpus cache path if reachable per m108
/// layout. None if the corpus isn't warm.
fn resolve_fingerprints_corpus() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let candidate = PathBuf::from(home)
        .join(".cache/waybill/fingerprints");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Locate the workspace root by walking up from `xtask/`'s manifest
/// dir. `CARGO_MANIFEST_DIR` for xtask is `<root>/xtask/`; parent is
/// the workspace root.
fn workspace_root_from_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Default output path per contract:
/// `<workspace-root>/target/bench/run-<waybill-sha-short>.json`.
fn default_output_path(workspace_root: &std::path::Path, waybill_sha: &str) -> PathBuf {
    let short = waybill_sha.get(..12).unwrap_or(waybill_sha);
    workspace_root
        .join("target/bench")
        .join(format!("run-{short}.json"))
}

/// Default waybill binary path — `target/release/waybill`. Assumes
/// the operator has run `cargo build --release -p waybill` before
/// invoking xtask bench. Absence surfaces later as a spawn error.
fn default_waybill_bin(workspace_root: &std::path::Path) -> PathBuf {
    workspace_root.join("target/release/waybill")
}

/// Load a persisted `BenchRun` (or baseline) from JSON. Applies V1
/// schema-version validation via `BenchRun::validate` so we fail-close
/// on future-schema baseline files rather than silently mis-comparing.
fn load_baseline(path: &std::path::Path) -> Result<BenchRun, Box<dyn Error>> {
    let bytes = std::fs::read(path).map_err(|e| -> Box<dyn Error> {
        format!("failed to read baseline at {}: {e}", path.display()).into()
    })?;
    let baseline: BenchRun = serde_json::from_slice(&bytes)?;
    baseline.validate()?;
    Ok(baseline)
}

/// Write a `RegressionDiff` atomically. Mirrors `write_run_atomically`
/// but takes the diff type — factoring out further would be premature.
fn write_regression_diff_atomically(
    diff: &RegressionDiff,
    path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let json = serde_json::to_vec_pretty(diff)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(&json)?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Markdown table summarizing a RegressionDiff — mirrors
/// `render_markdown_table` shape for the run itself. Empty-set
/// regressions are called out explicitly so operators know the
/// comparison ran and found nothing (as opposed to didn't run).
fn render_regression_diff_table(diff: &RegressionDiff) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "## Regression diff — subject `{}` vs baseline `{}` (threshold {:.0}%)\n\n",
        diff.subject_sha,
        diff.baseline_sha,
        diff.threshold * 100.0,
    ));
    if diff.regressions.is_empty() {
        s.push_str("### Regressions\n\n_none — threshold not breached in the worse direction._\n\n");
    } else {
        s.push_str("### Regressions\n\n");
        s.push_str("| fixture | mode | dimension | baseline | subject | delta |\n");
        s.push_str("|---|---|---|---:|---:|---:|\n");
        for e in &diff.regressions {
            s.push_str(&format!(
                "| {} | {:?} | {:?} | {} | {} | +{:.1}% |\n",
                e.fixture_name,
                e.mode,
                e.dimension,
                e.baseline_value,
                e.subject_value,
                e.percentage_delta * 100.0,
            ));
        }
        s.push('\n');
    }
    if !diff.improvements.is_empty() {
        s.push_str("### Improvements (informational)\n\n");
        s.push_str("| fixture | mode | dimension | baseline | subject | delta |\n");
        s.push_str("|---|---|---|---:|---:|---:|\n");
        for e in &diff.improvements {
            s.push_str(&format!(
                "| {} | {:?} | {:?} | {} | {} | {:.1}% |\n",
                e.fixture_name,
                e.mode,
                e.dimension,
                e.baseline_value,
                e.subject_value,
                e.percentage_delta * 100.0,
            ));
        }
        s.push('\n');
    }
    if !diff.matrix_asymmetry.is_empty() {
        s.push_str("### Matrix asymmetry\n\n");
        for a in &diff.matrix_asymmetry {
            s.push_str(&format!(
                "- `{}` [{:?}] — {:?}\n",
                a.fixture_name, a.mode, a.side,
            ));
        }
        s.push('\n');
    }
    s
}

/// Write `run` to `path` atomically via tempfile-in-same-dir +
/// `persist()`. Creates parent dirs if missing.
fn write_run_atomically(run: &BenchRun, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let json = serde_json::to_vec_pretty(run)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(&json)?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Render a compact Markdown table of the run's Results. One row per
/// (fixture, mode). Docs-page rendering (US3) is a richer surface;
/// this is just the stdout summary US1 asks for.
fn render_markdown_table(run: &BenchRun) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# xtask bench — {} results\n\n",
        run.results.len()
    ));
    s.push_str(&format!(
        "- waybill: `{}`\n- fixtures: `{}`\n- runner: `{}` ({:?})\n- started: {}\n- duration: {}s\n\n",
        run.metadata.waybill_commit_sha,
        run.metadata.fixture_sha,
        run.metadata.runner_uname,
        run.metadata.noise_class,
        run.metadata.started_at,
        run.metadata.total_duration_sec,
    ));
    s.push_str("| fixture | mode | median (ms) | peak RSS (KB) | output bytes | components | exit |\n");
    s.push_str("|---|---|---:|---:|---:|---:|---|\n");
    for r in &run.results {
        s.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} |\n",
            r.fixture_name,
            r.mode,
            r.median_wall_clock_ms,
            r.max_rss_kb,
            r.output_bytes,
            r.component_count,
            exit_status_str(&r.exit_status),
        ));
    }
    s
}

fn exit_status_str(s: &ExitStatus) -> &'static str {
    match s {
        ExitStatus::Success => "success",
        ExitStatus::Timeout => "timeout",
        ExitStatus::CorpusUnreachable => "corpus-unreachable",
        ExitStatus::NonZeroExitCode => "non-zero-exit-code",
        ExitStatus::SchemaParseError => "schema-parse-error",
    }
}

/// T041 driver — handles `xtask bench --preflight-check`. Reads the
/// committed baseline, runs the R7 staleness algorithm, exits 1 with
/// the C-5 diagnostic if waybill-runtime code changed since baseline,
/// exits 0 silently otherwise.
fn handle_preflight_check() -> Result<(), Box<dyn Error>> {
    let workspace_root = workspace_root_from_manifest();
    let baseline_path = workspace_root.join("docs/perf/baseline.json");
    // Contract C-5.1: refuse to run if baseline is missing; that's
    // the initial-bootstrap case handled by --update-baseline, not
    // pre-flight.
    if !baseline_path.exists() {
        return Err(format!(
            "docs/perf/baseline.json is missing; bootstrap it via \
             `cargo run -p xtask -- bench --update-baseline` before \
             running --preflight-check. (checked at {})",
            baseline_path.display()
        )
        .into());
    }
    let outcome = preflight::check(&baseline_path, &workspace_root)?;
    if outcome.is_stale {
        eprintln!("{}", preflight::format_diagnostic(&outcome));
        std::process::exit(1);
    }
    Ok(())
}

// ────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::{BenchResult, Mode, NoiseClass, RunMetadata};

    fn stub_run() -> BenchRun {
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
            results: vec![BenchResult {
                fixture_name: "cargo-workspace-medium".into(),
                mode: Mode::Default,
                median_wall_clock_ms: 300,
                max_rss_kb: 47280,
                output_bytes: 82734,
                component_count: 42,
                exit_status: ExitStatus::Success,
                waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
                fixture_sha: "1111111111111111111111111111111111111111".into(),
                raw_samples_ms: [280, 300, 300, 310, 320],
            }],
        }
    }

    #[test]
    fn render_markdown_table_includes_metadata_and_row_shape() {
        let s = render_markdown_table(&stub_run());
        assert!(s.contains("waybill: `abcdef0123456789abcdef0123456789abcdef01`"));
        assert!(s.contains("Reference"));
        assert!(s.contains("cargo-workspace-medium"));
        assert!(s.contains(" 300 "));
        assert!(s.contains(" 47280 "));
        assert!(s.contains(" 42 "));
        assert!(s.contains("success"));
        assert!(s.contains("| fixture | mode |"));
    }

    #[test]
    fn write_run_atomically_creates_parent_dirs_and_persists() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("nested/subdir/run.json");
        assert!(!out.exists());
        write_run_atomically(&stub_run(), &out).unwrap();
        assert!(out.exists());
        let contents = std::fs::read_to_string(&out).unwrap();
        // Round-trip: JSON we wrote is deserializable.
        let back: BenchRun = serde_json::from_str(&contents).unwrap();
        assert_eq!(back, stub_run());
    }

    #[test]
    fn default_output_path_uses_short_sha_prefix() {
        let root = std::path::Path::new("/x/workspace");
        let sha = "abcdef0123456789abcdef0123456789abcdef01";
        let p = default_output_path(root, sha);
        assert_eq!(
            p.to_str().unwrap(),
            "/x/workspace/target/bench/run-abcdef012345.json"
        );
    }

    #[test]
    fn default_output_path_short_sha_falls_back_on_short_input() {
        // Won't happen in practice (SHAs are 40 chars) but the fn
        // shouldn't panic on a shorter input.
        let root = std::path::Path::new("/x");
        let p = default_output_path(root, "abc");
        assert_eq!(p.to_str().unwrap(), "/x/target/bench/run-abc.json");
    }

    #[test]
    fn resolve_fixtures_dir_prefers_cli_flag() {
        let args = BenchArgs {
            fixtures_dir: Some(PathBuf::from("/explicit/path")),
            ..Default::default()
        };
        let p = resolve_fixtures_dir(&args, "does-not-matter").unwrap();
        assert_eq!(p, PathBuf::from("/explicit/path"));
    }

    #[test]
    fn filter_arg_returns_none_for_empty_vec() {
        let args = BenchArgs::default();
        assert!(filter_arg(&args).is_none());
    }

    #[test]
    fn filter_arg_returns_some_for_populated_vec() {
        let args = BenchArgs {
            filter: vec!["cargo-*".into()],
            ..Default::default()
        };
        let f = filter_arg(&args);
        assert!(f.is_some());
        assert_eq!(f.unwrap().len(), 1);
    }
}
