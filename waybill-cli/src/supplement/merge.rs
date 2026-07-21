// Milestone 119 — supplement merge into the scanner's resolved-component
// stream. Runs once per scan, after the scanner finishes discovery and
// dedup, before the CDX/SPDX builders consume the component set.
//
// The merge is PURL-keyed exact-match (FR-010 / clarification Q2). Two
// cases:
//
// - **Solo**: supplement PURL doesn't collide with any scanner PURL.
//   A new `ResolvedComponent` is constructed from the supplement's
//   declared fields, tagged `waybill:source-tier = "declared"`, and
//   appended to the output components vec.
// - **Collision**: supplement PURL matches a scanner PURL. The
//   `conflict::resolve_component()` function applies the FR-006/FR-007
//   partition; each conflict is stamped on the merged component's
//   `waybill:assertion-conflict` array.
//
// FR-015 safety property: `merge_outcome.components.len() >=
// scanner_components.len()` ALWAYS — supplement entries never suppress
// scanner output. Collisions REPLACE; solos APPEND; scanner-only
// passes through unchanged.

use std::collections::HashMap;

use waybill_common::resolution::{
    EnrichmentProvenance, RelationshipType, ResolutionEvidence, ResolutionTechnique,
    Relationship, ResolvedComponent,
};
use waybill_common::types::purl::Purl;

use super::annotation;
use super::conflict::{resolve_component, ConflictRecord};
use super::parser::{Supplement, SupplementComponent, SupplementService};
use super::parser::SupplementError;

/// The merge step's structured return — feeds the CDX/SPDX builders.
#[derive(Debug, Clone)]
#[allow(dead_code)] // `conflicts` is preserved for test-time + future audit-tool inspection.
pub(crate) struct MergeOutcome {
    /// Scanner-discovered components augmented with supplement-only
    /// entries; collisions are merged in place. Length is always
    /// `>= scanner.len()` (FR-015 safety property).
    pub(crate) components: Vec<ResolvedComponent>,
    /// Service entries (CDX-native; no scanner-side equivalent in
    /// v0.1). Flow through to the CDX builder's new `build_services()`
    /// and to the SPDX 2.3 / SPDX 3 projection (Decision 4).
    pub(crate) services: Vec<SupplementService>,
    /// Augmented dependency edges: scanner-side + supplement-side
    /// (with `bom-ref` references re-anchored to canonical PURLs
    /// where matches exist per contracts/supplement-format.md
    /// § "Re-anchoring semantics").
    pub(crate) dependencies: Vec<Relationship>,
    /// Document-scope provenance for the FR-012 annotation.
    pub(crate) supplement_provenance: SupplementProvenance,
    /// Per-component conflict records (already stamped onto each
    /// component's `extra_annotations`; returned for any test-time
    /// inspection).
    pub(crate) conflicts: Vec<ComponentConflicts>,
}

