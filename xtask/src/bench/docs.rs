// milestone 669 - see specs/669-bench-harness/plan.md
// bench-docs: deterministic Markdown emission from the committed
// baseline. Every quoted perf number cites its fixture-SHA +
// waybill-SHA (SC-006 anchor).
//
// T035: generate_markdown(baseline) — pure-function Markdown builder.
// T036: BenchDocsArgs clap struct (--baseline, --output, --dry-run).
// T037: run() reads baseline via V1 schema-version gate, dispatches
//       between --dry-run (print) and default (atomic write).

use std::error::Error;
use std::io::Write;
use std::path::PathBuf;

use clap::Args;

use crate::bench::schema::{BenchResult, BenchRun, ExitStatus, Mode};

/// CLI flags for `xtask bench-docs`. Wire shape per contract
/// specs/669-bench-harness/contracts/xtask-bench-cli.md § `bench-docs`.
#[derive(Args, Debug, Clone, Default)]
pub struct BenchDocsArgs {
    /// Read from a non-default baseline location. Default:
    /// `docs/perf/baseline.json` under the workspace root.
    #[arg(long, value_name = "PATH")]
    pub baseline: Option<PathBuf>,

    /// Write to a non-default output location. Default:
    /// `docs/perf/numbers.md` under the workspace root.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Print what would be written to stdout instead of touching disk.
    #[arg(long)]
    pub dry_run: bool,
}

/// Runs the docs-generation subcommand. Contract:
/// - reads baseline JSON (V1 schema-version gate applied via
///   `BenchRun::validate` inside the loader — future-schema files are
///   rejected fail-close),
/// - calls `generate_markdown`,
/// - writes to `--output` (default `docs/perf/numbers.md`) atomically,
/// - or prints to stdout with `--dry-run`.
pub fn run(args: BenchDocsArgs) -> Result<(), Box<dyn Error>> {
    let workspace_root = workspace_root_from_manifest();
    let baseline_path = args
        .baseline
        .clone()
        .unwrap_or_else(|| workspace_root.join("docs/perf/baseline.json"));
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| workspace_root.join("docs/perf/numbers.md"));

    let baseline = load_baseline(&baseline_path)?;
    let markdown = generate_markdown(&baseline);

    if args.dry_run {
        print!("{markdown}");
        return Ok(());
    }

    write_markdown_atomically(&markdown, &output_path)?;
    println!("Wrote {}", output_path.display());
    Ok(())
}

/// Render the baseline as Markdown per T035 spec:
/// - Title
/// - Generation-context block (waybill-SHA, fixture-SHA, runner class)
/// - Reference-architecture note
/// - Per-fixture section (grouped by fixture_name)
/// - Per-mode table row-per-mode with the 4 measured dimensions +
///   both SHAs on every row
///
/// The output is a pure function of `baseline` — no wall-clock, no
/// environment reads, no timestamps. This is T038's C-7 anchor
/// (repeatability across invocations).
pub fn generate_markdown(baseline: &BenchRun) -> String {
    let mut s = String::new();

    // Title.
    s.push_str("# waybill perf numbers\n\n");

    // Generation context — everything sourced from the baseline itself
    // so this stays pure (T038).
    s.push_str(&format!(
        "Generated from `docs/perf/baseline.json` captured at:\n\
         \n\
         - **waybill commit**: `{}`\n\
         - **fixtures pin**: `{}`\n\
         - **runner**: `{}` (noise class: `{:?}`)\n\
         - **duration**: {}s\n\
         - **schema version**: {}\n\n",
        baseline.metadata.waybill_commit_sha,
        baseline.metadata.fixture_sha,
        baseline.metadata.runner_uname,
        baseline.metadata.noise_class,
        baseline.metadata.total_duration_sec,
        baseline.schema_version,
    ));

    // Reference-architecture note. SC-002 pins docs citations to
    // Linux x86_64 GHA class.
    s.push_str(
        "## Reference architecture\n\
         \n\
         Numbers below reflect the Linux x86_64 GitHub-hosted-runner\n\
         class per waybill spec 669 Assumption 1. Cross-host projections\n\
         are deferred to a future milestone; use these numbers as an\n\
         upper-bound reference on quieter hardware and expect drift on\n\
         macOS runners (m094 noise-class = `Noisy`).\n\n",
    );

    // Group results by fixture_name. BTreeMap gives stable iteration
    // for deterministic output.
    let mut by_fixture: std::collections::BTreeMap<&str, Vec<&BenchResult>> =
        std::collections::BTreeMap::new();
    for r in &baseline.results {
        by_fixture.entry(r.fixture_name.as_str()).or_default().push(r);
    }

    if by_fixture.is_empty() {
        s.push_str("## Results\n\n_baseline contains no results._\n");
        return s;
    }

    // Per-fixture section.
    for (fixture_name, results) in by_fixture {
        s.push_str(&format!("## `{fixture_name}`\n\n"));
        s.push_str(
            "| mode | median wall-clock (ms) | peak RSS (KB) | output bytes | components | exit | fixture-sha | waybill-sha |\n\
             |---|---:|---:|---:|---:|---|---|---|\n",
        );
        // Sort modes deterministically by their Debug-name for stable
        // row ordering across invocations.
        let mut rs = results;
        rs.sort_by_key(|r| format!("{:?}", r.mode));
        for r in rs {
            s.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | `{}` | `{}` |\n",
                mode_label(&r.mode),
                r.median_wall_clock_ms,
                r.max_rss_kb,
                r.output_bytes,
                r.component_count,
                exit_status_label(&r.exit_status),
                r.fixture_sha,
                r.waybill_commit_sha,
            ));
        }
        s.push('\n');
    }

    // Footer footnote — matches T035 "generation-date footer" ask by
    // sourcing the timestamp from the baseline's own captured_at
    // (rather than wall-clock at doc-gen time — which would break
    // T038's byte-identity contract).
    s.push_str(&format!(
        "---\n\
         \n\
         _Baseline captured at {} ({}s). Regenerate this page after\n\
         each `docs/perf/baseline.json` refresh via\n\
         `cargo run -p xtask -- bench-docs`._\n",
        baseline.metadata.started_at, baseline.metadata.total_duration_sec,
    ));

    s
}

