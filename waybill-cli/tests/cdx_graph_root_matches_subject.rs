//! #1572: CycloneDX roots its dependency graph at the component
//! `metadata.component` names. When the root selector chooses a synthetic
//! placeholder over two main modules, the graph used to be rooted at the
//! first main module instead, and the orphan fallback ("the root depends on
//! everything nothing else depends on") made that package declare
//! `dependsOn` every orphan in the tree. SPDX 2.3 and SPDX 3 anchored the
//! same orphans to the subject.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::installed_lib::{scan, write};
use serde_json::Value;

/// Two Python main modules (one versioned, one dynamic) and some orphans:
/// the shape of an installed SDK carrying Python tools.
fn tree(root: &Path) {
    write(root, "share/waybill_fixture_a/pyproject.toml", "[project]\nname = \"waybill-fixture-a\"\nversion = \"1.2.3\"\n");
    write(root, "share/waybill_fixture_a/setup.py", "from setuptools import setup\nsetup()\n");
    write(root, "libexec/waybill-fixture-b/pyproject.toml", "[project]\nname = \"waybill-fixture-b\"\ndynamic = [\"version\"]\n");
    write(root, "libexec/waybill-fixture-b/requirements.txt", "waybill-fixture-dep==1.0\n");
    write(root, "lib/pkgconfig/waybill-fixture-c.pc", "Version: 4.5\n");
    for f in ["bin/tool-one", "bin/tool-two", "lib/libwaybill-fixture-c.so.4"] {
        write(root, f, &format!("#!/bin/sh\n# {f}\n"));
    }
}

fn cdx_names(cdx: &Value) -> std::collections::BTreeMap<String, String> {
    let mut m: std::collections::BTreeMap<String, String> = cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["bom-ref"].as_str().unwrap().to_string(), c["name"].as_str().unwrap().to_string()))
        .collect();
    let s = &cdx["metadata"]["component"];
    m.insert(s["bom-ref"].as_str().unwrap().to_string(), s["name"].as_str().unwrap().to_string());
    m
}

fn cdx_depends(cdx: &Value, r: &str) -> Option<BTreeSet<String>> {
    let names = cdx_names(cdx);
    cdx["dependencies"].as_array().unwrap().iter().find(|e| e["ref"] == r).map(|e| {
        e["dependsOn"].as_array().unwrap().iter().map(|x| names[x.as_str().unwrap()].clone()).collect()
    })
}

#[test]
fn the_cdx_graph_is_rooted_at_the_subject() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sdk");
    tree(&root);
    let docs = scan(&root);

    let subject = docs.cdx["metadata"]["component"]["bom-ref"].as_str().unwrap().to_string();
    let anchored = cdx_depends(&docs.cdx, &subject).expect("the subject has a dependencies[] entry");

    // SPDX 2.3 anchors the same components to the subject it describes.
    let spdx = &docs.spdx23;
    let names: std::collections::BTreeMap<&str, &str> = spdx["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["SPDXID"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let described = spdx["documentDescribes"][0].as_str().unwrap();
    let spdx_anchored: BTreeSet<String> = spdx["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["spdxElementId"] == described && r["relationshipType"] == "DEPENDS_ON")
        .map(|r| names[r["relatedSpdxElement"].as_str().unwrap()].to_string())
        .collect();
    assert_eq!(anchored, spdx_anchored, "CycloneDX and SPDX 2.3 anchor different sets to the subject");

    // A main module that declares nothing depends on nothing.
    let a = docs.cdx["components"].as_array().unwrap().iter().find(|c| c["name"] == "waybill-fixture-a").unwrap();
    let a_deps = cdx_depends(&docs.cdx, a["bom-ref"].as_str().unwrap()).unwrap_or_default();
    assert!(a_deps.is_empty(), "waybill-fixture-a declares nothing, but CycloneDX says it depends on {a_deps:?}");
}
