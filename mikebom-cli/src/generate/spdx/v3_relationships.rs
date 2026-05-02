//! SPDX 3.0.1 `Relationship` element builder (milestone 011).
//!
//! Per `data-model.md` Element Catalog §`Relationship`: emits one
//! `Relationship` element per typed edge — `dependsOn`,
//! `devDependencyOf`, `buildDependencyOf`, `contains`,
//! `hasDeclaredLicense`, `hasConcludedLicense`, `suppliedBy`,
//! `originatedBy`, `describes`. Direction-reversal applies for
//! `devDependencyOf` and `buildDependencyOf` (target/source swap),
//! mirroring the SPDX 2.3 emitter's convention.
//!
//! Each Relationship's IRI is `<doc IRI>/rel-<base32(SHA256(
//! "<from>|<type>|<to>"))[..16]>`; output is sorted by `spdxId`
//! for determinism.

use std::collections::BTreeMap;

use data_encoding::BASE32_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use mikebom_common::resolution::{Relationship, ResolvedComponent};

/// Build a single `Relationship` element value-object.
///
/// IRI is content-derived from `(from, rel_type, to)` so two runs
/// of the same scan produce identical Relationship IRIs.
pub fn build_relationship(
    from_iri: &str,
    rel_type: &str,
    to_iri: &str,
    doc_iri: &str,
    creation_info_id: &str,
) -> Value {
    let rel_iri = format!(
        "{doc_iri}/rel-{}",
        hash_prefix(format!("{from_iri}|{rel_type}|{to_iri}").as_bytes(), 16)
    );
    json!({
        "type": "Relationship",
        "spdxId": rel_iri,
        "creationInfo": creation_info_id,
        "from": from_iri,
        "to": [to_iri],
        "relationshipType": rel_type,
    })
}

/// Build dependency-edge `Relationship` elements.
///
/// SPDX 3.0.1's `relationshipType` enum does NOT carry over
/// SPDX 2.3's `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF`
/// distinction — all four mikebom relationship kinds
/// (`DependsOn`, `DevDependsOn`, `BuildDependsOn`,
/// `TestDependsOn`) emit as `dependsOn` in SPDX 3.0.1. The
/// dev/build/test subtype signal is preserved via the
/// **`scope`** field on each `Relationship` element — SPDX
/// 3.0.1's native `LifecycleScopeType` enum (`development`,
/// `build`, `test`, `runtime`, `design`). Milestone 052/part-2
/// emits `scope` for `Dev`/`Build`/`TestDependsOn` variants and
/// omits it for plain `DependsOn` (default = scope-unspecified
/// per the spec).
pub fn build_dependency_relationships(
    relationships: &[Relationship],
    package_iri_by_purl: &BTreeMap<String, String>,
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value> {
    use mikebom_common::resolution::RelationshipType;
    let mut out: Vec<Value> = Vec::new();
    for rel in relationships {
        let Some(from_iri) = package_iri_by_purl.get(&rel.from) else {
            continue;
        };
        let Some(to_iri) = package_iri_by_purl.get(&rel.to) else {
            continue;
        };
        let mut element = build_relationship(
            from_iri,
            "dependsOn",
            to_iri,
            doc_iri,
            creation_info_id,
        );
        // Milestone 052/part-2: native LifecycleScopeType field.
        if let Some(scope) = match rel.relationship_type {
            RelationshipType::DevDependsOn => Some("development"),
            RelationshipType::BuildDependsOn => Some("build"),
            RelationshipType::TestDependsOn => Some("test"),
            RelationshipType::DependsOn => None,
        } {
            element["scope"] = json!(scope);
        }
        out.push(element);
    }
    sort_by_spdx_id(&mut out);
    out
}

/// Build containment-edge `Relationship` elements (`contains`)
/// from CDX-style nested component data. SPDX 3 (like SPDX 2.3)
/// has no native nesting; containment is expressed by edges
/// between flat Package elements.
///
/// Source data: `ResolvedComponent.parent_purl` — when set, the
/// component is contained by another component identified by that
/// PURL. Emits one `contains` Relationship per (parent → child).
pub fn build_containment_relationships(
    components: &[ResolvedComponent],
    package_iri_by_purl: &BTreeMap<String, String>,
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for c in components {
        let Some(parent_purl) = c.parent_purl.as_ref() else {
            continue;
        };
        let Some(parent_iri) = package_iri_by_purl.get(parent_purl) else {
            continue;
        };
        let Some(child_iri) = package_iri_by_purl.get(c.purl.as_str()) else {
            continue;
        };
        out.push(build_relationship(
            parent_iri,
            "contains",
            child_iri,
            doc_iri,
            creation_info_id,
        ));
    }
    sort_by_spdx_id(&mut out);
    out
}

/// Build the single `describes` Relationship from the SpdxDocument
/// to its root Package, mirroring SPDX 2.3's `documentDescribes`
/// shape.
pub fn build_describes_relationship(
    doc_iri: &str,
    root_package_iri: &str,
    creation_info_id: &str,
) -> Option<Value> {
    if doc_iri == root_package_iri {
        // Degenerate empty-scan case: document and root are the
        // same IRI. No describes edge — the document describes
        // itself implicitly via `rootElement`.
        return None;
    }
    Some(build_relationship(
        doc_iri,
        "describes",
        root_package_iri,
        doc_iri,
        creation_info_id,
    ))
}

/// Sort Relationship elements by their spdxId for determinism.
fn sort_by_spdx_id(relationships: &mut [Value]) {
    relationships.sort_by(|a, b| {
        let key = |v: &Value| v["spdxId"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
}

fn hash_prefix(input: &[u8], chars: usize) -> String {
    let digest = Sha256::digest(input);
    let encoded = BASE32_NOPAD.encode(&digest);
    encoded[..chars].to_string()
}