#[derive(Debug, Clone)]
pub(crate) struct SupplementProvenance {
    pub(crate) source_path: String,
    pub(crate) source_sha256: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Preserved for test-time + future audit-tool inspection.
pub(crate) struct ComponentConflicts {
    pub(crate) component_purl: Purl,
    pub(crate) records: Vec<ConflictRecord>,
}

/// Merge the supplement into the scanner-discovered stream. Returns
/// `Err(SupplementError::DanglingDependsOn)` when any supplement
/// `dependsOn` entry references neither a supplement-internal `bom-ref`
/// nor any scanner-side PURL.
pub(crate) fn merge(
    scanner_components: Vec<ResolvedComponent>,
    scanner_dependencies: Vec<Relationship>,
    supplement: Supplement,
) -> Result<MergeOutcome, SupplementError> {
    let initial_scanner_len = scanner_components.len();
    let mut components = scanner_components;

    // Index of canonical PURL → position in `components`. Built once;
    // updated as solo supplement entries append.
    let mut purl_index: HashMap<Purl, usize> = HashMap::with_capacity(components.len());
    for (i, c) in components.iter().enumerate() {
        purl_index.insert(c.purl.clone(), i);
    }

    let mut conflicts: Vec<ComponentConflicts> = Vec::new();

    for supp_component in &supplement.components {
        match purl_index.get(&supp_component.purl).copied() {
            Some(scanner_idx) => {
                // COLLISION: same PURL on both sides. Resolve per
                // FR-006/FR-007 partition.
                let scanner_entry = components[scanner_idx].clone();
                let (mut merged, records) =
                    resolve_component(scanner_entry, supp_component);
                for r in &records {
                    annotation::stamp_assertion_conflict(
                        &mut merged.extra_annotations,
                        r,
                    );
                }
                components[scanner_idx] = merged;
                if !records.is_empty() {
                    conflicts.push(ComponentConflicts {
                        component_purl: supp_component.purl.clone(),
                        records,
                    });
                }
            }
            None => {
                // SOLO: supplement entry has no scanner counterpart.
                // Synthesize a ResolvedComponent and append.
                let mut resolved = synthesize_resolved(supp_component);
                annotation::stamp_source_tier_declared(&mut resolved.extra_annotations);
                let new_idx = components.len();
                purl_index.insert(resolved.purl.clone(), new_idx);
                components.push(resolved);
            }
        }
    }

    // FR-015 safety property: post-condition assertion. The merge can
    // only ADD components (solo path) or REPLACE-IN-PLACE (collision
    // path); never REMOVE.
    debug_assert!(
        components.len() >= initial_scanner_len,
        "FR-015 violation: merge reduced component count from {} to {}",
        initial_scanner_len,
        components.len()
    );

    // Build edges from the supplement's dependencies[], re-anchoring
    // bom-refs to canonical PURLs where matches exist. Dangling refs
    // are an operator error (spec edge case 6 / FR-005).
    let supplement_edges =
        build_supplement_edges(&supplement, &components, &purl_index)?;

    let mut dependencies = scanner_dependencies;
    dependencies.extend(supplement_edges);

    Ok(MergeOutcome {
        components,
        services: supplement.services,
        dependencies,
        supplement_provenance: SupplementProvenance {
            source_path: supplement.source_path,
            source_sha256: supplement.source_sha256,
        },
        conflicts,
    })
}

/// Construct a `ResolvedComponent` from a supplement-only entry.
/// Leaves scanner-specific fields (evidence, occurrences, deep-hash
/// occurrences) empty.
fn synthesize_resolved(supp: &SupplementComponent) -> ResolvedComponent {
    let name = supp
        .name
        .clone()
        .unwrap_or_else(|| supp.purl.name().to_string());
    let version = supp
        .version
        .clone()
        .or_else(|| supp.purl.version().map(String::from))
        .unwrap_or_else(|| "0.0.0".to_string());
    let mut extra_annotations: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();

    if let Some(licenses) = supp.licenses.as_ref() {
        extra_annotations.insert(
            "waybill:supplement-licenses".to_string(),
            serde_json::Value::Array(licenses.clone()),
        );
    }
    if let Some(copyright) = supp.copyright.as_ref() {
        extra_annotations.insert(
            "waybill:copyright".to_string(),
            serde_json::Value::String(copyright.clone()),
        );
    }
    if let Some(description) = supp.description.as_ref() {
        extra_annotations.insert(
            "waybill:description".to_string(),
            serde_json::Value::String(description.clone()),
        );
    }
    if let Some(ext_refs) = supp.external_references.as_ref() {
        extra_annotations.insert(
            "waybill:supplement-externalReferences".to_string(),
            serde_json::Value::Array(ext_refs.clone()),
        );
    }
    if let Some(hashes) = supp.hashes.as_ref() {
        extra_annotations.insert(
            "waybill:declared-hashes".to_string(),
            serde_json::Value::Array(hashes.clone()),
        );
    }

    ResolvedComponent {
        purl: supp.purl.clone(),
        name,
        version,
        evidence: ResolutionEvidence {
            // The supplement is itself the evidence; we tag it via
            // source-tier annotation. The technique enum has no
            // "declared" variant — closest existing is FilePathPattern
            // for the lowest-confidence-derivative-evidence case. For
            // declared content the technique is effectively "operator
            // assertion" (confidence is operator-domain, not algorithmic).
            technique: ResolutionTechnique::FilePathPattern,
            confidence: 1.0,
            source_connection_ids: Vec::new(),
            source_file_paths: Vec::new(),
            deps_dev_match: None,
        },
        licenses: Vec::new(),
        concluded_licenses: Vec::new(),
        hashes: Vec::new(),
        supplier: supp.supplier.clone(),
        cpes: supp.cpes.clone().unwrap_or_default(),
        advisories: Vec::new(),
        occurrences: Vec::new(),
        lifecycle_scope: None,
        build_inclusion: None,
        requirement_ranges: Vec::new(),
        source_type: None,
        sbom_tier: Some("declared".to_string()),
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
        extra_annotations,
        binary_role: None,
    }
}

/// Build `Relationship` edges from the supplement's `dependencies[]`
/// block. Each edge's `ref` and `dependsOn[]` strings are re-anchored
/// per contracts/supplement-format.md § "Re-anchoring semantics":
///
/// 1. If a string matches a supplement-component's `bom-ref` (and that
///    component has a canonical PURL), use the canonical PURL.
/// 2. If a string already IS a canonical PURL matching a scanner-side
///    or supplement-side entry, use it verbatim.
/// 3. Otherwise: dangling reference → `DanglingDependsOn` error.
fn build_supplement_edges(
    supplement: &Supplement,
    merged_components: &[ResolvedComponent],
    purl_index: &HashMap<Purl, usize>,
) -> Result<Vec<Relationship>, SupplementError> {
    // Supplement-side bom-ref → canonical PURL string lookups for
    // components AND services. Services have no canonical PURL in
    // CDX 1.6 so we keep their bom-ref verbatim.
    let mut bom_ref_to_purl: HashMap<&str, String> = HashMap::new();
    for c in &supplement.components {
        if let Some(bref) = c.bom_ref.as_deref() {
            bom_ref_to_purl.insert(bref, c.purl.as_str().to_string());
        }
    }
    let mut supplement_service_bom_refs: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    for s in &supplement.services {
        if let Some(bref) = s.bom_ref.as_deref() {
            supplement_service_bom_refs.insert(bref);
        }
    }

    let merged_purl_strings: std::collections::HashSet<String> = merged_components
        .iter()
        .map(|c| c.purl.as_str().to_string())
        .collect();
    let _ = purl_index; // shape preserved for future fast-path use

    let mut edges: Vec<Relationship> = Vec::new();
    for dep in &supplement.dependencies {
        let from = resolve_ref(
            &dep.ref_str,
            &bom_ref_to_purl,
            &supplement_service_bom_refs,
            &merged_purl_strings,
        )?;
        for raw in &dep.depends_on {
            let to = resolve_ref(
                raw,
                &bom_ref_to_purl,
                &supplement_service_bom_refs,
                &merged_purl_strings,
            )?;
            edges.push(Relationship {
                from: from.clone(),
                to,
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: "supplement-cdx".to_string(),
                    data_type: "dependency-edge".to_string(),
                },
            });
        }
    }
    Ok(edges)
}

