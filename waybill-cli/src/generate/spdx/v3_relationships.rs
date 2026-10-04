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

use waybill_common::resolution::{Relationship, ResolvedComponent};

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

/// Milestone 1069 (#878) — group dependency relationships and qualify them
/// with SPDX 3's native `completeness`.
///
/// `completeness` says whether a relationship's targets are the whole set,
/// so it can only describe a component's dependencies when they sit in one
/// relationship. Every producer emits one relationship per edge; this pass
/// runs once over the final list, so a producer added later cannot bypass it.
///
/// - **Grouping:** `dependsOn` relationships group by `(from, type, scope)`;
///   targets are sorted and deduplicated.
/// - **Completeness:** from the same [`DependencyClaims`] CycloneDX
///   `compositions[]` uses:
///   - `incomplete` if `from` is in `unknown`, else `complete` if it is in
///     `complete`;
///   - otherwise the root takes `complete` when `root_complete`;
///   - anything else carries no `completeness`, which claims nothing.
///
///   A main-module root that is also in an ecosystem set takes the
///   per-component claim; `unknown` wins, as the weaker claim.
/// - **Unknown leaves:** a component in `unknown` with no outgoing dependency
///   gets one `dependsOn → NoAssertionElement` marked `noAssertion`, SPDX's
///   own "cannot reach a determination".
/// - **IRI:** a set with one target and no scope keeps its pre-1069 IRI,
///   `from|dependsOn|to`; the set's targets join with `,`, and a scope is
///   appended as `|<scope>`.
///
/// [`DependencyClaims`]: crate::generate::cyclonedx::compositions::DependencyClaims
pub(crate) fn group_dependency_relationships(
    relationships: Vec<Value>,
    claims: &crate::generate::cyclonedx::compositions::DependencyClaims,
    root_iri: Option<&str>,
    package_iri_by_purl: &BTreeMap<String, String>,
    doc_iri: &str,
    creation_info_id: &str,
) -> Vec<Value> {
    let purl_by_iri: BTreeMap<&str, &str> = package_iri_by_purl
        .iter()
        .map(|(p, i)| (i.as_str(), p.as_str()))
        .collect();
    let completeness = |from: &str| -> Option<&'static str> {
        match purl_by_iri.get(from) {
            Some(p) if claims.unknown.contains(*p) => return Some("incomplete"),
            Some(p) if claims.complete.contains(*p) => return Some("complete"),
            _ => {}
        }
        (root_iri == Some(from) && claims.root_complete).then_some("complete")
    };

    let mut out: Vec<Value> = Vec::with_capacity(relationships.len());
    // (from, element type, scope) -> targets; BTreeMap for determinism.
    let mut sets: BTreeMap<(String, String, Option<String>), std::collections::BTreeSet<String>> =
        BTreeMap::new();
    for rel in relationships {
        let is_dependency = rel["relationshipType"] == "dependsOn"
            && matches!(
                rel["type"].as_str(),
                Some("Relationship" | "LifecycleScopedRelationship")
            );
        let (Some(from), Some(targets)) = (rel["from"].as_str(), rel["to"].as_array()) else {
            out.push(rel);
            continue;
        };
        if !is_dependency {
            out.push(rel);
            continue;
        }
        let key = (
            from.to_string(),
            rel["type"].as_str().unwrap_or("Relationship").to_string(),
            rel["scope"].as_str().map(str::to_string),
        );
        sets.entry(key)
            .or_default()
            .extend(targets.iter().filter_map(|t| t.as_str()).map(str::to_string));
    }

    let mut has_dependencies: std::collections::BTreeSet<String> = Default::default();
    for ((from, element_type, scope), targets) in sets {
        let joined = targets.iter().cloned().collect::<Vec<_>>().join(",");
        let key = match &scope {
            Some(sc) => format!("{from}|dependsOn|{joined}|{sc}"),
            None => format!("{from}|dependsOn|{joined}"),
        };
        let mut element = json!({
            "type": element_type,
            "spdxId": format!("{doc_iri}/rel-{}", hash_prefix(key.as_bytes(), 16)),
            "creationInfo": creation_info_id,
            "from": from,
            "to": targets.into_iter().collect::<Vec<_>>(),
            "relationshipType": "dependsOn",
        });
        if let Some(sc) = scope {
            element["scope"] = json!(sc);
        }
        if let Some(c) = completeness(&from) {
            element["completeness"] = json!(c);
        }
        has_dependencies.insert(from);
        out.push(element);
    }

    let mut unknown: Vec<&String> = claims.unknown.iter().collect();
    unknown.sort();
    for purl in unknown {
        let Some(iri) = package_iri_by_purl.get(purl) else {
            continue;
        };
        if has_dependencies.contains(iri) {
            continue;
        }
        let mut element = build_relationship(
            iri,
            "dependsOn",
            "NoAssertionElement",
            doc_iri,
            creation_info_id,
        );
        element["completeness"] = json!("noAssertion");
        out.push(element);
    }
    out
}