fn mode_label(m: &Mode) -> &'static str {
    match m {
        Mode::Default => "default",
        Mode::NoDeepHash => "no-deep-hash",
        Mode::TripleFormat => "triple-format",
        Mode::NoDeepHashPlusTripleFormat => "no-deep-hash-plus-triple-format",
        Mode::FingerprintsCorpus => "fingerprints-corpus",
    }
}

fn exit_status_label(s: &ExitStatus) -> &'static str {
    match s {
        ExitStatus::Success => "success",
        ExitStatus::Timeout => "timeout",
        ExitStatus::CorpusUnreachable => "corpus-unreachable",
        ExitStatus::NonZeroExitCode => "non-zero-exit-code",
        ExitStatus::SchemaParseError => "schema-parse-error",
    }
}

fn load_baseline(path: &std::path::Path) -> Result<BenchRun, Box<dyn Error>> {
    let bytes = std::fs::read(path).map_err(|e| -> Box<dyn Error> {
        format!("failed to read baseline at {}: {e}", path.display()).into()
    })?;
    let baseline: BenchRun = serde_json::from_slice(&bytes)?;
    baseline.validate()?;
    Ok(baseline)
}

fn write_markdown_atomically(md: &str, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(md.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn workspace_root_from_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

// ────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::{NoiseClass, RunMetadata};

    fn stub_run(results: Vec<BenchResult>) -> BenchRun {
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
            results,
        }
    }

    fn stub_result(fixture: &str, mode: Mode, wall: u64) -> BenchResult {
        BenchResult {
            fixture_name: fixture.into(),
            mode,
            median_wall_clock_ms: wall,
            max_rss_kb: 47280,
            output_bytes: 82734,
            component_count: 42,
            exit_status: ExitStatus::Success,
            waybill_commit_sha: "abcdef0123456789abcdef0123456789abcdef01".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
            raw_samples_ms: [wall, wall, wall, wall, wall],
        }
    }

    #[test]
    fn generate_markdown_is_pure_function() {
        // C-7 anchor: same input → byte-identical output across
        // repeated invocations (no wall-clock, no environment reads).
        let run = stub_run(vec![
            stub_result("cargo-workspace-medium", Mode::Default, 300),
            stub_result("cargo-workspace-medium", Mode::NoDeepHash, 280),
        ]);
        let a = generate_markdown(&run);
        let b = generate_markdown(&run);
        assert_eq!(a, b);
    }

    #[test]
    fn generate_markdown_cites_both_shas_per_result_row() {
        // SC-006 anchor: every quoted number cites fixture-SHA + waybill-SHA.
        let run = stub_run(vec![stub_result("x", Mode::Default, 300)]);
        let md = generate_markdown(&run);
        // Each result row appears once with backticked SHAs.
        let fixture_hits = md.matches("`1111111111111111111111111111111111111111`").count();
        let waybill_hits = md.matches("`abcdef0123456789abcdef0123456789abcdef01`").count();
        // At least one per row (result table) + generation-context block.
        assert!(fixture_hits >= 2, "fixture-sha not cited enough: {fixture_hits}");
        assert!(waybill_hits >= 2, "waybill-sha not cited enough: {waybill_hits}");
    }

    #[test]
    fn generate_markdown_groups_by_fixture_deterministically() {
        // Two fixtures added in reverse-alphabetical order; output
        // must present them alphabetically (BTreeMap iteration).
        let run = stub_run(vec![
            stub_result("z-fixture", Mode::Default, 300),
            stub_result("a-fixture", Mode::Default, 300),
        ]);
        let md = generate_markdown(&run);
        let a_pos = md.find("## `a-fixture`").expect("a-fixture missing");
        let z_pos = md.find("## `z-fixture`").expect("z-fixture missing");
        assert!(a_pos < z_pos, "expected alphabetical fixture order");
    }

    #[test]
    fn generate_markdown_orders_modes_deterministically_within_fixture() {
        // Two modes added in reverse order → deterministic-by-Debug-name output.
        let run = stub_run(vec![
            stub_result("f", Mode::NoDeepHash, 300),
            stub_result("f", Mode::Default, 300),
        ]);
        let md = generate_markdown(&run);
        let default_pos = md.find("| `default` |").expect("default row missing");
        let no_deep_pos = md.find("| `no-deep-hash` |").expect("no-deep-hash row missing");
        assert!(default_pos < no_deep_pos, "expected Default before NoDeepHash");
    }

    #[test]
    fn generate_markdown_empty_results_shows_placeholder() {
        let run = stub_run(vec![]);
        let md = generate_markdown(&run);
        assert!(md.contains("_baseline contains no results._"));
    }

    #[test]
    fn generate_markdown_includes_reference_architecture_section() {
        let run = stub_run(vec![stub_result("x", Mode::Default, 300)]);
        let md = generate_markdown(&run);
        assert!(md.contains("## Reference architecture"));
        assert!(md.contains("Linux x86_64"));
    }

    #[test]
    fn run_dry_run_does_not_touch_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let baseline_path = tmp.path().join("baseline.json");
        let run = stub_run(vec![stub_result("x", Mode::Default, 300)]);
        std::fs::write(&baseline_path, serde_json::to_string_pretty(&run).unwrap()).unwrap();

        let output_path = tmp.path().join("nested/should-not-exist.md");
        let args = BenchDocsArgs {
            baseline: Some(baseline_path),
            output: Some(output_path.clone()),
            dry_run: true,
        };
        super::run(args).unwrap();
        assert!(!output_path.exists(), "dry-run must not write to disk");
    }

    #[test]
    fn run_writes_output_to_disk_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let baseline_path = tmp.path().join("baseline.json");
        let run = stub_run(vec![stub_result("x", Mode::Default, 300)]);
        std::fs::write(&baseline_path, serde_json::to_string_pretty(&run).unwrap()).unwrap();

        let output_path = tmp.path().join("nested/numbers.md");
        let args = BenchDocsArgs {
            baseline: Some(baseline_path),
            output: Some(output_path.clone()),
            dry_run: false,
        };
        super::run(args).unwrap();
        assert!(output_path.exists());
        let contents = std::fs::read_to_string(&output_path).unwrap();
        assert!(contents.contains("# waybill perf numbers"));
    }

    #[test]
    fn run_load_baseline_rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let args = BenchDocsArgs {
            baseline: Some(tmp.path().join("does-not-exist.json")),
            output: Some(tmp.path().join("out.md")),
            dry_run: true,
        };
        assert!(super::run(args).is_err());
    }

    #[test]
    fn run_load_baseline_rejects_future_schema_version() {
        // V1 fail-close: a baseline with schema_version=2 must be
        // refused rather than silently mis-compared.
        let tmp = tempfile::tempdir().unwrap();
        let baseline_path = tmp.path().join("future.json");
        let mut run = stub_run(vec![]);
        run.schema_version = 2;
        std::fs::write(&baseline_path, serde_json::to_string_pretty(&run).unwrap()).unwrap();
        let args = BenchDocsArgs {
            baseline: Some(baseline_path),
            output: Some(tmp.path().join("out.md")),
            dry_run: true,
        };
        let err = super::run(args).unwrap_err();
        assert!(err.to_string().contains("V1 violation"));
    }
}
