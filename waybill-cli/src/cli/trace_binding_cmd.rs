//! Milestone 072 T025 — `waybill sbom trace-binding` subcommand.
//!
//! Operator-triage tool answering "which source-tier SBOM (if any)
//! corresponds to this image-tier component?" per FR-006 / US3.
//!
//! Given an image-tier SBOM and one or more candidate source-tier
//! SBOMs, the command finds ALL instances of the supplied PURL in
//! the image SBOM and reports each instance's binding state against
//! every candidate source SBOM. Useful when an operator triaging a
//! component (e.g., a CVE-flagged transitive dep) wants to know
//! whether the image-tier instance traces back to a known source-
//! tier build.
//!
//! Behavior:
//!
//! - The command is **informational, not validating** — it always
//!   exits 0. (Contrast with `verify-binding`, which is the
//!   validating sibling that exits non-zero on hash mismatch.)
//! - Multi-instance images surface one row per `bom-ref`/`SPDXID`
//!   instance. Each instance is independently traced against every
//!   candidate.
//! - When the image-tier instance carries a pre-emitted
//!   `waybill:source-document-binding` annotation, the trace
//!   answers from that. When absent, the trace falls back to
//!   "PURL match against the candidate's component set" and
//!   reports `binding: unknown` with a structured `reason`.
//!
//! Mirrors the JSON output shape from `quickstart.md` Recipe 6.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use serde::Serialize;
use serde_json::Value;

use waybill::binding::{
    deserialize_from_cdx_property, BindingStrength, SourceDocumentBinding,
    SourceSbomContext, BINDING_PROPERTY_NAME,
};

/// Output format for `trace-binding`. Pattern mirrors
/// `verify-binding`'s `VerifyBindingOutputFormat` enum.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum TraceBindingOutputFormat {
    /// Plain-text per-row table. Default.
    Table,
    /// `TraceReport` JSON per `quickstart.md` Recipe 6 for CI
    /// pipelines / machine consumption.
    Json,
}

/// Args for `waybill sbom trace-binding`. Either `--source-sbom` or
/// `--candidate-sources-dir` MUST be supplied (mutually exclusive
/// per clap `conflicts_with`).
#[derive(Args, Debug)]
pub struct TraceBindingArgs {
    /// PURL of the component to trace.
    #[arg(long)]
    pub component_purl: String,

    /// Image-tier SBOM (CDX 1.6 / SPDX 2.3 / SPDX 3 JSON).
    #[arg(long)]
    pub image_sbom: PathBuf,

    /// Single candidate source-tier SBOM. Mutually exclusive with
    /// `--candidate-sources-dir`.
    #[arg(long, conflicts_with = "candidate_sources_dir")]
    pub source_sbom: Option<PathBuf>,

    /// Directory containing candidate source-tier SBOMs. Every
    /// `*.cdx.json` / `*.spdx.json` / `*.spdx3.json` / `*.json`
    /// file in the directory is loaded and tested.
    #[arg(long)]
    pub candidate_sources_dir: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = TraceBindingOutputFormat::Table)]
    pub format: TraceBindingOutputFormat,
}

/// One instance row in the report — one (image-tier component,
/// matched-source-SBOM) pair.
#[derive(Debug, Clone, Serialize)]
pub struct TraceInstance {
    /// CDX `bom-ref` or SPDX `SPDXID` of the image-tier instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bom_ref: Option<String>,
    /// The traced binding for this instance.
    pub binding: TraceBinding,
    /// One-line audit summary suitable for an audit log.
    pub audit_summary: String,
}

