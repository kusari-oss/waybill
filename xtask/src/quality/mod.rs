// milestone 770 - see specs/770-sbom-quality-corpus/plan.md
//
// Entry point for the `xtask quality` subcommand: measure SBOM quality
// across a corpus of pinned public repositories and gate on hand-authored
// per-target ranges.
//
// T002/T010: CLI args + exit-code policy.
// T019: orchestration — parse → verify sbomqs → per-target → report.
// T020: --filter glob matching.
// T028/T029: exit-code wiring and --no-gate.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;

pub mod analyze;
pub mod config;
pub mod evaluate;
pub mod fetch;
pub mod measure;
pub mod report;
pub mod score;

use crate::quality::config::{CorpusConfig, Target};
use crate::quality::report::{
    MeasurementStatus, QualityReport, TargetMeasurement, UnmeasurableReason, SCHEMA_VERSION,
};

/// Exit-code policy per contract quality-report.md § C-5. `ConfigError`
/// is distinct from `Violations` so a broken corpus file is never
/// mistaken for a waybill regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitPolicy {
    Clean = 0,
    Violations = 1,
    ConfigError = 2,
}

/// CLI flags per contract xtask-quality-cli.md § C-1.
#[derive(Args, Debug, Clone, Default)]
pub struct QualityArgs {
    /// Restrict to matching target names. Repeatable; multiple flags
    /// union. `*` is the only metacharacter. An empty match set is not an
    /// error.
    #[arg(long, value_name = "GLOB", action = clap::ArgAction::Append)]
    pub filter: Vec<String>,

    /// Override the corpus file.
    #[arg(long, value_name = "PATH")]
    pub corpus: Option<PathBuf>,

    /// Override the report path.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Override the repository cache root.
    #[arg(long, value_name = "PATH")]
    pub cache_dir: Option<PathBuf>,

    /// Override the binary under measurement.
    #[arg(long, value_name = "PATH")]
    pub waybill_bin: Option<PathBuf>,

    /// Override every target's scan budget.
    #[arg(long, value_name = "SECS")]
    pub timeout_secs: Option<u64>,

    /// Measure and report, but always exit 0. For range-authoring runs.
    /// Does NOT suppress the missing-sbomqs failure (C-1.2).
    #[arg(long)]
    pub no_gate: bool,

    /// Ignore cached checkouts and re-fetch.
    #[arg(long)]
    pub refresh: bool,
}

