//! `waybill sbom enrich` — feature 006 US5 + milestone 072 US2.
//!
//! Two enrichment paths:
//!
//! - `--patch <PATH>`: RFC 6902 JSON Patch operations applied to the
//!   target SBOM with `waybill:enrichment-patch[N]` provenance
//!   recording. Untouched by milestone 072.
//! - `--vex-overrides <PATH>` + `--vex-propagation-mode {permissive,
//!   caveated,strict}` (milestone 072 / T021): propagate VEX
//!   statements from a source-tier OpenVEX 0.2.0 document onto the
//!   target SBOM, gated by per-instance `waybill:source-document-
//!   binding` strength per `contracts/openvex-instance-identifiers
//!   .md` C-3 + C-4 + C-5.
//!
//! The two paths can be combined in one invocation; `--patch` runs
//! first, then VEX propagation.

use std::path::PathBuf;

use clap::Args;

use waybill::binding::VexPropagationMode;

#[derive(Args)]
pub struct EnrichArgs {
    /// Path to CycloneDX SBOM file to enrich in place.
    pub sbom_file: PathBuf,

    /// Output path. Defaults to overwriting `sbom_file`.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// RFC 6902 JSON Patch file. Repeatable: patches are applied in
    /// order (later ops see earlier ones).
    #[arg(long = "patch", value_name = "PATH")]
    pub patch: Vec<PathBuf>,

    /// Recorded author of the enrichment. Defaults to "unknown" with
    /// a warning.
    #[arg(long)]
    pub author: Option<String>,

    /// Optional path to the attestation this SBOM was derived from.
    /// Its SHA-256 gets embedded so verifiers can walk back.
    #[arg(long = "base-attestation", value_name = "PATH")]
    pub base_attestation: Option<PathBuf>,

    // ─ Legacy no-op flags preserved on the stub signature ─
    #[arg(long)]
    pub skip_vex: bool,
    #[arg(long)]
    pub skip_licenses: bool,
    #[arg(long)]
    pub skip_supplier: bool,
    /// Path to a source-tier OpenVEX 0.2.0 document whose statements
    /// will be propagated onto components in `sbom_file`. Each
    /// propagation is gated by the target component's
    /// `waybill:source-document-binding` strength per
    /// `--vex-propagation-mode`.
    #[arg(long = "vex-overrides", value_name = "PATH")]
    pub vex_overrides: Option<PathBuf>,
    /// VEX propagation mode (milestone 072 / FR-007). The default
    /// flipped from implicit-permissive (alpha.14) to `caveated` in
    /// alpha.15 — a documented breaking change. Pass
    /// `--vex-propagation-mode permissive` for pre-072 behavior.
    #[arg(
        long = "vex-propagation-mode",
        value_enum,
        default_value_t = VexPropagationMode::Caveated,
        help = "VEX propagation mode. caveated (default) tags non-verified \
                bindings with waybill:vex-binding-status: unverified. strict \
                refuses propagation onto non-verified bindings (exit non-zero). \
                permissive matches pre-072 behavior — propagate by PURL match \
                without binding check."
    )]
    pub vex_propagation_mode: VexPropagationMode,
    #[arg(long, default_value = "5000")]
    pub deps_dev_timeout: u64,
    #[arg(long)]
    pub json: bool,
}

