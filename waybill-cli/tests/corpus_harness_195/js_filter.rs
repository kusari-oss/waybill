//! JS-only golden filter — issue #760 option B, feature 675 FR-008.
//!
//! Filters emitted SBOMs down to the `pkg:npm/*` surface only, before
//! layer 2 byte-identity comparison. Applied per-target (dispatched by
//! target name in `layer2_golden::compare_golden`) so that the six
//! pre-675 corpus targets remain byte-identical to their pre-feature
//! output.
//!
//! Contract at `specs/675-pants-js-corpus/contracts/js-golden-filter.md`.
//!
//! The pants-example-javascript corpus target's full CDX is ~570 KB
//! (302 pkg:npm/* components + a mix of doc-scope annotations), which
//! violates SC-004's 500 KB combined-goldens budget. Filtering to the
//! JS surface only lands under 200 KB across all three formats.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::HashSet;

/// Filter a CDX 1.6 JSON document to the `pkg:npm/*` surface only.
///
/// Retains: envelope fields, `.metadata`, `.components[]` entries with
/// PURL prefix `pkg:npm/`, `.dependencies[]` entries whose `.ref` is a
/// retained PURL, with each retained entry's `.dependsOn[]` filtered
/// to only npm PURLs.
///
/// Idempotent: applying twice yields byte-identical output.
pub fn filter_cdx_to_js(v: &mut serde_json::Value) {
    let Some(obj) = v.as_object_mut() else { return };

    // Filter components — drop entries whose .purl doesn't start with pkg:npm/
    // (or is missing).
    if let Some(components) = obj.get_mut("components").and_then(|c| c.as_array_mut()) {
        components.retain(|c| {
            c.get("purl")
                .and_then(|p| p.as_str())
                .is_some_and(|p| p.starts_with("pkg:npm/"))
        });
    }

    // Filter dependencies — drop entries whose .ref isn't an npm PURL; for
    // retained entries, prune .dependsOn to npm PURLs only.
    if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_array_mut()) {
        deps.retain_mut(|dep| {
            let is_npm_ref = dep
                .get("ref")
                .and_then(|r| r.as_str())
                .is_some_and(|r| r.starts_with("pkg:npm/"));
            if !is_npm_ref {
                return false;
            }
            if let Some(depends_on) = dep.get_mut("dependsOn").and_then(|d| d.as_array_mut()) {
                depends_on.retain(|t| {
                    t.as_str().is_some_and(|s| s.starts_with("pkg:npm/"))
                });
            }
            true
        });
    }
}

/// Filter a SPDX 2.3 JSON document to the `pkg:npm/*` surface only.
///
/// Retains: envelope fields, `.creationInfo`, `.documentDescribes` (kept
/// unmodified — root document reference), `.packages[]` entries that
/// either (a) have any `.externalRefs[]` entry with a `pkg:npm/*`
/// referenceLocator, or (b) are the root document package (SPDXID matches
/// `SPDXRef-DOCUMENT` or appears in `documentDescribes`).
/// `.relationships[]` entries are retained only when both endpoint
/// SPDXIDs are in the retained set.
///
/// Retained packages keep ALL their externalRefs (not filtered per-entry).
pub fn filter_spdx23_to_js(v: &mut serde_json::Value) {
    let Some(obj) = v.as_object_mut() else { return };

    // Compute the "always keep" set: documentDescribes references + the
    // canonical root SPDXID.
    let mut always_keep: HashSet<String> = HashSet::new();
    always_keep.insert("SPDXRef-DOCUMENT".to_string());
    if let Some(desc) = obj.get("documentDescribes").and_then(|d| d.as_array()) {
        for id in desc {
            if let Some(s) = id.as_str() {
                always_keep.insert(s.to_string());
            }
        }
    }

    // Build the kept-SPDXID set: always_keep ∪ any package with npm
    // externalRef.
    let mut kept_spdxids: HashSet<String> = always_keep.clone();
    if let Some(packages) = obj.get("packages").and_then(|p| p.as_array()) {
        for pkg in packages {
            let Some(spdxid) = pkg.get("SPDXID").and_then(|s| s.as_str()) else {
                continue;
            };
            if always_keep.contains(spdxid) {
                continue;
            }
            let has_npm = pkg
                .get("externalRefs")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        r.get("referenceLocator")
                            .and_then(|l| l.as_str())
                            .is_some_and(|l| l.starts_with("pkg:npm/"))
                    })
                })
                .unwrap_or(false);
            if has_npm {
                kept_spdxids.insert(spdxid.to_string());
            }
        }
    }

    // Filter packages by the kept set.
    if let Some(packages) = obj.get_mut("packages").and_then(|p| p.as_array_mut()) {
        packages.retain(|pkg| {
            pkg.get("SPDXID")
                .and_then(|s| s.as_str())
                .is_some_and(|s| kept_spdxids.contains(s))
        });
    }

    // Filter relationships — both endpoints must be in the kept set.
    if let Some(rels) = obj.get_mut("relationships").and_then(|r| r.as_array_mut()) {
        rels.retain(|rel| {
            let a = rel.get("spdxElementId").and_then(|s| s.as_str());
            let b = rel.get("relatedSpdxElement").and_then(|s| s.as_str());
            matches!((a, b), (Some(a), Some(b)) if kept_spdxids.contains(a) && kept_spdxids.contains(b))
        });
    }
}

