//! Milestone 924 (#932) — `waybill repo report`.
//!
//! Produces a machine-readable account of what waybill saw, claimed, ignored
//! and could not determine. Never enriches, never emits an SBOM, never sends
//! anything anywhere (FR-022 / FR-023 / FR-024).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use crate::report::schema::RedactionMode;

#[derive(Debug, Parser)]
pub struct RepoCommand {
    #[command(subcommand)]
    pub command: RepoSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RepoSubcommand {
    /// Report what waybill understood, ignored, and could not determine.
    Report(ReportArgs),
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Repository to observe.
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Where to write the report. Defaults to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Replace repository-relative path segments with stable identifiers
    /// (FR-019b). Off by default: names are what make a report actionable to
    /// someone who cannot see your repository (FR-019a).
    #[arg(long)]
    pub redact: bool,

    /// Skip directory subtrees. Same semantics as `sbom scan --exclude-path`.
    #[arg(long = "exclude-path", value_name = "PATH_OR_PATTERN")]
    pub exclude_path: Vec<String>,
}

pub fn execute(cmd: RepoCommand) -> anyhow::Result<ExitCode> {
    match cmd.command {
        RepoSubcommand::Report(args) => run_report(args),
    }
}

fn run_report(args: ReportArgs) -> anyhow::Result<ExitCode> {
    let root = std::fs::canonicalize(&args.path)
        .map_err(|e| anyhow::anyhow!("cannot read --path {}: {e}", args.path.display()))?;

    let exclude_set =
        crate::scan_fs::package_db::exclude_path::ExclusionSet::from_iter(args.exclude_path.iter())
            .map_err(|e| anyhow::anyhow!("invalid --exclude-path: {e}"))?;

    let redaction = if args.redact { RedactionMode::Paths } else { RedactionMode::None };

    let report = crate::report::build(&root, &exclude_set, redaction)?;

    // FR-003 — a report whose totals do not reconcile is invalid, not merely
    // imperfect. Refuse to write one rather than hand out a document whose
    // central claim is false.
    let t = &report.totals;
    let skipped: u64 = t.files_skipped.values().sum();
    if !crate::report::totals_reconcile(t) {
        anyhow::bail!(
            "refusing to write a report that does not reconcile (FR-003): \
             walked={} claimed={} unclaimed={} skipped={}",
            t.files_walked, t.files_claimed, t.files_unclaimed, skipped,
        );
    }

    let json = serde_json::to_string_pretty(&report)?;
    match &args.output {
        Some(p) => {
            std::fs::write(p, json.as_bytes())
                .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", p.display()))?;
            tracing::info!(
                output = %p.display(),
                directories_recorded = report.totals.directories_recorded,
                files_walked = report.totals.files_walked,
                "repo observation report written",
            );
        }
        None => println!("{json}"),
    }

    // FR-019d — tell the operator the stricter mode exists at the moment it
    // matters, rather than expecting them to find it in documentation after
    // they have already shared something.
    if redaction == RedactionMode::None {
        tracing::info!(
            "this report contains repository-relative directory names. \
             Re-run with --redact to replace them with stable identifiers \
             before sharing, if those names are sensitive."
        );
    }

    Ok(ExitCode::SUCCESS)
}
