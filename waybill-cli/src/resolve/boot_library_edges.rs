//! Spec 1653 (#1688) — GHC boot libraries carry their dependency edges, or
//! the document says they don't.
//!
//! Under `--nix-closure` every boot library (a component carrying C218
//! `waybill:boot-library-compiler`) was a leaf: `base` had 0 edges on all five
//! closure goldens, and shellcheck read `complete`. This pass draws the edges
//! the compiler release itself records, from the committed table in
//! `haskell_packages::boot_dependencies`, between boot libraries **of the
//! same compiler** that the document already contains. It never synthesises
//! a component (FR-002). Spec 1655 (#1696) lifts that: with the closure's
//! compilers it adds every boot library a present one reaches.
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

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent};

use crate::scan_fs::package_db::nix::haskell_packages::closure_compilers::{ClosureCompilers, GhcRelease};
use crate::scan_fs::package_db::nix::haskell_packages::{
    boot_dependencies, boot_table, closure_identity, ANN_BOOT_LIBRARY_COMPILER,
};

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
    /// Boot libraries added because a present one reaches them (spec 1655).
    pub(crate) added: usize,
}

/// `ghc-9.10.3` -> `9.10.3`; `ghc-binary-9.8.4` -> `9.8.4`.
fn release_of(compiler: &str) -> &str {
    compiler
        .strip_prefix("ghc-binary-")
        .or_else(|| compiler.strip_prefix("ghc-"))
        .unwrap_or(compiler)
}

/// Draw boot-library edges and stamp C247 (FR-002 to FR-004).
///
/// Spec 1655 (#1696): with the closure's compilers, first add every boot
/// library a present one reaches through the table and that no component
/// holds, then resolve each dependency by `(label, name)` and, failing that,
/// by the PURL of the version its release ships, whatever that component's
/// C218 label. One version is one component: the closure labels a shared
/// version with the project's compiler, so a bootstrap compiler's library
/// would otherwise list a dependency the document holds as `absent`
/// (research R1). Without compilers, behaviour is spec 1653's.
pub(crate) fn apply(
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<Relationship>,
    compilers: Option<&ClosureCompilers>,
) -> BootEdgeSummary {
    let versions = |release: &str, name: &str| -> Option<String> {
        let cc = compilers?;
        boot_table::table().version_of(release, name, &cc.system).map(str::to_string)
    };
    apply_with(boot_dependencies::table(), &versions, compilers, components, relationships)
}

/// `(label, name) -> PURL` for every component carrying C218.
fn index_by_label(components: &[ResolvedComponent]) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut by_compiler: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for c in components {
        if let Some(compiler) = c.extra_annotations.get(ANN_BOOT_LIBRARY_COMPILER).and_then(|v| v.as_str()) {
            by_compiler
                .entry(compiler.to_string())
                .or_default()
                .insert(c.purl.name().to_string(), c.purl.as_str().to_string());
        }
    }
    by_compiler
}

/// The PURL a dependency resolves to from a library of `label`: the
/// component its own compiler holds by name, else whichever component holds
/// the version that release ships.
fn resolve_dependency(
    by_compiler: &BTreeMap<String, BTreeMap<String, String>>,
    held: &HashSet<String>,
    versions: &dyn Fn(&str, &str) -> Option<String>,
    label: &str,
    dep: &str,
) -> Option<String> {
    if let Some(p) = by_compiler.get(label).and_then(|m| m.get(dep)) {
        return Some(p.clone());
    }
    let purl = format!("pkg:hackage/{dep}@{}", versions(release_of(label), dep)?);
    held.contains(&purl).then_some(purl)
}