/// Filter a SPDX 3.0.1 JSON-LD document to the `pkg:npm/*` surface only.
///
/// Retains: `@context`; doc-scope typed nodes (`SpdxDocument`, `CreationInfo`,
/// `Person`, `Organization`, `Tool`); component nodes whose PURL
/// externalIdentifier starts with `pkg:npm/`; relationship nodes where both
/// `from` and (filtered) `to` reference retained spdxIds.
///
/// `to` may be an array — its members are filtered to retained spdxIds
/// (drop the relationship if `to` becomes empty). `to` may also be a
/// scalar string — drop the relationship if it references a removed node.
pub fn filter_spdx3_to_js(v: &mut serde_json::Value) {
    let Some(obj) = v.as_object_mut() else { return };

    // SPDX 3 uses "type" (per the JPEWdev validator we gate on); support
    // "@type" as a fallback for JSON-LD variants.
    fn node_type(n: &serde_json::Value) -> Option<&str> {
        n.get("type")
            .and_then(|t| t.as_str())
            .or_else(|| n.get("@type").and_then(|t| t.as_str()))
    }

    fn node_spdxid(n: &serde_json::Value) -> Option<&str> {
        n.get("spdxId").and_then(|s| s.as_str())
    }

    const DOC_SCOPE_TYPES: &[&str] = &[
        "SpdxDocument",
        "CreationInfo",
        "Person",
        "Organization",
        "Tool",
    ];

    let Some(graph) = obj.get_mut("@graph").and_then(|g| g.as_array_mut()) else {
        return;
    };

    // Pass 1: collect the set of retained spdxIds. Doc-scope nodes are
    // always retained; component nodes are retained iff they have an
    // externalIdentifier of type "purl" starting with pkg:npm/.
    let mut kept_ids: HashSet<String> = HashSet::new();
    for node in graph.iter() {
        let Some(ty) = node_type(node) else { continue };
        let Some(id) = node_spdxid(node) else { continue };
        if DOC_SCOPE_TYPES.contains(&ty) {
            kept_ids.insert(id.to_string());
            continue;
        }
        // Component-like nodes: software_Package, software_File, software_Sbom
        let has_npm_purl = node
            .get("externalIdentifier")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter().any(|r| {
                    let ty_is_purl = r
                        .get("externalIdentifierType")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t == "purl");
                    let ident_is_npm = r
                        .get("identifier")
                        .and_then(|i| i.as_str())
                        .is_some_and(|i| i.starts_with("pkg:npm/"));
                    ty_is_purl && ident_is_npm
                })
            })
            .unwrap_or(false);
        if has_npm_purl {
            kept_ids.insert(id.to_string());
        }
    }

    // Pass 2: filter. Component / file / SBOM nodes: retain iff in kept_ids.
    // Relationship nodes: retain iff both endpoints are in kept_ids
    // (with `to` array pruning).
    graph.retain_mut(|node| {
        let Some(ty) = node_type(node).map(str::to_string) else {
            return true; // untyped nodes pass through (defensive)
        };
        if ty == "Relationship" {
            let from_ok = node
                .get("from")
                .and_then(|f| f.as_str())
                .is_some_and(|f| kept_ids.contains(f));
            if !from_ok {
                return false;
            }
            // .to may be a string or an array.
            let to_ref = node.get_mut("to");
            match to_ref {
                Some(serde_json::Value::String(s)) => {
                    return kept_ids.contains(s.as_str());
                }
                Some(serde_json::Value::Array(arr)) => {
                    arr.retain(|item| {
                        item.as_str().is_some_and(|s| kept_ids.contains(s))
                    });
                    return !arr.is_empty();
                }
                _ => return false,
            }
        }
        // Non-relationship: retain iff in kept set (which includes both
        // doc-scope and component nodes).
        node_spdxid(node)
            .map(|id| kept_ids.contains(id))
            .unwrap_or(false)
    });
}