pub async fn execute(args: EnrichArgs, _offline: bool) -> anyhow::Result<()> {
    if args.patch.is_empty() && args.vex_overrides.is_none() {
        anyhow::bail!(
            "at least one --patch <PATH> or --vex-overrides <PATH> is required for enrichment"
        );
    }

    let author = match &args.author {
        Some(a) => a.clone(),
        None => {
            tracing::warn!(
                "enrichment author not specified — downstream traceability degraded"
            );
            "unknown".to_string()
        }
    };

    let base_sha = match &args.base_attestation {
        Some(p) => Some(crate::sbom::mutator::attestation_sha256(p).map_err(|e| {
            anyhow::anyhow!("cannot hash base attestation {}: {e}", p.display())
        })?),
        None => None,
    };

    let sbom_text = std::fs::read_to_string(&args.sbom_file).map_err(|e| {
        anyhow::anyhow!("cannot read SBOM {}: {e}", args.sbom_file.display())
    })?;
    let mut sbom: serde_json::Value = serde_json::from_str(&sbom_text)
        .map_err(|e| anyhow::anyhow!("SBOM JSON parse failed: {e}"))?;

    let now = chrono::Utc::now();
    let mut patches_applied = 0usize;

    // ─── JSON-Patch path (untouched by milestone 072) ───
    if !args.patch.is_empty() {
        // Load each patch file into an owned Value; keep vectors parallel
        // so EnrichmentPatch can borrow from them.
        let mut patch_values: Vec<serde_json::Value> = Vec::with_capacity(args.patch.len());
        for p in &args.patch {
            let txt = std::fs::read_to_string(p)
                .map_err(|e| anyhow::anyhow!("cannot read patch {}: {e}", p.display()))?;
            let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| {
                anyhow::anyhow!("patch JSON parse failed for {}: {e}", p.display())
            })?;
            patch_values.push(v);
        }

        let patches: Vec<crate::sbom::mutator::EnrichmentPatch<'_>> = patch_values
            .iter()
            .map(|ops| crate::sbom::mutator::EnrichmentPatch {
                operations: ops,
                author: &author,
                timestamp: now,
                base_attestation_sha256: base_sha.clone(),
            })
            .collect();

        sbom = crate::sbom::mutator::enrich(&sbom, &patches)
            .map_err(|e| anyhow::anyhow!("enrichment failed: {e}"))?;
        patches_applied = patches.len();
    }

    // ─── VEX propagation path (milestone 072 / T021) ───
    let mut propagation_summary = serde_json::json!(null);
    let mut propagation_failed = false;
    if let Some(vex_path) = &args.vex_overrides {
        let vex_text = std::fs::read_to_string(vex_path).map_err(|e| {
            anyhow::anyhow!("cannot read --vex-overrides {}: {e}", vex_path.display())
        })?;
        let source_vex: serde_json::Value = serde_json::from_str(&vex_text).map_err(|e| {
            anyhow::anyhow!(
                "OpenVEX JSON parse failed for {}: {e}",
                vex_path.display()
            )
        })?;
        let report = crate::sbom::mutator::propagate_vex_with_binding(
            args.vex_propagation_mode,
            &source_vex,
            &mut sbom,
            now,
        )
        .map_err(|e| anyhow::anyhow!("VEX propagation failed: {e}"))?;
        tracing::info!(
            mode = ?args.vex_propagation_mode,
            propagated = report.statements_propagated,
            caveated = report.statements_caveated,
            refused = report.statements_refused,
            "VEX propagation summary"
        );
        propagation_failed = !report.is_clean();
        propagation_summary = serde_json::json!({
            "mode": format!("{:?}", args.vex_propagation_mode).to_lowercase(),
            "statements_propagated": report.statements_propagated,
            "statements_caveated": report.statements_caveated,
            "statements_refused": report.statements_refused,
        });
    }

    let out_path = args.output.as_ref().unwrap_or(&args.sbom_file);
    std::fs::write(out_path, serde_json::to_string_pretty(&sbom)?)?;
    tracing::info!(
        "Enriched SBOM written to {} ({} patch(es) applied)",
        out_path.display(),
        patches_applied,
    );
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "sbom_file": out_path.to_string_lossy(),
                "patches_applied": patches_applied,
                "author": author,
                "vex_propagation": propagation_summary,
            }))?
        );
    }

    // Strict-mode refusals → exit non-zero per VR-006.
    if propagation_failed {
        anyhow::bail!(
            "VEX propagation refused at least one statement under \
             --vex-propagation-mode {:?}: see waybill:vex-propagation-refusals \
             property in the output SBOM for per-instance rationales",
            args.vex_propagation_mode
        );
    }
    Ok(())
}
