// milestone 669 - see specs/669-bench-harness/plan.md
// Per-fixture-mode driver (T022) + full-matrix orchestration (T023).
//
// T022: run_one_fixture(fixture, mode, cfg) — 1 warmup + 5 timed
//       passes via measure_one; median = sorted[2]; validates BenchResult.
// T023: run_matrix(matrix, cfg) — iterates matrix sequentially, fills
//       RunMetadata (waybill SHA, fixture SHA, uname, noise-class,
//       started/finished timestamps, total duration).
//
// Design decisions:
// - RunConfig owns pre-computed waybill + fixture SHAs so the runner
//   isn't invoking git rev-parse once per fixture. CLI wiring (T025)
//   populates them.
// - Per-invocation output files land in a caller-scoped tempdir so
//   they don't accumulate under the operator's home dir across many
//   passes. Files are dropped when the tempdir is dropped.
// - waybill CLI shape mirrors the m011 sbom-emission surface:
//   `waybill sbom scan --path <fixture> [--no-deep-hash]
//     --format cyclonedx-json --output <cdx> [--format spdx-2.3-json
//     --output <spdx2>] [--format spdx-3-json --output <spdx3>]
//     [--fingerprints-corpus <path>]`.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::bench::measure::{measure_one, parse_output_metadata, OutputMeta};
use crate::bench::schema::{
    BenchResult, BenchRun, ExitStatus, Fixture, FixtureKind, Mode, NoiseClass, RunMetadata,
};

/// Runner-scoped configuration threaded through every fixture-mode
/// invocation. Populated by the CLI wiring (T025) before `run_matrix`
/// is called.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Root of the fixtures-repo cache — e.g.,
    /// `~/.cache/waybill/fixtures/<sha>/`. Fixture `path` fields are
    /// interpreted relative to this directory.
    pub fixtures_dir: PathBuf,
    /// Path to the `waybill` binary — typically `target/release/waybill`
    /// after `cargo build --release -p waybill`.
    pub waybill_bin: PathBuf,
    /// Per-fixture (per-invocation) wall-clock cap. On timeout, the
    /// child is SIGKILLed and the sample's `exit_status` becomes
    /// `Timeout`. Q3 default: 5 minutes.
    pub per_fixture_timeout: Duration,
    /// Optional path to the fingerprints-corpus cache used by the
    /// `FingerprintsCorpus` mode (`--fingerprints-corpus <path>`).
    /// `None` means the corpus is unreachable → those modes emit
    /// `ExitStatus::CorpusUnreachable` per spec Assumption 9.
    pub fingerprints_corpus: Option<PathBuf>,
    /// waybill commit SHA — output of `git rev-parse HEAD` in the
    /// waybill main-repo working tree at run start. Non-empty 40-char
    /// lowercase hex per V4.
    pub waybill_sha: String,
    /// Fixtures-repo pin SHA — the value in `tests/fixtures.rev`
    /// (== the directory-name tail of `fixtures_dir` under the
    /// default m090 cache layout). Non-empty 40-char lowercase hex.
    pub fixture_sha: String,
}

// ────────────────────────────────────────────────────────────────
// T022 — run_one_fixture
// ────────────────────────────────────────────────────────────────

