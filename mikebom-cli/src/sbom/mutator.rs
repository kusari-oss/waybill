//! RFC 6902 JSON Patch applier + provenance recorder.
//!
//! Called from `mikebom sbom enrich` to apply operator-supplied patches
//! to a CycloneDX SBOM. Every patch is recorded under the SBOM's
//! top-level `properties[]` array as a `mikebom:enrichment-patch[N]`
//! entry carrying the author, timestamp, base-attestation SHA-256 (if
//! any), and op count. Downstream consumers walking the SBOM can tell
//! attested data from post-hoc enrichment via this property group.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum EnrichmentError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid JSON Patch (RFC 6902): {0}")]
    InvalidPatch(String),

    #[error("patch application failed: {0}")]
    PatchApply(String),
}

/// Per-patch provenance, embedded in the SBOM's properties after apply.
#[derive(Debug, Clone)]
pub struct EnrichmentPatch<'a> {
    /// RFC 6902 operations.
    pub operations: &'a Value,
    /// Recorded author identifier (email, name, "unknown").
    pub author: &'a str,
    /// Timestamp the enrichment was applied.
    pub timestamp: DateTime<Utc>,
    /// Optional SHA-256 hex of the base attestation file the SBOM was
    /// derived from; lets verifiers walk back to the attested source.
    pub base_attestation_sha256: Option<String>,
}

/// Compute a hex-encoded SHA-256 of a file.
pub fn attestation_sha256(path: &Path) -> Result<String, EnrichmentError> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    Ok(out)
}

/// Apply a JSON Patch to a mutable SBOM `Value`. On failure, `sbom` is
/// left at its partial state — callers needing atomicity should clone
/// before calling.
pub fn apply_patch(sbom: &mut Value, ops: &Value) -> Result<usize, EnrichmentError> {
    let patch: json_patch::Patch = serde_json::from_value(ops.clone())
        .map_err(|e| EnrichmentError::InvalidPatch(e.to_string()))?;
    let count = patch.0.len();
    json_patch::patch(sbom, &patch).map_err(|e| EnrichmentError::PatchApply(e.to_string()))?;
    Ok(count)
}

/// Append a `mikebom:enrichment-patch[N]` entry to the SBOM's top-level
/// `properties[]` array. If `properties` is absent, it's created.
pub fn append_provenance_property(
    sbom: &mut Value,
    patch_index: usize,
    patch: &EnrichmentPatch<'_>,
) -> Result<(), EnrichmentError> {
    let op_count = patch.operations.as_array().map(|a| a.len()).unwrap_or(0);
    let mut value_obj = serde_json::Map::new();
    value_obj.insert(
        "author".to_string(),
        Value::String(patch.author.to_string()),
    );
    value_obj.insert(
        "timestamp".to_string(),
        Value::String(
            patch
                .timestamp
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ),
    );
    value_obj.insert("op_count".to_string(), Value::Number(op_count.into()));
    if let Some(ref sha) = patch.base_attestation_sha256 {
        value_obj.insert("base_attestation".to_string(), Value::String(sha.clone()));
    }
    let value_json = Value::Object(value_obj).to_string();

    let property = serde_json::json!({
        "name": format!("mikebom:enrichment-patch[{patch_index}]"),
        "value": value_json,
    });

    // Ensure top-level "properties" array exists.
    let obj = sbom
        .as_object_mut()
        .ok_or_else(|| EnrichmentError::InvalidPatch("SBOM root is not an object".to_string()))?;
    let props = obj
        .entry("properties")
        .or_insert_with(|| Value::Array(Vec::new()));
    let arr = props
        .as_array_mut()
        .ok_or_else(|| EnrichmentError::InvalidPatch("properties is not an array".to_string()))?;
    arr.push(property);
    Ok(())
}

