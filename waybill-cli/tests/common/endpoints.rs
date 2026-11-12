//! #1436 T008 — every reference a document makes must name something it
//! carries, in each of the three formats.
//!
//! The per-suite copies of this check (`document_integrity.rs`,
//! `closure_edges.rs`, `closure_runtime_scope.rs`) each look at one format or
//! one edge population. This one is for suites that change a component's
//! identity: a PURL is identity, and a change that misses an edge orphans it
//! silently (#980) — CycloneDX keeps the edge, SPDX drops it, and neither
//! errors. So it checks all three, and the VEX references as well as the
//! edges.

use std::collections::BTreeSet;

use serde_json::Value;

/// Every `bom-ref` in a CycloneDX document, nested components and the root
/// included.
fn cdx_refs(doc: &Value) -> BTreeSet<&str> {
    fn walk<'a>(v: &'a Value, out: &mut BTreeSet<&'a str>) {
        for c in v.as_array().into_iter().flatten() {
            if let Some(r) = c["bom-ref"].as_str() {
                out.insert(r);
            }
            walk(&c["components"], out);
        }
    }
    let mut out = BTreeSet::new();
    if let Some(r) = doc["metadata"]["component"]["bom-ref"].as_str() {
        out.insert(r);
    }
    walk(&doc["components"], &mut out);
    out
}

/// CycloneDX references naming no `bom-ref`: `dependencies[]` and
/// `vulnerabilities[].affects[]`.
pub fn cdx_dangling(doc: &Value) -> Vec<String> {
    let refs = cdx_refs(doc);
    let mut out = Vec::new();
    let mut check = |what: &str, r: &str| {
        if !refs.contains(r) {
            out.push(format!("cdx {what}: {r}"));
        }
    };
    for d in doc["dependencies"].as_array().into_iter().flatten() {
        check("dependencies.ref", d["ref"].as_str().unwrap_or_default());
        for t in d["dependsOn"].as_array().into_iter().flatten() {
            check("dependencies.dependsOn", t.as_str().unwrap_or_default());
        }
    }
    for v in doc["vulnerabilities"].as_array().into_iter().flatten() {
        for a in v["affects"].as_array().into_iter().flatten() {
            check("vulnerabilities.affects", a["ref"].as_str().unwrap_or_default());
        }
    }
    out
}

/// SPDX 2.3 relationship endpoints naming no package, file or the document.
pub fn spdx23_dangling(doc: &Value) -> Vec<String> {
    let ids: BTreeSet<&str> = ["packages", "files"]
        .iter()
        .flat_map(|k| doc[*k].as_array().into_iter().flatten())
        .filter_map(|p| p["SPDXID"].as_str())
        .chain(doc["SPDXID"].as_str())
        .chain(["NOASSERTION", "NONE"])
        .collect();
    let mut out = Vec::new();
    for r in doc["relationships"].as_array().into_iter().flatten() {
        for k in ["spdxElementId", "relatedSpdxElement"] {
            let id = r[k].as_str().unwrap_or_default();
            if !ids.contains(id) && !id.starts_with("DocumentRef-") {
                out.push(format!("spdx-2.3 {} {k}: {id}", r["relationshipType"]));
            }
        }
    }
    out
}

/// SPDX 3 `Relationship` endpoints naming no element (the vocabulary's own
/// individuals excepted).
pub fn spdx3_dangling(doc: &Value) -> Vec<String> {
    let graph = doc["@graph"].as_array().cloned().unwrap_or_default();
    let ids: BTreeSet<&str> = graph.iter().filter_map(|e| e["spdxId"].as_str()).collect();
    let individual = |iri: &str| {
        let name = iri.strip_prefix("https://spdx.org/rdf/3.0.1/terms/Core/").unwrap_or(iri);
        matches!(name, "NoAssertionElement" | "NoneElement")
    };
    let mut out = Vec::new();
    for e in &graph {
        if e["type"].as_str() != Some("Relationship") {
            continue;
        }
        let ends = e["from"]
            .as_str()
            .into_iter()
            .chain(e["to"].as_array().into_iter().flatten().filter_map(Value::as_str));
        for id in ends {
            if !ids.contains(id) && !individual(id) {
                out.push(format!("spdx-3 Relationship endpoint: {id}"));
            }
        }
    }
    out
}

/// Every dangling reference in the three documents of one scan.
pub fn dangling(cdx: &Value, spdx23: &Value, spdx3: &Value) -> Vec<String> {
    let mut out = cdx_dangling(cdx);
    out.extend(spdx23_dangling(spdx23));
    out.extend(spdx3_dangling(spdx3));
    out
}

/// How many references the three documents make, so a caller can show the
/// check had something to look at.
pub fn reference_count(cdx: &Value, spdx23: &Value, spdx3: &Value) -> usize {
    let cdx_n: usize = cdx["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| 1 + d["dependsOn"].as_array().map_or(0, Vec::len))
        .sum();
    let s23 = spdx23["relationships"].as_array().map_or(0, Vec::len);
    let s3 = spdx3["@graph"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["type"].as_str() == Some("Relationship"))
        .count();
    cdx_n + s23 + s3
}
