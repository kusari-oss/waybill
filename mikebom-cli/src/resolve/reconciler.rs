//! Milestone 191 (issue #560) — design-tier / source-tier reconciliation.
//!
//! At emission time (after `deduplicate` runs), collapse each design-tier
//! component into its matching source-tier sibling when one exists in the
//! same workspace scope. Transfer the design-tier's `mikebom:requirement-
//! range` and `mikebom:source-manifest` annotations onto the surviving
//! source-tier component; rewrite dep-graph edges pointing at the removed
//! design-tier component so they target the survivor.
//!
//! Called from both dedup sites — `scan_fs/mod.rs:807` (post-first-dedup)
//! and `cli/scan_cmd.rs:2742` (post-second-dedup).
//!
//! # MVP scope (m191)
//!
//! - Match by `(ecosystem, canonical_name, source_manifest_dir)` — same-
//!   directory match, no workspace-parent walk in the initial pass.
//! - 1:1 reconciliation (one design-tier collapses into one source-tier).
//! - Annotation transfer preserves single-declaration case verbatim; if
//!   the source-tier already carries `mikebom:requirement-range`, the
//!   design-tier's range is dropped (multi-declaration handling deferred
//!   to follow-up per plan tradeoff).
//! - Edge rewriting: any `Relationship.from` or `.to` matching a removed
//!   design-tier PURL is rewritten to the source-tier PURL; duplicate
//!   edges are deduped post-rewrite.
//! - INFO + DEBUG observability per FR-020 / Q4.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use mikebom_common::resolution::{Relationship, ResolvedComponent};

