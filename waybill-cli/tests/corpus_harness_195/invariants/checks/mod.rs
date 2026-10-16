//! One module per invariant, plus the document walkers they share.
//!
//! Adding an invariant is a new file here, a `REGISTRY` line in the
//! parent module, and a catalog row — which is the shape that makes the
//! whole layer worth having.
//!
//! **Every checker tests its precondition first and returns
//! `NotApplicable`**, never `Pass`. A pass on an unmet precondition is a
//! vacuous pass, and a vacuous pass is indistinguishable from a working
//! check (FR-006, SC-009).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub use super::super::harness::EmittedSboms;

pub mod completeness_honesty;
pub mod file_tier_edges;
pub mod identity_unique;
pub mod orphan_explained;
pub mod root_edge_parity;
pub mod scope_propagation;
pub mod version_sentinels;

// -----------------------------------------------------------------------
// CycloneDX walkers
// -----------------------------------------------------------------------

pub fn cdx_components(cdx: &serde_json::Value) -> &[serde_json::Value] {
    cdx.get("components").and_then(|c| c.as_array()).map(|v| v.as_slice()).unwrap_or(&[])
}

/// A component's `properties[]` entry by name. waybill emits every
/// non-native datum this way — see `docs/reference/sbom-format-mapping.md`.
pub fn cdx_prop<'a>(component: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    component
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value")?.as_str())
}

/// A document-scope property from `metadata.properties[]`.
pub fn cdx_doc_prop<'a>(cdx: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    cdx.get("metadata")?
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value")?.as_str())
}

/// The subject's `bom-ref`. This is the identifier the `dependencies[]`
/// graph is keyed by, and it is **not** the subject's PURL — the two
/// differ for every corpus target (`image-postgres16@4c9405bdf36a` vs
/// `pkg:generic/image-postgres16@4c9405bdf36a`).
pub fn cdx_subject_ref(cdx: &serde_json::Value) -> Option<&str> {
    cdx.get("metadata")?.get("component")?.get("bom-ref")?.as_str()
}

pub fn cdx_subject_purl(cdx: &serde_json::Value) -> Option<&str> {
    cdx.get("metadata")?.get("component")?.get("purl")?.as_str()
}

/// A component's graph identity: `bom-ref` if present, else `purl`.
pub fn cdx_component_ref(component: &serde_json::Value) -> Option<&str> {
    component
        .get("bom-ref")
        .and_then(|r| r.as_str())
        .or_else(|| component.get("purl").and_then(|p| p.as_str()))
}

/// The `dependencies[]` adjacency, as `ref -> dependsOn[]`.
pub fn cdx_adjacency(cdx: &serde_json::Value) -> HashMap<&str, Vec<&str>> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    let Some(deps) = cdx.get("dependencies").and_then(|d| d.as_array()) else {
        return out;
    };
    for d in deps {
        let Some(from) = d.get("ref").and_then(|r| r.as_str()) else { continue };
        let to: Vec<&str> = d
            .get("dependsOn")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        out.entry(from).or_default().extend(to);
    }
    out
}

/// Refs reachable from the subject by any `dependsOn` edge.
pub fn cdx_reachable(cdx: &serde_json::Value) -> HashSet<&str> {
    let mut seen: HashSet<&str> = HashSet::new();
    let Some(root) = cdx_subject_ref(cdx) else { return seen };
    let adj = cdx_adjacency(cdx);
    let mut q: VecDeque<&str> = VecDeque::new();
    q.push_back(root);
    seen.insert(root);
    while let Some(cur) = q.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
            if seen.insert(next) {
                q.push_back(next);
            }
        }
    }
    seen
}

/// The lifecycle scopes waybill uses to mark a non-runtime dependency.
/// Measured across the 17 committed goldens: `development`, `optional`,
/// `test`, `build` and nothing else.
pub const NON_RUNTIME_SCOPES: &[&str] = &["development", "optional", "test", "build"];

pub fn cdx_is_non_runtime(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:lifecycle-scope")
        .map(|s| NON_RUNTIME_SCOPES.contains(&s))
        .unwrap_or(false)
}

