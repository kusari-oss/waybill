use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use waybill_common::resolution::{Relationship, ResolvedComponent};

/// Build the CycloneDX `dependencies` array from enriched relationships.
///
/// Each entry has a `ref` (the component bom-ref, which is its PURL)
/// and a `dependsOn` array listing direct dependencies.
///
/// Components without explicit dependencies still appear with an
/// empty `dependsOn` array.
///
/// **Dangling bom-refs are intentional when `--include-declared-deps`
/// is off (default).** The relaxed pipeline guard rail at
/// `enrich/pipeline.rs:~85` retains edges where at least one endpoint
/// is a known component, so `dependsOn` arrays may reference PURLs
/// that aren't present in `components[]` — specifically, declared deps
/// from deps.dev that don't ship on disk (Maven provided-scope,
/// JDK-bundled, optional, aggressive-shade-stripped). Strict CDX
/// expects every ref to resolve; we trade strict-validity for
/// topology-preservation so consumers can see what an on-disk
/// component declares as a dependency even when the declared target
/// doesn't physically ship. Users who want the old strictly-anchored
/// shape pass `--include-declared-deps`, which re-emits every
/// declared-but-not-shipped target as a `source_type =
/// "declared-not-cached"` component. See
/// `enrich/deps_dev_graph.rs` for the emission gate + the
/// TODO(declared-scope) pointing at CDX 1.6 `scope: "excluded"` as
/// the future CDX-canonical representation.
///
/// Primary-dependency fallback (sbomqs `comp_with_dependencies`): the
/// CDX scoring tooling expects the primary component
/// (`metadata.component`, here `target_ref`) to have at least one
/// outgoing `dependsOn` edge — without it, sbomqs reports
/// "no dependency graph present" even when component-to-component
/// edges are populated. When waybill skips the lockfile root entry
/// (workspace root, npm path_key=""), the primary has no source for
/// direct-dep names. Fall back to "primary depends on all root
/// components" — i.e., every component that isn't pointed at by any
/// other component's dependsOn. This produces a usable graph: the
/// primary at the top, transitives correctly chained underneath.
pub fn build_dependencies(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    target_ref: &str,
    cross_ecosystem_edges_report: Option<
        &crate::generate::cross_ecosystem_edges::CrossEcosystemEdgesReport,
    >,
    // Spec 1536 (V5): each PURL's emitted copies. `None` keeps PURLs as refs.
    bom_refs: Option<&super::builder::BomRefIndex>,
) -> serde_json::Value {
    // Build a map of ref -> set of dependency refs.
    let mut dep_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    // Initialize all components with empty dependency sets.
    for component in components {
        dep_map.entry(component.purl.as_str().to_string()).or_default();
    }

    // Also include the target ref (the build artifact itself).
    dep_map.entry(target_ref.to_string()).or_default();

    // Populate relationships.
    for rel in relationships {
        dep_map
            .entry(rel.from.clone())
            .or_default()
            .insert(rel.to.clone());
    }

    // Spec 1537 (#1566), research R2: the root's synthesized targets come
    // from the one rule every format and the completeness classifier share
    // (`generate::root_edges`). It replaces this module's own copy of the
    // primary-dependency fallback (#236), its resolve-anchor exemption
    // (#894) and its anchor loop (#1009, spec 1530). The anchors used to be
    // inserted here BEFORE the gate was read, so a git submodule switched
    // the fallback off in CycloneDX while SPDX 3, which added them after,
    // kept it on.
    let anchors = crate::supplement::current_root_anchors();
    let synthesized = crate::generate::root_edges::synthesized_root_targets(
        components,
        relationships,
        target_ref,
        &anchors,
    );
    if !synthesized.is_empty() {
        dep_map
            .entry(target_ref.to_string())
            .or_default()
            .extend(synthesized);
    }

    // Convert to CycloneDX format.
    // Milestone 218 (waybill#633): for source PURLs that have
    // entries in the cross-ecosystem report, emit C137
    // (`waybill:cross-ecosystem-inference`) + conditionally C138
    // (`waybill:cross-ecosystem-inference-ambiguous`) as
    // `dependencies[i].properties[]` objects. Per-edge disambiguation
    // via the `target_purl` field inside the property value payload.
    let dep_map = match bom_refs {
        Some(index) => resolve_bom_refs(dep_map, index),
        None => dep_map,
    };
    let entries: Vec<serde_json::Value> = dep_map
        .into_iter()
        .map(|(ref_str, depends_on)| {
            let dep_vec: Vec<String> = depends_on.into_iter().collect();
            let properties = cross_ecosystem_edges_report
                .map(|report| {
                    let mut props: Vec<serde_json::Value> = Vec::new();
                    for target in &dep_vec {
                        let key = (ref_str.clone(), target.clone());
                        if let Some(base) = report.crossed_edges.get(&key) {
                            if let Ok(value) = serde_json::to_string(base) {
                                props.push(json!({
                                    "name": "waybill:cross-ecosystem-inference",
                                    "value": value,
                                }));
                            }
                        }
                        if let Some(ambig) = report.ambiguous_edges.get(&key) {
                            if let Ok(value) = serde_json::to_string(ambig) {
                                props.push(json!({
                                    "name": "waybill:cross-ecosystem-inference-ambiguous",
                                    "value": value,
                                }));
                            }
                        }
                    }
                    props
                })
                .unwrap_or_default();
            if properties.is_empty() {
                json!({
                    "ref": ref_str,
                    "dependsOn": dep_vec,
                })
            } else {
                json!({
                    "ref": ref_str,
                    "dependsOn": dep_vec,
                    "properties": properties,
                })
            }
        })
        .collect();

    json!(entries)
}

