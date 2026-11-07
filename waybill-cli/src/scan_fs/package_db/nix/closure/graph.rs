//! Milestone 1280 (#1346) — the closure's dependency graph.
//!
//! The tier emitted components and **no dependency edges**, so every closure
//! component was an orphan by construction: measured, 352 of 406 on the pinned
//! `nix-closure-moat` golden. nix records the graph in `inputs.drvs`; waybill
//! read the *declared* attributes instead and used them only to assign the
//! C182 role.
//!
//! **This module never reads a declared dependency attribute.** Roles come
//! from `buildInputs` and its siblings, edges come from `inputs.drvs`, and the
//! separation is what lets SC-005 assert the role census is unchanged rather
//! than hope so. Two axes: an operator filters on the first across the second.
//!
//! **Transitivity needs no extra pass, but the projection does.** Most
//! derivations are not emitted as components — a source tarball, a wrapper, a
//! setup hook — so an edge whose target is one of those has to go somewhere.
//! The rule (FR-010, `contracts/projection.md`) is semantic rather than a
//! tuning parameter:
//!
//! - a dependency declaring **no** `pname` is passed **through**: a tarball is
//!   not a dependency, it is how a dependency arrived, so walking through it
//!   preserves the real edge;
//! - a dependency declaring one **stops** the walk, whether or not it is
//!   emitted: passing through a package would assert `A depends on C` where
//!   nix said `A → B → C`, which is a transitive-closure artefact and not a
//!   fact nix stated.
//!
//! Measured on x86_64-linux against the three alternatives: this gives 3,429
//! edges and 5 residual orphans, where direct-only gives 3,088 and **381 of
//! 381** still orphaned (the closure's root is not itself an emitted
//! component, so there is no anchor), and full transitive gives 16,187 and 0.
//! A graph that looks populated and is disconnected is worse than none.
//!
//! The 5 that remain are not a defect to engineer away. Each is consumed only
//! by a derivation that declares a `pname` and is not emitted — `stdenv-linux`,
//! `patch 2.8`, `build 1.4.4` — and they carry a reason saying so, because
//! "nothing depends on this" and "waybill cannot show you what does" are
//! different facts and conflating them is the whole of #1346.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::derivation::{RawClosure, RawDerivation};

/// A component identity: the `(name, version)` pair several derivations may
/// collapse onto.
pub(crate) type Identity = (String, String);

/// The projected graph, keyed on `.drv` paths.
///
/// **Keyed on `.drv` paths and not on PURLs.** A `.drv` path is rewritten by
/// nothing, so the projection is independent of the identity passes that run
/// between emission and conversion — the milestone-1275 one rewrites 104 of
/// 381 closure component PURLs from `pkg:generic/…` to `pkg:hackage/…`.
///
/// That independence is a simplification rather than a rescue. This module's
/// research first claimed an edge built against a PURL before that pass would
/// dangle; it would not. `closure_identity.rs:662` calls `apply_renames` on
/// the relationship vector in the same step it renames the components, by a
/// convention #980 established and `haskell_packages/mod.rs:530` states as a
/// requirement on callers. The claim is retracted in
/// `specs/1280-nix-closure-edges/research.md`.
#[derive(Debug, Default)]
pub(crate) struct ProjectedGraph {
    /// `(consumer, consumed)` pairs. Sorted, so emission is byte-stable
    /// without a later sort (FR-003).
    pub(crate) edges: BTreeSet<(String, String)>,
    /// The derivation no other derivation's `inputs.drvs` names.
    ///
    /// `None` when every node has a consumer, which on a well-formed closure
    /// means a cycle spans all of it. The anchor is then unavailable and the
    /// caller emits what it has rather than guessing a root.
    pub(crate) root: Option<String>,
    /// How many derivations the projection visited, for FR-012.
    ///
    /// Asserted rather than timed: both measured closures are small enough
    /// that 381 full traversals would pass a wall-clock assertion.
    pub(crate) visits: usize,
}