/// Full enrichment pipeline: apply patch(es) in order, record
/// provenance, return the mutated SBOM as an owned `Value`.
pub fn enrich(
    sbom_in: &Value,
    patches: &[EnrichmentPatch<'_>],
) -> Result<Value, EnrichmentError> {
    let mut sbom = sbom_in.clone();
    for (i, patch) in patches.iter().enumerate() {
        let _ops = apply_patch(&mut sbom, patch.operations)?;
        append_provenance_property(&mut sbom, i, patch)?;
    }
    Ok(sbom)
}

// =====================================================================
// Milestone 072 / T020 — VEX propagation with binding-strength gating.
// =====================================================================
//
// Reads a source-tier OpenVEX document (any JSON file matching the
// `https://openvex.dev/ns/v0.2.0` shape — minimum: an object with a
// `statements` array, each with a `products` array and a `status`
// field) and propagates each statement onto matching components in
// the target SBOM. The propagation respects each target component's
// `mikebom:source-document-binding` annotation per
// `contracts/openvex-instance-identifiers.md` C-3 + C-4 + C-5.
//
// Three modes from `crate::binding::VexPropagationMode`:
//
// - `Permissive` — broadcast unchanged to every PURL match. Pre-072
//   semantic preserved.
// - `Caveated` — broadcast to every PURL match, attaching a
//   `mikebom:vex-binding-status: unverified` caveat on every instance
//   whose binding strength is not `verified` (per FR-008 corner-case
//   F2-b). Verified-bound instances get the propagated statement
//   clean.
// - `Strict` — propagate ONLY to verified-bound instances. Refuse the
//   rest with a structured refusal-rationale annotation under
//   `mikebom:enrichment-patch[N]`. Exit non-zero per VR-006.
//
// The propagation result is recorded both in the target SBOM (as
// new entries in `vulnerabilities[]` and as `mikebom:enrichment-
// patch[N]` provenance entries) and returned as a `PropagationReport`
// so the CLI can render a per-instance summary and pick an exit code.

use mikebom::binding::{
    deserialize_from_cdx_property, BindingStrength, SourceDocumentBinding, VexPropagationMode,
    BINDING_PROPERTY_NAME,
};

/// One target-side instance discovered during PURL match.
#[derive(Debug, Clone)]
pub struct TargetInstance {
    /// PURL the instance matches (`components[].purl`).
    pub purl: String,
    /// CDX `bom-ref` for this instance — the per-instance
    /// identifier T020 propagates into `OpenVexProduct.identifiers`.
    pub bom_ref: Option<String>,
    /// Decoded `mikebom:source-document-binding` annotation, if
    /// present. `None` when the target component has no binding —
    /// this counts as `Unknown` strength for propagation purposes.
    pub binding: Option<SourceDocumentBinding>,
}

impl TargetInstance {
    fn effective_strength(&self) -> BindingStrength {
        self.binding
            .as_ref()
            .map(|b| b.strength)
            .unwrap_or(BindingStrength::Unknown)
    }
}

/// Per-statement propagation outcome for one (statement, instance)
/// pair.
#[derive(Debug, Clone)]
pub enum PropagationOutcome {
    /// Statement was propagated cleanly. `verified` binding (or
    /// `permissive` mode regardless of strength).
    Propagated,
    /// Statement was propagated WITH a `mikebom:vex-binding-status:
    /// unverified` caveat — the instance's binding strength is not
    /// `verified` and the mode is `caveated`.
    Caveated { reason: String },
    /// Statement was REFUSED — strict mode, non-verified binding.
    /// Exit non-zero per VR-006.
    Refused { reason: String },
}

/// One row of the propagation report.
#[derive(Debug, Clone)]
pub struct PropagationRow {
    pub vulnerability: String,
    pub purl: String,
    pub bom_ref: Option<String>,
    pub strength: BindingStrength,
    pub outcome: PropagationOutcome,
}

/// Propagation report — drives the CLI exit code (non-zero when any
/// `Refused` row appears, per VR-006) and renders the operator-
/// visible summary.
#[derive(Debug, Clone, Default)]
pub struct PropagationReport {
    pub rows: Vec<PropagationRow>,
    pub statements_propagated: usize,
    pub statements_caveated: usize,
    pub statements_refused: usize,
}

impl PropagationReport {
    /// `true` when no statement was refused. Drives the
    /// `mikebom sbom enrich --vex-propagation-mode strict` exit code
    /// (non-zero when false) per VR-006.
    pub fn is_clean(&self) -> bool {
        self.statements_refused == 0
    }
}

/// Walk the target SBOM and find every component instance whose
/// PURL matches `purl`. Decodes each instance's
/// `mikebom:source-document-binding` annotation. Currently CDX-only
/// (target SBOM the `mikebom sbom enrich` command operates on is
/// CycloneDX per the existing JSON-Patch path).
fn find_target_instances(target_sbom: &Value, purl: &str) -> Vec<TargetInstance> {
    let mut out = Vec::new();
    let Some(components) = target_sbom.get("components").and_then(|v| v.as_array()) else {
        return out;
    };
    for c in components.iter() {
        let component_purl = match c.get("purl").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        if component_purl != purl {
            continue;
        }
        let bom_ref = c
            .get("bom-ref")
            .and_then(|v| v.as_str())
            .map(String::from);
        let mut binding: Option<SourceDocumentBinding> = None;
        if let Some(props) = c.get("properties").and_then(|v| v.as_array()) {
            for p in props {
                if p.get("name").and_then(|v| v.as_str()) == Some(BINDING_PROPERTY_NAME) {
                    if let Some(value_str) = p.get("value").and_then(|v| v.as_str()) {
                        binding = deserialize_from_cdx_property(value_str).ok();
                    }
                }
            }
        }
        out.push(TargetInstance {
            purl: component_purl.to_string(),
            bom_ref,
            binding,
        });
    }
    out
}

/// Extract an OpenVEX statement's source-side per-instance identifier
/// from `Product.identifiers` per `contracts/openvex-instance-
/// identifiers.md` C-1. Returns `None` when the source statement has
/// no per-instance identifier (typical of pre-072 OpenVEX input) —
/// the matching falls back to one-to-many on PURL per analyze F2.
fn product_per_instance_id(product: &Value) -> Option<String> {
    let identifiers = product.get("identifiers")?.as_object()?;
    if let Some(Value::String(s)) = identifiers.get("cyclonedx-bom-ref") {
        return Some(s.clone());
    }
    if let Some(Value::String(s)) = identifiers.get("spdx-spdxid") {
        return Some(s.clone());
    }
    None
}

/// Build the propagated CDX `vulnerabilities[]` entry for a (vuln,
/// matched-instances) tuple. Per T022, each `affects[]` entry binds
/// to a specific bom-ref; the `mikebom:vex-binding-status` field is
/// a sibling on each `affects` entry per
/// `contracts/openvex-instance-identifiers.md` C-5.
fn build_vulnerability_entry(
    vuln_name: &str,
    status: &str,
    justification: Option<&str>,
    rows: &[(TargetInstance, PropagationOutcome)],
) -> Value {
    let mut affects = Vec::with_capacity(rows.len());
    for (inst, outcome) in rows {
        // Skip refused rows — those don't appear in `affects[]`.
        if matches!(outcome, PropagationOutcome::Refused { .. }) {
            continue;
        }
        let ref_value = inst
            .bom_ref
            .clone()
            .unwrap_or_else(|| inst.purl.clone());
        let mut entry = serde_json::Map::new();
        entry.insert("ref".to_string(), Value::String(ref_value));
        if let PropagationOutcome::Caveated { reason } = outcome {
            let mut caveat = serde_json::Map::new();
            caveat.insert(
                "status".to_string(),
                Value::String("unverified".to_string()),
            );
            caveat.insert("reason".to_string(), Value::String(reason.clone()));
            entry.insert(
                "mikebom:vex-binding-status".to_string(),
                Value::Object(caveat),
            );
        }
        affects.push(Value::Object(entry));
    }

    let mut analysis = serde_json::Map::new();
    analysis.insert("state".to_string(), Value::String(status.to_string()));
    if let Some(j) = justification {
        analysis.insert("justification".to_string(), Value::String(j.to_string()));
    }

    let mut vuln_obj = serde_json::Map::new();
    vuln_obj.insert("id".to_string(), Value::String(vuln_name.to_string()));
    vuln_obj.insert("analysis".to_string(), Value::Object(analysis));
    vuln_obj.insert("affects".to_string(), Value::Array(affects));
    Value::Object(vuln_obj)
}

/// Append an entry to `vulnerabilities[]` on the target SBOM,
/// creating the array if absent.
fn push_vulnerability(target: &mut Value, entry: Value) -> Result<(), EnrichmentError> {
    let obj = target.as_object_mut().ok_or_else(|| {
        EnrichmentError::InvalidPatch("SBOM root is not an object".to_string())
    })?;
    let vulns = obj
        .entry("vulnerabilities")
        .or_insert_with(|| Value::Array(Vec::new()));
    let arr = vulns.as_array_mut().ok_or_else(|| {
        EnrichmentError::InvalidPatch("vulnerabilities is not an array".to_string())
    })?;
    arr.push(entry);
    Ok(())
}

/// Append a refusal-rationale entry under `properties[]` so the
/// strict-mode operator can audit which (vuln, instance) pairs were
/// refused. Reuses the milestone-006 `mikebom:enrichment-patch[N]`
/// provenance scheme so existing tooling doesn't need a new property
/// type.
fn append_refusal_rationale(
    target: &mut Value,
    refusals: &[PropagationRow],
    timestamp: DateTime<Utc>,
) -> Result<(), EnrichmentError> {
    if refusals.is_empty() {
        return Ok(());
    }
    let mut entries: Vec<Value> = Vec::with_capacity(refusals.len());
    for r in refusals {
        let reason = match &r.outcome {
            PropagationOutcome::Refused { reason } => reason.clone(),
            _ => continue,
        };
        let mut e = serde_json::Map::new();
        e.insert(
            "vulnerability".to_string(),
            Value::String(r.vulnerability.clone()),
        );
        e.insert("purl".to_string(), Value::String(r.purl.clone()));
        if let Some(br) = &r.bom_ref {
            e.insert("bom_ref".to_string(), Value::String(br.clone()));
        }
        e.insert(
            "binding_strength".to_string(),
            Value::String(match r.strength {
                BindingStrength::Verified => "verified".to_string(),
                BindingStrength::Weak => "weak".to_string(),
                BindingStrength::Unknown => "unknown".to_string(),
            }),
        );
        e.insert("reason".to_string(), Value::String(reason));
        entries.push(Value::Object(e));
    }
    if entries.is_empty() {
        return Ok(());
    }
    let mut value_obj = serde_json::Map::new();
    value_obj.insert(
        "kind".to_string(),
        Value::String("vex-propagation-refusal".to_string()),
    );
    value_obj.insert(
        "timestamp".to_string(),
        Value::String(timestamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
    );
    value_obj.insert("refusals".to_string(), Value::Array(entries));
    let value_json = Value::Object(value_obj).to_string();

    let property = serde_json::json!({
        "name": "mikebom:vex-propagation-refusals",
        "value": value_json,
    });

    let obj = target.as_object_mut().ok_or_else(|| {
        EnrichmentError::InvalidPatch("SBOM root is not an object".to_string())
    })?;
    let props = obj
        .entry("properties")
        .or_insert_with(|| Value::Array(Vec::new()));
    let arr = props.as_array_mut().ok_or_else(|| {
        EnrichmentError::InvalidPatch("properties is not an array".to_string())
    })?;
    arr.push(property);
    Ok(())
}

/// Set or update `OpenVexProduct.identifiers` for one matched
/// target-side instance — the propagation-path identifier population
/// per T020. The map merges the source-supplied keys (`purl`) with
/// the target-known keys (`cyclonedx-bom-ref`).
///
/// Returned as a `serde_json::Map` ready to slot into the rebuilt
/// statement's `products[]`. Pure helper — no I/O.
fn build_product_identifiers(
    purl: &str,
    bom_ref: Option<&str>,
) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("purl".to_string(), Value::String(purl.to_string()));
    if let Some(br) = bom_ref {
        m.insert(
            "cyclonedx-bom-ref".to_string(),
            Value::String(br.to_string()),
        );
    }
    m
}

/// Reason string for a non-verified caveat. Includes the binding
/// strength and any source-tier-supplied reason for transparency.
fn caveat_reason(binding: &Option<SourceDocumentBinding>) -> String {
    match binding {
        Some(b) => match (b.strength, &b.reason) {
            (BindingStrength::Weak, Some(r)) => {
                format!("binding-strength-weak: {r}")
            }
            (BindingStrength::Weak, None) => {
                "binding-strength-weak: insufficient evidence to verify cross-tier identity".to_string()
            }
            (BindingStrength::Unknown, Some(r)) => {
                format!("binding-strength-unknown: {r}")
            }
            (BindingStrength::Unknown, None) => {
                "binding-strength-unknown: no cross-tier identity evidence".to_string()
            }
            (BindingStrength::Verified, _) => {
                // Should never reach here — verified bindings don't
                // get a caveat. Defensive default.
                "binding-strength-verified: no caveat needed".to_string()
            }
        },
        None => "no-binding-annotation: target component has no source-document-binding".to_string(),
    }
}

/// Propagate VEX statements from `source_vex` (an OpenVEX 0.2.0
/// document parsed as `serde_json::Value`) to `target_sbom` (a
/// CycloneDX SBOM, mutated in place). Returns a per-instance
/// `PropagationReport`.
///
/// Algorithm per `contracts/openvex-instance-identifiers.md` C-3 +
/// C-4 + C-5:
///
/// For each `statement` in `source_vex.statements[]`, for each
/// `product` in `statement.products[]`:
///
/// - Extract `purl` (from `Product.@id` or `identifiers.purl`).
/// - If `Product.identifiers.cyclonedx-bom-ref` (or `spdx-spdxid`)
///   is set, match exactly one target instance with that
///   bom-ref/SPDXID; else match all target instances with the same
///   PURL (one-to-many broadcast).
/// - Apply mode-specific behavior to each matched instance:
///   `Permissive` propagates clean to every match; `Caveated`
///   propagates clean to verified instances and adds
///   `mikebom:vex-binding-status: unverified` to non-verified
///   instances; `Strict` propagates clean to verified instances and
///   REFUSES non-verified instances with a refusal-rationale
///   annotation, exiting non-zero per VR-006.
///
/// Then aggregate per-instance outcomes into one `vulnerabilities[]`
/// entry per (vuln, matched-instances) tuple. Per T022, each
/// `affects[].ref` is a specific bom-ref; the
/// `mikebom:vex-binding-status` field is a sibling on each
/// `affects` entry.
pub fn propagate_vex_with_binding(
    mode: VexPropagationMode,
    source_vex: &Value,
    target_sbom: &mut Value,
    timestamp: DateTime<Utc>,
) -> Result<PropagationReport, EnrichmentError> {
    let mut report = PropagationReport::default();
    let Some(statements) = source_vex.get("statements").and_then(|v| v.as_array()) else {
        // Empty/missing statements list — no-op, clean report.
        return Ok(report);
    };

    let mut all_refusals: Vec<PropagationRow> = Vec::new();

    for statement in statements {
        let vuln_name = match statement
            .get("vulnerability")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
        {
            Some(s) => s.to_string(),
            None => continue,
        };
        let status = match statement.get("status").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let justification = statement
            .get("justification")
            .and_then(|v| v.as_str())
            .map(String::from);

        let products = match statement.get("products").and_then(|v| v.as_array()) {
            Some(p) => p,
            None => continue,
        };

        // Aggregate matched instances across all products in this
        // statement. Each (vuln, instance) pair becomes one row.
        let mut matched_rows: Vec<(TargetInstance, PropagationOutcome)> = Vec::new();

        for product in products {
            // Source-side PURL: prefer `@id`, fall back to
            // `identifiers.purl` per C-1.
            let purl = match product
                .get("@id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    product
                        .get("identifiers")
                        .and_then(|v| v.get("purl"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                }) {
                Some(p) => p,
                None => continue,
            };

            let per_instance_id = product_per_instance_id(product);
            let candidates = find_target_instances(target_sbom, &purl);

            // Apply matching rules (analyze F2):
            //   per-instance id present → 1:1 match
            //   per-instance id absent  → 1:many (all PURL matches)
            let matched: Vec<TargetInstance> = if let Some(id) = per_instance_id {
                candidates
                    .into_iter()
                    .filter(|c| c.bom_ref.as_deref() == Some(id.as_str()))
                    .collect()
            } else {
                candidates
            };

            for inst in matched {
                let strength = inst.effective_strength();
                let outcome = match (mode, strength) {
                    (VexPropagationMode::Permissive, _) => PropagationOutcome::Propagated,
                    (VexPropagationMode::Caveated, BindingStrength::Verified) => {
                        PropagationOutcome::Propagated
                    }
                    (VexPropagationMode::Caveated, _) => PropagationOutcome::Caveated {
                        reason: caveat_reason(&inst.binding),
                    },
                    (VexPropagationMode::Strict, BindingStrength::Verified) => {
                        PropagationOutcome::Propagated
                    }
                    (VexPropagationMode::Strict, _) => PropagationOutcome::Refused {
                        reason: caveat_reason(&inst.binding),
                    },
                };
                matched_rows.push((inst, outcome));
            }
        }

        if matched_rows.is_empty() {
            continue;
        }

        // Record per-instance outcome rows on the report and
        // emit one `vulnerabilities[]` entry covering the
        // statement.
        for (inst, outcome) in &matched_rows {
            let row = PropagationRow {
                vulnerability: vuln_name.clone(),
                purl: inst.purl.clone(),
                bom_ref: inst.bom_ref.clone(),
                strength: inst.effective_strength(),
                outcome: outcome.clone(),
            };
            match &row.outcome {
                PropagationOutcome::Propagated => report.statements_propagated += 1,
                PropagationOutcome::Caveated { .. } => report.statements_caveated += 1,
                PropagationOutcome::Refused { .. } => {
                    report.statements_refused += 1;
                    all_refusals.push(row.clone());
                }
            }
            report.rows.push(row);
        }

        // Build & push the `vulnerabilities[]` entry. Refused rows
        // are filtered out inside `build_vulnerability_entry`.
        let has_non_refused = matched_rows
            .iter()
            .any(|(_, o)| !matches!(o, PropagationOutcome::Refused { .. }));
        if has_non_refused {
            let entry = build_vulnerability_entry(
                &vuln_name,
                &status,
                justification.as_deref(),
                &matched_rows,
            );
            push_vulnerability(target_sbom, entry)?;
        }
    }

    // Refusal rationale annotation — surfaces strict-mode refusals
    // so the operator can audit. Constitution Principle X
    // transparency: failures must be explicit, not silent.
    append_refusal_rationale(target_sbom, &all_refusals, timestamp)?;

    Ok(report)
}

// Suppress dead_code warning for the propagation-path helper that
// doesn't have any in-tree caller yet — the propagation flow itself
// uses the data inline. Kept for symmetry with the contract C-1
// `Product.identifiers` shape: the moment a future caller wants to
// emit a propagated OpenVEX sidecar, this helper builds the
// identifier map for that sidecar's `products[]` per analyze F1.
#[allow(dead_code)]
pub(crate) fn product_identifiers_for_target(
    purl: &str,
    bom_ref: Option<&str>,
) -> serde_json::Map<String, Value> {
    build_product_identifiers(purl, bom_ref)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_sbom() -> Value {
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [
                {"type": "library", "name": "alpha", "version": "1.0.0"},
                {"type": "library", "name": "beta", "version": "2.0.0"},
                {"type": "library", "name": "gamma", "version": "3.0.0"}
            ]
        })
    }

    #[test]
    fn apply_add_operation_sets_field() {
        let mut sbom = sample_sbom();
        let ops = json!([
            {"op": "add", "path": "/components/0/supplier", "value": {"name": "Example"}}
        ]);
        let n = apply_patch(&mut sbom, &ops).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            sbom["components"][0]["supplier"]["name"],
            json!("Example")
        );
    }

    #[test]
    fn apply_add_operation_appends_to_array() {
        let mut sbom = sample_sbom();
        let ops = json!([
            {"op": "add", "path": "/components/0/licenses", "value": []},
            {"op": "add", "path": "/components/0/licenses/-", "value": {"license": {"id": "Apache-2.0"}}}
        ]);
        apply_patch(&mut sbom, &ops).unwrap();
        assert_eq!(
            sbom["components"][0]["licenses"][0]["license"]["id"],
            json!("Apache-2.0")
        );
    }

    #[test]
    fn invalid_patch_is_reported() {
        let mut sbom = sample_sbom();
        let ops = json!([{"op": "wiggle", "path": "/components", "value": null}]);
        assert!(matches!(
            apply_patch(&mut sbom, &ops),
            Err(EnrichmentError::InvalidPatch(_))
        ));
    }

    #[test]
    fn test_op_failure_aborts_patch() {
        let mut sbom = sample_sbom();
        let ops = json!([
            {"op": "test", "path": "/components/0/name", "value": "NOT_ALPHA"},
            {"op": "add", "path": "/components/0/supplier", "value": {"name": "never-applied"}}
        ]);
        assert!(matches!(
            apply_patch(&mut sbom, &ops),
            Err(EnrichmentError::PatchApply(_))
        ));
        // Partial state: the test op may or may not roll back, so don't
        // assert on sbom contents here.
    }

    #[test]
    fn append_provenance_property_creates_properties_array() {
        let mut sbom = sample_sbom();
        let patch = EnrichmentPatch {
            operations: &json!([{"op": "add", "path": "/x", "value": 1}]),
            author: "security-team@example.com",
            timestamp: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            base_attestation_sha256: Some("abc123".to_string()),
        };
        append_provenance_property(&mut sbom, 0, &patch).unwrap();
        let props = sbom["properties"].as_array().unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0]["name"], json!("mikebom:enrichment-patch[0]"));
        let value_str = props[0]["value"].as_str().unwrap();
        assert!(value_str.contains("security-team@example.com"));
        assert!(value_str.contains("\"base_attestation\":\"abc123\""));
    }

    #[test]
    fn enrich_applies_multiple_patches_in_order() {
        let sbom = sample_sbom();
        let p1_ops = json!([
            {"op": "add", "path": "/components/0/supplier", "value": {"name": "First"}}
        ]);
        let p2_ops = json!([
            {"op": "add", "path": "/components/0/supplier/contact", "value": "ops@example.com"}
        ]);
        let patches = vec![
            EnrichmentPatch {
                operations: &p1_ops,
                author: "alice",
                timestamp: Utc::now(),
                base_attestation_sha256: None,
            },
            EnrichmentPatch {
                operations: &p2_ops,
                author: "bob",
                timestamp: Utc::now(),
                base_attestation_sha256: None,
            },
        ];
        let out = enrich(&sbom, &patches).unwrap();
        assert_eq!(out["components"][0]["supplier"]["name"], json!("First"));
        assert_eq!(
            out["components"][0]["supplier"]["contact"],
            json!("ops@example.com")
        );
        let props = out["properties"].as_array().unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(
            props[0]["name"],
            json!("mikebom:enrichment-patch[0]")
        );
        assert_eq!(
            props[1]["name"],
            json!("mikebom:enrichment-patch[1]")
        );
    }

    #[test]
    fn attestation_sha256_matches_manual_hash() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"attestation-content").unwrap();
        let hex = attestation_sha256(tmp.path()).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(b"attestation-content");
        let mut expected = String::new();
        for b in hasher.finalize() {
            use std::fmt::Write;
            let _ = write!(expected, "{b:02x}");
        }
        assert_eq!(hex, expected);
    }

    // =================================================================
    // Milestone 072 / T020 — VEX propagation engine tests.
    // =================================================================

    use mikebom::binding::{
        serialize_to_cdx_property, BindingHash, BindingHashInputs, SourceDocumentBinding,
        SourceDocumentId,
    };

    fn fixture_now() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn binding_with_strength(
        strength: BindingStrength,
        reason: Option<&str>,
    ) -> SourceDocumentBinding {
        // The binding annotation is opaque to the propagation engine
        // beyond `.strength` and `.reason`; we don't need to compute
        // the inputs in the test fixture, just build a representative
        // SourceDocumentBinding shape with the right strength.
        let _ = BindingHashInputs::empty(); // anchor the import
        let hash = if matches!(strength, BindingStrength::Unknown) {
            None
        } else {
            Some(BindingHash::from_hex("d".repeat(64)).unwrap())
        };
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: "e".repeat(64),
                iri: None,
            },
            hash,
            strength,
            reason: reason.map(String::from),
            algo: "v1".to_string(),
            alias_from: None,
            alias_to: None,
            alias_source: None,
        }
    }

    /// Build a CDX target SBOM with one or more component instances,
    /// each with a `(purl, bom-ref, optional binding)` triple.
    fn cdx_target(
        instances: &[(&str, &str, Option<&SourceDocumentBinding>)],
    ) -> Value {
        let components: Vec<Value> = instances
            .iter()
            .map(|(purl, bom_ref, binding)| {
                let mut c = json!({
                    "type": "library",
                    "name": "test",
                    "version": "1.0.0",
                    "purl": *purl,
                    "bom-ref": *bom_ref,
                });
                if let Some(b) = binding {
                    let serialized = serialize_to_cdx_property(b).unwrap();
                    c.as_object_mut().unwrap().insert(
                        "properties".to_string(),
                        json!([{
                            "name": BINDING_PROPERTY_NAME,
                            "value": serialized,
                        }]),
                    );
                }
                c
            })
            .collect();
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": components,
        })
    }

    /// Build a source-side OpenVEX statement document.
    fn openvex_doc(
        vuln: &str,
        purl: &str,
        status: &str,
        per_instance_id: Option<&str>,
    ) -> Value {
        let mut product = json!({ "@id": purl });
        let mut idents = serde_json::Map::new();
        idents.insert("purl".to_string(), Value::String(purl.to_string()));
        if let Some(id) = per_instance_id {
            idents.insert(
                "cyclonedx-bom-ref".to_string(),
                Value::String(id.to_string()),
            );
        }
        product
            .as_object_mut()
            .unwrap()
            .insert("identifiers".to_string(), Value::Object(idents));

        json!({
            "@context": "https://openvex.dev/ns/v0.2.0",
            "statements": [{
                "vulnerability": { "name": vuln },
                "products": [product],
                "status": status,
                "justification": "vulnerable_code_not_present",
            }]
        })
    }

    /// Permissive mode: propagate to every PURL match, regardless of
    /// binding strength.
    #[test]
    fn permissive_mode_propagates_unconditionally() {
        let unknown = binding_with_strength(BindingStrength::Unknown, Some("no-evidence"));
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", Some(&unknown))]);
        let source = openvex_doc("CVE-2026-0001", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Permissive,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert!(report.is_clean());
        assert_eq!(report.statements_propagated, 1);
        assert_eq!(report.statements_caveated, 0);
        assert_eq!(report.statements_refused, 0);

        let vulns = target["vulnerabilities"].as_array().unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["id"], "CVE-2026-0001");
        let affects = vulns[0]["affects"].as_array().unwrap();
        assert_eq!(affects.len(), 1);
        assert_eq!(affects[0]["ref"], "a-bom-1");
        // Permissive mode → no caveat sibling.
        assert!(affects[0].get("mikebom:vex-binding-status").is_none());
    }

    /// Caveated mode + verified binding → propagate clean.
    #[test]
    fn caveated_mode_propagates_clean_for_verified() {
        let verified = binding_with_strength(BindingStrength::Verified, None);
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", Some(&verified))]);
        let source = openvex_doc("CVE-2026-0002", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Caveated,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert!(report.is_clean());
        assert_eq!(report.statements_propagated, 1);
        assert_eq!(report.statements_caveated, 0);

        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        assert!(affects[0].get("mikebom:vex-binding-status").is_none());
    }

    /// Caveated mode + weak binding → propagate WITH caveat.
    #[test]
    fn caveated_mode_propagates_with_caveat_for_weak() {
        let weak = binding_with_strength(BindingStrength::Weak, Some("no-vcs-commit"));
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", Some(&weak))]);
        let source = openvex_doc("CVE-2026-0003", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Caveated,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert_eq!(report.statements_caveated, 1);
        assert_eq!(report.statements_propagated, 0);

        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        let caveat = affects[0]["mikebom:vex-binding-status"].as_object().unwrap();
        assert_eq!(caveat["status"], "unverified");
        let reason = caveat["reason"].as_str().unwrap();
        assert!(reason.contains("binding-strength-weak"));
        assert!(reason.contains("no-vcs-commit"));
    }

    /// Strict mode + non-verified binding → REFUSED, exit non-zero,
    /// rationale annotation written.
    #[test]
    fn strict_mode_refuses_non_verified_and_records_rationale() {
        let weak = binding_with_strength(BindingStrength::Weak, Some("no-vcs-commit"));
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", Some(&weak))]);
        let source = openvex_doc("CVE-2026-0004", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Strict,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert!(!report.is_clean(), "strict mode must not be clean on weak binding");
        assert_eq!(report.statements_refused, 1);
        assert_eq!(report.statements_propagated, 0);

        // No vulnerabilities[] entry was written for the refused
        // statement (no non-refused matched instances).
        assert!(target.get("vulnerabilities").is_none());

        // Rationale annotation present.
        let props = target["properties"].as_array().unwrap();
        let refusal = props
            .iter()
            .find(|p| p["name"] == "mikebom:vex-propagation-refusals")
            .expect("refusal property present");
        let value_str = refusal["value"].as_str().unwrap();
        assert!(value_str.contains("CVE-2026-0004"));
        assert!(value_str.contains("vex-propagation-refusal"));
    }

    /// Strict mode + verified binding → propagate clean.
    #[test]
    fn strict_mode_allows_verified() {
        let verified = binding_with_strength(BindingStrength::Verified, None);
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", Some(&verified))]);
        let source = openvex_doc("CVE-2026-0005", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Strict,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert!(report.is_clean());
        assert_eq!(report.statements_propagated, 1);
        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        assert!(affects[0].get("mikebom:vex-binding-status").is_none());
    }

    /// Worked-example case (US2 AS#4 / SC-003): two instances of the
    /// same PURL — one verified, one unknown. Source statement has
    /// no per-instance id (broadcast). In `caveated` mode:
    ///   - instance A (verified) propagates clean.
    ///   - instance B (unknown) propagates WITH caveat.
    #[test]
    fn caveated_broadcast_per_purl_handles_mixed_strength_correctly() {
        let verified = binding_with_strength(BindingStrength::Verified, None);
        let unknown = binding_with_strength(BindingStrength::Unknown, Some("base-layer"));
        let mut target = cdx_target(&[
            ("pkg:golang/golang.org/x/net@v0.28.0", "foo-net-instance", Some(&verified)),
            (
                "pkg:golang/golang.org/x/net@v0.28.0",
                "baselayer-net-instance",
                Some(&unknown),
            ),
        ]);
        let source = openvex_doc(
            "CVE-2024-12345",
            "pkg:golang/golang.org/x/net@v0.28.0",
            "not_affected",
            None,
        );

        let report = propagate_vex_with_binding(
            VexPropagationMode::Caveated,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert_eq!(report.statements_propagated, 1, "verified instance");
        assert_eq!(report.statements_caveated, 1, "unverified instance");
        assert_eq!(report.statements_refused, 0);

        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        assert_eq!(affects.len(), 2);

        // Find each instance by ref and check caveat.
        let foo_entry = affects
            .iter()
            .find(|a| a["ref"] == "foo-net-instance")
            .unwrap();
        let baselayer_entry = affects
            .iter()
            .find(|a| a["ref"] == "baselayer-net-instance")
            .unwrap();
        assert!(
            foo_entry.get("mikebom:vex-binding-status").is_none(),
            "verified-bound instance must not carry caveat"
        );
        let caveat = baselayer_entry["mikebom:vex-binding-status"]
            .as_object()
            .unwrap();
        assert_eq!(caveat["status"], "unverified");
    }

    /// Per-instance id in source → 1:1 match against the target
    /// instance with the same bom-ref. Other instance untouched.
    #[test]
    fn per_instance_id_matches_only_specific_instance() {
        let verified = binding_with_strength(BindingStrength::Verified, None);
        let mut target = cdx_target(&[
            ("pkg:cargo/a@1", "a-bom-A", Some(&verified)),
            ("pkg:cargo/a@1", "a-bom-B", Some(&verified)),
        ]);
        let source = openvex_doc(
            "CVE-2026-0006",
            "pkg:cargo/a@1",
            "not_affected",
            Some("a-bom-A"),
        );

        let report = propagate_vex_with_binding(
            VexPropagationMode::Permissive,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert_eq!(report.statements_propagated, 1);
        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        assert_eq!(affects.len(), 1);
        assert_eq!(affects[0]["ref"], "a-bom-A");
    }

    /// Component with no binding annotation at all → counts as
    /// `Unknown` strength (per `effective_strength`). Caveated mode
    /// caveats it.
    #[test]
    fn component_without_binding_treated_as_unknown() {
        let mut target = cdx_target(&[("pkg:cargo/a@1", "a-bom-1", None)]);
        let source = openvex_doc("CVE-2026-0007", "pkg:cargo/a@1", "not_affected", None);

        let report = propagate_vex_with_binding(
            VexPropagationMode::Caveated,
            &source,
            &mut target,
            fixture_now(),
        )
        .unwrap();

        assert_eq!(report.statements_caveated, 1);
        let affects = target["vulnerabilities"][0]["affects"].as_array().unwrap();
        let caveat = affects[0]["mikebom:vex-binding-status"].as_object().unwrap();
        assert_eq!(caveat["status"], "unverified");
        assert!(caveat["reason"]
            .as_str()
            .unwrap()
            .contains("no-binding-annotation"));
    }

    /// Build the propagation-path identifiers helper — public-ish
    /// surface used to populate `OpenVexProduct.identifiers` for
    /// propagated statements per analyze F1.
    #[test]
    fn product_identifiers_for_target_includes_purl_and_bom_ref() {
        let m = product_identifiers_for_target("pkg:cargo/a@1", Some("a-bom-1"));
        assert_eq!(m["purl"], "pkg:cargo/a@1");
        assert_eq!(m["cyclonedx-bom-ref"], "a-bom-1");
    }

    #[test]
    fn product_identifiers_for_target_omits_bom_ref_when_absent() {
        let m = product_identifiers_for_target("pkg:cargo/a@1", None);
        assert_eq!(m["purl"], "pkg:cargo/a@1");
        assert!(!m.contains_key("cyclonedx-bom-ref"));
    }
}