/// Reconcile design-tier components into matching source-tier siblings.
///
/// Returns the new component list (with reconciled design-tier entries
/// removed) and mutates `relationships` in place to rewrite any edges
/// that pointed at removed design-tier PURLs.
pub fn reconcile_design_source_tiers(
    components: Vec<ResolvedComponent>,
    relationships: &mut Vec<Relationship>,
) -> Vec<ResolvedComponent> {
    // Fast path: if there are no design-tier components, return early
    // (byte-identity guarantee for pre-m191 shapes with no design tier).
    let has_design = components
        .iter()
        .any(|c| c.sbom_tier.as_deref() == Some("design"));
    if !has_design {
        return components;
    }

    // Split into (source_or_other, design) partitions by index so we can
    // mutate the source components in place and then filter out design
    // entries at the end.
    //
    // Ecosystem + canonical name + source-manifest directory jointly
    // identify a scope-matched pair. `source_manifest_dir` is derived
    // from the design-tier component's `mikebom:source-manifest`
    // annotation (typically `package.json` or `packages/foo/package.json`
    // — we take the parent directory).
    let source_index: HashMap<MatchKey, Vec<usize>> = build_source_index(&components);

    let mut design_indices_to_remove: Vec<usize> = Vec::new();
    let mut purl_rewrite: HashMap<String, String> = HashMap::new();
    // Deferred mutations: (source_idx, key, value) — applied AFTER the
    // read-only scan of design components completes, so we don't hold
    // an immutable borrow of `components` while trying to mutate.
    let mut pending_annotations: Vec<(usize, String, serde_json::Value)> = Vec::new();

    for (design_idx, design) in components.iter().enumerate() {
        if design.sbom_tier.as_deref() != Some("design") {
            continue;
        }
        let Some(key) = match_key_for(design) else {
            continue;
        };
        let Some(matches) = source_index.get(&key) else {
            continue; // standalone design-tier — no source match; leave as-is.
        };
        // Reconcile: transfer annotations onto every matching source, rewrite PURL.
        for &src_idx in matches {
            let src = &components[src_idx];
            purl_rewrite.insert(
                design.purl.as_str().to_string(),
                src.purl.as_str().to_string(),
            );
            // Transfer requirement-range: only if source doesn't have one
            // (MVP scope — multi-declaration handling deferred).
            if !src
                .extra_annotations
                .contains_key("mikebom:requirement-range")
            {
                if let Some(range) = &design.requirement_range {
                    pending_annotations.push((
                        src_idx,
                        "mikebom:requirement-range".to_string(),
                        serde_json::Value::String(range.clone()),
                    ));
                }
            }
            // Transfer source-manifest.
            if !src
                .extra_annotations
                .contains_key("mikebom:source-manifest")
            {
                if let Some(sm) = design
                    .extra_annotations
                    .get("mikebom:source-manifest")
                    .cloned()
                {
                    pending_annotations.push((src_idx, "mikebom:source-manifest".to_string(), sm));
                }
            }
            tracing::debug!(
                design_purl = %design.purl.as_str(),
                source_purl = %src.purl.as_str(),
                "reconcile_design_source_tiers: matched design → source",
            );
        }
        design_indices_to_remove.push(design_idx);
    }

    // Apply deferred annotation writes.
    let mut components = components;
    for (idx, key, value) in pending_annotations {
        components[idx].extra_annotations.insert(key, value);
    }

    // Rewrite edges pointing at removed design PURLs, then dedup.
    let reconciled_count = design_indices_to_remove.len();
    let mut standalone_count = 0usize;
    if !purl_rewrite.is_empty() {
        for rel in relationships.iter_mut() {
            if let Some(new_from) = purl_rewrite.get(&rel.from) {
                rel.from = new_from.clone();
            }
            if let Some(new_to) = purl_rewrite.get(&rel.to) {
                rel.to = new_to.clone();
            }
        }
        // Post-rewrite dedup of (from, to, relationship_type) triples.
        let mut seen: HashSet<(String, String, String)> = HashSet::new();
        relationships.retain(|r| {
            let key = (
                r.from.clone(),
                r.to.clone(),
                format!("{:?}", r.relationship_type),
            );
            seen.insert(key)
        });
    }

    // Remove reconciled design-tier components + count remaining
    // standalone design-tier for the summary log.
    let mut remove_set: HashSet<usize> = design_indices_to_remove.into_iter().collect();
    let filtered: Vec<ResolvedComponent> = components
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if remove_set.remove(&i) {
                None
            } else {
                if c.sbom_tier.as_deref() == Some("design") {
                    standalone_count += 1;
                }
                Some(c)
            }
        })
        .collect();

    tracing::info!(
        reconciled = reconciled_count,
        standalone = standalone_count,
        "reconcile_design_source_tiers: reconciled N design-tier components into source-tier siblings; K standalone design-tier components emitted"
    );

    filtered
}

/// Composite match key: ecosystem + canonical name + source-manifest
/// directory. Two components with the same key are considered a
/// reconciliation candidate pair.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MatchKey {
    ecosystem: String,
    name: String,
    manifest_dir: PathBuf,
}

fn build_source_index(components: &[ResolvedComponent]) -> HashMap<MatchKey, Vec<usize>> {
    let mut idx: HashMap<MatchKey, Vec<usize>> = HashMap::new();
    for (i, c) in components.iter().enumerate() {
        // "source" or "analyzed" both count as concrete-tier survivors
        // (both carry a resolved version and can absorb design-tier metadata).
        let is_source_tier = matches!(
            c.sbom_tier.as_deref(),
            Some("source") | Some("analyzed") | None
        );
        if !is_source_tier {
            continue;
        }
        if let Some(key) = match_key_for(c) {
            idx.entry(key).or_default().push(i);
        }
    }
    idx
}

fn match_key_for(c: &ResolvedComponent) -> Option<MatchKey> {
    let ecosystem = c.purl.ecosystem().to_string();
    let name = c.name.clone();
    // Extract manifest directory from `mikebom:source-manifest`
    // annotation. Value may be a scalar string (single-manifest case)
    // or an array (post-reconciliation case — take the first entry for
    // matching). If the annotation is absent, fall back to the
    // component's evidence.source_file_paths[0] directory.
    let manifest_dir = source_manifest_dir(c)?;
    Some(MatchKey {
        ecosystem,
        name,
        manifest_dir,
    })
}

