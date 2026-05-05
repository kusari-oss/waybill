//! Milestone 072 T015 — consumer-side `verify_binding` subroutine.
//!
//! Reads two SBOMs (image-tier + source-tier), walks the image-tier
//! components, decodes each component's `mikebom:source-document-binding`
//! annotation, recomputes the binding hash from the matching source-tier
//! component's evidence, and reports per-component pass/fail.
//!
//! The recompute side for PR-A: the source SBOM is expected to carry a
//! sibling `mikebom:source-document-binding` annotation on the matching
//! component (PURL match). When present, its hash is the recompute
//! reference. When absent, the verifier returns `unknown` strength
//! with reason `source-tier-binding-evidence-missing` per FR-003.
//!
//! Future PR-B work may walk the source-tier component's evidence
//! directly off-disk to recompute lockfile + manifest hashes; the
//! PR-A verify path is the metadata-only case and matches what
//! `mikebom sbom verify-binding` operating purely on two SBOM files
//! can answer.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::binding::{
    deserialize_from_cdx_property, deserialize_from_envelope_value, BindingError,
    BindingHash, BindingStrength, SourceDocumentBinding, BINDING_PROPERTY_NAME,
};

/// One row of the verification report — one image-tier component
/// resolved against the source SBOM.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyRow {
    pub purl: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bom_ref: Option<String>,
    pub strength: BindingStrength,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asserted_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recomputed_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Aggregate verification report across all image-tier components.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub summary: VerifySummary,
    pub rows: Vec<VerifyRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifySummary {
    pub components_checked: usize,
    pub verified: usize,
    pub weak: usize,
    pub unknown: usize,
    pub verification_failures: usize,
}

impl VerifyReport {
    /// `true` when every component verified cleanly. Drives the
    /// `mikebom sbom verify-binding` exit code (non-zero when false)
    /// per FR-005 / VR-005.
    pub fn is_clean(&self) -> bool {
        self.summary.verification_failures == 0
    }

    /// JSON-serialize for `--format json`.
    pub fn to_json_pretty(&self) -> Result<String, BindingError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Plain-text table for `--format table`. One line per row plus
    /// a trailing summary line.
    pub fn to_table(&self) -> String {
        let mut lines = Vec::with_capacity(self.rows.len() + 2);
        lines.push(format!(
            "{:<60}  {:<8}  {:<10}",
            "purl", "strength", "reason",
        ));
        for row in &self.rows {
            lines.push(format!(
                "{:<60}  {:<8}  {}",
                row.purl,
                strength_label(&row.strength),
                row.reason.as_deref().unwrap_or(""),
            ));
        }
        lines.push(format!(
            "summary: checked={} verified={} weak={} unknown={} failures={}",
            self.summary.components_checked,
            self.summary.verified,
            self.summary.weak,
            self.summary.unknown,
            self.summary.verification_failures,
        ));
        lines.join("\n")
    }
}

fn strength_label(s: &BindingStrength) -> &'static str {
    match s {
        BindingStrength::Verified => "verified",
        BindingStrength::Weak => "weak",
        BindingStrength::Unknown => "unknown",
    }
}

/// Top-level entry: load two SBOMs from disk, verify, return a report.
/// Currently parses JSON shapes only (every mikebom emission is JSON).
pub fn verify_binding_from_paths(
    image_sbom_path: &Path,
    source_sbom_path: &Path,
) -> Result<VerifyReport, BindingError> {
    let image_bytes = std::fs::read(image_sbom_path).map_err(|e| BindingError::Io {
        path: image_sbom_path.display().to_string(),
        source: e,
    })?;
    let source_bytes = std::fs::read(source_sbom_path).map_err(|e| BindingError::Io {
        path: source_sbom_path.display().to_string(),
        source: e,
    })?;

    let image: Value = serde_json::from_slice(&image_bytes)?;
    let source: Value = serde_json::from_slice(&source_bytes)?;
    Ok(verify_binding(&image, &source))
}

