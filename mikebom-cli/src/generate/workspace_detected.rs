//! Milestone 176 (US3 / FR-003): compute the document-scope
//! `mikebom:workspaces-detected` annotation value from the per-
//! component `mikebom:workspace-member` values populated in
//! `scan_fs::tag_components_with_workspace_member`.
//!
//! Shared across all three format emitters (CDX 1.6, SPDX 2.3, SPDX
//! 3.0.1) so the FR-012 cross-annotation invariant — C121.value ==
//! sorted-deduplicated union of every C120.value — is guaranteed by
//! construction: all three emitters compute the aggregate the same
//! way from the same source.
//!
//! Returns an alphabetically-sorted `Vec<String>` (empty when no
//! workspace attribution was found — caller MUST omit the annotation
//! entirely on empty per FR-003).

use mikebom_common::resolution::ResolvedComponent;
use std::collections::BTreeSet;

/// Union every component's `mikebom:workspace-member` annotation
/// values into a single sorted-deduplicated `Vec<String>`.
///
/// Reads `c.extra_annotations["mikebom:workspace-member"]` — a
/// `Value::String` carrying a JSON-encoded array (per the m134 /
/// m147 / m173 wire convention). Silently skips components whose
/// entry is absent, non-string, or contains malformed JSON — the
/// same forgiveness policy the CDX/SPDX emitters apply to the
/// extra_annotations bag generally.
pub fn compute(components: &[ResolvedComponent]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for c in components {
        let Some(v) = c.extra_annotations.get("mikebom:workspace-member") else {
            continue;
        };
        let Some(s) = v.as_str() else {
            continue;
        };
        let Ok(paths) = serde_json::from_str::<Vec<String>>(s) else {
            continue;
        };
        for p in paths {
            out.insert(p);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{ResolutionEvidence, ResolutionTechnique};
    use mikebom_common::types::purl::Purl;

    fn stub_component(workspace_member: Option<serde_json::Value>) -> ResolvedComponent {
        let purl = Purl::new("pkg:pypi/stub@0.1.0").unwrap();
        let mut c = ResolvedComponent {
            name: purl.name().to_string(),
            version: purl.version().unwrap_or("0.0.0").to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 1.0,
                source_connection_ids: Vec::new(),
                source_file_paths: Vec::new(),
                deps_dev_match: None,
            },
            licenses: Vec::new(),
            concluded_licenses: Vec::new(),
            hashes: Vec::new(),
            supplier: None,
            cpes: Vec::new(),
            advisories: Vec::new(),
            occurrences: Vec::new(),
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: Vec::new(),
            extra_annotations: Default::default(),
            binary_role: None,
        };
        if let Some(v) = workspace_member {
            c.extra_annotations
                .insert("mikebom:workspace-member".to_string(), v);
        }
        c
    }

    #[test]
    fn empty_when_no_components() {
        assert!(compute(&[]).is_empty());
    }

    #[test]
    fn empty_when_no_components_carry_annotation() {
        let comps = [stub_component(None)];
        assert!(compute(&comps).is_empty());
    }

    #[test]
    fn unions_and_sorts_alphabetically() {
        let comps = [
            stub_component(Some(serde_json::Value::String(
                "[\"subproject_b\",\"subproject_a\"]".into(),
            ))),
            stub_component(Some(serde_json::Value::String(
                "[\".\",\"subproject_a\"]".into(),
            ))),
        ];
        assert_eq!(
            compute(&comps),
            vec![
                ".".to_string(),
                "subproject_a".to_string(),
                "subproject_b".to_string()
            ]
        );
    }

    #[test]
    fn skips_malformed_json() {
        let comps = [stub_component(Some(serde_json::Value::String(
            "not valid json".into(),
        )))];
        assert!(compute(&comps).is_empty());
    }

    #[test]
    fn skips_non_string_value() {
        let comps = [stub_component(Some(serde_json::Value::Bool(true)))];
        assert!(compute(&comps).is_empty());
    }
}