fn resolve_ref(
    raw: &str,
    bom_ref_to_purl: &HashMap<&str, String>,
    supplement_service_bom_refs: &std::collections::HashSet<&str>,
    merged_purl_strings: &std::collections::HashSet<String>,
) -> Result<String, SupplementError> {
    if let Some(canonical) = bom_ref_to_purl.get(raw) {
        return Ok(canonical.clone());
    }
    if supplement_service_bom_refs.contains(raw) {
        return Ok(raw.to_string());
    }
    if merged_purl_strings.contains(raw) {
        return Ok(raw.to_string());
    }
    Err(SupplementError::DanglingDependsOn(raw.to_string()))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::supplement::parser::{SupplementComponent, SupplementDependency};

    fn scanner_component(purl: &str) -> ResolvedComponent {
        ResolvedComponent {
            purl: Purl::new(purl).unwrap(),
            name: "scanned".to_string(),
            version: "1.0.0".to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
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
            sbom_tier: Some("source".to_string()),
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
            extra_annotations: std::collections::BTreeMap::new(),
            binary_role: None,
        }
    }

    fn supp_component(purl: &str) -> SupplementComponent {
        SupplementComponent {
            purl: Purl::new(purl).unwrap(),
            bom_ref: None,
            name: None,
            version: None,
            supplier: None,
            licenses: None,
            copyright: None,
            description: None,
            external_references: None,
            hashes: None,
            cpes: None,
        }
    }

    fn empty_supplement() -> Supplement {
        Supplement {
            source_sha256: "0".repeat(64),
            source_path: "test.json".to_string(),
            components: Vec::new(),
            services: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn empty_supplement_is_noop_on_components() {
        let scanner = vec![scanner_component("pkg:cargo/x@1.0.0")];
        let scan_deps: Vec<Relationship> = Vec::new();
        let out = merge(scanner.clone(), scan_deps, empty_supplement()).unwrap();
        assert_eq!(out.components.len(), 1);
        assert_eq!(out.components[0].purl, scanner[0].purl);
        assert!(out.services.is_empty());
        assert!(out.conflicts.is_empty());
    }

    #[test]
    fn solo_supplement_entry_becomes_declared_component() {
        let scanner: Vec<ResolvedComponent> = Vec::new();
        let mut supp = empty_supplement();
        let mut c = supp_component("pkg:generic/liberror@1.2.3");
        c.name = Some("liberror".to_string());
        c.supplier = Some("Acme".to_string());
        supp.components.push(c);
        let out = merge(scanner, Vec::new(), supp).unwrap();
        assert_eq!(out.components.len(), 1);
        assert_eq!(
            out.components[0]
                .extra_annotations
                .get("waybill:source-tier")
                .and_then(|v| v.as_str()),
            Some("declared")
        );
        assert_eq!(out.components[0].supplier.as_deref(), Some("Acme"));
    }

    #[test]
    fn collision_resolves_via_partition_and_records_conflict() {
        let scanner = vec![scanner_component("pkg:cargo/x@1.0.0")];
        let mut supp = empty_supplement();
        let mut c = supp_component("pkg:cargo/x@1.0.0");
        c.licenses = Some(vec![serde_json::json!({"license":{"id":"MIT"}})]);
        supp.components.push(c);
        let out = merge(scanner, Vec::new(), supp).unwrap();
        // No new component added — the collision replaces in place.
        assert_eq!(out.components.len(), 1);
        // Conflict was recorded.
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].records.len(), 1);
        // assertion-conflict annotation stamped on the merged component.
        let arr = out.components[0]
            .extra_annotations
            .get("waybill:assertion-conflict")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn fr015_post_condition_never_loses_a_scanner_component() {
        let scanner = vec![
            scanner_component("pkg:cargo/a@1.0.0"),
            scanner_component("pkg:cargo/b@1.0.0"),
        ];
        let mut supp = empty_supplement();
        // Supplement asserts "b is something else" — collision case;
        // the b component MUST remain in the output.
        let mut c = supp_component("pkg:cargo/b@1.0.0");
        c.licenses = Some(vec![serde_json::json!({"license":{"id":"Apache-2.0"}})]);
        supp.components.push(c);
        let out = merge(scanner.clone(), Vec::new(), supp).unwrap();
        assert!(out.components.len() >= scanner.len());
        assert!(out
            .components
            .iter()
            .any(|c| c.purl.as_str() == "pkg:cargo/b@1.0.0"));
    }

    #[test]
    fn dangling_depends_on_returns_err() {
        let scanner: Vec<ResolvedComponent> = Vec::new();
        let mut supp = empty_supplement();
        supp.dependencies.push(SupplementDependency {
            ref_str: "ghost-ref".to_string(),
            depends_on: vec!["other-ghost".to_string()],
        });
        let err = merge(scanner, Vec::new(), supp).unwrap_err();
        assert!(matches!(err, SupplementError::DanglingDependsOn(_)));
    }

    #[test]
    fn dependency_re_anchors_bom_ref_to_canonical_purl() {
        let scanner = vec![scanner_component("pkg:cargo/app@1.0.0")];
        let mut supp = empty_supplement();
        let mut c = supp_component("pkg:generic/liberror@1.2.3");
        c.bom_ref = Some("liberror-1.2.3".to_string());
        supp.components.push(c);
        supp.dependencies.push(SupplementDependency {
            ref_str: "pkg:cargo/app@1.0.0".to_string(),
            depends_on: vec!["liberror-1.2.3".to_string()],
        });
        let out = merge(scanner, Vec::new(), supp).unwrap();
        // 1 supplement-derived edge.
        assert_eq!(out.dependencies.len(), 1);
        assert_eq!(out.dependencies[0].from, "pkg:cargo/app@1.0.0");
        assert_eq!(out.dependencies[0].to, "pkg:generic/liberror@1.2.3");
    }
}
