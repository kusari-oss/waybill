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
        // Milestone 085: corrected to use the SPDX 3.0.1
        // `LifecycleScopedRelationship` element type (a subtype of
        // `Relationship` per the SPDX 3 schema). The `scope` field
        // is only valid on `LifecycleScopedRelationship`; pre-085
        // the code emitted `scope` on a plain `Relationship` which
        // failed JSON-Schema validation (the
        // `LifecycleScopedRelationship_props` allOf branch wasn't
        // selected, so `scope` was an unknown property). Pre-085
        // this code path was untested by the SPDX 3 conformance
        // gate (milestone 078) because cargo/gem/etc. fixtures only
        // emit plain `DependsOn` — never the typed
        // `Dev/Build/TestDependsOn` variants that would hit this
        // branch. Maven (milestone 070 + 085) is the first
        // ecosystem whose fixture has a TestDependsOn edge AND a
        // SPDX 3 conformance check; surfaced the type-mismatch.
        // Milestone 179 FR-017: `OptionalDependsOn` intentionally maps
        // to `None` — SPDX 3.0.1's `LifecycleScopeType` enum has no
        // `optional` value at spec version 3.0.1. The classification
        // information rides on the `mikebom:optional-derivation`
        // component-level annotation instead (Principle V KEEP-BOTH
        // carve-out for the SPDX 3 side). If a future SPDX 3 minor
        // (3.1, 3.2, ...) adds an `optional` value, a follow-up
        // milestone can add it here.
        if let Some(scope) = match rel.relationship_type {
            RelationshipType::DevDependsOn => Some("development"),
            RelationshipType::BuildDependsOn => Some("build"),
            RelationshipType::TestDependsOn => Some("test"),
            RelationshipType::DependsOn | RelationshipType::OptionalDependsOn => None,
        } {
            element["type"] = json!("LifecycleScopedRelationship");
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

/// Build the `describes` Relationship(s) from the SpdxDocument to its
/// root Package(s), mirroring SPDX 2.3's `documentDescribes` shape.
/// Multi-root case (cargo workspace, polyglot scans with multiple
/// per-ecosystem main-modules) emits one `describes` Relationship per
/// root — SPDX 3.0.1's `to` field is a plural array on the
/// Relationship, but emitting one Relationship per `(from, to)` pair
/// keeps the spdxId determinism + sort-by-spdxId convention simple.
pub fn build_describes_relationships(
    doc_iri: &str,
    root_package_iris: &[String],
    creation_info_id: &str,
) -> Vec<Value> {
    root_package_iris
        .iter()
        .filter(|iri| iri.as_str() != doc_iri)
        .map(|iri| {
            build_relationship(
                doc_iri,
                "describes",
                iri.as_str(),
                doc_iri,
                creation_info_id,
            )
        })
        .collect()
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use mikebom_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};

    fn mk_rel(from: &str, to: &str, rt: RelationshipType) -> Relationship {
        Relationship {
            from: from.to_string(),
            to: to.to_string(),
            relationship_type: rt,
            provenance: EnrichmentProvenance {
                source: "test".to_string(),
                data_type: "relationship".to_string(),
            },
        }
    }

    fn mk_iri_map() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert(
            "pkg:cargo/my-app@1".to_string(),
            "spdx:MyApp".to_string(),
        );
        m.insert("pkg:cargo/foo@1".to_string(), "spdx:Foo".to_string());
        m
    }

    #[test]
    fn optional_depends_on_emits_no_lifecycle_scope_on_spdx3() {
        // Milestone 179 T008 / FR-017 — SPDX 3.0.1's
        // `LifecycleScopeType` enum has no `optional` value; the
        // emission MUST NOT set `scope` on the relationship element
        // (otherwise the `LifecycleScopedRelationship` subtype gets
        // selected and the `optional` value fails JSON-Schema
        // validation via the m078 conformance gate).
        let rels = vec![mk_rel(
            "pkg:cargo/my-app@1",
            "pkg:cargo/foo@1",
            RelationshipType::OptionalDependsOn,
        )];
        let iri = mk_iri_map();
        let out = build_dependency_relationships(&rels, &iri, "spdx:Doc", "_:ci");
        assert_eq!(out.len(), 1);
        assert!(
            out[0].get("scope").is_none(),
            "SPDX 3 OptionalDependsOn MUST NOT emit a `scope` field: {}",
            out[0]
        );
        // Confirm the element stays a plain `Relationship`, not the
        // LifecycleScopedRelationship subtype.
        assert_eq!(
            out[0].get("type").and_then(|v| v.as_str()),
            Some("Relationship"),
            "OptionalDependsOn stays plain Relationship (no lifecycleScope): {}",
            out[0]
        );
    }
}