/// Run one `(fixture, mode)` combination — 1 warmup + 5 timed passes,
/// take the median wall-clock, parse the last invocation's SBOM
/// output files for `output_bytes` + `component_count`, construct
/// `BenchResult`, validate, return.
///
/// Failure semantics: any invocation whose `exit_status` is not
/// `Success` propagates into the returned `BenchResult.exit_status`
/// (first non-success wins). The median wall-clock is still computed
/// over all 5 samples — a partially-failed fixture is measurable but
/// flagged, not silently dropped.
pub fn run_one_fixture(
    fixture: &Fixture,
    mode: Mode,
    cfg: &RunConfig,
) -> Result<BenchResult, Box<dyn Error>> {
    // Handle FingerprintsCorpus with an unreachable corpus explicitly
    // per spec Assumption 9 — synthesize a Result, don't attempt to
    // spawn waybill without the flag.
    if matches!(mode, Mode::FingerprintsCorpus) && cfg.fingerprints_corpus.is_none() {
        return Ok(unreachable_corpus_result(fixture, mode, cfg));
    }

    let target = cfg.fixtures_dir.join(&fixture.path);
    let scratch = tempfile::tempdir()?;

    let build_cmd = || -> (Command, Vec<PathBuf>) { build_waybill_cmd(fixture, mode, cfg, &target, scratch.path()) };

    // Warmup — result discarded, but we still enforce the timeout.
    let (warmup_cmd, _) = build_cmd();
    let _ = measure_one(warmup_cmd, cfg.per_fixture_timeout)?;

    // 5 timed passes. Track first non-success + max RSS across samples
    // + last invocation's output paths for post-hoc metadata parse.
    let mut samples: [u64; 5] = [0; 5];
    let mut peak_rss_kb: u64 = 0;
    let mut first_failure: Option<ExitStatus> = None;
    let mut last_output_paths: Vec<PathBuf> = Vec::new();

    for slot in samples.iter_mut() {
        let (cmd, out_paths) = build_cmd();
        let sample = measure_one(cmd, cfg.per_fixture_timeout)?;
        if sample.exit_status != ExitStatus::Success && first_failure.is_none() {
            first_failure = Some(sample.exit_status);
        }
        *slot = sample.wall_clock_ms;
        if sample.max_rss_kb > peak_rss_kb {
            peak_rss_kb = sample.max_rss_kb;
        }
        last_output_paths = out_paths;
    }

    let median = median_of_5(samples);

    let (total_output_bytes, component_count) = aggregate_output_metadata(&last_output_paths);

    let exit_status = first_failure.unwrap_or(ExitStatus::Success);

    let result = BenchResult {
        fixture_name: fixture.name.clone(),
        mode,
        median_wall_clock_ms: median,
        max_rss_kb: peak_rss_kb,
        output_bytes: total_output_bytes,
        component_count,
        exit_status,
        waybill_commit_sha: cfg.waybill_sha.clone(),
        fixture_sha: cfg.fixture_sha.clone(),
        raw_samples_ms: samples,
    };

    // V3 + V4 asserted here — refuses to emit malformed data.
    result.validate()?;
    Ok(result)
}

