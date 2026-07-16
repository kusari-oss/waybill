//! Milestone 191 (issue #560) — design-tier / source-tier reconciliation.
//! Milestone 199 (issue #564 + #565) — always-array shape for the
//! declaration-provenance annotations + npm-alias resolved-identity
//! accumulation via `mikebom:declared-as`.
//!
//! At emission time (after `deduplicate` runs), collapse each design-tier
//! component into its matching source-tier sibling when one exists in the
//! same workspace scope. Accumulate every design-tier match's requirement
//! range + source manifest path + (optional) npm-alias `mikebom:declared-as`
//! onto the surviving source-tier component. Rewrite dep-graph edges
//! pointing at the removed design-tier component so they target the
//! survivor.
//!
//! Called from both dedup sites — `scan_fs/mod.rs:807` (post-first-dedup)
//! and `cli/scan_cmd.rs:2742` (post-second-dedup).
//!
//! # m199 wire shape
//!
//! Every reconciler-survivor with at least one design-tier match carries:
//! - `mikebom:requirement-ranges` — always JSON array of range strings.
//!   N-element for N-manifest cases; duplicates preserved (range/manifest
//!   count IS provenance signal per data-model E2).
//! - `mikebom:source-manifests` — always JSON array of workspace-relative
//!   manifest paths, sorted lex-ascending. Ordering 1:1 with ranges (Nth
//!   range came from Nth manifest).
//! - `mikebom:declared-as` (US2) — JSON array of npm alias names, sorted
//!   lex-ascending + DEDUPED (unlike ranges/manifests: alias-count is not
//!   provenance per data-model E1). Emitted only when at least one
//!   design-tier match carried the annotation.
//!
//! The m191 singular scalars `mikebom:requirement-range` /
//! `mikebom:source-manifest` MUST NOT appear on reconciler survivors
//! post-m199 (FR-001).
//!
//! # Design decisions
//!
//! - Match key: `(ecosystem, canonical_name, source_manifest_dir)`.
//!   Under US2, the canonical_name is the aliased_name (resolved
//!   identity) because the design-tier component's PURL is keyed on
//!   the resolved name — so match-by-resolved-identity works naturally
//!   without a separate branch (research R3).
//! - Edge rewriting: any `Relationship.from` or `.to` matching a removed
//!   design-tier PURL is rewritten to the source-tier PURL; duplicate
//!   edges are deduped post-rewrite (unchanged from m191).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use mikebom_common::resolution::{Relationship, ResolvedComponent};

/// Per-survivor accumulator for the always-array declaration-provenance
/// annotations. One instance per matched source-tier survivor; populated
/// during the design-component scan then finalized + stamped as
/// `extra_annotations` once the scan completes.
#[derive(Default, Debug)]
struct ReconcilerAccumulator {
    /// Raw range strings, one per matched design-tier hit. Duplicates
    /// preserved (E2 data-model — range count IS provenance signal).
    ranges: Vec<String>,
    /// Workspace-relative manifest paths, one per matched design-tier
    /// hit. Sorted lex + reordered 1:1 with ranges at finalize time.
    manifests: Vec<String>,
    /// npm alias names (E1 data-model). Deduped + sorted lex at finalize
    /// time — alias-count is not provenance.
    declared_as: BTreeSet<String>,
}