/// Per-instance binding trace — either the asserted binding from
/// the image-tier annotation, or a constructed `Unknown` shape
/// when no annotation was present and no candidate matched.
#[derive(Debug, Clone, Serialize)]
pub struct TraceBinding {
    pub strength: BindingStrength,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doc_id: Option<TraceSourceDocId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceSourceDocId {
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iri: Option<String>,
}

/// Top-level report emitted by `trace-binding`.
#[derive(Debug, Clone, Serialize)]
pub struct TraceReport {
    pub component_purl: String,
    pub instances: Vec<TraceInstance>,
}

impl TraceReport {
    fn to_table(&self) -> String {
        let mut lines = Vec::with_capacity(self.instances.len() + 2);
        lines.push(format!("component_purl: {}", self.component_purl));
        if self.instances.is_empty() {
            lines.push("(no instances of this PURL found in the image SBOM)".into());
            return lines.join("\n");
        }
        lines.push(format!(
            "{:<48}  {:<8}  {}",
            "bom_ref", "strength", "summary",
        ));
        for inst in &self.instances {
            lines.push(format!(
                "{:<48}  {:<8}  {}",
                inst.bom_ref.as_deref().unwrap_or("(none)"),
                strength_label(&inst.binding.strength),
                inst.audit_summary,
            ));
        }
        lines.join("\n")
    }

    fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn strength_label(s: &BindingStrength) -> &'static str {
    match s {
        BindingStrength::Verified => "verified",
        BindingStrength::Weak => "weak",
        BindingStrength::Unknown => "unknown",
    }
}

pub async fn execute(args: TraceBindingArgs) -> anyhow::Result<ExitCode> {
    if args.source_sbom.is_none() && args.candidate_sources_dir.is_none() {
        anyhow::bail!(
            "trace-binding requires either --source-sbom <path> or \
             --candidate-sources-dir <dir>",
        );
    }

    // Load the image SBOM once.
    let image_bytes = std::fs::read(&args.image_sbom).map_err(|e| {
        anyhow::anyhow!(
            "failed to read --image-sbom {}: {}",
            args.image_sbom.display(),
            e,
        )
    })?;
    let image_doc: Value = serde_json::from_slice(&image_bytes).map_err(|e| {
        anyhow::anyhow!(
            "failed to parse --image-sbom {} as JSON: {}",
            args.image_sbom.display(),
            e,
        )
    })?;

    // Load every candidate source SBOM (one path or a directory of
    // paths). Each SourceSbomContext also carries the doc's SHA-256
    // and a PURL → SourceDocumentBinding lookup.
    let mut sources: Vec<(PathBuf, SourceSbomContext)> = Vec::new();
    if let Some(p) = &args.source_sbom {
        let ctx = SourceSbomContext::load(p).map_err(|e| {
            anyhow::anyhow!("failed to load --source-sbom {}: {}", p.display(), e)
        })?;
        sources.push((p.clone(), ctx));
    } else if let Some(d) = &args.candidate_sources_dir {
        if !d.is_dir() {
            anyhow::bail!(
                "--candidate-sources-dir {} is not a directory",
                d.display()
            );
        }
        let entries = std::fs::read_dir(d).map_err(|e| {
            anyhow::anyhow!("failed to read --candidate-sources-dir {}: {}", d.display(), e)
        })?;
        let mut paths: Vec<PathBuf> = Vec::new();
        for ent in entries.flatten() {
            let p = ent.path();
            if !p.is_file() {
                continue;
            }
            // Accept any *.json suffix — operators may name files
            // freely (foo.cdx.json, bar.spdx3.json, raw.json).
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            paths.push(p);
        }
        // Sort for deterministic report ordering across runs.
        paths.sort();
        for p in paths {
            // Best-effort load — non-SBOM JSON files in the
            // candidate dir are skipped with a debug log.
            match SourceSbomContext::load(&p) {
                Ok(ctx) => sources.push((p, ctx)),
                Err(e) => {
                    tracing::debug!(
                        path = %p.display(),
                        error = %e,
                        "trace-binding: skipping unparseable candidate source SBOM",
                    );
                }
            }
        }
    }

    let report = trace(&args.component_purl, &image_doc, &sources);

    match args.format {
        TraceBindingOutputFormat::Table => println!("{}", report.to_table()),
        TraceBindingOutputFormat::Json => println!("{}", report.to_json_pretty()?),
    }

    // Trace is informational; exit 0 across all outcomes per
    // FR-006 / quickstart.md Recipe 6.
    Ok(ExitCode::from(0))
}

/// Build the per-instance trace report for `purl` against `image`
/// and the candidate `sources`. Pure function so it's unit-testable
/// without spawning the CLI.
pub fn trace(
    purl: &str,
    image: &Value,
    sources: &[(PathBuf, SourceSbomContext)],
) -> TraceReport {
    let instances = walk_image_instances(image, purl);
    let mut rows: Vec<TraceInstance> = Vec::with_capacity(instances.len());
    for inst in instances {
        rows.push(trace_instance_with_purl(inst, purl, sources));
    }
    TraceReport {
        component_purl: purl.to_string(),
        instances: rows,
    }
}

/// One image-tier instance carrying its bom-ref/SPDXID and any
/// pre-emitted `SourceDocumentBinding` annotation.
struct ImageInstance {
    bom_ref: Option<String>,
    asserted_binding: Option<SourceDocumentBinding>,
}

fn walk_image_instances(doc: &Value, target_purl: &str) -> Vec<ImageInstance> {
    if doc.get("@graph").is_some() {
        walk_spdx3_instances(doc, target_purl)
    } else if doc.get("packages").is_some() {
        walk_spdx23_instances(doc, target_purl)
    } else {
        walk_cdx_instances(doc, target_purl)
    }
}

fn walk_cdx_instances(doc: &Value, target_purl: &str) -> Vec<ImageInstance> {
    let mut out = Vec::new();
    walk_cdx_recursive(doc, target_purl, &mut out);
    out
}

fn walk_cdx_recursive(node: &Value, target_purl: &str, out: &mut Vec<ImageInstance>) {
    if let Some(arr) = node.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if c.get("purl").and_then(|v| v.as_str()) == Some(target_purl) {
                let bom_ref = c
                    .get("bom-ref")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let asserted_binding = decode_cdx_binding(c);
                out.push(ImageInstance {
                    bom_ref,
                    asserted_binding,
                });
            }
            walk_cdx_recursive(c, target_purl, out);
        }
    }
}

