//! Spec 1653 (#1688) — GHC boot libraries carry their dependency edges, or
//! the document says they don't.
//!
//! Under `--nix-closure` every boot library (a component carrying C218
//! `waybill:boot-library-compiler`) was a leaf: `base` had 0 edges on all five
//! closure goldens, and shellcheck read `complete`. This pass draws the edges
//! the compiler release itself records, from the committed table in
//! `haskell_packages::boot_dependencies`, between boot libraries **of the
//! same compiler** that the document already contains. It never synthesises
//! a component (FR-002).
//!
//! Every boot library gets C247 `waybill:boot-library-dependencies`, saying
//! where its dependencies came from and which of them are not components of
//! the document (`rts` and `ghc-internal` never are; research R3). The
//! completeness pass reads C247 to count boot libraries whose edges are
//! incomplete or unknown (FR-005).
//!
//! Keyed by (compiler, name): measured on all five goldens, no compiler has
//! two boot libraries of one name and no PURL is shared by two compilers
//! (research R4). Runs after `closure_resolved_tier::retier`, so after scope
//! propagation and role scope: scopes cannot move (FR-007).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent};

use crate::scan_fs::package_db::nix::haskell_packages::{boot_dependencies, ANN_BOOT_LIBRARY_COMPILER};

/// C247 — per boot library: where its dependencies came from.
pub(crate) const ANN_BOOT_LIBRARY_DEPENDENCIES: &str = "waybill:boot-library-dependencies";

/// The provenance recorded on each edge this pass adds.
const EDGE_SOURCE: &str = "ghc-boot-dependencies";

/// What [`apply`] did, for the INFO line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootEdgeSummary {
    /// Edges added.
    pub(crate) edges: usize,
    /// Boot libraries whose every dependency is a component of the document.
    pub(crate) complete: usize,
    /// Boot libraries with at least one dependency that is not a component.
    pub(crate) incomplete: usize,
    /// Boot libraries the table does not cover.
    pub(crate) unknown: usize,
}

/// `ghc-9.10.3` -> `9.10.3`; `ghc-binary-9.8.4` -> `9.8.4`.
fn release_of(compiler: &str) -> &str {
    compiler
        .strip_prefix("ghc-binary-")
        .or_else(|| compiler.strip_prefix("ghc-"))
        .unwrap_or(compiler)
}

/// Draw boot-library edges and stamp C247 (FR-002 to FR-004).
pub(crate) fn apply(components: &mut [ResolvedComponent], relationships: &mut Vec<Relationship>) -> BootEdgeSummary {
    apply_with(boot_dependencies::table(), components, relationships)
}

