#[allow(unused_imports)]
use std::collections::HashSet;
use crate::corpus_harness_195::js_filter::*;
use serde_json::json;

/// #1677: the Haskell filter keeps `pkg:hackage/*` components, their
/// edges and the document's metadata (where C200 lives), and drops every
/// other ecosystem.
#[test]
fn haskell_prefix_keeps_hackage_and_document_metadata() {
    let mut v = json!({
        "metadata": {
            "component": {"bom-ref": "pkg:generic/hpack-x@1"},
            "properties": [{"name": "waybill:hpack-read-outcome", "value": "{}"}]
        },
        "components": [
            {"purl": "pkg:hackage/base@4.18.0", "bom-ref": "pkg:hackage/base@4.18.0"},
            {"purl": "pkg:npm/left-pad@1.0.0", "bom-ref": "pkg:npm/left-pad@1.0.0"},
            {"purl": "pkg:cargo/serde@1.0.0", "bom-ref": "pkg:cargo/serde@1.0.0"}
        ],
        "dependencies": [
            {"ref": "pkg:generic/hpack-x@1", "dependsOn": ["pkg:hackage/base@4.18.0", "pkg:npm/left-pad@1.0.0"]},
            {"ref": "pkg:npm/left-pad@1.0.0", "dependsOn": []}
        ]
    });
    filter_cdx_to_prefix(&mut v, HASKELL_PREFIX);
    let purls: Vec<&str> = v["components"].as_array().unwrap().iter().map(|c| c["purl"].as_str().unwrap()).collect();
    assert_eq!(purls, vec!["pkg:hackage/base@4.18.0"]);
    assert_eq!(v["metadata"]["properties"][0]["name"], "waybill:hpack-read-outcome");
    assert_eq!(v["dependencies"], json!([{"ref": "pkg:generic/hpack-x@1", "dependsOn": ["pkg:hackage/base@4.18.0"]}]));
}

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