/// Build dependency-edge `Relationship` elements.
///
/// SPDX 3.0.1's `relationshipType` enum does NOT carry over
/// SPDX 2.3's `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF`
/// distinction — all four waybill relationship kinds
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
    use waybill_common::resolution::RelationshipType;
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
        // information rides on the `waybill:optional-derivation`
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

/// Apply SPDX 3.0.1's native `LifecycleScopeType` to a relationship
/// element, given a target component's `lifecycle_scope` (#1000).
///
/// SPDX 3 has no `DEV_DEPENDENCY_OF` verb — every dependency edge is
/// `dependsOn`, and the dev/build/test signal rides the `scope` field
/// on a `LifecycleScopedRelationship`. This mirrors the mapping in
/// [`build_dependency_relationships`] so the issue-#236 root fallback
/// in `v3_document.rs`, which synthesizes edges after the typed
/// rewrite has already run, classifies them the same way.
///
/// `Optional` maps to `None` per m179 FR-017: SPDX 3.0.1's enum has no
/// `optional` value, and the signal rides the
/// `waybill:optional-derivation` annotation instead.
pub fn apply_lifecycle_scope(
    element: &mut Value,
    scope: Option<waybill_common::resolution::LifecycleScope>,
) {
    use waybill_common::resolution::LifecycleScope;
    let Some(name) = (match scope {
        Some(LifecycleScope::Development) => Some("development"),
        Some(LifecycleScope::Build) => Some("build"),
        Some(LifecycleScope::Test) => Some("test"),
        Some(LifecycleScope::Runtime) | Some(LifecycleScope::Optional) | None => None,
    }) else {
        return;
    };
    element["type"] = json!("LifecycleScopedRelationship");
    element["scope"] = json!(name);
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
    use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};

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