/// Spec 1655 FR-001: add every boot library reachable from the present ones
/// that no component holds. Walks components, each at its own label's
/// release, because that is the release whose dependencies the edge step
/// draws for it: a library reached any other way would have no edge into it.
/// An added library is labelled with the project's compiler when that
/// compiler ships the same version (the closure's rule for a shared version,
/// `closure_identity::emit_declared_edges`), else with the first compiler to
/// reach it in a walk ordered by `(label, PURL)`.
fn close_reachable_set(
    table: &boot_dependencies::BootDependencyTable,
    versions: &dyn Fn(&str, &str) -> Option<String>,
    cc: &ClosureCompilers,
    components: &mut Vec<ResolvedComponent>,
) -> usize {
    let mut by_compiler = index_by_label(components);
    let mut held: HashSet<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();
    let project = cc.project_in_table();
    let mut queue: VecDeque<(String, String)> = by_compiler
        .iter()
        .flat_map(|(label, libs)| libs.keys().map(move |n| (label.clone(), n.clone())))
        .collect();
    let mut walked: HashSet<(String, String)> = queue.iter().cloned().collect();
    let mut added = 0;
    while let Some((label, name)) = queue.pop_front() {
        let release = release_of(&label).to_string();
        let Some(deps) = table.depends(&release, &name) else { continue };
        for dep in deps {
            if resolve_dependency(&by_compiler, &held, versions, &label, dep).is_some() {
                // Present: it is walked from its own entry in the queue.
                continue;
            }
            let Some(version) = versions(&release, dep) else {
                // Fail closed: no version, no component. C247 lists it.
                continue;
            };
            let dep_label = match project {
                Some(p) if versions(&p.version, dep).as_deref() == Some(version.as_str()) => p.label.clone(),
                _ => label.clone(),
            };
            let shipper = GhcRelease {
                version: release_of(&dep_label).to_string(),
                bootstrap: dep_label.starts_with("ghc-binary-"),
                label: dep_label.clone(),
            };
            let Some(c) = closure_identity::compiler_supplied_component(dep, Some((&shipper, &version))) else {
                continue;
            };
            let purl = c.purl.as_str().to_string();
            if !held.insert(purl.clone()) {
                continue;
            }
            by_compiler.entry(dep_label.clone()).or_default().insert(dep.clone(), purl);
            components.push(c);
            added += 1;
            if walked.insert((dep_label.clone(), dep.clone())) {
                queue.push_back((dep_label, dep.clone()));
            }
        }
    }
    added
}