/// Construct the waybill CLI Command for a given fixture-mode pair.
/// Returns the Command plus the list of output-file paths so the
/// caller can parse them post-run.
fn build_waybill_cmd(
    fixture: &Fixture,
    mode: Mode,
    cfg: &RunConfig,
    target: &Path,
    scratch: &Path,
) -> (Command, Vec<PathBuf>) {
    let mut c = Command::new(&cfg.waybill_bin);
    c.arg("sbom").arg("scan");
    // Perf-baseline: `--offline` disables EVERY network path — deps.dev
    // license + dep-graph enrichment, ClearlyDefined, and (critically)
    // the Go graph resolver's proxy.golang.org fetches for transitive
    // module edges. Measured 2026-08-31 on go-module-medium: 17.7s
    // with `--no-deps-dev` (which leaves the Go proxy fetch alive)
    // vs 56ms with `--offline` — a 315× drop for identical component
    // output (72 components in both). Any network path in the scan
    // pipeline turns the baseline into a measurement of the remote
    // service's response time rather than waybill's code cost, which
    // defeats the point of a regression-detection harness. `--offline`
    // is the single catch-all flag propagating through every enrich
    // source + the golang graph resolver's proxy_fetch gate.
    c.arg("--offline");

    match fixture.kind {
        FixtureKind::SourceTree | FixtureKind::BinarySet => {
            c.arg("--path").arg(target);
        }
        FixtureKind::ContainerImage => {
            c.arg("--image").arg(target);
        }
    }

    let no_deep_hash = matches!(
        mode,
        Mode::NoDeepHash | Mode::NoDeepHashPlusTripleFormat
    );
    if no_deep_hash {
        c.arg("--no-deep-hash");
    }

    let triple = matches!(
        mode,
        Mode::TripleFormat | Mode::NoDeepHashPlusTripleFormat
    );

    let mut out_paths = Vec::new();
    if triple {
        // Multi-format emission: waybill rejects bare `--output <path>`
        // when more than one format is requested. Per-format form is
        // `--output <fmt>=<path>` (verified against
        // `waybill sbom scan --help` output — see the "--output" doc).
        let cdx = scratch.join("out.cdx.json");
        let spdx2 = scratch.join("out.spdx.json");
        let spdx3 = scratch.join("out.spdx3.json");
        c.arg("--format").arg("cyclonedx-json");
        c.arg("--format").arg("spdx-2.3-json");
        c.arg("--format").arg("spdx-3-json");
        c.arg("--output").arg(format!("cyclonedx-json={}", cdx.display()));
        c.arg("--output").arg(format!("spdx-2.3-json={}", spdx2.display()));
        c.arg("--output").arg(format!("spdx-3-json={}", spdx3.display()));
        out_paths.push(cdx);
        out_paths.push(spdx2);
        out_paths.push(spdx3);
    } else {
        // Single format: bare `--output <path>` is the accepted form.
        let cdx = scratch.join("out.cdx.json");
        c.arg("--format").arg("cyclonedx-json").arg("--output").arg(&cdx);
        out_paths.push(cdx);
    }

    if matches!(mode, Mode::FingerprintsCorpus) {
        if let Some(corpus) = &cfg.fingerprints_corpus {
            c.arg("--fingerprints-corpus").arg(corpus);
        }
    }

    (c, out_paths)
}

/// Sum output-bytes across all emitted files; take component_count
/// from the CDX file (first path by convention). Missing/malformed
/// files are treated as 0 rather than erroring — the runner already
/// records the exit_status; metadata parse is best-effort.
fn aggregate_output_metadata(paths: &[PathBuf]) -> (u64, u64) {
    let mut total_bytes = 0u64;
    let mut component_count = 0u64;
    for (i, p) in paths.iter().enumerate() {
        if !p.exists() {
            continue;
        }
        let meta = parse_output_metadata(p).unwrap_or(OutputMeta {
            output_bytes: 0,
            component_count: 0,
        });
        total_bytes += meta.output_bytes;
        if i == 0 {
            component_count = meta.component_count;
        }
    }
    (total_bytes, component_count)
}

/// Sort 5 samples in place and return the middle value (index 2).
/// Matches `waybill-cli/tests/dual_format_perf.rs::median_of_5`
/// posture per research.md R5.
fn median_of_5(mut samples: [u64; 5]) -> u64 {
    samples.sort_unstable();
    samples[2]
}

/// Synthesize a "corpus unreachable" Result without spawning waybill.
/// The wall-clock and RSS dimensions are zero; exit_status is
/// `CorpusUnreachable`. V3 (median == sorted[2]) still holds because
/// all-zero samples produce a zero median.
fn unreachable_corpus_result(fixture: &Fixture, mode: Mode, cfg: &RunConfig) -> BenchResult {
    BenchResult {
        fixture_name: fixture.name.clone(),
        mode,
        median_wall_clock_ms: 0,
        max_rss_kb: 0,
        output_bytes: 0,
        component_count: 0,
        exit_status: ExitStatus::CorpusUnreachable,
        waybill_commit_sha: cfg.waybill_sha.clone(),
        fixture_sha: cfg.fixture_sha.clone(),
        raw_samples_ms: [0; 5],
    }
}

// ────────────────────────────────────────────────────────────────
// T023 — run_matrix
// ────────────────────────────────────────────────────────────────

