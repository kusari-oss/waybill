use clap::{Args, Subcommand};

use std::process::ExitCode;

use super::enrich::EnrichArgs;
use super::generate::GenerateArgs;
use super::parity_cmd::ParityCheckArgs;
use super::scan_cmd::ScanArgs;
use super::trace_binding_cmd::TraceBindingArgs;
use super::verify::VerifyArgs;
use super::verify_binding_cmd::VerifyBindingArgs;

#[derive(Args)]
pub struct SbomCommand {
    #[command(subcommand)]
    pub command: SbomSubcommand,
}

// Milestone 076 — `ScanArgs` grows two new repeatable `Vec<...>`
// fields (`subject_hash`, `component_id`) and the variant size
// diverges further from the smallest sibling. The enum stays the
// natural way to model clap subcommands; boxing every variant is
// invasive and would require pattern-match updates throughout the
// dispatch layer.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
pub enum SbomSubcommand {
    /// Generate an SBOM from an attestation file
    Generate(GenerateArgs),
    /// Add license, VEX, and supplier data to an existing SBOM
    Enrich(EnrichArgs),
    /// Verify a signed attestation (DSSE envelope) against a key /
    /// identity / layout
    Verify(VerifyArgs),
    /// Walk a directory (or an extracted container image) and produce
    /// an SBOM from the package artifacts on disk. No eBPF required —
    /// runs anywhere Rust runs.
    Scan(ScanArgs),
    /// Run a per-datum × per-format coverage check against three
    /// already-emitted format outputs and report any parity gaps.
    /// Drives the milestone-013 user-facing diagnostic.
    ParityCheck(ParityCheckArgs),
    /// Verify that an image-tier SBOM's per-component
    /// `mikebom:source-document-binding` annotations match the
    /// recompute against a source-tier SBOM (milestone 072, FR-005).
    /// Exits non-zero on any verification failure.
    VerifyBinding(VerifyBindingArgs),
    /// Trace an image-tier component back to its candidate
    /// source-tier SBOMs (milestone 072, FR-006). For each instance
    /// of the supplied PURL in the image SBOM, reports the binding
    /// state against every candidate source SBOM.
    /// Always exits 0 (informational; not validating).
    TraceBinding(TraceBindingArgs),
}

pub async fn execute(
    cmd: SbomCommand,
    offline: bool,
    exclude_scope: Vec<waybill_common::resolution::LifecycleScope>,
    include_legacy_rpmdb: bool,
    include_declared_deps: bool,
    exclude_set: crate::scan_fs::package_db::exclude_path::ExclusionSet,
    supplement_cdx: Option<std::path::PathBuf>,
) -> anyhow::Result<ExitCode> {
    match cmd.command {
        SbomSubcommand::Generate(args) => {
            super::generate::execute(args, offline).await?;
            Ok(ExitCode::from(0))
        }
        SbomSubcommand::Enrich(args) => {
            super::enrich::execute(args, offline).await?;
            Ok(ExitCode::from(0))
        }
        SbomSubcommand::Verify(args) => super::verify::execute(args).await,
        SbomSubcommand::Scan(args) => {
            super::scan_cmd::execute(
                args,
                offline,
                exclude_scope,
                include_legacy_rpmdb,
                include_declared_deps,
                exclude_set,
                supplement_cdx,
            )
            .await?;
            Ok(ExitCode::from(0))
        }
        SbomSubcommand::ParityCheck(args) => super::parity_cmd::execute(args).await,
        SbomSubcommand::VerifyBinding(args) => {
            super::verify_binding_cmd::execute(args).await
        }
        SbomSubcommand::TraceBinding(args) => {
            super::trace_binding_cmd::execute(args).await
        }
    }
}