pub fn cdx_is_file_tier(component: &serde_json::Value) -> bool {
    cdx_prop(component, "waybill:component-tier") == Some("file")
        || component.get("type").and_then(|t| t.as_str()) == Some("file")
}

// -----------------------------------------------------------------------
// SPDX 2.3 walkers
// -----------------------------------------------------------------------

/// SPDX 2.3 relationship types that express a dependency with the
/// **subject on the right**: the dependency is `spdxElementId` and the
/// thing depending on it is `relatedSpdxElement`.
///
/// Reading only `DEPENDS_ON` fabricates a mismatch on every target
/// carrying any scoped dependency — `maven-guice` alone has 35 of these
/// against 28 `DEPENDS_ON`, and `jellyfin-web` carries 2655
/// `DEV_DEPENDENCY_OF`. This is the trap T019 of milestone 1155 exists
/// to name.
pub const SPDX_INVERSE_DEPENDENCY_TYPES: &[&str] = &[
    "DEV_DEPENDENCY_OF",
    "OPTIONAL_DEPENDENCY_OF",
    "TEST_DEPENDENCY_OF",
    "BUILD_DEPENDENCY_OF",
    "PROVIDED_DEPENDENCY_OF",
    "RUNTIME_DEPENDENCY_OF",
    "DEPENDENCY_OF",
];

pub fn spdx_packages(spdx: &serde_json::Value) -> &[serde_json::Value] {
    spdx.get("packages").and_then(|p| p.as_array()).map(|v| v.as_slice()).unwrap_or(&[])
}

/// `SPDXID -> PURL`, read from each package's `externalRefs[]`
/// PACKAGE-MANAGER / purl entry.
pub fn spdx_id_to_purl(spdx: &serde_json::Value) -> BTreeMap<&str, &str> {
    let mut out = BTreeMap::new();
    for p in spdx_packages(spdx) {
        let Some(id) = p.get("SPDXID").and_then(|i| i.as_str()) else { continue };
        let purl = p
            .get("externalRefs")
            .and_then(|r| r.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|r| r.get("referenceType").and_then(|t| t.as_str()) == Some("purl"))
                    .and_then(|r| r.get("referenceLocator")?.as_str())
            });
        if let Some(purl) = purl {
            out.insert(id, purl);
        }
    }
    out
}

/// The subject's SPDXID, via `documentDescribes` and falling back to the
/// `DESCRIBES` relationship.
pub fn spdx_subject_id(spdx: &serde_json::Value) -> Option<&str> {
    if let Some(first) = spdx
        .get("documentDescribes")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    {
        return Some(first);
    }
    spdx.get("relationships")?.as_array()?.iter().find_map(|r| {
        if r.get("relationshipType").and_then(|t| t.as_str()) == Some("DESCRIBES") {
            r.get("relatedSpdxElement")?.as_str()
        } else {
            None
        }
    })
}

/// Dependency edges normalised to `(depender, dependency)` SPDXID
/// pairs, with the inverse relationship types flipped into the same
/// direction as `DEPENDS_ON`.
pub fn spdx_dependency_edges(spdx: &serde_json::Value) -> Vec<(&str, &str)> {
    let Some(rels) = spdx.get("relationships").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in rels {
        let Some(ty) = r.get("relationshipType").and_then(|t| t.as_str()) else { continue };
        let Some(from) = r.get("spdxElementId").and_then(|t| t.as_str()) else { continue };
        let Some(to) = r.get("relatedSpdxElement").and_then(|t| t.as_str()) else { continue };
        if ty == "DEPENDS_ON" {
            out.push((from, to));
        } else if SPDX_INVERSE_DEPENDENCY_TYPES.contains(&ty) {
            // Inverse: `from` is the dependency, `to` is the depender.
            out.push((to, from));
        }
    }
    out
}

/// The subject's direct dependency set, as PURLs.
pub fn spdx_subject_direct_purls(spdx: &serde_json::Value) -> Option<BTreeSet<String>> {
    let subject = spdx_subject_id(spdx)?;
    let map = spdx_id_to_purl(spdx);
    Some(
        spdx_dependency_edges(spdx)
            .into_iter()
            .filter(|(from, _)| *from == subject)
            .filter_map(|(_, to)| map.get(to).map(|p| (*p).to_string()))
            .collect(),
    )
}