fn decode_cdx_binding(c: &Value) -> Option<SourceDocumentBinding> {
    let props = c.get("properties").and_then(|v| v.as_array())?;
    for p in props {
        if p.get("name").and_then(|v| v.as_str()) == Some(BINDING_PROPERTY_NAME) {
            if let Some(value_str) = p.get("value").and_then(|v| v.as_str()) {
                return deserialize_from_cdx_property(value_str).ok();
            }
        }
    }
    None
}

fn walk_spdx23_instances(doc: &Value, target_purl: &str) -> Vec<ImageInstance> {
    let mut out = Vec::new();
    let Some(packages) = doc.get("packages").and_then(|v| v.as_array()) else {
        return out;
    };
    for p in packages {
        if extract_spdx23_purl(p).as_deref() != Some(target_purl) {
            continue;
        }
        let spdxid = p.get("SPDXID").and_then(|v| v.as_str()).map(String::from);
        let asserted_binding = decode_spdx_envelope_binding_from_annotations(
            p.get("annotations").and_then(|v| v.as_array()),
        );
        out.push(ImageInstance {
            bom_ref: spdxid,
            asserted_binding,
        });
    }
    out
}

fn extract_spdx23_purl(pkg: &Value) -> Option<String> {
    let arr = pkg.get("externalRefs").and_then(|v| v.as_array())?;
    for r in arr {
        if r.get("referenceType").and_then(|v| v.as_str()) == Some("purl") {
            return r
                .get("referenceLocator")
                .and_then(|v| v.as_str())
                .map(String::from);
        }
    }
    None
}