fn source_manifest_dir(c: &ResolvedComponent) -> Option<PathBuf> {
    let raw = c
        .extra_annotations
        .get("mikebom:source-manifest")
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Array(a) => a
                .first()
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        })
        .or_else(|| {
            // Fallback: pick first source-file-path.
            c.evidence.source_file_paths.first().cloned()
        })?;
    let path = Path::new(&raw);
    // Manifest path → parent directory; if the value already IS a
    // directory (rare), keep it as-is.
    path.parent()
        .map(|p| p.to_path_buf())
        .or_else(|| Some(path.to_path_buf()))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{
        EnrichmentProvenance, LifecycleScope, RelationshipType, ResolutionEvidence,
        ResolutionTechnique,
    };
    use mikebom_common::types::purl::Purl;
    use std::collections::BTreeMap;

    fn mk_component(
        name: &str,
        version: &str,
        purl: &str,
        sbom_tier: Option<&str>,
        manifest: Option<&str>,
        range: Option<&str>,
    ) -> ResolvedComponent {
        let mut extra = BTreeMap::new();
        if let Some(m) = manifest {
            extra.insert(
                "mikebom:source-manifest".to_string(),
                serde_json::Value::String(m.to_string()),
            );
        }
        ResolvedComponent {
            purl: Purl::new(purl).unwrap(),
            name: name.to_string(),
            version: version.to_string(),
            supplier: None,
            licenses: Vec::new(),
            concluded_licenses: Vec::new(),
            cpes: Vec::new(),
            advisories: Vec::new(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                deps_dev_match: None,
                source_connection_ids: Vec::new(),
                source_file_paths: manifest.map(|m| vec![m.to_string()]).unwrap_or_default(),
            },
            hashes: Vec::new(),
            occurrences: Vec::new(),
            lifecycle_scope: Some(LifecycleScope::Runtime),
            build_inclusion: None,
            requirement_range: range.map(str::to_string),
            source_type: None,
            sbom_tier: sbom_tier.map(str::to_string),
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
            extra_annotations: extra,
            binary_role: None,
        }
    }

    #[test]
    fn no_design_tier_is_identity() {
        let components = vec![mk_component(
            "foo",
            "1.0.0",
            "pkg:npm/foo@1.0.0",
            Some("source"),
            Some("package.json"),
            None,
        )];
        let mut rels = Vec::new();
        let out = reconcile_design_source_tiers(components.clone(), &mut rels);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].purl.as_str(), "pkg:npm/foo@1.0.0");
    }

    #[test]
    fn matching_design_and_source_reconcile_into_source() {
        let components = vec![
            mk_component(
                "commander",
                "",
                "pkg:npm/commander",
                Some("design"),
                Some("package.json"),
                Some("^11.1.0"),
            ),
            mk_component(
                "commander",
                "11.1.0",
                "pkg:npm/commander@11.1.0",
                Some("source"),
                Some("package.json"),
                None,
            ),
        ];
        let mut rels = Vec::new();
        let out = reconcile_design_source_tiers(components, &mut rels);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].purl.as_str(), "pkg:npm/commander@11.1.0");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("source"));
        assert_eq!(
            out[0]
                .extra_annotations
                .get("mikebom:requirement-range")
                .and_then(|v| v.as_str()),
            Some("^11.1.0"),
            "requirement-range must transfer to source-tier survivor"
        );
        assert_eq!(
            out[0]
                .extra_annotations
                .get("mikebom:source-manifest")
                .and_then(|v| v.as_str()),
            Some("package.json"),
            "source-manifest must transfer to source-tier survivor"
        );
    }

    #[test]
    fn standalone_design_tier_is_preserved() {
        let components = vec![mk_component(
            "no-match",
            "",
            "pkg:npm/no-match",
            Some("design"),
            Some("package.json"),
            Some("^1.0.0"),
        )];
        let mut rels = Vec::new();
        let out = reconcile_design_source_tiers(components, &mut rels);
        assert_eq!(out.len(), 1, "standalone design-tier must be preserved");
        assert_eq!(out[0].sbom_tier.as_deref(), Some("design"));
        assert_eq!(out[0].purl.as_str(), "pkg:npm/no-match");
    }

    #[test]
    fn edge_from_design_rewrites_to_source() {
        let components = vec![
            mk_component(
                "commander",
                "",
                "pkg:npm/commander",
                Some("design"),
                Some("package.json"),
                Some("^11.1.0"),
            ),
            mk_component(
                "commander",
                "11.1.0",
                "pkg:npm/commander@11.1.0",
                Some("source"),
                Some("package.json"),
                None,
            ),
            mk_component(
                "parent",
                "1.0.0",
                "pkg:npm/parent@1.0.0",
                Some("source"),
                Some("package.json"),
                None,
            ),
        ];
        let mut rels = vec![Relationship {
            from: "pkg:npm/parent@1.0.0".to_string(),
            to: "pkg:npm/commander".to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "package_lock.json".to_string(),
                data_type: "dependency-edge".to_string(),
            },
        }];
        let out = reconcile_design_source_tiers(components, &mut rels);
        assert_eq!(out.len(), 2, "commander design collapses into source");
        assert_eq!(rels.len(), 1, "one edge remains");
        assert_eq!(
            rels[0].to, "pkg:npm/commander@11.1.0",
            "edge target rewritten from design PURL to source PURL"
        );
    }

    #[test]
    fn duplicate_edge_after_rewrite_is_deduped() {
        // Sibling `parent` had TWO edges pre-m191: one pointing at
        // design commander, one pointing at source commander. Post-
        // m191 both would target the source PURL; dedup collapses
        // them into one edge.
        let components = vec![
            mk_component(
                "commander",
                "",
                "pkg:npm/commander",
                Some("design"),
                Some("package.json"),
                Some("^11.1.0"),
            ),
            mk_component(
                "commander",
                "11.1.0",
                "pkg:npm/commander@11.1.0",
                Some("source"),
                Some("package.json"),
                None,
            ),
            mk_component(
                "parent",
                "1.0.0",
                "pkg:npm/parent@1.0.0",
                Some("source"),
                Some("package.json"),
                None,
            ),
        ];
        let mut rels = vec![
            Relationship {
                from: "pkg:npm/parent@1.0.0".to_string(),
                to: "pkg:npm/commander".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                source: "package_lock.json".to_string(),
                data_type: "dependency-edge".to_string(),
            },
            },
            Relationship {
                from: "pkg:npm/parent@1.0.0".to_string(),
                to: "pkg:npm/commander@11.1.0".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                source: "package_lock.json".to_string(),
                data_type: "dependency-edge".to_string(),
            },
            },
        ];
        let out = reconcile_design_source_tiers(components, &mut rels);
        assert_eq!(out.len(), 2);
        assert_eq!(rels.len(), 1, "duplicate edges deduped after rewrite");
    }

    #[test]
    fn different_manifest_dirs_do_not_reconcile() {
        // Same name + ecosystem BUT in different manifest directories
        // (independent projects, not workspace peers). Must NOT
        // reconcile across scopes.
        let components = vec![
            mk_component(
                "commander",
                "",
                "pkg:npm/commander",
                Some("design"),
                Some("project-a/package.json"),
                Some("^11.1.0"),
            ),
            mk_component(
                "commander",
                "11.1.0",
                "pkg:npm/commander@11.1.0",
                Some("source"),
                Some("project-b/package.json"),
                None,
            ),
        ];
        let mut rels = Vec::new();
        let out = reconcile_design_source_tiers(components, &mut rels);
        assert_eq!(
            out.len(),
            2,
            "cross-directory pairs must NOT reconcile in MVP scope"
        );
    }
}