fn finalize_accumulator(mut acc: ReconcilerAccumulator) -> ReconcilerAccumulator {
    // Sort manifests lex-ascending and reorder ranges to match (FR-003).
    // We build (manifest, range) pairs, sort, then split back.
    let mut pairs: Vec<(String, String)> = acc
        .manifests
        .into_iter()
        .zip(acc.ranges)
        .collect();
    pairs.sort();
    acc.manifests = pairs.iter().map(|(m, _)| m.clone()).collect();
    acc.ranges = pairs.into_iter().map(|(_, r)| r).collect();
    // BTreeSet<declared_as> is already sorted + deduped by construction.
    acc
}

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
    // Milestone 199: per-survivor accumulator instead of first-wins
    // scalar transfer. Every matched design-tier contributes ITS range +
    // manifest + (optional) alias name; finalize_accumulator sorts +
    // dedups at flush time.
    let mut accumulators: HashMap<usize, ReconcilerAccumulator> = HashMap::new();

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
        // Extract the design-tier's manifest path once — used for accumulator entries.
        // Milestone 199: check both plural (post-m199 shape) and singular
        // (legacy) forms; treat scalar OR array values equivalently.
        let design_manifest = design
            .extra_annotations
            .get("mikebom:source-manifests")
            .or_else(|| design.extra_annotations.get("mikebom:source-manifest"))
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(a) => a
                    .first()
                    .and_then(|el| el.as_str())
                    .map(|s| s.to_string()),
                _ => None,
            });
        // Extract US2 declared-as (may be array from design-tier stamp
        // site or scalar for defensive-forward-compat).
        let design_declared_as: Vec<String> = design
            .extra_annotations
            .get("mikebom:declared-as")
            .map(|v| match v {
                serde_json::Value::String(s) => vec![s.clone()],
                serde_json::Value::Array(a) => a
                    .iter()
                    .filter_map(|el| el.as_str().map(|s| s.to_string()))
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        // Reconcile: accumulate onto every matching source + record PURL rewrite.
        for &src_idx in matches {
            let src = &components[src_idx];
            purl_rewrite.insert(
                design.purl.as_str().to_string(),
                src.purl.as_str().to_string(),
            );
            let acc = accumulators.entry(src_idx).or_default();
            // Push every requirement range from the design-tier's vec
            // (typically 1-element per design-tier; N-element only in
            // the exotic case where a reader itself aggregated ranges).
            for r in &design.requirement_ranges {
                acc.ranges.push(r.clone());
            }
            if let Some(mp) = &design_manifest {
                acc.manifests.push(mp.clone());
            }
            for alias in &design_declared_as {
                acc.declared_as.insert(alias.clone());
            }
            tracing::debug!(
                design_purl = %design.purl.as_str(),
                source_purl = %src.purl.as_str(),
                "reconcile_design_source_tiers: matched design → source",
            );
        }
        design_indices_to_remove.push(design_idx);
    }

    // Flush accumulators onto survivors. Also strip any m191 singular
    // scalars that may have been written by earlier passes — post-m199
    // reconciler survivors MUST NOT carry `mikebom:requirement-range` or
    // `mikebom:source-manifest` (FR-001).
    let mut components = components;
    for (src_idx, acc) in accumulators {
        let acc = finalize_accumulator(acc);
        // Ranges and manifests are guaranteed non-empty (a survivor gets
        // an accumulator only when at least one design-tier match wrote
        // to it).
        if !acc.ranges.is_empty() {
            components[src_idx].extra_annotations.insert(
                "mikebom:requirement-ranges".to_string(),
                serde_json::json!(acc.ranges),
            );
        }
        if !acc.manifests.is_empty() {
            components[src_idx].extra_annotations.insert(
                "mikebom:source-manifests".to_string(),
                serde_json::json!(acc.manifests),
            );
        }
        if !acc.declared_as.is_empty() {
            let sorted: Vec<String> = acc.declared_as.into_iter().collect();
            components[src_idx]
                .extra_annotations
                .insert("mikebom:declared-as".to_string(), serde_json::json!(sorted));
        }
        // Strip m191 singular scalars — supersede-by-pluralization.
        components[src_idx]
            .extra_annotations
            .remove("mikebom:requirement-range");
        components[src_idx]
            .extra_annotations
            .remove("mikebom:source-manifest");
        // Also clear the ResolvedComponent's requirement_ranges field
        // to avoid double-emission by the CDX/SPDX emitters (which read
        // the struct field). The extra_annotations bag is the m199
        // source-of-truth for reconciler survivors.
        components[src_idx].requirement_ranges.clear();
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
    // Milestone 199: prefer plural `mikebom:source-manifests` (post-m199
    // shape). Fall back to singular `mikebom:source-manifest` for legacy
    // readers that still stamp the scalar form. Both accept string OR
    // array values (defense-in-depth).
    let extract = |key: &str| -> Option<String> {
        c.extra_annotations.get(key).and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Array(a) => a
                .first()
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        })
    };
    let raw = extract("mikebom:source-manifests")
        .or_else(|| extract("mikebom:source-manifest"))
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
            requirement_ranges: range.map(|r| vec![r.to_string()]).unwrap_or_default(),
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
        // Milestone 199 — always-array shape (FR-001).
        let ranges = out[0]
            .extra_annotations
            .get("mikebom:requirement-ranges")
            .expect("requirement-ranges must transfer to survivor as array");
        assert_eq!(
            ranges,
            &serde_json::json!(["^11.1.0"]),
            "requirement-ranges must be 1-element array in single-manifest case"
        );
        let manifests = out[0]
            .extra_annotations
            .get("mikebom:source-manifests")
            .expect("source-manifests must transfer to survivor as array");
        assert_eq!(
            manifests,
            &serde_json::json!(["package.json"]),
            "source-manifests must be 1-element array in single-manifest case"
        );
        // Singular scalars MUST NOT appear on m199 reconciler survivors.
        assert!(
            !out[0]
                .extra_annotations
                .contains_key("mikebom:requirement-range"),
            "m191 singular scalar must be stripped from reconciler survivor"
        );
        assert!(
            !out[0]
                .extra_annotations
                .contains_key("mikebom:source-manifest"),
            "m191 singular scalar must be stripped from reconciler survivor"
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
