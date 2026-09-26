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

    // Primary-dependency fallback: if target_ref has no outgoing edges
    // but we DO have components, synthesize edges from target_ref to
    // every component that nothing else depends on (the roots of the
    // component-side graph). For a flat scan with no relationships,
    // this means target_ref → every component. For a transitive scan,
    // it's just the top-level packages.
    //
    // Milestone 894 (#894): "has edges" means has DECLARED dependency edges.
    // A milestone-868 resolve anchor (root -> `pkg:generic/<resolve-name>`)
    // is synthetic ownership, not a dependency the project declared, so it
    // must not count here.
    //
    // It did count, and the consequence was that one honest root edge
    // switched off a fallback that had been supplying twenty-one. On
    // `pants-example-django` reachability fell from 46 of 46 to 34 of 47:
    // the resolve anchors its declared lockfile requirements, and the
    // design-tier components sourced from requirements files — which the
    // fallback had been covering — were stranded. Invisible on milestone
    // 868's own measured target, whose root already had a real edge, so the
    // fallback was never doing any work there.
    let resolve_component_refs: BTreeSet<&str> = components
        .iter()
        .filter(|c| {
            c.extra_annotations
                .get("waybill:component-kind")
                .and_then(|v| v.as_str())
                == Some("lockfile-resolve")
        })
        .map(|c| c.purl.as_str())
        .collect();
    // #1009: anchor supplement-contributed components whose only
    // declared parent was the supplement's own subject. Those edges could
    // not be emitted at merge time — the subject is not a component and
    // the root is chosen here, per format — so the targets ride the
    // supplement channel and are attached to the root now.
    //
    // Unconditional, NOT gated on `target_has_no_edges`: the whole defect
    // was that the fallback below never fires when the root already has
    // edges, which is the normal case (mongo's root has 10, so 43 of 44
    // supplement components arrived orphaned).
    //
    // Empty when no supplement is active, so the default path is
    // byte-identical.
    for anchor_purl in crate::supplement::current_root_anchors() {
        if components.iter().any(|c| c.purl.as_str() == anchor_purl) {
            dep_map
                .entry(target_ref.to_string())
                .or_default()
                .insert(anchor_purl);
        }
    }

    let target_has_no_edges = dep_map
        .get(target_ref)
        .map(|set| {
            set.iter()
                .all(|t| resolve_component_refs.contains(t.as_str()))
        })
        .unwrap_or(true);
    if target_has_no_edges && !components.is_empty() {
        let mut depended_on: BTreeSet<String> = BTreeSet::new();
        for set in dep_map.values() {
            depended_on.extend(set.iter().cloned());
        }
        // Milestone 084 — also exclude target_ref itself so the
        // primary-dep fallback doesn't synthesize a self-loop. Pre-084
        // target_ref was always the legacy `<name>@0.0.0` short-form
        // which never matched any component's PURL, so this filter
        // was implicit. Post-084 target_ref equals the main-module
        // PURL (when promotion is in effect), and that PURL is also
        // in `components` — without this filter the fallback emits
        // `<root> dependsOn [<root>, ...]`.
        let roots: BTreeSet<String> = components
            .iter()
            .map(|c| c.purl.as_str().to_string())
            .filter(|r| !depended_on.contains(r) && r != target_ref)
            .collect();
        if !roots.is_empty() {
            // Union rather than replace: the resolve anchors deliberately did
            // not count toward the gate above, so they must survive it.
            let existing = dep_map.remove(target_ref).unwrap_or_default();
            dep_map.insert(
                target_ref.to_string(),
                roots.into_iter().chain(existing).collect(),
            );
        }
    }

    // Convert to CycloneDX format.
    // Milestone 218 (waybill#633): for source PURLs that have
    // entries in the cross-ecosystem report, emit C137
    // (`waybill:cross-ecosystem-inference`) + conditionally C138
    // (`waybill:cross-ecosystem-inference-ambiguous`) as
    // `dependencies[i].properties[]` objects. Per-edge disambiguation
    // via the `target_purl` field inside the property value payload.
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

        let result = build_dependencies(&components, &[], "myapp@0.1.0", None);
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

        let result = build_dependencies(&components, &relationships, "myapp@0.1.0", None);
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
        let result = build_dependencies(&components, &relationships, "myapp@0.1.0", None);
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

        let result = build_dependencies(&components, &relationships, "target@0.1.0", None);
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

        let result = build_dependencies(&components, &[], "target@0.1.0", None);
        let deps = result.as_array().expect("array");

        // BTreeMap ensures alphabetical ordering by ref.
        let refs: Vec<&str> = deps.iter().map(|d| d["ref"].as_str().unwrap()).collect();
        assert!(refs.windows(2).all(|w| w[0] <= w[1]));
    }
}