pub fn run(args: QualityArgs) -> Result<(), Box<dyn Error>> {
    let root = workspace_root();
    let corpus_path = args
        .corpus
        .clone()
        .unwrap_or_else(|| root.join("xtask/corpus/quality-corpus.toml"));

    // Step 1 (C-2): parse and validate. Any configuration error reports
    // ALL of them and exits 2 without fetching anything.
    let corpus = match CorpusConfig::load(&corpus_path) {
        Ok(c) => c,
        Err(errs) => {
            eprint!("{errs}");
            std::process::exit(ExitPolicy::ConfigError as i32);
        }
    };

    // Step 2 (C-2): sbomqs must exist. FR-016 — a missing scorer is a
    // failed run, never a passing one, and --no-gate cannot weaken this.
    let sbomqs_bin = match score::locate() {
        Some(b) => b,
        None => {
            eprintln!(
                "sbomqs not found on $PATH and WAYBILL_SBOMQS_BIN is unset.\n\
                 The corpus expects {}. Install it with:\n\
                 \n    go install github.com/interlynk-io/sbomqs/v2@{}\n",
                corpus.sbomqs_version, corpus.sbomqs_version
            );
            std::process::exit(ExitPolicy::Violations as i32);
        }
    };
    let actual_version = score::version(&sbomqs_bin).unwrap_or_else(|| "unknown".into());
    if actual_version != corpus.sbomqs_version {
        // A warning, not a failure: the score is still comparable, but the
        // difference must be visible because sbomqs's output shape moves
        // between releases.
        eprintln!(
            "warning: sbomqs version mismatch — corpus expects {}, found {actual_version}",
            corpus.sbomqs_version
        );
    }

    let selected: Vec<&Target> = corpus
        .targets
        .iter()
        .filter(|t| matches_any(&args.filter, t.name.as_str()))
        .collect();
    if selected.is_empty() {
        println!("note: filter matched zero targets — nothing selected.");
        return Ok(());
    }

    let cache_root = args
        .cache_dir
        .clone()
        .unwrap_or_else(|| default_cache_root(&root));
    let waybill_bin = args
        .waybill_bin
        .clone()
        .unwrap_or_else(|| root.join("target/release/waybill"));
    if !waybill_bin.exists() {
        return Err(format!(
            "waybill binary not found at {}. Build it with:\n    cargo build --release -p waybill --bin waybill",
            waybill_bin.display()
        )
        .into());
    }

    let scratch = tempfile::tempdir()?;
    let gomodcache = scratch.path().join("gomodcache");
    std::fs::create_dir_all(&gomodcache)?;
    let docs_dir = scratch.path().join("docs");
    std::fs::create_dir_all(&docs_dir)?;

    let started_at = now_rfc3339();
    let mut measurements: Vec<TargetMeasurement> = Vec::new();
    let mut violations = Vec::new();

    for t in &selected {
        eprintln!("measuring {} ...", t.name);
        let m = measure_one(
            t,
            &cache_root,
            &waybill_bin,
            &docs_dir,
            &gomodcache,
            &sbomqs_bin,
            args.timeout_secs.unwrap_or_else(|| t.effective_timeout(corpus.default_timeout_secs)),
            args.refresh,
        );
        // FR-018: evaluate every target; never short-circuit.
        violations.extend(evaluate::evaluate(&m, t.expect.as_ref()));
        measurements.push(m);
    }

    let mut rep = QualityReport {
        schema_version: SCHEMA_VERSION,
        waybill_sha: git_head(&root).unwrap_or_else(|| "unknown".into()),
        corpus_sha: git_head(&root).unwrap_or_else(|| "unknown".into()),
        sbomqs_version: actual_version,
        started_at,
        finished_at: now_rfc3339(),
        runner: runner_string(),
        measurements,
        violations,
        config_errors: Vec::new(),
    };
    rep.sort();

    let out_path = args.output.clone().unwrap_or_else(|| {
        root.join("target/quality")
            .join(format!("run-{}.json", rep.waybill_sha.chars().take(12).collect::<String>()))
    });
    // C-2.1: written BEFORE the exit decision, so a failing run still
    // leaves a report behind (FR-029).
    report::write_report(&rep, &out_path)?;

    print!("{}", report::render_summary(&rep));
    println!("\nWrote {}", out_path.display());

    let failed = !rep.violations.is_empty() || rep.unmeasurable_count() > 0;
    if failed && !args.no_gate {
        std::process::exit(ExitPolicy::Violations as i32);
    }
    if failed && args.no_gate {
        eprintln!("\n--no-gate: {} problem(s) found but exiting 0.", rep.violations.len() + rep.unmeasurable_count());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn measure_one(
    t: &Target,
    cache_root: &Path,
    waybill_bin: &Path,
    docs_dir: &Path,
    gomodcache: &Path,
    sbomqs_bin: &Path,
    timeout_secs: u64,
    refresh: bool,
) -> TargetMeasurement {
    let checkout = match fetch::fetch(cache_root, t, refresh) {
        Ok(o) => o.path,
        Err(detail) => {
            return TargetMeasurement::unmeasurable(
                t.name.clone(),
                UnmeasurableReason::FetchFailed { detail },
            )
        }
    };

    let (wall_ms, doc_path) = match measure::scan(
        waybill_bin, t, &checkout, docs_dir, gomodcache, timeout_secs,
    ) {
        measure::ScanOutcome::Ok { wall_ms, document } => (wall_ms, document),
        measure::ScanOutcome::Failed { detail } => {
            return TargetMeasurement::unmeasurable(
                t.name.clone(),
                UnmeasurableReason::ScanFailed { detail },
            )
        }
        measure::ScanOutcome::TimedOut { budget_secs } => {
            return TargetMeasurement::unmeasurable(
                t.name.clone(),
                UnmeasurableReason::ScanTimedOut { budget_secs },
            )
        }
    };

    let bytes = std::fs::metadata(&doc_path).map(|m| m.len()).ok();
    let doc: serde_json::Value = match std::fs::read(&doc_path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(detail) => {
            return TargetMeasurement::unmeasurable(
                t.name.clone(),
                UnmeasurableReason::ScanFailed { detail },
            )
        }
    };
    let a = analyze::analyze(&doc);

    let scores = match score::score_map(sbomqs_bin, &doc_path) {
        Ok(m) => m,
        Err(detail) => {
            return TargetMeasurement::unmeasurable(
                t.name.clone(),
                UnmeasurableReason::ScoringFailed { detail },
            )
        }
    };

    TargetMeasurement {
        name: t.name.clone(),
        status: MeasurementStatus::Measured,
        wall_ms: Some(wall_ms),
        sbomqs: Some(scores),
        pkgs: Some(a.pkgs),
        files: Some(a.files),
        edges: Some(a.edges),
        nodes_with_out_edges: Some(a.nodes_with_out_edges),
        max_depth: Some(a.max_depth),
        flat: Some(a.flat),
        graph_completeness: a.graph_completeness,
        sbom_bytes: bytes,
    }
}

// ────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────

/// `*`-only glob, matching `xtask bench`'s filter semantics exactly so
/// the two subcommands behave the same way.
fn matches_glob(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let first = parts[0];
    if !name.starts_with(first) {
        return false;
    }
    let mut cursor = &name[first.len()..];
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match cursor.find(part) {
            Some(i) => cursor = &cursor[i + part.len()..],
            None => return false,
        }
    }
    cursor.ends_with(parts[parts.len() - 1])
}

fn matches_any(filters: &[String], name: &str) -> bool {
    filters.is_empty() || filters.iter().any(|p| matches_glob(p, name))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// `~/.cache/waybill/quality-corpus`, mirroring the m090 fixture cache and
/// m195 corpus cache. `USERPROFILE` is checked so Windows hosts get a real
/// per-user cache rather than silently falling back to a relative directory
/// beside wherever the command happened to be invoked.
fn default_cache_root(_root: &Path) -> PathBuf {
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(h) = std::env::var(var) {
            if !h.is_empty() {
                return PathBuf::from(h).join(".cache/waybill/quality-corpus");
            }
        }
    }
    PathBuf::from(".waybill-quality-cache")
}

fn git_head(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Host descriptor for the report. `uname -a` where available; elsewhere
/// (notably Windows) the compile-time OS/arch pair, which is coarser but
/// still tells a reader which machine class produced the numbers.
fn runner_string() -> String {
    match Command::new("uname").arg("-a").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn no_filter_matches_everything() {
        assert!(matches_any(&[], "anything"));
    }

    #[test]
    fn literal_filter_matches_exactly() {
        let f = vec!["go-cobra".to_string()];
        assert!(matches_any(&f, "go-cobra"));
        assert!(!matches_any(&f, "go-kubernetes"));
    }

    #[test]
    fn prefix_glob_matches() {
        let f = vec!["gradle-*".to_string()];
        assert!(matches_any(&f, "gradle-apache-solr"));
        assert!(!matches_any(&f, "go-cobra"));
    }

    #[test]
    fn multiple_filters_union() {
        let f = vec!["gradle-*".to_string(), "go-cobra".to_string()];
        assert!(matches_any(&f, "gradle-apache-solr"));
        assert!(matches_any(&f, "go-cobra"));
        assert!(!matches_any(&f, "rust-zizmor"));
    }

    #[test]
    fn suffix_and_bracketing_globs() {
        assert!(matches_glob("*-solr", "gradle-apache-solr"));
        assert!(matches_glob("gradle-*-solr", "gradle-apache-solr"));
        assert!(matches_glob("*", "anything"));
        assert!(!matches_glob("*-solr", "gradle-apache-lucene"));
    }

    #[test]
    fn exit_policy_codes_are_stable() {
        // Contract C-5: 2 must stay distinct from 1 so a broken corpus is
        // never read as a waybill regression.
        assert_eq!(ExitPolicy::Clean as i32, 0);
        assert_eq!(ExitPolicy::Violations as i32, 1);
        assert_eq!(ExitPolicy::ConfigError as i32, 2);
    }
}