/// Does this derivation stop the walk?
///
/// A derivation declaring a `pname` is a package. The test is on the
/// *declaration* and not on whether it was emitted, which is the distinction
/// that produces FR-011's residual-orphan population rather than hiding it.
fn stops(drv: &RawDerivation) -> bool {
    match drv.declared_pname() {
        // A derivation declaring a `pname` is a package, as before.
        Some(_) => true,
        // Milestone 1282 FR-013 — of the members declaring none, `source`
        // and `package` are stopping points and the other two are not.
        //
        // The walk therefore passes *through* scaffolding and fragments, so
        // an edge names the source a package was built from rather than the
        // build script that stood between them. Consulting
        // [`member::classify`] here rather than re-deriving the test is what
        // keeps this in step with `emit::emitted_drv_keys`, which asks the
        // same question of the same function.
        None => super::member::classify(drv).is_edge_endpoint(),
    }
}

/// Project `inputs.drvs` onto the derivations in `emitted`.
///
/// One memoized post-order pass: each derivation's nearest stopping
/// descendants are computed once and reused. The naive alternative walks from
/// every emitted component — 381 traversals of a 10,342-edge graph on the
/// measured closure — and FR-012 exists because both measured closures are
/// small enough to hide the difference.
pub(crate) fn project(closure: &RawClosure, emitted: &HashSet<&str>) -> ProjectedGraph {
    debug_assert!(
        emitted.iter().all(|k| {
            closure
                .derivations
                .get(*k)
                .is_some_and(stops)
        }),
        "every edge endpoint must be a derivation the walk stops at"
    );
    let drvs = &closure.derivations;
    let mut memo: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    let mut visits = 0usize;

    for key in drvs.keys() {
        nearest_stopping(drvs, key, &mut memo, &mut HashSet::new(), &mut visits);
    }

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    let root = root_of(drvs);
    // The root is a source of edges even when it is not an emitted component.
    // Without it, measured, 381 of 381 components stay orphaned while 3,088
    // edges are emitted (FR-004).
    let sources = emitted
        .iter()
        .copied()
        .chain(root.as_deref())
        .collect::<BTreeSet<&str>>();
    for src in sources {
        let Some(reached) = memo.get(src) else { continue };
        for target in reached {
            // Emit only to nodes that became components; a stopping node that
            // was not emitted is where FR-011's population comes from.
            if emitted.contains(target) && *target != src {
                edges.insert((src.to_string(), (*target).to_string()));
            }
        }
    }

    ProjectedGraph {
        edges,
        root,
        visits,
    }
}

/// The nearest stopping derivations reachable from `key`'s inputs.
///
/// `in_progress` is the cycle guard (FR-006): a node already on the stack is
/// treated as a leaf, so the pass terminates with a partial answer rather than
/// recursing. nix cannot emit a cycle; the guard does not depend on that.
fn nearest_stopping<'a>(
    drvs: &'a BTreeMap<String, RawDerivation>,
    key: &'a str,
    memo: &mut HashMap<&'a str, BTreeSet<&'a str>>,
    in_progress: &mut HashSet<&'a str>,
    visits: &mut usize,
) -> BTreeSet<&'a str> {
    if let Some(done) = memo.get(key) {
        return done.clone();
    }
    if !in_progress.insert(key) {
        // On the stack already: a cycle. Contribute nothing and unwind.
        return BTreeSet::new();
    }
    *visits += 1;

    let mut out: BTreeSet<&'a str> = BTreeSet::new();
    if let Some(drv) = drvs.get(key) {
        for input in drv.input_drvs(drvs) {
            // `input_drvs` has already dropped entries naming a derivation
            // outside this closure (FR-005), so the lookup cannot miss.
            let Some(target) = drvs.get(input) else { continue };
            if stops(target) {
                out.insert(input);
            } else {
                out.extend(nearest_stopping(drvs, input, memo, in_progress, visits));
            }
        }
    }

    in_progress.remove(key);
    memo.insert(key, out.clone());
    out
}

/// The derivation no other derivation's `inputs.drvs` names.
fn root_of(drvs: &BTreeMap<String, RawDerivation>) -> Option<String> {
    let consumed: HashSet<&str> = drvs
        .values()
        .flat_map(|d| d.input_drvs(drvs))
        .collect();
    let mut roots = drvs.keys().filter(|k| !consumed.contains(k.as_str()));
    let first = roots.next()?;
    // More than one root is possible in principle and has not been observed:
    // measured, both closures have exactly one. Taking the first of a
    // `BTreeMap`'s keys is deterministic, so a second root cannot make the
    // output vary between runs.
    Some(first.clone())
}