fn apply_with(
    table: &boot_dependencies::BootDependencyTable,
    versions: &dyn Fn(&str, &str) -> Option<String>,
    compilers: Option<&ClosureCompilers>,
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<Relationship>,
) -> BootEdgeSummary {
    let mut summary = BootEdgeSummary::default();
    if !components.iter().any(|c| c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_COMPILER)) {
        return summary;
    }
    if let Some(cc) = compilers {
        summary.added = close_reachable_set(table, versions, cc, components);
    }

    let by_compiler = index_by_label(components);
    let held: HashSet<String> = components.iter().map(|c| c.purl.as_str().to_string()).collect();
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
            let from = c.purl.as_str().to_string();
            let mut absent: BTreeSet<&str> = BTreeSet::new();
            for dep in deps {
                match resolve_dependency(&by_compiler, &held, versions, &compiler, dep) {
                    Some(to) => {
                        if existing.insert((from.clone(), to.clone())) {
                            relationships.push(Relationship {
                                from: from.clone(),
                                to,
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
        let s = apply(&mut c, &mut r, None);
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
        let s = apply(&mut c, &mut r, None);
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
        let s = apply(&mut c, &mut r, None);
        assert_eq!(c.len(), 3, "no component is synthesised");
        assert_eq!(c245(&c[0])["absent"], serde_json::json!(["ghc-internal"]));
        assert_eq!(c245(&c[1])["absent"], serde_json::json!(["rts"]));
        assert_eq!(c245(&c[2])["absent"], serde_json::json!(["ghc-bignum", "ghc-prim", "rts"]));
        assert_eq!(s, BootEdgeSummary { edges: 1, complete: 0, incomplete: 3, unknown: 0, added: 0 });
    }

    #[test]
    fn the_source_names_the_table_and_its_date() {
        let mut c = vec![boot("pkg:hackage/rts@1.0.2", "ghc-9.10.3")];
        apply(&mut c, &mut Vec::new(), None);
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
        let s = apply(&mut c, &mut Vec::new(), None);
        assert_eq!(c245(&c[0]), serde_json::json!({"reason": "release-not-in-table"}));
        assert_eq!(c245(&c[1]), serde_json::json!({"reason": "library-not-in-table"}));
        assert_eq!(s.unknown, 2);
    }

    #[test]
    fn components_without_c218_are_untouched() {
        let mut c = vec![mk_component("pkg:hackage/waybill-fixture-app@1.0"), mk_component(PRIM_910)];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r, None);
        assert_eq!(s, BootEdgeSummary::default());
        assert!(r.is_empty());
        assert!(c.iter().all(|c| !c.extra_annotations.contains_key(ANN_BOOT_LIBRARY_DEPENDENCIES)));
    }

    // ------------------------------------------------------------------
    // Spec 1655 (#1696): the reachable set, and lookup by PURL
    // ------------------------------------------------------------------

    fn release(label: &str) -> GhcRelease {
        GhcRelease {
            version: release_of(label).to_string(),
            bootstrap: label.starts_with("ghc-binary-"),
            label: label.to_string(),
        }
    }

    fn compilers(project: Option<&str>) -> ClosureCompilers {
        ClosureCompilers { system: "x86_64-linux".to_string(), project: project.map(release), ..Default::default() }
    }

    fn by_purl<'a>(c: &'a [ResolvedComponent], purl: &str) -> Vec<&'a ResolvedComponent> {
        c.iter().filter(|c| c.purl.as_str() == purl).collect()
    }

    fn label(c: &ResolvedComponent) -> &str {
        c.extra_annotations[ANN_BOOT_LIBRARY_COMPILER].as_str().unwrap()
    }

    const CONTAINERS_98: &str = "pkg:hackage/containers@0.6.8";
    const ARRAY: &str = "pkg:hackage/array@0.5.8.0";
    const RTS: &str = "pkg:hackage/rts@1.0.2";

    /// Research R1, the moat case: 9.8.4's `containers` depends on `array`,
    /// and 9.8.4 ships `array@0.5.8.0`, which the closure holds once,
    /// labelled with the project compiler.
    #[test]
    fn a_dependency_held_under_another_label_resolves_by_purl() {
        let mut c = vec![boot(CONTAINERS_98, "ghc-binary-9.8.4"), boot(ARRAY, "ghc-9.10.3")];
        let mut r = Vec::new();
        apply(&mut c, &mut r, Some(&compilers(Some("ghc-9.10.3"))));
        assert!(edges(&r).contains(&(CONTAINERS_98.to_string(), ARRAY.to_string())));
        let v = c245(by_purl(&c, CONTAINERS_98)[0]);
        assert!(v.get("absent").is_none(), "{v}");
        assert_eq!(by_purl(&c, ARRAY).len(), 1, "array is not added a second time");
    }

    /// FR-001: 9.10.3's `base` alone reaches `ghc-internal` and `ghc-prim`,
    /// and through them `ghc-bignum` and `rts`. Exactly those are added.
    #[test]
    fn the_reachable_set_is_closed() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3")];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r, Some(&compilers(Some("ghc-9.10.3"))));
        let purls: BTreeSet<&str> = c.iter().map(|c| c.purl.as_str()).collect();
        let want = BTreeSet::from([
            BASE_910,
            "pkg:hackage/ghc-internal@9.1003.0",
            PRIM_910,
            "pkg:hackage/ghc-bignum@1.3",
            RTS,
        ]);
        assert_eq!(purls, want);
        assert_eq!(s.added, 4);
        for x in &c {
            assert_eq!(label(x), "ghc-9.10.3", "{}", x.purl.as_str());
            let v = c245(x);
            assert!(v.get("absent").is_none(), "{}: {v}", x.purl.as_str());
        }
        assert!(edges(&r).contains(&(BASE_910.to_string(), "pkg:hackage/ghc-internal@9.1003.0".to_string())));
        assert!(edges(&r).contains(&(PRIM_910.to_string(), RTS.to_string())));
        assert_eq!(s.incomplete, 0);
    }

    /// One version is one component: both compilers' `base` reach
    /// `rts@1.0.2`, added once and labelled with the project compiler, while
    /// the two `ghc-prim` versions are two components, each its own.
    #[test]
    fn a_shared_version_is_one_component() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3"), boot(BASE_98, "ghc-binary-9.8.4")];
        let mut r = Vec::new();
        apply(&mut c, &mut r, Some(&compilers(Some("ghc-9.10.3"))));
        let rts = by_purl(&c, RTS);
        assert_eq!(rts.len(), 1);
        assert_eq!(label(rts[0]), "ghc-9.10.3");
        assert_eq!(label(by_purl(&c, PRIM_98)[0]), "ghc-binary-9.8.4");
        assert_eq!(label(by_purl(&c, PRIM_910)[0]), "ghc-9.10.3");
        assert!(edges(&r).contains(&(BASE_98.to_string(), PRIM_98.to_string())));
        assert!(!edges(&r).contains(&(BASE_98.to_string(), PRIM_910.to_string())), "9.8.4 does not ship ghc-prim 0.12.0");
        assert!(edges(&r).contains(&(BASE_98.to_string(), RTS.to_string())));

        // Reached only from the bootstrap compiler, a version the project
        // compiler also ships still takes the project's label, as the
        // closure labels a shared version whoever reaches it first.
        let mut c = vec![boot(BASE_98, "ghc-binary-9.8.4")];
        apply(&mut c, &mut Vec::new(), Some(&compilers(Some("ghc-9.10.3"))));
        assert_eq!(label(by_purl(&c, RTS)[0]), "ghc-9.10.3");
        assert_eq!(label(by_purl(&c, PRIM_98)[0]), "ghc-binary-9.8.4", "0.11.0 is not the project's version");
    }

    /// Not the whole set: `Cabal`, which nothing present depends on, is
    /// never added.
    #[test]
    fn an_unreached_library_is_not_added() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3")];
        apply(&mut c, &mut Vec::new(), Some(&compilers(Some("ghc-9.10.3"))));
        assert!(c.iter().all(|c| c.purl.name() != "Cabal"));
        assert!(c.iter().all(|c| c.purl.name() != "process"));
    }

    /// A component already holding the PURL, here without C218, is the
    /// target; nothing is duplicated.
    #[test]
    fn a_held_purl_is_never_duplicated() {
        let mut c = vec![boot(PRIM_910, "ghc-9.10.3"), mk_component(RTS)];
        let mut r = Vec::new();
        let s = apply(&mut c, &mut r, Some(&compilers(Some("ghc-9.10.3"))));
        assert_eq!(by_purl(&c, RTS).len(), 1);
        assert_eq!(s.added, 0);
        assert!(edges(&r).contains(&(PRIM_910.to_string(), RTS.to_string())));
    }

    /// The reason's positive control (research R6): when the version table
    /// has no version for a dependency, nothing is synthesised for it, and
    /// C247 says so.
    #[test]
    fn a_version_the_table_lacks_stays_absent() {
        let real = |r: &str, n: &str| boot_table::table().version_of(r, n, "x86_64-linux").map(str::to_string);
        let versions = |r: &str, n: &str| if n == "rts" { None } else { real(r, n) };
        let mut c = vec![boot(BASE_910, "ghc-9.10.3")];
        let mut r = Vec::new();
        let cc = compilers(Some("ghc-9.10.3"));
        let s = apply_with(boot_dependencies::table(), &versions, Some(&cc), &mut c, &mut r);
        assert!(by_purl(&c, RTS).is_empty(), "no version, no component");
        assert_eq!(c245(by_purl(&c, PRIM_910)[0])["absent"], serde_json::json!(["rts"]));
        assert_eq!(c245(by_purl(&c, "pkg:hackage/ghc-internal@9.1003.0")[0])["absent"], serde_json::json!(["rts"]));
        assert_eq!(s.incomplete, 2);
    }

    /// Without the closure's compilers, spec 1653's behaviour exactly.
    #[test]
    fn none_compilers_is_todays_behaviour() {
        let mut c = vec![boot(CONTAINERS_98, "ghc-binary-9.8.4"), boot(ARRAY, "ghc-9.10.3")];
        let s = apply(&mut c, &mut Vec::new(), None);
        assert_eq!(c.len(), 2);
        assert_eq!(s.added, 0);
        let absent = c245(&c[0])["absent"].clone();
        assert!(absent.as_array().unwrap().contains(&serde_json::json!("array")), "{absent}");
    }

    /// FR-003: the version comes from milestone 966's table, which has `rts`
    /// on 9.2 where spec 1653's does not.
    #[test]
    fn the_version_is_milestone_966s() {
        let mut c = vec![boot("pkg:hackage/base@4.16.3.0", "ghc-binary-9.2.4")];
        apply(&mut c, &mut Vec::new(), Some(&compilers(None)));
        let rts = by_purl(&c, RTS);
        assert_eq!(rts.len(), 1, "{:?}", c.iter().map(|c| c.purl.as_str()).collect::<Vec<_>>());
        assert_eq!(label(rts[0]), "ghc-binary-9.2.4");
    }

    #[test]
    fn c245_is_canonical_json_with_sorted_keys() {
        let mut c = vec![boot(BASE_910, "ghc-9.10.3")];
        apply(&mut c, &mut Vec::new(), None);
        let raw = c[0].extra_annotations[ANN_BOOT_LIBRARY_DEPENDENCIES].as_str().unwrap().to_string();
        assert!(raw.starts_with("{\"absent\":["), "{raw}");
        assert!(raw.find("\"absent\"").unwrap() < raw.find("\"source\"").unwrap(), "{raw}");
    }
}