fn decode_spdx_envelope_binding_from_annotations(
    annotations: Option<&Vec<Value>>,
) -> Option<SourceDocumentBinding> {
    let arr = annotations?;
    for a in arr {
        let comment = a.get("comment").and_then(|v| v.as_str())?;
        if let Some(b) = decode_envelope_binding(comment) {
            return Some(b);
        }
    }
    None
}

fn decode_envelope_binding(serialized: &str) -> Option<SourceDocumentBinding> {
    let v: Value = serde_json::from_str(serialized).ok()?;
    if v.get("schema").and_then(|x| x.as_str()) != Some("waybill-annotation/v1") {
        return None;
    }
    if v.get("field").and_then(|x| x.as_str()) != Some(BINDING_PROPERTY_NAME) {
        return None;
    }
    let value = v.get("value")?;
    waybill::binding::deserialize_from_envelope_value(value).ok()
}

fn walk_spdx3_instances(doc: &Value, target_purl: &str) -> Vec<ImageInstance> {
    let mut out = Vec::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };

    let mut by_iri: std::collections::BTreeMap<String, ImageInstance> =
        std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("software_Package") {
            continue;
        }
        let Some(spdxid) = el.get("spdxId").and_then(|v| v.as_str()) else {
            continue;
        };
        let purl = el.get("software_packageUrl").and_then(|v| v.as_str());
        if purl != Some(target_purl) {
            continue;
        }
        by_iri.insert(
            spdxid.to_string(),
            ImageInstance {
                bom_ref: Some(spdxid.to_string()),
                asserted_binding: None,
            },
        );
    }

    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("Annotation") {
            continue;
        }
        let Some(subject) = el.get("subject").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(statement) = el.get("statement").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(b) = decode_envelope_binding(statement) {
            if let Some(inst) = by_iri.get_mut(subject) {
                inst.asserted_binding = Some(b);
            }
        }
    }

    out.extend(by_iri.into_values());
    out
}

/// Build a `TraceInstance` row from one image-tier instance, the
/// target PURL, and the candidate sources. Resolution order:
///
/// 1. If the image-tier instance carries a `waybill:source-document-binding`
///    annotation AND any candidate source SBOM's content-SHA matches
///    the asserted `source_doc_id.sha256`, return the asserted
///    binding (provenance preserved + content-SHA matched).
/// 2. If the image-tier instance carries an asserted binding but no
///    candidate source matches the doc-SHA, return the asserted
///    binding as-is with a summary noting the unmatched candidate
///    set.
/// 3. If the image-tier instance has no asserted binding, return
///    `Unknown { reason: "source-not-found-in-bind-target" }` per
///    quickstart.md Recipe 6 (b). The trace does NOT infer a
///    binding from PURL-match alone — PURL collisions across
///    independent build paths are the worked-example concern from
///    US2 AS#4 / SC-003.
///
/// `target_purl` is reserved for future-extensions where the trace
/// considers PURL-only candidate matching (e.g., as a "best-effort"
/// override flag); today it is only used for context in audit
/// summaries.
fn trace_instance_with_purl(
    inst: ImageInstance,
    _target_purl: &str,
    sources: &[(PathBuf, SourceSbomContext)],
) -> TraceInstance {
    let bom_ref = inst.bom_ref.clone();

    if let Some(asserted) = &inst.asserted_binding {
        let matched = sources
            .iter()
            .find(|(_, ctx)| ctx.source_doc_id.sha256 == asserted.source_doc_id.sha256);
        if let Some((path, ctx)) = matched {
            let summary = format!(
                "instance bound to {} ({}) — content-SHA matched",
                path.display(),
                strength_label(&asserted.strength),
            );
            return TraceInstance {
                bom_ref,
                binding: TraceBinding {
                    strength: asserted.strength,
                    reason: asserted.reason.clone(),
                    source_doc_id: Some(TraceSourceDocId {
                        sha256: ctx.source_doc_id.sha256.clone(),
                        iri: ctx.source_doc_id.iri.clone(),
                    }),
                    binding_hash: asserted.hash.as_ref().map(|h| h.as_hex().to_string()),
                },
                audit_summary: summary,
            };
        }
        let summary = format!(
            "instance asserts binding to source-doc-sha {} (not in candidate set; \
             {} strength)",
            asserted.source_doc_id.sha256,
            strength_label(&asserted.strength),
        );
        return TraceInstance {
            bom_ref,
            binding: TraceBinding {
                strength: asserted.strength,
                reason: asserted.reason.clone(),
                source_doc_id: Some(TraceSourceDocId {
                    sha256: asserted.source_doc_id.sha256.clone(),
                    iri: asserted.source_doc_id.iri.clone(),
                }),
                binding_hash: asserted.hash.as_ref().map(|h| h.as_hex().to_string()),
            },
            audit_summary: summary,
        };
    }

    // No asserted binding on the image instance — the trace cannot
    // definitively bind this instance to a source. Even if a
    // candidate source SBOM carries the same PURL, that's a
    // heuristic-only match (PURL-collision is plausible across
    // independent build paths — exactly the worked-example concern
    // from US2 AS#4 / SC-003). Return `Unknown` with a reason that
    // names what the trace observed.
    //
    // Per quickstart.md Recipe 6 (b): "image contains code waybill
    // can't trace" is the operator-actionable answer.
    TraceInstance {
        bom_ref,
        binding: TraceBinding {
            strength: BindingStrength::Unknown,
            reason: Some("source-not-found-in-bind-target".to_string()),
            source_doc_id: None,
            binding_hash: None,
        },
        audit_summary:
            "instance has no asserted source-document-binding; no source SBOM bound to this instance"
                .to_string(),
    }
}