/// Spec 1536 (V5): rewrite PURL-keyed edges onto `bom-ref`s.
///
/// A nested component's `bom-ref` is not its PURL, so an edge naming the
/// PURL would point at nothing. An edge from a PURL with several copies is
/// emitted from each copy. Its target resolves to the copy nested under that
/// source, else the top-level copy, else the first copy. A PURL with no
/// emitted copy (a declared-not-shipped target, the document root) keeps its
/// PURL. A document without nesting is unchanged: each PURL's only copy is
/// its own `bom-ref`.
fn resolve_bom_refs(
    dep_map: BTreeMap<String, BTreeSet<String>>,
    index: &super::builder::BomRefIndex,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (from, targets) in dep_map {
        let sources: Vec<String> = match index.get(&from) {
            Some(copies) => copies.iter().map(|(r, _)| r.clone()).collect(),
            None => vec![from],
        };
        for source in sources {
            let set = out.entry(source.clone()).or_default();
            for to in &targets {
                let resolved = index
                    .get(to)
                    .and_then(|copies| {
                        copies
                            .iter()
                            .find(|(_, parent)| parent.as_deref() == Some(source.as_str()))
                            .or_else(|| copies.iter().find(|(_, parent)| parent.is_none()))
                            .or_else(|| copies.first())
                    })
                    .map_or_else(|| to.clone(), |(r, _)| r.clone());
                set.insert(resolved);
            }
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{
        EnrichmentProvenance, RelationshipType, ResolutionEvidence, ResolutionTechnique,
    };
    use waybill_common::types::purl::Purl;

    fn make_component(name: &str, version: &str) -> ResolvedComponent {
        let purl_str = format!("pkg:cargo/{name}@{version}");
        ResolvedComponent {
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            purl: Purl::new(&purl_str).expect("valid purl"),
            name: name.to_string(),
            version: version.to_string(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.9,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: Vec::new(),
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
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
        }
    }

    #[test]
    fn dependencies_include_all_components() {
        let components = vec![
            make_component("serde", "1.0.197"),
            make_component("tokio", "1.38.0"),
        ];

        let result = build_dependencies(&components, &[], "myapp@0.1.0", None, None);
        let deps = result.as_array().expect("array");

        // Should have 3 entries: myapp target + 2 components.
        assert_eq!(deps.len(), 3);

        // Primary-dependency fallback (sbomqs comp_with_dependencies):
        // when no relationships exist, the target_ref synthetically
        // depends on all components. Here both serde and tokio are
        // roots (nothing else points at them).
        let target = deps
            .iter()
            .find(|d| d["ref"] == "myapp@0.1.0")
            .expect("target dep entry");
        let target_deps: Vec<&str> = target["dependsOn"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(target_deps.len(), 2);
        // Components themselves remain leaves.
        for dep in deps {
            if dep["ref"] != "myapp@0.1.0" {
                assert!(dep["dependsOn"].as_array().expect("array").is_empty());
            }
        }
    }

    #[test]
    fn primary_fallback_only_lists_roots_not_transitives() {
        // express depends on body-parser; body-parser depends on bytes.
        // Roots = [express] (the only one nothing depends on).
        // Target should depend on express only.
        let components = vec![
            make_component("express", "4.18.2"),
            make_component("body-parser", "1.20.1"),
            make_component("bytes", "3.1.2"),
        ];
        let relationships = vec![
            Relationship {
                from: "pkg:cargo/express@4.18.2".to_string(),
                to: "pkg:cargo/body-parser@1.20.1".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: "test".to_string(),
                    data_type: "test".to_string(),
                },
            },
            Relationship {
                from: "pkg:cargo/body-parser@1.20.1".to_string(),
                to: "pkg:cargo/bytes@3.1.2".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: "test".to_string(),
                    data_type: "test".to_string(),
                },
            },
        ];

        let result = build_dependencies(&components, &relationships, "myapp@0.1.0", None, None);
        let deps = result.as_array().unwrap();
        let target = deps.iter().find(|d| d["ref"] == "myapp@0.1.0").unwrap();
        let target_deps: Vec<&str> = target["dependsOn"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(target_deps, vec!["pkg:cargo/express@4.18.2"]);
    }

    #[test]
    fn primary_fallback_skipped_when_target_already_has_deps() {
        // If something already populated target's dependsOn, don't
        // override it.
        let components = vec![make_component("serde", "1.0.197")];
        let relationships = vec![Relationship {
            from: "myapp@0.1.0".to_string(),
            to: "pkg:cargo/serde@1.0.197".to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "test".to_string(),
            },
        }];
        let result = build_dependencies(&components, &relationships, "myapp@0.1.0", None, None);
        let deps = result.as_array().unwrap();
        let target = deps.iter().find(|d| d["ref"] == "myapp@0.1.0").unwrap();
        let target_deps: Vec<&str> = target["dependsOn"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(target_deps, vec!["pkg:cargo/serde@1.0.197"]);
    }

    #[test]
    fn dependencies_map_relationships() {
        let components = vec![
            make_component("myapp", "0.1.0"),
            make_component("serde", "1.0.197"),
            make_component("tokio", "1.38.0"),
        ];

        let relationships = vec![
            Relationship {
                from: "pkg:cargo/myapp@0.1.0".to_string(),
                to: "pkg:cargo/serde@1.0.197".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: "test".to_string(),
                    data_type: "test".to_string(),
                },
            },
            Relationship {
                from: "pkg:cargo/myapp@0.1.0".to_string(),
                to: "pkg:cargo/tokio@1.38.0".to_string(),
                relationship_type: RelationshipType::DependsOn,
                provenance: EnrichmentProvenance {
                    source: "test".to_string(),
                    data_type: "test".to_string(),
                },
            },
        ];

        let result = build_dependencies(&components, &relationships, "target@0.1.0", None, None);
        let deps = result.as_array().expect("array");

        // Find the myapp entry.
        let myapp_dep = deps
            .iter()
            .find(|d| d["ref"] == "pkg:cargo/myapp@0.1.0")
            .expect("myapp dependency entry");

        let depends_on = myapp_dep["dependsOn"].as_array().expect("array");
        assert_eq!(depends_on.len(), 2);
        assert!(depends_on.iter().any(|v| v == "pkg:cargo/serde@1.0.197"));
        assert!(depends_on.iter().any(|v| v == "pkg:cargo/tokio@1.38.0"));
    }

    #[test]
    fn dependencies_are_sorted_deterministically() {
        let components = vec![
            make_component("zebra", "1.0.0"),
            make_component("alpha", "1.0.0"),
        ];

        let result = build_dependencies(&components, &[], "target@0.1.0", None, None);
        let deps = result.as_array().expect("array");

        // BTreeMap ensures alphabetical ordering by ref.
        let refs: Vec<&str> = deps.iter().map(|d| d["ref"].as_str().unwrap()).collect();
        assert!(refs.windows(2).all(|w| w[0] <= w[1]));
    }
}