// Milestone 1069 (#878) — dependency relationships grouped per component and
// kind, qualified with SPDX 3's native `completeness`.
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m1069_tests {
    use std::collections::{BTreeMap, HashSet};

    use super::*;
    use crate::generate::cyclonedx::compositions::DependencyClaims;

    const DOC: &str = "https://example.test/doc";
    fn dep(from: &str, to: &str) -> Value {
        build_relationship(from, "dependsOn", to, DOC, "_:ci")
    }
    fn scoped(from: &str, to: &str, scope: &str) -> Value {
        let mut e = dep(from, to);
        e["type"] = json!("LifecycleScopedRelationship");
        e["scope"] = json!(scope);
        e
    }
    fn iris() -> BTreeMap<String, String> {
        [
            ("pkg:cargo/app@1", "spdx:App"),
            ("pkg:cargo/a@1", "spdx:A"),
            ("pkg:cargo/b@1", "spdx:B"),
            ("pkg:npm/x@1", "spdx:X"),
            ("pkg:npm/leaf@1", "spdx:Leaf"),
            ("pkg:pypi/p@1", "spdx:P"),
        ]
        .into_iter()
        .map(|(p, i)| (p.to_string(), i.to_string()))
        .collect()
    }
    fn claims() -> DependencyClaims {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<HashSet<_>>();
        DependencyClaims {
            complete: s(&["pkg:cargo/app@1", "pkg:cargo/a@1", "pkg:cargo/b@1"]),
            unknown: s(&["pkg:npm/x@1", "pkg:npm/leaf@1"]),
            root_complete: true,
        }
    }
    fn deps_from<'a>(out: &'a [Value], from: &str) -> Vec<&'a Value> {
        out.iter()
            .filter(|e| e["from"] == from && e["relationshipType"] == "dependsOn")
            .collect()
    }
    fn group(rels: Vec<Value>, c: &DependencyClaims, root: Option<&str>) -> Vec<Value> {
        group_dependency_relationships(rels, c, root, &iris(), DOC, "_:ci")
    }

    #[test]
    fn edges_from_one_component_become_one_relationship() {
        let out = group(
            vec![dep("spdx:A", "spdx:B"), dep("spdx:A", "spdx:X"), dep("spdx:A", "spdx:B")],
            &claims(),
            None,
        );
        let a = deps_from(&out, "spdx:A");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["to"], json!(["spdx:B", "spdx:X"]));
        assert_eq!(a[0]["completeness"], "complete");
    }

    #[test]
    fn each_lifecycle_scope_is_its_own_set() {
        let out = group(
            vec![dep("spdx:A", "spdx:B"), scoped("spdx:A", "spdx:X", "development")],
            &claims(),
            None,
        );
        let a = deps_from(&out, "spdx:A");
        assert_eq!(a.len(), 2);
        let dev = a.iter().find(|e| e["scope"] == "development").unwrap();
        assert_eq!(dev["type"], "LifecycleScopedRelationship");
        assert_eq!(dev["to"], json!(["spdx:X"]));
        assert_eq!(dev["completeness"], "complete");
    }

    #[test]
    fn other_relationships_are_untouched() {
        let contains = build_relationship("spdx:App", "contains", "spdx:A", DOC, "_:ci");
        let out = group(vec![contains.clone()], &claims(), None);
        assert!(out.contains(&contains));
    }

    /// A single-target, unscoped set keeps its pre-1069 IRI, so documents
    /// whose components each have one dependency do not churn.
    #[test]
    fn the_grouped_iri_is_deterministic_and_stable_for_single_targets() {
        let single = dep("spdx:A", "spdx:B");
        let out = group(vec![single.clone()], &claims(), None);
        assert_eq!(deps_from(&out, "spdx:A")[0]["spdxId"], single["spdxId"]);
        let two = |order: [&str; 2]| {
            let out = group(order.iter().map(|t| dep("spdx:A", t)).collect(), &claims(), None);
            deps_from(&out, "spdx:A")[0]["spdxId"].clone()
        };
        assert_eq!(two(["spdx:B", "spdx:X"]), two(["spdx:X", "spdx:B"]));
        assert_ne!(two(["spdx:B", "spdx:X"]), single["spdxId"]);
    }

    #[test]
    fn completeness_follows_the_contract_table() {
        let out = group(
            vec![
                dep("spdx:A", "spdx:B"),   // complete
                dep("spdx:X", "spdx:Leaf"), // unknown
                dep("spdx:P", "spdx:A"),   // unclaimed
            ],
            &claims(),
            None,
        );
        assert_eq!(deps_from(&out, "spdx:A")[0]["completeness"], "complete");
        assert_eq!(deps_from(&out, "spdx:X")[0]["completeness"], "incomplete");
        assert!(deps_from(&out, "spdx:P")[0].get("completeness").is_none());
        // An unknown leaf gets the native "cannot determine".
        let leaf = deps_from(&out, "spdx:Leaf");
        assert_eq!(leaf.len(), 1);
        assert_eq!(leaf[0]["to"], json!(["NoAssertionElement"]));
        assert_eq!(leaf[0]["completeness"], "noAssertion");
        // A complete leaf (B) and an unclaimed one get nothing added.
        assert!(deps_from(&out, "spdx:B").is_empty());
    }

    #[test]
    fn the_root_follows_its_own_record_unless_it_is_in_an_ecosystem_set() {
        let mut c = claims();
        c.complete.remove("pkg:cargo/app@1");
        let root = || vec![dep("spdx:App", "spdx:A")];
        let out = group(root(), &c, Some("spdx:App"));
        assert_eq!(deps_from(&out, "spdx:App")[0]["completeness"], "complete");
        c.root_complete = false;
        let out = group(root(), &c, Some("spdx:App"));
        assert!(deps_from(&out, "spdx:App")[0].get("completeness").is_none());
        // A main-module root that is also in an unresolved ecosystem takes the
        // weaker, per-component claim.
        c.root_complete = true;
        c.unknown.insert("pkg:cargo/app@1".to_string());
        let out = group(root(), &c, Some("spdx:App"));
        assert_eq!(deps_from(&out, "spdx:App")[0]["completeness"], "incomplete");
    }
}