/// Drive the full matrix sequentially. No parallelism v1 per contract
/// xtask-bench-cli.md — parallel measurements would interfere with
/// each other's RSS/wall-clock signal.
pub fn run_matrix(
    matrix: Vec<(Fixture, Mode)>,
    cfg: &RunConfig,
) -> Result<BenchRun, Box<dyn Error>> {
    let started_at = Utc::now().to_rfc3339();
    let start_instant = Instant::now();

    let mut results = Vec::with_capacity(matrix.len());
    for (fixture, mode) in &matrix {
        let result = run_one_fixture(fixture, *mode, cfg)?;
        results.push(result);
    }

    let finished_at = Utc::now().to_rfc3339();
    let total_duration_sec = start_instant.elapsed().as_secs();

    let runner_uname = read_uname_srmn();
    let noise_class = classify_noise(&runner_uname);

    let metadata = RunMetadata {
        waybill_commit_sha: cfg.waybill_sha.clone(),
        fixture_sha: cfg.fixture_sha.clone(),
        runner_uname,
        noise_class,
        started_at,
        finished_at,
        total_duration_sec,
    };

    let run = BenchRun {
        schema_version: BenchRun::schema_version(),
        metadata,
        results,
    };

    // V1 + V6 self-consistency. V5 (against-manifest) is the
    // caller's responsibility (T025 in mod.rs::run has the manifest).
    run.validate()?;
    Ok(run)
}

/// Best-effort `uname -srmn` capture on Unix. On non-Unix hosts,
/// emits `<os>-<arch>` per std::env::consts. Never returns Err —
/// this is metadata, not correctness.
fn read_uname_srmn() -> String {
    #[cfg(unix)]
    {
        Command::new("uname")
            .args(["-srmn"])
            .output()
            .ok()
            .and_then(|o| if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            })
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))
    }
    #[cfg(not(unix))]
    {
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Classify the runner's host per the m669 noise-budget tiers:
/// - Reference: Linux x86_64 GitHub-hosted runner class.
/// - Noisy: macOS runners per m094 (loaded VMs, spawn jitter).
/// - Other: everything else — treat as unknown-noise.
fn classify_noise(uname: &str) -> NoiseClass {
    let lower = uname.to_lowercase();
    if lower.contains("linux") && lower.contains("x86_64") {
        NoiseClass::Reference
    } else if lower.contains("darwin") {
        NoiseClass::Noisy
    } else {
        NoiseClass::Other
    }
}

/// Read the waybill commit SHA via `git rev-parse HEAD`. Called by
/// the CLI wiring (T025) once at run start; the resulting String is
/// stored in `RunConfig.waybill_sha` and reused across every
/// fixture-mode invocation.
pub fn read_waybill_commit_sha() -> Result<String, Box<dyn Error>> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse HEAD exited non-zero: {}",
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    let sha = String::from_utf8(out.stdout)?.trim().to_string();
    Ok(sha)
}

/// Read the fixture-repo pin SHA from `tests/fixtures.rev` relative
/// to the given workspace root. This is the authoritative source
/// (m090 build.rs reads the same file).
pub fn read_fixture_pin_sha(workspace_root: &Path) -> Result<String, Box<dyn Error>> {
    let pin_path = workspace_root.join("tests/fixtures.rev");
    let sha = std::fs::read_to_string(&pin_path)?.trim().to_string();
    Ok(sha)
}