/// Turn the `.drv`-keyed graph into PURL-keyed relationships.
///
/// `components` must be the **final** set, which is why the caller invokes
/// this after the milestone-1275 identity pass: the identity→PURL join reads
/// whatever PURL the document finally gave each component, and that pass
/// rewrites 104 of 381 of them.
///
/// It is **not** the case that an earlier call would leave dangling endpoints
/// — `apply_renames` moves relationship endpoints alongside identities
/// (#980/#981). This milestone's research R1 claimed otherwise and carries the
/// retraction. The guarantee against a dangling endpoint is the integration
/// assertion that every closure edge's endpoints resolve to a present
/// component, which holds whatever the ordering; measured on the real closure,
/// 0 dangling of 3,907.
///
/// **The root anchor is deliberately not done here.** FR-004 wants the closure
/// root's edges attached to the document root, but `target_ref` — the string
/// all three emitters use for the subject — is derived inside each builder
/// from `select_root` and main-module detection (`cyclonedx/builder.rs:832`).
/// Reproducing it at this layer would be a second copy of the logic
/// `select_root` exists to centralise, so anchoring belongs beside
/// `graph_completeness::anchor_retained_mainmod_edges`, which solves the same
/// problem for other readers with `target_ref` in hand.
///
/// On the measured closure this is moot: the root derivation `moat-0.1` is
/// already a component — `pkg:hackage/moat@0.1` from the Haskell manifest
/// reader, which the document root already depends on — so the join resolves
/// it like any other node and the returned unanchored count is **0**. When it
/// does not resolve, those edges are dropped and counted rather than silently
/// lost.
pub(crate) fn relationships(
    graph: &ProjectedGraph,
    closure: &RawClosure,
    components: &[waybill_common::resolution::ResolvedComponent],
) -> (Vec<waybill_common::resolution::Relationship>, usize) {
    use waybill_common::resolution::{EnrichmentProvenance, Relationship, RelationshipType};

    let by_identity: HashMap<Identity, &str> = components
        .iter()
        .map(|c| ((c.name.clone(), c.version.clone()), c.purl.as_str()))
        .collect();
    let ident: HashMap<&str, Identity> = closure
        .derivations
        .iter()
        .filter_map(|(key, drv)| {
            let name = drv.pname()?;
            Some((
                key.as_str(),
                (name.to_string(), drv.version().unwrap_or_default().to_string()),
            ))
        })
        .collect();
    let purl_of = |key: &str| -> Option<&str> { by_identity.get(ident.get(key)?).copied() };

    // A `BTreeSet` rather than a `Vec`: variant-merged components produce the
    // same pair several times at the `.drv` level (FR-003), and the dedup has
    // to happen after the identity join rather than before it.
    let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut unanchored = 0usize;
    for (from, to) in &graph.edges {
        let Some(target) = purl_of(to) else { continue };
        let Some(source) = purl_of(from) else {
            // Only the root can fail to resolve: every other source is an
            // emitted component by construction. Counted, not swallowed.
            if graph.root.as_deref() == Some(from.as_str()) {
                unanchored += 1;
            }
            continue;
        };
        if source != target {
            pairs.insert((source, target));
        }
    }

    let edges: Vec<_> = pairs
        .into_iter()
        .map(|(from, to)| Relationship {
            from: from.to_string(),
            to: to.to_string(),
            // FR-007a — one type whatever the endpoint's C182 role. SPDX 2.3
            // could say `BUILD_TOOL_OF` where CycloneDX cannot, so mapping
            // role to type would stop the three formats carrying one graph,
            // and would stop the role being usable as a filter over it.
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "nix-closure".to_string(),
                // `nix-build-graph`, not `dependency`, and the distinction is
                // load-bearing: `resolve/scope_propagation.rs` declines to
                // traverse this `data_type`.
                //
                // These edges are read from nix's own record, so they are more
                // authoritative than a manifest — but they answer a different
                // question. `inputs.drvs` records what the build *consumed*,
                // and nixpkgs' Haskell builder passes test dependencies
                // through `buildInputs`. Measured: traversing them made 7 test
                // libraries stop asserting `scope: "excluded"`, which for a
                // test library was the correct claim.
                data_type: "nix-build-graph".to_string(),
            },
        })
        .collect();
    (edges, unanchored)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn fixture(name: &str) -> RawClosure {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nix_closure_edges")
            .join(name);
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        RawClosure::parse(&json).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
    }

    /// The emission rule, applied through the emitter's own predicate so the
    /// test cannot disagree with production about what gets emitted.
    fn emitted(c: &RawClosure) -> HashSet<&str> {
        super::super::emit::emitted_drv_keys(c, &super::super::classify::classify(c))
    }

    fn edges_of(c: &RawClosure) -> Vec<(String, String)> {
        project(c, &emitted(c)).edges.into_iter().collect()
    }

    #[test]
    fn m1282_t030_a_version_shaped_fetch_became_a_stopping_point() {
        // **This test asserted the opposite until milestone 1282, and the
        // change is the feature.** Under milestone 1280 `fetch.drv` declared
        // no `pname`, so it was neither emitted nor a stop, and the only
        // possible edge was the pass-through `pkga -> pkgb`.
        //
        // FR-013 makes `source` and `package` members stopping points.
        // `fetch.drv` is named `waybill-fixture-pkgb-1.0.tar.gz`, whose
        // `-1.0` is version-shaped and which records no content hash, so the
        // classifier calls it a `package` and the walk now stops there.
        //
        // That is the spec's "the graph changes, and that is accepted"
        // (Clarifications): stopping earlier replaces a transitive
        // fan-out with a direct edge, and `pkgb` behind it loses its only
        // inbound edge. The pass-through itself is NOT abandoned -- it is
        // still exercised, through a `scaffolding` member, which FR-013
        // keeps out of the endpoint set. See
        // `tests/closure_members.rs::m1282_us3_a_source_behind_scaffolding_is_the_endpoint`.
        let c = fixture("through_fetch.json");

        // CONTROL: the fixture still holds what the assertions below read.
        assert_eq!(c.derivations.len(), 4, "fixture shape changed");
        let em = emitted(&c);
        assert!(em.contains("pkgb.drv"), "pkgb must be emitted; got {em:?}");
        assert!(
            em.contains("fetch.drv"),
            "m1282: a version-shaped pname-less member is now an endpoint; got {em:?}"
        );

        let edges = edges_of(&c);
        assert!(
            edges.contains(&("pkga.drv".to_string(), "fetch.drv".to_string())),
            "the walk must stop at the fetch rather than passing through it. got {edges:?}"
        );
        assert!(
            !edges.contains(&("pkga.drv".to_string(), "pkgb.drv".to_string())),
            "pkga must no longer reach pkgb transitively: the fetch stops the walk. \
             got {edges:?}"
        );
    }

    #[test]
    fn t014_the_walk_stops_at_a_package_that_was_not_emitted() {
        // Contract P2. This is the row every "is it connected" test misses,
        // and the entire population of FR-011's orphan reason.
        //
        // **The emitted set is constructed here rather than derived from
        // `classify`, and milestone 1372 (#1366) is why.** That milestone
        // gives a role to every derivation the build graph names, so a
        // derivation reachable through `inputs.drvs` now emits — which means
        // "a package in the middle of a walk that was not emitted" can no
        // longer arise from a real closure. Measured: 1,274 of the pinned
        // closure's 1,275 derivations emit afterwards, against 1,174 before.
        //
        // The pass-through refusal this test protects is still correct, still
        // reachable by any caller that restricts the emitted set, and still
        // the thing FR-011's orphan reason depends on. Deriving the set from
        // `classify` would have made the test assert the classifier's
        // behaviour instead of the projection's, and it would have silently
        // stopped exercising P2 at all.
        let c = fixture("stop_at_package.json");

        // CONTROL: pkgmid really is a package. If it were not, the walk would
        // pass through it for an unrelated reason and the assertion below
        // would hold vacuously.
        assert!(
            c.derivations["pkgmid.drv"].declared_pname().is_some(),
            "pkgmid must declare a pname -- it is what makes the walk stop"
        );

        // The emitted set, stated rather than inferred: everything but the
        // package in the middle.
        let em: HashSet<&str> = c
            .derivations
            .keys()
            .map(String::as_str)
            .filter(|k| *k != "pkgmid.drv")
            .collect();
        assert!(em.contains("pkgleaf.drv"), "pkgleaf must be emitted");

        let edges: Vec<(String, String)> =
            project(&c, &em).edges.into_iter().collect();
        assert!(
            !edges.iter().any(|(_, to)| to == "pkgleaf.drv"),
            "pkgleaf is reachable only through a PACKAGE that was not emitted; \
             passing through it would assert an edge nix did not state. \
             got {edges:?}"
        );
    }

    #[test]
    fn t014_a_cycle_terminates() {
        // FR-006. nix cannot emit this; the guard does not depend on that.
        // The assertion is that we get here at all -- a non-terminating walk
        // does not fail this test, it hangs it, which is why the fixture is
        // tiny and the suite is not the place this is relied on alone.
        let c = fixture("cycle.json");
        let g = project(&c, &emitted(&c));
        assert!(
            g.visits <= c.derivations.len(),
            "each derivation is visited at most once even through a cycle; \
             {} visits for {} derivations",
            g.visits,
            c.derivations.len()
        );
    }

    #[test]
    fn t014_variants_of_one_identity_do_not_duplicate_their_edges() {
        // FR-003 at the .drv level: two derivations of one `(pname, version)`
        // each name pkgc, so two .drv-keyed edges exist here and collapse to
        // one relationship at PURL conversion. Asserting the .drv pair is
        // deliberate -- this layer must not know about PURLs.
        let c = fixture("variants.json");
        let edges = edges_of(&c);
        let to_pkgc: Vec<_> = edges.iter().filter(|(_, to)| to == "pkgc.drv").collect();
        assert_eq!(
            to_pkgc.len(),
            2,
            "both variants name pkgc, so both .drv edges exist at this layer; \
             the merge happens at conversion. got {edges:?}"
        );
    }

    #[test]
    fn t014_the_root_anchors_the_graph_even_though_it_is_not_a_component() {
        // FR-004. Measured: without the root as a source, 381 of 381
        // components stay orphaned while 3,088 edges are emitted.
        let c = fixture("anchor.json");
        let g = project(&c, &emitted(&c));
        assert_eq!(g.root.as_deref(), Some("root.drv"));
        assert!(
            !emitted(&c).contains("root.drv"),
            "the root must NOT be an emitted component, or this tests nothing"
        );
        for target in ["anchora.drv", "anchorb.drv"] {
            assert!(
                g.edges.contains(&("root.drv".to_string(), target.to_string())),
                "the root is a source of edges despite not being a component"
            );
        }
    }

    #[test]
    fn t014_an_input_outside_the_closure_contributes_no_edge() {
        // FR-005, at the projection layer rather than the parse layer.
        let c = fixture("dangling_input.json");
        let edges = edges_of(&c);
        assert!(
            !edges.iter().any(|(_, to)| to == "absent-from-this-closure.drv"),
            "got {edges:?}"
        );
    }

    #[test]
    fn t015_the_projection_is_deterministic() {
        // SC-004's unit half. `BTreeSet` makes this structural rather than
        // incidental, which is why the assertion is on the full sequence and
        // not on the length.
        let c = fixture("through_fetch.json");
        let a = edges_of(&c);
        let b = edges_of(&c);
        assert_eq!(a, b);
        assert!(!a.is_empty(), "an empty sequence compares equal to itself");
    }

    #[test]
    fn t042_each_derivation_is_visited_at_most_once() {
        // FR-012. Asserted on a visit counter rather than on wall-clock: both
        // measured closures are small enough that 381 full traversals would
        // pass a timing assertion.
        for name in [
            "through_fetch.json",
            "stop_at_package.json",
            "variants.json",
            "anchor.json",
        ] {
            let c = fixture(name);
            let g = project(&c, &emitted(&c));
            assert!(
                g.visits <= c.derivations.len(),
                "{name}: {} visits for {} derivations -- the memo is not being hit",
                g.visits,
                c.derivations.len()
            );
        }
    }
}