#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn cdx_sbom_with_two_instances() -> Value {
        let bound = json!({
            "algo": "v1",
            "hash": "a".repeat(64),
            "source_doc_id": { "sha256": "e".repeat(64) },
            "strength": "verified",
        });
        let bound_str = serde_json::to_string(&bound).unwrap();
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [
                {
                    "type": "library",
                    "name": "net",
                    "version": "0.28.0",
                    "purl": "pkg:golang/golang.org/x/net@v0.28.0",
                    "bom-ref": "golang-net-from-foo",
                    "properties": [{
                        "name": "waybill:source-document-binding",
                        "value": bound_str,
                    }],
                },
                {
                    "type": "library",
                    "name": "net",
                    "version": "0.28.0",
                    "purl": "pkg:golang/golang.org/x/net@v0.28.0",
                    "bom-ref": "golang-net-from-baselayer-server",
                }
            ]
        })
    }

    #[test]
    fn trace_finds_two_instances_one_bound_one_unbound() {
        let image = cdx_sbom_with_two_instances();
        let report = trace(
            "pkg:golang/golang.org/x/net@v0.28.0",
            &image,
            &[],
        );
        assert_eq!(report.instances.len(), 2, "two instances of same PURL");
        let first = &report.instances[0];
        let second = &report.instances[1];
        assert_eq!(first.bom_ref.as_deref(), Some("golang-net-from-foo"));
        assert_eq!(first.binding.strength, BindingStrength::Verified);
        assert_eq!(second.bom_ref.as_deref(), Some("golang-net-from-baselayer-server"));
        assert_eq!(second.binding.strength, BindingStrength::Unknown);
        assert_eq!(
            second.binding.reason.as_deref(),
            Some("source-not-found-in-bind-target")
        );
    }

    #[test]
    fn trace_no_instances_returns_empty_list() {
        let image = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": []
        });
        let report = trace(
            "pkg:cargo/no-such-thing@9.9.9",
            &image,
            &[],
        );
        assert_eq!(report.instances.len(), 0);
    }
}