// ────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::schema::{FixtureKind, ScanClass};

    fn stub_fixture(name: &str, kind: FixtureKind, path: &str) -> Fixture {
        Fixture {
            name: name.into(),
            path: path.into(),
            kind,
            ecosystem: Some("cargo".into()),
            supported_modes: vec![Mode::Default],
            expected_scan_class: ScanClass::Medium,
        }
    }

    fn stub_cfg() -> RunConfig {
        RunConfig {
            fixtures_dir: PathBuf::from("/tmp/fixtures"),
            waybill_bin: PathBuf::from("waybill"),
            per_fixture_timeout: Duration::from_secs(300),
            fingerprints_corpus: None,
            waybill_sha: "0000000000000000000000000000000000000000".into(),
            fixture_sha: "1111111111111111111111111111111111111111".into(),
        }
    }

    #[test]
    fn median_of_5_is_middle_sorted_value() {
        assert_eq!(median_of_5([100, 200, 300, 400, 500]), 300);
        assert_eq!(median_of_5([500, 400, 300, 200, 100]), 300);
        assert_eq!(median_of_5([5, 5, 5, 5, 5]), 5);
        assert_eq!(median_of_5([1, 100, 1, 100, 50]), 50);
    }

    #[test]
    fn build_waybill_cmd_source_tree_default_mode_shape() {
        let f = stub_fixture(
            "cargo-workspace-medium",
            FixtureKind::SourceTree,
            "benchmark/source-tier/cargo-workspace-medium",
        );
        let cfg = stub_cfg();
        let target = PathBuf::from("/x/cargo-workspace-medium");
        let scratch = PathBuf::from("/tmp/scratch-abc");
        let (cmd, outs) = build_waybill_cmd(&f, Mode::Default, &cfg, &target, &scratch);
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args[0], "sbom");
        assert_eq!(args[1], "scan");
        assert_eq!(args[2], "--offline");
        assert_eq!(args[3], "--path");
        assert_eq!(args[4], "/x/cargo-workspace-medium");
        // Default mode: single CDX output.
        assert_eq!(outs.len(), 1);
        assert!(outs[0].to_str().unwrap().ends_with("out.cdx.json"));
        // No --no-deep-hash for Default.
        assert!(!args.iter().any(|a| *a == "--no-deep-hash"));
        // --offline is always injected to isolate bench measurements
        // from ANY remote service's response-time variance — deps.dev,
        // ClearlyDefined, proxy.golang.org (see build_waybill_cmd comment).
        assert!(args.iter().any(|a| *a == "--offline"));
    }

    #[test]
    fn build_waybill_cmd_container_image_uses_image_flag() {
        let f = stub_fixture(
            "debian-slim",
            FixtureKind::ContainerImage,
            "benchmark/container-images/debian-slim.tar",
        );
        let cfg = stub_cfg();
        let target = PathBuf::from("/x/debian-slim.tar");
        let scratch = PathBuf::from("/tmp/scratch-abc");
        let (cmd, _) = build_waybill_cmd(&f, Mode::Default, &cfg, &target, &scratch);
        let args: Vec<_> = cmd.get_args().collect();
        assert!(args.iter().any(|a| *a == "--image"));
        assert!(!args.iter().any(|a| *a == "--path"));
    }

    #[test]
    fn build_waybill_cmd_triple_format_emits_three_outputs() {
        let f = stub_fixture("x", FixtureKind::SourceTree, "x");
        let cfg = stub_cfg();
        let target = PathBuf::from("/x");
        let scratch = PathBuf::from("/tmp/s");
        let (cmd, outs) = build_waybill_cmd(&f, Mode::TripleFormat, &cfg, &target, &scratch);
        assert_eq!(outs.len(), 3);
        let args: Vec<_> = cmd.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.iter().filter(|a| **a == "--format").count() == 3);
        assert!(args.contains(&"cyclonedx-json"));
        assert!(args.contains(&"spdx-2.3-json"));
        assert!(args.contains(&"spdx-3-json"));
        // Multi-format must use per-format --output <fmt>=<path> form
        // (waybill rejects bare --output when more than one format is
        // requested). Lock the wire shape here so a future refactor
        // can't silently regress the fix from the T027 acceptance run.
        assert!(args.iter().filter(|a| **a == "--output").count() == 3);
        assert!(args.iter().any(|a| a.starts_with("cyclonedx-json=") && a.ends_with("out.cdx.json")));
        assert!(args.iter().any(|a| a.starts_with("spdx-2.3-json=") && a.ends_with("out.spdx.json")));
        assert!(args.iter().any(|a| a.starts_with("spdx-3-json=") && a.ends_with("out.spdx3.json")));
    }

    #[test]
    fn build_waybill_cmd_no_deep_hash_plus_triple_combines_both() {
        let f = stub_fixture("x", FixtureKind::SourceTree, "x");
        let cfg = stub_cfg();
        let target = PathBuf::from("/x");
        let scratch = PathBuf::from("/tmp/s");
        let (cmd, outs) = build_waybill_cmd(
            &f,
            Mode::NoDeepHashPlusTripleFormat,
            &cfg,
            &target,
            &scratch,
        );
        assert_eq!(outs.len(), 3);
        let args: Vec<_> = cmd.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.contains(&"--no-deep-hash"));
    }

    #[test]
    fn build_waybill_cmd_fingerprints_corpus_wires_flag_when_set() {
        let f = stub_fixture("bins", FixtureKind::BinarySet, "x");
        let mut cfg = stub_cfg();
        cfg.fingerprints_corpus = Some(PathBuf::from("/tmp/corpus"));
        let target = PathBuf::from("/x");
        let scratch = PathBuf::from("/tmp/s");
        let (cmd, _) = build_waybill_cmd(&f, Mode::FingerprintsCorpus, &cfg, &target, &scratch);
        let args: Vec<_> = cmd.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.contains(&"--fingerprints-corpus"));
        assert!(args.contains(&"/tmp/corpus"));
    }

    #[test]
    fn run_one_fixture_fingerprints_corpus_unreachable_synthesizes_result() {
        // Corpus is None → should synthesize a CorpusUnreachable
        // Result without ever attempting to spawn waybill.
        let f = stub_fixture("bins", FixtureKind::BinarySet, "x");
        let cfg = stub_cfg(); // fingerprints_corpus is None
        let r = run_one_fixture(&f, Mode::FingerprintsCorpus, &cfg).unwrap();
        assert_eq!(r.exit_status, ExitStatus::CorpusUnreachable);
        assert_eq!(r.median_wall_clock_ms, 0);
        assert_eq!(r.raw_samples_ms, [0, 0, 0, 0, 0]);
        assert_eq!(r.fixture_name, "bins");
        assert_eq!(r.mode, Mode::FingerprintsCorpus);
    }

    #[test]
    fn classify_noise_by_uname_string() {
        assert_eq!(
            classify_noise("Linux ci-runner 6.5.0 x86_64"),
            NoiseClass::Reference
        );
        assert_eq!(
            classify_noise("Darwin mba.local 25.5.0 arm64"),
            NoiseClass::Noisy
        );
        assert_eq!(
            classify_noise("Linux edge-arm 6.5.0 aarch64"),
            NoiseClass::Other
        );
        assert_eq!(classify_noise("windows-11-x86_64"), NoiseClass::Other);
    }

    #[test]
    fn read_uname_srmn_returns_non_empty() {
        // Smoke: should return something even on non-Unix (fallback).
        let s = read_uname_srmn();
        assert!(!s.is_empty());
    }

    #[test]
    fn read_fixture_pin_sha_reads_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let tests_dir = tmp.path().join("tests");
        std::fs::create_dir(&tests_dir).unwrap();
        std::fs::write(
            tests_dir.join("fixtures.rev"),
            "4de48e97a9771a884cfe1c64279bb428657a4161\n",
        )
        .unwrap();
        let sha = read_fixture_pin_sha(tmp.path()).unwrap();
        assert_eq!(sha, "4de48e97a9771a884cfe1c64279bb428657a4161");
    }

    #[test]
    fn read_fixture_pin_sha_errs_when_pin_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_fixture_pin_sha(tmp.path()).is_err());
    }

    #[test]
    fn aggregate_output_metadata_handles_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.cdx.json");
        std::fs::write(
            &a,
            r#"{"components":[{"name":"x"},{"name":"y"}]}"#,
        )
        .unwrap();
        let missing = tmp.path().join("missing.spdx.json");
        let (bytes, comp) = aggregate_output_metadata(&[a.clone(), missing]);
        // Only file a contributes bytes.
        assert!(bytes > 0);
        // component_count from the first (CDX) file.
        assert_eq!(comp, 2);
    }
}