// ------------------------------------------------------------------
// Unit tests — per contracts/js-golden-filter.md testing section
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // (a) Happy-path CDX with mixed npm + pypi components → npm-only remain.
    #[test]
    fn cdx_happy_path_mixed_ecosystems() {
        let mut v = json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.6",
            "metadata": {"timestamp": "<masked>"},
            "components": [
                {"purl": "pkg:npm/left-pad@1.0.0", "name": "left-pad"},
                {"purl": "pkg:pypi/requests@2.0.0", "name": "requests"},
                {"purl": "pkg:npm/right-pad@2.0.0", "name": "right-pad"},
                {"name": "no-purl-component"}
            ],
            "dependencies": [
                {"ref": "pkg:npm/left-pad@1.0.0", "dependsOn": ["pkg:npm/right-pad@2.0.0", "pkg:pypi/requests@2.0.0"]},
                {"ref": "pkg:pypi/requests@2.0.0", "dependsOn": []}
            ]
        });
        filter_cdx_to_js(&mut v);

        let comps = v["components"].as_array().unwrap();
        assert_eq!(comps.len(), 2, "expected 2 npm components after filter");
        assert!(comps.iter().all(|c| c["purl"].as_str().unwrap().starts_with("pkg:npm/")));

        let deps = v["dependencies"].as_array().unwrap();
        assert_eq!(deps.len(), 1, "expected 1 dep entry (only left-pad's) after filter");
        let left = &deps[0];
        assert_eq!(left["ref"], "pkg:npm/left-pad@1.0.0");
        let depends_on = left["dependsOn"].as_array().unwrap();
        assert_eq!(depends_on.len(), 1, "dependsOn should be pruned to npm only");
        assert_eq!(depends_on[0], "pkg:npm/right-pad@2.0.0");

        assert!(v["metadata"].is_object(), "metadata retained");
    }

    // (b) Missing .dependencies field → filter still runs, no panic.
    #[test]
    fn cdx_missing_dependencies_field() {
        let mut v = json!({
            "bomFormat": "CycloneDX",
            "components": [
                {"purl": "pkg:npm/x@1", "name": "x"},
                {"purl": "pkg:pypi/y@1", "name": "y"}
            ]
        });
        filter_cdx_to_js(&mut v);
        assert_eq!(v["components"].as_array().unwrap().len(), 1);
        assert!(v.get("dependencies").is_none(), "no field synthesized");
    }

    // (c) Idempotency — applying twice yields byte-identical output.
    #[test]
    fn cdx_idempotent() {
        let mut a = json!({
            "components": [
                {"purl": "pkg:npm/x@1"},
                {"purl": "pkg:pypi/y@1"}
            ],
            "dependencies": [
                {"ref": "pkg:npm/x@1", "dependsOn": ["pkg:npm/y@1", "pkg:pypi/z@1"]}
            ]
        });
        let mut b = a.clone();
        filter_cdx_to_js(&mut a);
        filter_cdx_to_js(&mut b);
        filter_cdx_to_js(&mut b); // twice
        let a_bytes = serde_json::to_vec_pretty(&a).unwrap();
        let b_bytes = serde_json::to_vec_pretty(&b).unwrap();
        assert_eq!(a_bytes, b_bytes, "filter is not idempotent");
    }

    // (d) SPDX 2.3 root document retention — root package with no npm
    // externalRef still survives.
    #[test]
    fn spdx23_root_document_retained() {
        let mut v = json!({
            "spdxVersion": "SPDX-2.3",
            "SPDXID": "SPDXRef-DOCUMENT",
            "documentDescribes": ["SPDXRef-ROOT-PKG"],
            "creationInfo": {"created": "<masked>"},
            "packages": [
                {
                    "SPDXID": "SPDXRef-ROOT-PKG",
                    "name": "root-project",
                    "externalRefs": []
                },
                {
                    "SPDXID": "SPDXRef-PKG-npm-x",
                    "name": "x",
                    "externalRefs": [{"referenceLocator": "pkg:npm/x@1", "referenceType": "purl"}]
                },
                {
                    "SPDXID": "SPDXRef-PKG-pypi-y",
                    "name": "y",
                    "externalRefs": [{"referenceLocator": "pkg:pypi/y@1", "referenceType": "purl"}]
                }
            ],
            "relationships": [
                {"spdxElementId": "SPDXRef-DOCUMENT", "relatedSpdxElement": "SPDXRef-ROOT-PKG", "relationshipType": "DESCRIBES"},
                {"spdxElementId": "SPDXRef-ROOT-PKG", "relatedSpdxElement": "SPDXRef-PKG-npm-x", "relationshipType": "DEPENDS_ON"},
                {"spdxElementId": "SPDXRef-ROOT-PKG", "relatedSpdxElement": "SPDXRef-PKG-pypi-y", "relationshipType": "DEPENDS_ON"}
            ]
        });
        filter_spdx23_to_js(&mut v);

        let packages = v["packages"].as_array().unwrap();
        let ids: Vec<&str> = packages.iter().map(|p| p["SPDXID"].as_str().unwrap()).collect();
        assert!(ids.contains(&"SPDXRef-ROOT-PKG"), "root pkg must survive despite no npm ref");
        assert!(ids.contains(&"SPDXRef-PKG-npm-x"), "npm pkg must survive");
        assert!(!ids.contains(&"SPDXRef-PKG-pypi-y"), "pypi pkg must be dropped");

        let rels = v["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 2, "expected 2 relationships (DESCRIBES + npm DEPENDS_ON), got {}", rels.len());
    }

    // (e) SPDX 3 relationship with mixed .to array — drop non-kept targets,
    // retain relationship if any kept remain, drop if all removed.
    #[test]
    fn spdx3_relationship_mixed_to_array() {
        let mut v = json!({
            "@context": "https://spdx.org/rdf/3.0.1/spdx-context.jsonld",
            "@graph": [
                {"type": "SpdxDocument", "spdxId": "https://example/doc"},
                {"type": "CreationInfo", "spdxId": "_:creation-info"},
                {
                    "type": "software_Package",
                    "spdxId": "https://example/pkg-npm-x",
                    "externalIdentifier": [{"externalIdentifierType": "purl", "identifier": "pkg:npm/x@1"}]
                },
                {
                    "type": "software_Package",
                    "spdxId": "https://example/pkg-npm-y",
                    "externalIdentifier": [{"externalIdentifierType": "purl", "identifier": "pkg:npm/y@1"}]
                },
                {
                    "type": "software_Package",
                    "spdxId": "https://example/pkg-pypi-z",
                    "externalIdentifier": [{"externalIdentifierType": "purl", "identifier": "pkg:pypi/z@1"}]
                },
                {
                    "type": "Relationship",
                    "spdxId": "_:rel-mixed",
                    "from": "https://example/pkg-npm-x",
                    "to": ["https://example/pkg-npm-y", "https://example/pkg-pypi-z"],
                    "relationshipType": "dependsOn"
                },
                {
                    "type": "Relationship",
                    "spdxId": "_:rel-all-dropped",
                    "from": "https://example/pkg-npm-x",
                    "to": ["https://example/pkg-pypi-z"],
                    "relationshipType": "dependsOn"
                }
            ]
        });
        filter_spdx3_to_js(&mut v);

        let graph = v["@graph"].as_array().unwrap();
        let ids: Vec<&str> = graph.iter().filter_map(|n| n["spdxId"].as_str()).collect();
        assert!(ids.contains(&"https://example/pkg-npm-x"), "npm x kept");
        assert!(ids.contains(&"https://example/pkg-npm-y"), "npm y kept");
        assert!(!ids.contains(&"https://example/pkg-pypi-z"), "pypi z dropped");
        assert!(ids.contains(&"_:rel-mixed"), "mixed relationship kept (has some npm)");
        assert!(!ids.contains(&"_:rel-all-dropped"), "all-dropped relationship removed");

        // Verify the retained relationship's `to` array was pruned.
        let rel = graph.iter().find(|n| n["spdxId"] == "_:rel-mixed").unwrap();
        let to = rel["to"].as_array().unwrap();
        assert_eq!(to.len(), 1);
        assert_eq!(to[0], "https://example/pkg-npm-y");
    }
}