fn apply_with(
    table: &boot_dependencies::BootDependencyTable,
    components: &mut [ResolvedComponent],
    relationships: &mut Vec<Relationship>,
) -> BootEdgeSummary {
    let mut summary = BootEdgeSummary::default();

    // (compiler, library name) -> PURL, for the boot libraries present.
    let mut by_compiler: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for c in components.iter() {
        if let Some(compiler) = c.extra_annotations.get(ANN_BOOT_LIBRARY_COMPILER).and_then(|v| v.as_str()) {
            by_compiler
                .entry(compiler.to_string())
                .or_default()
                .insert(c.purl.name().to_string(), c.purl.as_str().to_string());
        }
    }
    if by_compiler.is_empty() {
        return summary;
    }

    let mut existing: HashSet<(String, String)> = relationships
        .iter()
        .filter(|r| r.relationship_type == RelationshipType::DependsOn)
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();
    let source = format!("{EDGE_SOURCE}/{}", table.observed());

    for c in components.iter_mut() {
        let Some(compiler) = c
            .extra_annotations
            .get(ANN_BOOT_LIBRARY_COMPILER)
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let release = release_of(&compiler);
        let name = c.purl.name().to_string();
        let value = if !table.has_release(release) {
            summary.unknown += 1;
            serde_json::json!({ "reason": "release-not-in-table" })
        } else if let Some(deps) = table.depends(release, &name) {
            let present = &by_compiler[&compiler];
            let from = c.purl.as_str().to_string();
            let mut absent: BTreeSet<&str> = BTreeSet::new();
            for dep in deps {
                match present.get(dep) {
                    Some(to) => {
                        if existing.insert((from.clone(), to.clone())) {
                            relationships.push(Relationship {
                                from: from.clone(),
                                to: to.clone(),
                                relationship_type: RelationshipType::DependsOn,
                                provenance: EnrichmentProvenance {
                                    source: EDGE_SOURCE.to_string(),
                                    data_type: "dependency".to_string(),
                                },
                            });
                            summary.edges += 1;
                        }
                    }
                    None => {
                        absent.insert(dep.as_str());
                    }
                }
            }
            if absent.is_empty() {
                summary.complete += 1;
                serde_json::json!({ "source": source })
            } else {
                summary.incomplete += 1;
                serde_json::json!({ "absent": absent, "source": source })
            }
        } else {
            summary.unknown += 1;
            serde_json::json!({ "reason": "library-not-in-table" })
        };
        c.extra_annotations
            .insert(ANN_BOOT_LIBRARY_DEPENDENCIES.to_string(), serde_json::Value::String(value.to_string()));
    }
    summary
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::{mk_component, mk_rel};

    fn boot(purl: &str, compiler: &str) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.extra_annotations
            .insert(ANN_BOOT_LIBRARY_COMPILER.to_string(), serde_json::json!(compiler));
        c
    }

    fn c245(c: &ResolvedComponent) -> serde_json::Value {
        let s = c.extra_annotations[ANN_BOOT_LIBRARY_DEPENDENCIES].as_str().unwrap();
        serde_json::from_str(s).unwrap()
    }

    fn edges(r: &[Relationship]) -> BTreeSet<(String, String)> {
        r.iter().map(|r| (r.from.clone(), r.to.clone())).collect()
    }

    const BASE_910: &str = "pkg:hackage/base@4.20.2.0";
    const PRIM_910: &str = "pkg:hackage/ghc-prim@0.12.0";
    const BASE_98: &str = "pkg:hackage/base@4.19.2.0";
    const PRIM_98: &str = "pkg:hackage/ghc-prim@0.11.0";

    /// Edges stay within one compiler, though both compilers have `base` and
    /// `ghc-prim`.
    #[test]
    fn edges_stay_within_a_compiler() {
        let mut c = vec![
            boot(BASE_910, "ghc-9.10.3"),
            boot(PRIM_910, "ghc-9.10.3"),
            boot(BASE_98, "ghc-binary-9.8.4"),
            boot(PRIM_98, "ghc-binary-9.8.4"),
        ];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r);
        let want: BTreeSet<(String, String)> = [(BASE_910, PRIM_910), (BASE_98, PRIM_98)]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(edges(&r), want);
        assert_eq!(s.edges, 2);
        assert!(r.iter().all(|r| r.provenance.source == EDGE_SOURCE));
    }

    #[test]
    fn an_existing_edge_is_not_duplicated() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3"), boot(PRIM_910, "ghc-9.10.3")];
        let mut r = vec![mk_rel(BASE_910, PRIM_910)];
        let s = apply(&mut c, &mut r);
        assert_eq!(r.len(), 1);
        assert_eq!(s.edges, 0);
    }

    /// 9.10.3's `base` depends on `ghc-internal` and `ghc-prim`, and 9.8.4's
    /// on `ghc-bignum`, `ghc-prim` and `rts`. What is not a component is
    /// listed, sorted, and no component is made for it.
    #[test]
    fn a_dependency_absent_from_the_document_is_listed_not_synthesised() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3"), boot(PRIM_910, "ghc-9.10.3"), boot(BASE_98, "ghc-binary-9.8.4")];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r);
        assert_eq!(c.len(), 3, "no component is synthesised");
        assert_eq!(c245(&c[0])["absent"], serde_json::json!(["ghc-internal"]));
        assert_eq!(c245(&c[1])["absent"], serde_json::json!(["rts"]));
        assert_eq!(c245(&c[2])["absent"], serde_json::json!(["ghc-bignum", "ghc-prim", "rts"]));
        assert_eq!(s, BootEdgeSummary { edges: 1, complete: 0, incomplete: 3, unknown: 0 });
    }

    #[test]
    fn the_source_names_the_table_and_its_date() {
        let mut c = vec![boot("pkg:hackage/rts@1.0.2", "ghc-9.10.3")];
        apply(&mut c, &mut Vec::new());
        let v = c245(&c[0]);
        assert_eq!(v["source"], serde_json::json!(format!("ghc-boot-dependencies/{}", boot_dependencies::table().observed())));
        assert!(v.get("absent").is_none(), "rts is a known leaf: {v}");
    }

    #[test]
    fn a_release_or_library_missing_from_the_table_is_unknown() {
        let mut c = vec![
            boot("pkg:hackage/base@4.14.3.0", "ghc-8.10.7"),
            boot("pkg:hackage/waybill-fixture-absent@1.0", "ghc-9.10.3"),
        ];
        let s = apply(&mut c, &mut Vec::new());
        assert_eq!(c245(&c[0]), serde_json::json!({"reason": "release-not-in-table"}));
        assert_eq!(c245(&c[1]), serde_json::json!({"reason": "library-not-in-table"}));
        assert_eq!(s.unknown, 2);
    }

    #[test]
    fn components_without_c218_are_untouched() {
        let mut c = vec![mk_component("pkg:hackage/waybill-fixture-app@1.0"), mk_component(PRIM_910)];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r);
        assert_eq!(s, BootEdgeSummary::default());
        assert!(r.is_empty());
        assert!(c.iter().all(|c| !c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_DEPENDENCIES)));
    }

    #[test]
    fn c245_is_canonical_json_with_sorted_keys() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3")];
        apply(&mut c, &mut Vec::new());
        let raw = c[0].extra_annotations[ANN_BOOT_LIBRARY_DEPENDENCIES].as_str().unwrap().to_string();
        assert!(raw.starts_with("{\"absent\":["), "{raw}");
        assert!(raw.find("\"absent\"").unwrap() < raw.find("\"source\"").unwrap(), "{raw}");
    }
}
