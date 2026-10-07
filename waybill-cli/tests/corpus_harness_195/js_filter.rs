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
/// retained PURL **or the primary component's bom-ref**, with each
/// retained entry's `.dependsOn[]` filtered to only npm PURLs.
///
/// Idempotent: applying twice yields byte-identical output.
pub fn filter_cdx_to_js(v: &mut serde_json::Value) {
    let Some(obj) = v.as_object_mut() else { return };

    // The primary component's bom-ref, retained unconditionally below.
    //
    // Its ref is the operator-supplied root identity (`<name>@<version>`,
    // or a `pkg:generic/*` PURL) — never an npm PURL, so the npm-prefix
    // test below drops it, and with it the one edge that says what the
    // project itself depends on. That is what made issue #881 look like a
    // CycloneDX emitter defect: the emitter had the edge all along
    // (measured: 1 root out-edge in all three formats on this target),
    // the CDX filter removed it, the SPDX 2.3 filter kept its equivalent
    // via `always_keep`, and the two goldens disagreed for a reason that
    // had nothing to do with either emitter.
    let root_ref: Option<String> = obj
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("bom-ref"))
        .and_then(|r| r.as_str())
        .map(str::to_string);

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
            let dep_ref = dep.get("ref").and_then(|r| r.as_str());
            let is_npm_ref = dep_ref.is_some_and(|r| r.starts_with("pkg:npm/"));
            let is_root = dep_ref.is_some() && dep_ref == root_ref.as_deref();
            if !is_npm_ref && !is_root {
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
/// `Person`, `Organization`, `Tool`); the document's `rootElement`; component
/// nodes carrying an npm PURL (via `software_packageUrl`, or an
/// `externalIdentifier` of type `packageUrl`/`purl`); `Relationship` and
/// `LifecycleScopedRelationship` nodes where both `from` and (filtered) `to`
/// reference retained spdxIds.
///
/// `Annotation` nodes are retained iff their `subject` is a retained spdxId.
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

    // Pass 1: collect the set of retained spdxIds. Doc-scope nodes and the
    // document's own rootElement are always retained; component nodes are
    // retained iff they carry an npm PURL.
    //
    // The PURL test reads `software_packageUrl` and accepts either
    // `packageUrl` or `purl` as the externalIdentifierType. `packageUrl` is
    // the value waybill emits and the one the SPDX 3.0.1 vocabulary
    // defines (`ExternalIdentifierType/packageUrl`); the original filter
    // tested only for `purl`, which matched nothing, so every
    // software_Package in the document was dropped. The committed golden
    // was 14 nodes — 12 Organizations, a Tool and the SpdxDocument — for a
    // target whose whole purpose is to regression-lock 304 npm packages.
    // It asserted nothing, and satisfied the size budget by asserting
    // nothing (SC-004 recorded "SPDX 3 ~4 KB" against ~550 KB and ~976 KB
    // for the other two formats, a 250x gap that was measured and not
    // questioned). The unit test below could not catch it: its fixture was
    // hand-built with the same wrong value the filter looked for.
    let mut kept_ids: HashSet<String> = HashSet::new();
    for node in graph.iter() {
        if node_type(node) == Some("SpdxDocument") {
            for key in ["rootElement", "software_rootElement"] {
                for id in node.get(key).and_then(|r| r.as_array()).into_iter().flatten() {
                    if let Some(s) = id.as_str() {
                        kept_ids.insert(s.to_string());
                    }
                }
            }
        }
    }
    for node in graph.iter() {
        let Some(ty) = node_type(node) else { continue };
        let Some(id) = node_spdxid(node) else { continue };
        if DOC_SCOPE_TYPES.contains(&ty) {
            kept_ids.insert(id.to_string());
            continue;
        }
        // Component-like nodes: software_Package, software_File, software_Sbom
        let has_npm_purl = node
            .get("software_packageUrl")
            .and_then(|u| u.as_str())
            .is_some_and(|u| u.starts_with("pkg:npm/"))
            || node
                .get("externalIdentifier")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter().any(|r| {
                        let ty_is_purl = r
                            .get("externalIdentifierType")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "packageUrl" || t == "purl");
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
        // Both spellings are edges. waybill emits `LifecycleScopedRelationship`
        // for every scoped dependency edge — 640 of the 641 on this target —
        // and the original filter recognised only `Relationship`, so the
        // scoped edges fell through to the non-relationship branch below and
        // were dropped for not being in the kept set.
        if ty == "Relationship" || ty == "LifecycleScopedRelationship" {
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
        // Annotations: retain iff their subject is kept, as relationships
        // are retained by their endpoints. Without this every annotation
        // was dropped, so this target's SPDX 3 golden locked none of them
        // while the CDX and SPDX 2.3 goldens, which carry annotations inside
        // the component, locked all of them (#1150).
        if ty == "Annotation" {
            return node
                .get("subject")
                .and_then(|s| s.as_str())
                .is_some_and(|s| kept_ids.contains(s));
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
    // #1150: annotations follow their subject, document-scope included.
    #[test]
    fn spdx3_annotations_follow_their_subject() {
        let mut v = json!({
            "@graph": [
                {"type": "SpdxDocument", "spdxId": "https://example/doc"},
                {"type": "software_Package", "spdxId": "https://example/pkg-npm-x", "software_packageUrl": "pkg:npm/x@1"},
                {"type": "software_Package", "spdxId": "https://example/pkg-pypi-z", "software_packageUrl": "pkg:pypi/z@1"},
                {"type": "Annotation", "spdxId": "https://example/anno-npm", "subject": "https://example/pkg-npm-x", "statement": "a"},
                {"type": "Annotation", "spdxId": "https://example/anno-pypi", "subject": "https://example/pkg-pypi-z", "statement": "b"},
                {"type": "Annotation", "spdxId": "https://example/anno-doc", "subject": "https://example/doc", "statement": "c"}
            ]
        });
        filter_spdx3_to_js(&mut v);
        let ids: Vec<&str> = v["@graph"].as_array().unwrap().iter().filter_map(|n| n["spdxId"].as_str()).collect();
        assert!(ids.contains(&"https://example/anno-npm"), "npm package's annotation kept");
        assert!(ids.contains(&"https://example/anno-doc"), "document annotation kept");
        assert!(!ids.contains(&"https://example/anno-pypi"), "dropped package's annotation dropped");
    }

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

    // ------------------------------------------------------------------
    // #881 regression — the three defects that made a cross-format root
    // comparison on this target meaningless. Every fixture below uses the
    // shapes waybill actually emits, copied from a real scan of the pinned
    // corpus target, NOT hand-invented ones. The pre-existing
    // `spdx3_relationship_mixed_to_array` test passed throughout, because
    // its fixture was written with the same wrong `externalIdentifierType`
    // the filter looked for.
    // ------------------------------------------------------------------

    /// One `software_Package` exactly as waybill emits it: the PURL lives in
    /// `software_packageUrl` and in an `externalIdentifier` of type
    /// `packageUrl` (not `purl`).
    fn real_spdx3_doc() -> serde_json::Value {
        json!({
            "@graph": [
                {"type": "SpdxDocument", "spdxId": "doc-1", "rootElement": ["pkg-root"]},
                {"type": "Tool", "spdxId": "tool-1"},
                {
                    "type": "software_Package", "spdxId": "pkg-root",
                    "name": "pants-example-javascript",
                    "software_packageUrl": "pkg:generic/pants-example-javascript@da76d5d"
                },
                {
                    "type": "software_Package", "spdxId": "pkg-demo", "name": "demo",
                    "software_packageUrl": "pkg:npm/demo@0.0.1",
                    "externalIdentifier": [
                        {"externalIdentifierType": "packageUrl", "identifier": "pkg:npm/demo@0.0.1"}
                    ]
                },
                {
                    "type": "software_Package", "spdxId": "pkg-py", "name": "requests",
                    "software_packageUrl": "pkg:pypi/requests@2.0.0",
                    "externalIdentifier": [
                        {"externalIdentifierType": "packageUrl", "identifier": "pkg:pypi/requests@2.0.0"}
                    ]
                },
                {"type": "Relationship", "spdxId": "rel-1", "from": "pkg-root",
                 "relationshipType": "dependsOn", "to": ["pkg-demo"]},
                {"type": "LifecycleScopedRelationship", "spdxId": "rel-2", "from": "pkg-demo",
                 "relationshipType": "dependsOn", "to": ["pkg-py"]},
                {"type": "LifecycleScopedRelationship", "spdxId": "rel-3", "from": "pkg-demo",
                 "relationshipType": "dependsOn", "to": ["pkg-demo"]}
            ]
        })
    }

    #[test]
    fn spdx3_retains_packages_emitted_with_packageurl_identifier_type() {
        let mut v = real_spdx3_doc();
        filter_spdx3_to_js(&mut v);
        let names: Vec<&str> = v["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["type"] == "software_Package")
            .map(|n| n["name"].as_str().unwrap())
            .collect();
        // demo is kept for its npm PURL; the root for being rootElement;
        // requests is dropped for being pypi.
        assert!(names.contains(&"demo"), "npm package dropped: {names:?}");
        assert!(names.contains(&"pants-example-javascript"), "root dropped: {names:?}");
        assert!(!names.contains(&"requests"), "pypi package retained: {names:?}");
    }

    #[test]
    fn spdx3_retains_lifecycle_scoped_relationships() {
        let mut v = real_spdx3_doc();
        filter_spdx3_to_js(&mut v);
        let g = v["@graph"].as_array().unwrap();
        let scoped = g.iter().filter(|n| n["type"] == "LifecycleScopedRelationship").count();
        let plain = g.iter().filter(|n| n["type"] == "Relationship").count();
        // rel-2's only target is pypi and is pruned to empty -> dropped.
        // rel-3 (demo -> demo) survives. rel-1 (root -> demo) survives.
        assert_eq!(scoped, 1, "scoped edges dropped; graph: {g:#?}");
        assert_eq!(plain, 1, "plain edge dropped; graph: {g:#?}");
    }

    #[test]
    fn spdx3_retains_the_root_out_edge() {
        let mut v = real_spdx3_doc();
        filter_spdx3_to_js(&mut v);
        let root_edges = v["@graph"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["from"] == "pkg-root")
            .count();
        assert_eq!(root_edges, 1, "the root's only out-edge was filtered away");
    }

    #[test]
    fn cdx_retains_the_primary_components_dependency_entry() {
        let mut v = json!({
            "metadata": {"component": {"bom-ref": "pants-example-javascript@da76d5d"}},
            "components": [{"purl": "pkg:npm/demo@0.0.1"}],
            "dependencies": [
                {"ref": "pants-example-javascript@da76d5d",
                 "dependsOn": ["pkg:npm/demo@0.0.1", "pkg:pypi/requests@2.0.0"]},
                {"ref": "pkg:npm/demo@0.0.1", "dependsOn": []},
                {"ref": "pkg:pypi/requests@2.0.0", "dependsOn": []}
            ]
        });
        filter_cdx_to_js(&mut v);
        let deps = v["dependencies"].as_array().unwrap();
        let root = deps
            .iter()
            .find(|e| e["ref"] == "pants-example-javascript@da76d5d")
            .expect("primary component's dependencies entry was dropped");
        // Retained, but its dependsOn is still pruned to the npm surface.
        assert_eq!(root["dependsOn"].as_array().unwrap().len(), 1);
        assert_eq!(root["dependsOn"][0], "pkg:npm/demo@0.0.1");
        assert_eq!(deps.len(), 2, "pypi entry should still be dropped");
    }

    #[test]
    fn cdx_root_retention_is_idempotent() {
        let mut v = json!({
            "metadata": {"component": {"bom-ref": "root@1"}},
            "components": [{"purl": "pkg:npm/x@1"}],
            "dependencies": [{"ref": "root@1", "dependsOn": ["pkg:npm/x@1"]}]
        });
        filter_cdx_to_js(&mut v);
        let once = v.clone();
        filter_cdx_to_js(&mut v);
        assert_eq!(once, v);
    }
}