/// Verify each image-tier component's binding against the source-tier
/// SBOM. Format-agnostic: walks both CDX and SPDX shapes by detecting
/// the document's distinctive top-level fields.
pub fn verify_binding(image_sbom: &Value, source_sbom: &Value) -> VerifyReport {
    // Build a PURL-keyed lookup of source-tier bindings (the
    // recompute reference) once.
    let source_bindings_by_purl = collect_source_bindings(source_sbom);

    // Walk image-tier components; for each, decode the
    // `mikebom:source-document-binding` annotation and compare against
    // the source-tier recompute.
    let mut rows: Vec<VerifyRow> = Vec::new();
    let image_components = walk_image_components(image_sbom);

    for ic in &image_components {
        let purl = ic.purl.clone();
        let bom_ref = ic.bom_ref.clone();

        let asserted = match &ic.binding {
            Some(b) => b,
            None => {
                rows.push(VerifyRow {
                    purl,
                    bom_ref,
                    strength: BindingStrength::Unknown,
                    binding_hash: None,
                    asserted_hash: None,
                    recomputed_hash: None,
                    reason: Some("no-binding-annotation".to_string()),
                });
                continue;
            }
        };

        let asserted_hash = asserted.hash.as_ref().map(|h| h.as_hex().to_string());

        let source_binding = source_bindings_by_purl.get(&purl);
        let recomputed_hash = source_binding
            .and_then(|b| b.hash.as_ref())
            .map(|h| h.as_hex().to_string());

        match (&asserted_hash, &recomputed_hash) {
            (Some(a), Some(r)) if a == r => {
                rows.push(VerifyRow {
                    purl,
                    bom_ref,
                    strength: asserted.strength,
                    binding_hash: Some(a.clone()),
                    asserted_hash: Some(a.clone()),
                    recomputed_hash: Some(r.clone()),
                    reason: asserted.reason.clone(),
                });
            }
            (Some(a), Some(r)) => {
                rows.push(VerifyRow {
                    purl,
                    bom_ref,
                    strength: BindingStrength::Unknown,
                    binding_hash: None,
                    asserted_hash: Some(a.clone()),
                    recomputed_hash: Some(r.clone()),
                    reason: Some("verification-failed".to_string()),
                });
            }
            (None, _) => {
                rows.push(VerifyRow {
                    purl,
                    bom_ref,
                    strength: BindingStrength::Unknown,
                    binding_hash: None,
                    asserted_hash: None,
                    recomputed_hash,
                    reason: asserted
                        .reason
                        .clone()
                        .or_else(|| Some("no-asserted-hash".to_string())),
                });
            }
            (Some(a), None) => {
                rows.push(VerifyRow {
                    purl,
                    bom_ref,
                    strength: BindingStrength::Unknown,
                    binding_hash: None,
                    asserted_hash: Some(a.clone()),
                    recomputed_hash: None,
                    reason: Some("source-tier-binding-evidence-missing".to_string()),
                });
            }
        }
    }

    let mut verified = 0;
    let mut weak = 0;
    let mut unknown = 0;
    let mut failures = 0;
    for row in &rows {
        match row.strength {
            BindingStrength::Verified => verified += 1,
            BindingStrength::Weak => weak += 1,
            BindingStrength::Unknown => {
                unknown += 1;
                if row.reason.as_deref() == Some("verification-failed") {
                    failures += 1;
                }
            }
        }
    }

    VerifyReport {
        summary: VerifySummary {
            components_checked: rows.len(),
            verified,
            weak,
            unknown,
            verification_failures: failures,
        },
        rows,
    }
}

/// One image-tier component's binding-relevant fields.
struct ImageComponent {
    purl: String,
    bom_ref: Option<String>,
    binding: Option<SourceDocumentBinding>,
}

/// Walk image-tier SBOM components across CDX / SPDX 2.3 / SPDX 3.
fn walk_image_components(doc: &Value) -> Vec<ImageComponent> {
    if doc.get("@graph").is_some() {
        walk_spdx3_components(doc)
    } else if doc.get("packages").is_some() {
        walk_spdx23_components(doc)
    } else {
        walk_cdx_image_components(doc)
    }
}

