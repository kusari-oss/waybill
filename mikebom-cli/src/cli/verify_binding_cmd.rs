//! Milestone 072 T016 — `mikebom sbom verify-binding` subcommand.
//!
//! Reads an image-tier SBOM and a source-tier SBOM and reports
//! per-component binding verification status per FR-005 / VR-005.
//! Exits non-zero on any verification failure so CI lanes can gate.
//!
//! Two output formats:
//!
//! - `table` (default) — human-readable per-row text + summary line.
//! - `json` — `mikebom::binding::VerifyReport` shape, one JSON object.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

/// Output format for `verify-binding`. Pattern mirrors
/// `parity-check`'s `OutputFormat` enum.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum VerifyBindingOutputFormat {
    /// Plain-text per-row table. Default.
    Table,
    /// `VerifyReport` JSON for CI pipelines / machine consumption.
    Json,
}

#[derive(Args, Debug)]
pub struct VerifyBindingArgs {
    /// Path to the image-tier SBOM (JSON; CDX 1.6 / SPDX 2.3 / SPDX 3).
    #[arg(long)]
    pub image_sbom: PathBuf,

    /// Path to the source-tier SBOM (JSON).
    #[arg(long)]
    pub source_sbom: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = VerifyBindingOutputFormat::Table)]
    pub format: VerifyBindingOutputFormat,
}

pub async fn execute(args: VerifyBindingArgs) -> anyhow::Result<ExitCode> {
    let report = mikebom::binding::verify_binding_from_paths(
        &args.image_sbom,
        &args.source_sbom,
    )?;

    match args.format {
        VerifyBindingOutputFormat::Table => println!("{}", report.to_table()),
        VerifyBindingOutputFormat::Json => println!("{}", report.to_json_pretty()?),
    }

    if report.is_clean() {
        Ok(ExitCode::from(0))
    } else {
        // Non-zero exit so CI gates can detect verification failure.
        Ok(ExitCode::from(1))
    }
}