fn walk_cdx_image_components(doc: &Value) -> Vec<ImageComponent> {
    let mut out = Vec::new();
    walk_cdx_recursive(doc, &mut out);
    out
}

fn walk_cdx_recursive(node: &Value, out: &mut Vec<ImageComponent>) {
    if let Some(arr) = node.get("components").and_then(|v| v.as_array()) {
        for c in arr {
            if let Some(comp) = decode_cdx_component(c) {
                out.push(comp);
            }
            walk_cdx_recursive(c, out);
        }
    }
}

fn decode_cdx_component(c: &Value) -> Option<ImageComponent> {
    let purl = c.get("purl").and_then(|v| v.as_str())?.to_string();
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
    Some(ImageComponent {
        purl,
        bom_ref,
        binding,
    })
}

fn walk_spdx23_components(doc: &Value) -> Vec<ImageComponent> {
    let mut out = Vec::new();
    let Some(packages) = doc.get("packages").and_then(|v| v.as_array()) else {
        return out;
    };
    for p in packages {
        let purl = match extract_spdx23_purl(p) {
            Some(x) => x,
            None => continue,
        };
        let spdxid = p.get("SPDXID").and_then(|v| v.as_str()).map(String::from);
        let binding = decode_spdx_envelope_binding_from_annotations(
            p.get("annotations").and_then(|v| v.as_array()),
        );
        out.push(ImageComponent {
            purl,
            bom_ref: spdxid,
            binding,
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
    if v.get("schema").and_then(|x| x.as_str()) != Some("mikebom-annotation/v1") {
        return None;
    }
    if v.get("field").and_then(|x| x.as_str()) != Some(BINDING_PROPERTY_NAME) {
        return None;
    }
    let value = v.get("value")?;
    deserialize_from_envelope_value(value).ok()
}

fn walk_spdx3_components(doc: &Value) -> Vec<ImageComponent> {
    let mut out = Vec::new();
    let Some(graph) = doc.get("@graph").and_then(|v| v.as_array()) else {
        return out;
    };

    let mut by_iri: std::collections::BTreeMap<String, ImageComponent> =
        std::collections::BTreeMap::new();
    for el in graph {
        if el.get("type").and_then(|v| v.as_str()) != Some("software_Package") {
            continue;
        }
        let Some(spdxid) = el.get("spdxId").and_then(|v| v.as_str()) else {
            continue;
        };
        let purl = el
            .get("software_packageUrl")
            .and_then(|v| v.as_str())
            .map(String::from);
        if let Some(purl) = purl {
            by_iri.insert(
                spdxid.to_string(),
                ImageComponent {
                    purl,
                    bom_ref: Some(spdxid.to_string()),
                    binding: None,
                },
            );
        }
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
            if let Some(comp) = by_iri.get_mut(subject) {
                comp.binding = Some(b);
            }
        }
    }

    out.extend(by_iri.into_values());
    out
}

/// Build a PURL-keyed map of source-tier `SourceDocumentBinding`
/// entries — the recompute-side lookup. Reads from the source SBOM's
/// own annotation set (mikebom emits this when the source SBOM was
/// itself produced via `--bind-to-source`).
fn collect_source_bindings(
    doc: &Value,
) -> std::collections::BTreeMap<String, SourceDocumentBinding> {
    let mut out = std::collections::BTreeMap::new();
    for ic in walk_image_components(doc) {
        if let Some(b) = ic.binding {
            out.insert(ic.purl, b);
        }
    }
    out
}

/// Compute a binding hash directly from a project-source-tree path
/// for an ecosystem. Convenience wrapper used by `--bind-to-source`
/// (T027). Returns the hash, derived `BindingStrength`, AND the
/// inputs (the caller may want to log which sides were populated).
pub fn compute_binding_for_source_tree(
    eco: crate::binding::BindingEcosystem,
    source_root: &Path,
) -> Result<(BindingHash, BindingStrength, crate::binding::BindingHashInputs), BindingError> {
    let inputs = crate::binding::extract_source_inputs(eco, source_root);
    let strength = BindingStrength::from_inputs(&inputs);
    let hash = crate::binding::compute_binding_hash(&inputs)?;
    Ok((hash, strength, inputs))
}

/// Loaded source SBOM context used by the `--bind-to-source` flow.
/// Owns the document SHA-256 (the `SourceDocumentId.sha256`), an
/// optional IRI (the file path the user supplied), and the parsed
/// JSON tree so the caller can index PURL → source-tier binding.
#[derive(Debug, Clone)]
pub struct SourceSbomContext {
    pub source_doc_id: crate::binding::SourceDocumentId,
    /// Set of PURLs present in the source SBOM. Used to decide
    /// which image-tier components get a binding annotation.
    pub source_purls: std::collections::BTreeSet<String>,
    /// PURL → source-tier `SourceDocumentBinding` lookup, populated
    /// when the source SBOM ITSELF carries pre-existing bindings
    /// (i.e., the source-tier scan was run with `--bind-to-source`
    /// against a still-earlier tier — typical of mikebom milestone
    /// 072+ build pipelines). Empty when source SBOM has no
    /// pre-existing bindings; then the bind-to-source flow computes
    /// fresh hashes from on-disk evidence.
    pub source_bindings_by_purl:
        std::collections::BTreeMap<String, SourceDocumentBinding>,
}

impl SourceSbomContext {
    /// Load and decode a source SBOM from disk per FR-011.
    pub fn load(path: &Path) -> Result<Self, BindingError> {
        use sha2::Digest;
        let bytes = std::fs::read(path).map_err(|e| BindingError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let doc: Value = serde_json::from_slice(&bytes)?;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        let sha256 = data_encoding::HEXLOWER.encode(&hasher.finalize());
        let source_doc_id = crate::binding::SourceDocumentId {
            sha256,
            iri: Some(path.display().to_string()),
        };

        let mut source_purls = std::collections::BTreeSet::new();
        let mut source_bindings_by_purl = std::collections::BTreeMap::new();
        for ic in walk_image_components(&doc) {
            source_purls.insert(ic.purl.clone());
            if let Some(b) = ic.binding {
                source_bindings_by_purl.insert(ic.purl, b);
            }
        }

        Ok(Self {
            source_doc_id,
            source_purls,
            source_bindings_by_purl,
        })
    }

    /// Build a `SourceDocumentBinding` for the supplied PURL.
    ///
    /// Behavior:
    ///
    /// - PURL is in `source_purls` AND a sibling `SourceDocumentBinding`
    ///   exists in `source_bindings_by_purl` → return a copy of that
    ///   binding (provenance preserved).
    /// - PURL is in `source_purls` but no sibling binding → return
    ///   `Unknown { reason: "source-tier-binding-evidence-missing" }`.
    /// - PURL is NOT in `source_purls` → return `Unknown { reason:
    ///   "source-not-found-in-bind-target" }` per FR-003.
    pub fn binding_for_purl(&self, purl: &str) -> SourceDocumentBinding {
        if !self.source_purls.contains(purl) {
            return SourceDocumentBinding::unknown(
                self.source_doc_id.clone(),
                "source-not-found-in-bind-target",
            );
        }
        match self.source_bindings_by_purl.get(purl) {
            Some(b) => SourceDocumentBinding {
                source_doc_id: self.source_doc_id.clone(),
                hash: b.hash.clone(),
                strength: b.strength,
                reason: b.reason.clone(),
                algo: b.algo.clone(),
            },
            None => SourceDocumentBinding::unknown(
                self.source_doc_id.clone(),
                "source-tier-binding-evidence-missing",
            ),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::binding::{
        compute_binding_hash, BindingHash, BindingHashInputs, SourceDocumentId,
    };
    use serde_json::json;

    fn cdx_sbom_with_binding(purl: &str, binding: &SourceDocumentBinding) -> Value {
        let serialized = crate::binding::serialize_to_cdx_property(binding).unwrap();
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [{
                "type": "library",
                "name": "foo",
                "version": "1.0.0",
                "purl": purl,
                "bom-ref": format!("{}-bom", purl),
                "properties": [{
                    "name": BINDING_PROPERTY_NAME,
                    "value": serialized,
                }],
            }],
        })
    }

    fn fixture_inputs() -> BindingHashInputs {
        BindingHashInputs {
            vcs: Some("deadbeef0123456789abcdef0123456789abcdef".to_string()),
            lockfile: Some("a".repeat(64)),
            manifest: Some("b".repeat(64)),
        }
    }

    fn fixture_binding(hash: BindingHash, source_sha: &str) -> SourceDocumentBinding {
        SourceDocumentBinding {
            source_doc_id: SourceDocumentId {
                sha256: source_sha.to_string(),
                iri: None,
            },
            hash: Some(hash),
            strength: BindingStrength::Verified,
            reason: None,
            algo: "v1".to_string(),
        }
    }

    /// Clean verify: image asserts hash X, source asserts hash X →
    /// verified, no failures.
    #[test]
    fn verify_clean_match() {
        let inputs = fixture_inputs();
        let h = compute_binding_hash(&inputs).unwrap();
        let binding = fixture_binding(h.clone(), "feedface");
        let purl = "pkg:cargo/foo@1.0.0";

        let image = cdx_sbom_with_binding(purl, &binding);
        let source = cdx_sbom_with_binding(purl, &binding);

        let report = verify_binding(&image, &source);
        assert!(report.is_clean(), "expected clean verify");
        assert_eq!(report.summary.components_checked, 1);
        assert_eq!(report.summary.verified, 1);
        assert_eq!(report.summary.verification_failures, 0);
    }

    /// Drift verify: image asserts hash X, source asserts hash Y →
    /// `verification-failed`, exit non-zero.
    #[test]
    fn verify_hash_mismatch_reports_failure() {
        let h_image = compute_binding_hash(&fixture_inputs()).unwrap();
        let h_source = compute_binding_hash(&BindingHashInputs {
            vcs: Some("c".repeat(40)),
            lockfile: Some("d".repeat(64)),
            manifest: Some("e".repeat(64)),
        })
        .unwrap();

        let purl = "pkg:cargo/foo@1.0.0";
        let image = cdx_sbom_with_binding(purl, &fixture_binding(h_image, "ff"));
        let source = cdx_sbom_with_binding(purl, &fixture_binding(h_source, "ff"));

        let report = verify_binding(&image, &source);
        assert!(!report.is_clean(), "expected verification failure");
        assert_eq!(report.summary.verification_failures, 1);
        assert_eq!(report.rows[0].reason.as_deref(), Some("verification-failed"));
    }

    /// No binding on image → unknown with `no-binding-annotation`.
    #[test]
    fn verify_image_with_no_binding_reports_unknown() {
        let image = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [{
                "type": "library",
                "name": "foo",
                "version": "1.0.0",
                "purl": "pkg:cargo/foo@1.0.0",
            }],
        });
        let source = json!({"bomFormat": "CycloneDX", "specVersion": "1.6", "components": []});
        let report = verify_binding(&image, &source);
        assert_eq!(report.summary.components_checked, 1);
        assert_eq!(report.summary.unknown, 1);
        assert_eq!(report.summary.verification_failures, 0);
        assert_eq!(
            report.rows[0].reason.as_deref(),
            Some("no-binding-annotation"),
        );
    }

    /// Image asserts hash but source has no matching component →
    /// `source-tier-binding-evidence-missing`.
    #[test]
    fn verify_no_source_match_reports_missing_evidence() {
        let h = compute_binding_hash(&fixture_inputs()).unwrap();
        let purl = "pkg:cargo/foo@1.0.0";
        let image = cdx_sbom_with_binding(purl, &fixture_binding(h, "ff"));
        let source = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "components": [],
        });
        let report = verify_binding(&image, &source);
        assert_eq!(
            report.rows[0].reason.as_deref(),
            Some("source-tier-binding-evidence-missing"),
        );
    }

    #[test]
    fn report_is_clean_returns_false_on_failure() {
        let report = VerifyReport {
            summary: VerifySummary {
                components_checked: 1,
                verified: 0,
                weak: 0,
                unknown: 1,
                verification_failures: 1,
            },
            rows: vec![],
        };
        assert!(!report.is_clean());
    }
}
