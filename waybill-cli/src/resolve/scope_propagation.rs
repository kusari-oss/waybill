//! Transitive propagation of non-runtime lifecycle scope (#1209).
//!
//! Readers assign `lifecycle_scope` from what their lockfile tells
//! them. `package-lock.json` stamps `dev: true` on every entry
//! including transitives, so npm arrives fully classified; `uv.lock`
//! lists only group *members* under `[package.dev-dependencies]`, so
//! the pip reader can mark depth 1 and no further. Nothing propagated
//! the rest, so a transitive dependency of dev tooling was emitted
//! with a real coordinate, a real version and nothing marking it
//! non-production — indistinguishable from something that ships.
//!
//! Two phases, one rule. Full contract, including pre/postconditions
//! and the degenerate inputs:
//! `specs/1269-transitive-dev-scope/contracts/propagation-algorithm.md`.
//!
//! **Phase A — immunity.** BFS from the root seed set, never
//! traversing *into* a component that already carries a non-runtime
//! scope. Anything it reaches is runtime-reachable and is absolutely
//! immune from propagation. This is the guard against the dangerous
//! direction: a pass that over-propagates satisfies every count-based
//! check while silently demoting dependencies that ship, which is
//! under-reporting, invisible in orphan counts, and worse than the bug
//! being fixed.
//!
//! **Phase B — propagation.** For each non-runtime scope, strongest
//! first, BFS from that scope's declared members and assign it to
//! every component reached that is not immune and carries no scope of
//! its own. Strongest-first iteration is how "the strongest scope
//! across all inbound paths wins" is implemented, and `Optional`
//! outranks `Development` because optional means the component may
//! ship while development means it does not — the weaker claim about
//! production is the safer one to publish.
//!
//! The pass is **strictly additive**: it never changes a scope a
//! reader assigned. That is what makes an already-correct ecosystem
//! byte-identical by construction rather than by test.

use std::collections::{HashMap, HashSet, VecDeque};

use waybill_common::resolution::{LifecycleScope, Relationship, ResolvedComponent};

/// The annotation key recording that a scope was inferred by this pass
/// rather than read from a declaration. Catalog row C199.
pub(crate) const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";

/// The only value the C199 vocabulary recognises in v1. A
/// reader-declared scope is identified by the key's *absence*, so
/// there is deliberately no `"declared"` value to write: emitting one
/// would add a field to every already-correct component and break the
/// byte-identity this pass otherwise guarantees.
pub(crate) const DERIVATION_TRANSITIVE: &str = "transitive";

/// `provenance.data_type` values marking an edge waybill *inferred*
/// by matching names across ecosystems rather than read from a
/// manifest (milestone 218, `--experimental-cross-ecosystem-edges`).
///
/// Excluded from both traversals (FR-002b). The two directions hold
/// the same evidentiary standard on purpose: were propagation to
/// travel edges that cannot confer immunity, a heuristic name match
/// could demote a component with no declared relationship to the
/// dependency whose scope it inherited.
///
/// Note this is `data_type`, not `source` — `source` holds the
/// manifest path for these edges, exactly as it does for a declared
/// one, so it cannot discriminate.
const INFERRED_EDGE_DATA_TYPES: &[&str] =
    &["cross-ecosystem-bridge", "cross-ecosystem-bridge-ambiguous"];

/// Provenance sources of edges waybill synthesizes from the subject so
/// that an otherwise-unreachable component is connected.
///
/// Every one of these is created *inside* a format builder
/// (`generate/cyclonedx/builder.rs`, `generate/spdx/document.rs`,
/// `generate/spdx/v3_document.rs`), downstream of this pass, so none
/// of them can appear in its input. FR-002a therefore holds by
/// placement — and `assert_no_anchoring_edges` below is what makes a
/// future change that moves anchoring upstream fail loudly instead of
/// silently immunizing the orphaned non-runtime closure, which is the
/// population this pass exists to mark.
const ANCHORING_EDGE_SOURCES: &[&str] = &[
    "milestone-868-resolve-ownership",
    "milestone-860-retained-main-module",
    "milestone-158-workspace-peer-linkage",
];

/// What the pass did. Emitted as one INFO line and never serialized
/// into an SBOM.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PropagationSummary {
    /// Size of the runtime-reachable immunity set.
    pub immune: usize,
    /// How many components were assigned each scope.
    pub assigned_by_scope: std::collections::BTreeMap<&'static str, usize>,
    /// Declared non-runtime components used as propagation seeds.
    pub seeded_from: usize,
}

impl PropagationSummary {
    pub fn assigned_total(&self) -> usize {
        self.assigned_by_scope.values().sum()
    }
}

/// True when this edge may be traversed — i.e. when it was read from a
/// manifest or lockfile rather than inferred by waybill (FR-002b).
fn is_declared_edge(r: &Relationship) -> bool {
    !INFERRED_EDGE_DATA_TYPES.contains(&r.provenance.data_type.as_str())
}

/// Panics if an anchoring edge reached this pass. See
/// `ANCHORING_EDGE_SOURCES` for why this is an assertion and not a
/// filter.
fn assert_no_anchoring_edges(relationships: &[Relationship]) {
    if let Some(r) = relationships
        .iter()
        .find(|r| ANCHORING_EDGE_SOURCES.contains(&r.provenance.source.as_str()))
    {
        panic!(
            "scope propagation received an anchoring edge ({} -> {}, provenance {:?}). \
             Anchoring is synthesized inside the format builders, downstream of this \
             pass; an anchoring edge here would make the orphaned non-runtime closure \
             look runtime-reachable and silently exempt it from propagation (#1209 \
             FR-002a). Exclude the new edge class from the traversal rather than \
             deleting this assertion.",
            r.from, r.to, r.provenance.source
        );
    }
}

/// Adjacency over declared edges only.
fn declared_adjacency(relationships: &[Relationship]) -> HashMap<&str, Vec<&str>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in relationships.iter().filter(|r| is_declared_edge(r)) {
        adj.entry(r.from.as_str()).or_default().push(r.to.as_str());
    }
    adj
}

/// Phase A. Components reachable from `seeds` over declared edges
/// without passing through anything already scoped non-runtime.
fn runtime_reachable<'a>(
    components: &'a [ResolvedComponent],
    adj: &HashMap<&'a str, Vec<&'a str>>,
    seeds: &HashSet<String>,
) -> HashSet<&'a str> {
    let non_runtime: HashSet<&str> = components
        .iter()
        .filter(|c| c.lifecycle_scope.is_some_and(|s| s.is_non_runtime()))
        .map(|c| c.purl.as_str())
        .collect();

    // Seed strings are owned by the caller; re-borrow them from the
    // component slice so every element of the returned set has the
    // same lifetime.
    let by_purl: HashMap<&str, ()> = components.iter().map(|c| (c.purl.as_str(), ())).collect();

    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    for seed in seeds {
        if let Some((purl, ())) = by_purl.get_key_value(seed.as_str()) {
            if seen.insert(purl) {
                queue.push_back(purl);
            }
        }
    }
    while let Some(current) = queue.pop_front() {
        for next in adj.get(current).into_iter().flatten() {
            if non_runtime.contains(next) {
                // A non-runtime component is where a runtime path
                // ends, by definition. Its subtree is Phase B's.
                continue;
            }
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    seen
}

/// Propagate non-runtime lifecycle scope across the dependency graph.
///
/// `seeds` is the root seed set (from
/// `generate::graph_completeness::bfs::component_root_seeds`, so this
/// pass and the emitters' completeness reporting measure reachability
/// from the same root). `ecosystems_with_root` is that function's
/// per-ecosystem root map keyset: an ecosystem absent from it has no
/// resolvable root, so immunity cannot be established there and the
/// pass declines to assign into it (FR-012) rather than guessing.
///
/// Returns what it did; mutates `components` in place.
pub(crate) fn propagate_non_runtime_scope(
    components: &mut [ResolvedComponent],
    relationships: &[Relationship],
    seeds: &HashSet<String>,
    ecosystems_with_root: &HashSet<String>,
) -> PropagationSummary {
    assert_no_anchoring_edges(relationships);

    let adj = declared_adjacency(relationships);

    // Phase A, and the borrow of `components` ends with it: the
    // assignment map below is keyed by owned PURL strings so the
    // mutation pass can take `&mut`.
    let (immune, assigned): (usize, HashMap<String, LifecycleScope>) = {
        let immune = runtime_reachable(components, &adj, seeds);

        let scope_by_purl: HashMap<&str, LifecycleScope> = components
            .iter()
            .filter_map(|c| c.lifecycle_scope.map(|s| (c.purl.as_str(), s)))
            .collect();
        let ecosystem_by_purl: HashMap<&str, &str> = components
            .iter()
            .map(|c| (c.purl.as_str(), c.purl.ecosystem()))
            .collect();

        // Phase B. Strongest first, so a component reached by two
        // scopes is claimed by the stronger one and the later pass
        // skips it — equivalent to a max over inbound scopes, and the
        // order is what makes the result independent of component and
        // relationship order.
        const BY_DESCENDING_STRENGTH: &[LifecycleScope] = &[
            LifecycleScope::Optional,
            LifecycleScope::Build,
            LifecycleScope::Test,
            LifecycleScope::Development,
        ];

        let mut assigned: HashMap<String, LifecycleScope> = HashMap::new();
        for scope in BY_DESCENDING_STRENGTH {
            let mut seen: HashSet<&str> = HashSet::new();
            let mut queue: VecDeque<&str> = VecDeque::new();
            for (purl, declared) in &scope_by_purl {
                if declared == scope && seen.insert(purl) {
                    queue.push_back(purl);
                }
            }
            while let Some(current) = queue.pop_front() {
                for next in adj.get(current).into_iter().flatten() {
                    if !seen.insert(next) {
                        continue;
                    }
                    // Traverse regardless of what `next` is — an
                    // unscoped component is transparent, and a scoped
                    // one is skipped for assignment below but still
                    // conducts: a declared dev member's own subtree is
                    // exactly what needs marking.
                    queue.push_back(next);
                }
            }
            for purl in seen {
                if immune.contains(purl) {
                    continue; // FR-002
                }
                if scope_by_purl.contains_key(purl) {
                    continue; // FR-003 — never touch a declared scope
                }
                let Some(ecosystem) = ecosystem_by_purl.get(purl) else {
                    continue;
                };
                if !ecosystems_with_root.contains(*ecosystem) {
                    continue; // FR-012
                }
                assigned
                    .entry((*purl).to_string())
                    .or_insert_with(|| *scope); // FR-004, by iteration order
            }
        }
        (immune.len(), assigned)
    };

    let mut summary = PropagationSummary {
        immune,
        seeded_from: components
            .iter()
            .filter(|c| c.lifecycle_scope.is_some_and(|s| s.is_non_runtime()))
            .count(),
        ..Default::default()
    };
    if assigned.is_empty() {
        // FR-013: nothing to assign means nothing changes, including
        // no annotation and no log line.
        return summary;
    }

    for component in components.iter_mut() {
        if let Some(scope) = assigned.get(component.purl.as_str()) {
            component.lifecycle_scope = Some(*scope);
            component.extra_annotations.insert(
                ANN_SCOPE_DERIVATION.to_string(),
                serde_json::Value::String(DERIVATION_TRANSITIVE.to_string()),
            );
            *summary
                .assigned_by_scope
                .entry(scope.as_str())
                .or_insert(0) += 1;
        }
    }

    tracing::info!(
        immune = summary.immune,
        seeded_from = summary.seeded_from,
        assigned = summary.assigned_total(),
        by_scope = ?summary.assigned_by_scope,
        "propagated non-runtime lifecycle scope to transitive dependencies",
    );
    summary
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::{mk_component, mk_rel};
    use waybill_common::resolution::{EnrichmentProvenance, RelationshipType};

    fn scoped(purl: &str, scope: LifecycleScope) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.lifecycle_scope = Some(scope);
        c
    }

    fn seeds(purls: &[&str]) -> HashSet<String> {
        purls.iter().map(|p| (*p).to_string()).collect()
    }

    fn ecosystems(names: &[&str]) -> HashSet<String> {
        names.iter().map(|p| (*p).to_string()).collect()
    }

    fn scope_of(components: &[ResolvedComponent], purl: &str) -> Option<LifecycleScope> {
        components
            .iter()
            .find(|c| c.purl.as_str() == purl)
            .expect("component present")
            .lifecycle_scope
    }

    fn derivation_of(components: &[ResolvedComponent], purl: &str) -> Option<String> {
        components
            .iter()
            .find(|c| c.purl.as_str() == purl)
            .expect("component present")
            .extra_annotations
            .get(ANN_SCOPE_DERIVATION)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    const ROOT: &str = "pkg:npm/waybill-fixture-root@1.0.0";
    const SHARED: &str = "pkg:npm/waybill-fixture-shared@1.0.0";
    const DEVTOOL: &str = "pkg:npm/waybill-fixture-devtool@1.0.0";
    const DEVDEP: &str = "pkg:npm/waybill-fixture-devdep@1.0.0";

    /// SC-003, the positive control. A component on BOTH a runtime and
    /// a dev path keeps runtime scope.
    ///
    /// This is the test the feature is most at risk from. A pass that
    /// over-propagates satisfies every count-based check in US1 while
    /// silently demoting dependencies that ship — under-reporting,
    /// invisible in orphan counts, and worse than the bug being fixed.
    /// No corpus target can prove this negative, so it is built here.
    ///
    /// Mutation-verified: deleting the `immune.contains` guard in
    /// Phase B makes this fail. See
    /// `specs/1269-transitive-dev-scope/measurements/mutation-evidence.md`.
    #[test]
    fn a_runtime_reachable_component_is_never_demoted() {
        let mut components = vec![
            mk_component(ROOT),
            mk_component(SHARED),
            scoped(DEVTOOL, LifecycleScope::Development),
        ];
        let relationships = vec![
            mk_rel(ROOT, SHARED),
            mk_rel(ROOT, DEVTOOL),
            mk_rel(DEVTOOL, SHARED),
        ];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, SHARED),
            None,
            "a component reachable from the root by a path with no non-runtime \
             component must keep its scope, whatever else reaches it (FR-002)"
        );
        assert_eq!(
            derivation_of(&components, SHARED),
            None,
            "an untouched component must gain no annotation (FR-008)"
        );
    }

    /// FR-003: the pass fills an absent scope and never overwrites one.
    #[test]
    fn a_reader_assigned_scope_is_never_changed() {
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            scoped(DEVDEP, LifecycleScope::Optional),
        ];
        let relationships = vec![mk_rel(ROOT, DEVTOOL), mk_rel(DEVTOOL, DEVDEP)];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, DEVDEP),
            Some(LifecycleScope::Optional),
            "a declared Optional reached from a Development seed stays Optional"
        );
        assert_eq!(derivation_of(&components, DEVDEP), None);
    }

    /// FR-012: an ecosystem with no resolvable root cannot have its
    /// immunity established, so the pass declines to act there rather
    /// than guessing — and the direction it would guess wrong in is
    /// demotion.
    #[test]
    fn an_ecosystem_without_a_root_is_left_alone() {
        const PY_DEV: &str = "pkg:pypi/waybill-fixture-pydev@1.0.0";
        const PY_TRANSITIVE: &str = "pkg:pypi/waybill-fixture-pytransitive@1.0.0";
        let mut components = vec![
            scoped(PY_DEV, LifecycleScope::Development),
            mk_component(PY_TRANSITIVE),
        ];
        let relationships = vec![mk_rel(PY_DEV, PY_TRANSITIVE)];

        // `npm` has a root; `pypi` does not.
        let summary = propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(scope_of(&components, PY_TRANSITIVE), None);
        assert_eq!(summary.assigned_total(), 0);
    }

    /// FR-001 / FR-005 / US1: the defect itself. A transitive
    /// dependency of dev tooling, unreachable from the root except
    /// through it, is marked.
    #[test]
    fn a_dev_only_transitive_is_marked() {
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            mk_component(DEVDEP),
        ];
        let relationships = vec![mk_rel(ROOT, DEVTOOL), mk_rel(DEVTOOL, DEVDEP)];

        let summary = propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, DEVDEP),
            Some(LifecycleScope::Development)
        );
        assert_eq!(
            derivation_of(&components, DEVDEP).as_deref(),
            Some(DERIVATION_TRANSITIVE),
            "a scope this pass inferred must be distinguishable from a declared one (FR-007)"
        );
        assert_eq!(summary.assigned_total(), 1);
    }

    /// FR-005, population B: the shape #1209 measured on pydantic,
    /// where the declared group members are themselves unreachable
    /// from the subject, so their transitives are counted as orphans.
    /// A pass seeded from root reachability would never see them.
    #[test]
    fn propagation_does_not_require_root_reachability() {
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            mk_component(DEVDEP),
        ];
        // No edge from ROOT to anything: DEVTOOL and DEVDEP are
        // unreachable from the subject, exactly as the 76 PEP 735
        // group members and their 103 transitives were.
        let relationships = vec![mk_rel(DEVTOOL, DEVDEP)];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, DEVDEP),
            Some(LifecycleScope::Development)
        );
    }

    /// FR-004: the strongest scope across all inbound paths wins, and
    /// the result does not depend on the order components or
    /// relationships happen to arrive in (postcondition Q6).
    ///
    /// Also FR-006: the assigned value is an existing `LifecycleScope`
    /// variant equal to the seed's, not a new "transitively optional"
    /// one — the variant is what every consumer-side filter and every
    /// format's native scope mapping already keys on.
    #[test]
    fn the_strongest_inbound_scope_wins_in_either_order() {
        const OPTIONAL_SEED: &str = "pkg:npm/waybill-fixture-optseed@1.0.0";
        for reversed in [false, true] {
            let mut components = vec![
                mk_component(ROOT),
                scoped(DEVTOOL, LifecycleScope::Development),
                scoped(OPTIONAL_SEED, LifecycleScope::Optional),
                mk_component(DEVDEP),
            ];
            let mut relationships = vec![
                mk_rel(ROOT, DEVTOOL),
                mk_rel(ROOT, OPTIONAL_SEED),
                mk_rel(DEVTOOL, DEVDEP),
                mk_rel(OPTIONAL_SEED, DEVDEP),
            ];
            if reversed {
                components.reverse();
                relationships.reverse();
            }

            propagate_non_runtime_scope(
                &mut components,
                &relationships,
                &seeds(&[ROOT]),
                &ecosystems(&["npm"]),
            );

            assert_eq!(
                scope_of(&components, DEVDEP),
                Some(LifecycleScope::Optional),
                "Optional outranks Development: optional means the component MAY \
                 ship and development means it does not, so the weaker claim about \
                 production is the one to publish (reversed = {reversed})"
            );
        }
    }

    /// FR-001 as clarified: an unscoped component is transparent to
    /// propagation. Requiring every inbound path to positively cross a
    /// non-runtime component would leave population B unmarked, since
    /// in an orphan subgraph almost nothing carries a scope at all.
    #[test]
    fn an_unscoped_inbound_path_does_not_block_marking() {
        const ORPHAN: &str = "pkg:npm/waybill-fixture-orphan@1.0.0";
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            mk_component(ORPHAN),
            mk_component(DEVDEP),
        ];
        let relationships = vec![
            mk_rel(ROOT, DEVTOOL),
            mk_rel(DEVTOOL, DEVDEP),
            // ORPHAN is unscoped and itself unreachable from ROOT.
            mk_rel(ORPHAN, DEVDEP),
        ];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, DEVDEP),
            Some(LifecycleScope::Development),
            "only positive runtime reachability blocks marking; the absence of a \
             scope on a neighbour is not evidence that a component ships"
        );
    }

    /// FR-011: a cycle among non-runtime components terminates, and
    /// every member is marked. Visitation is per-component, not
    /// per-path.
    #[test]
    fn a_cycle_terminates_and_every_member_is_marked() {
        const A: &str = "pkg:npm/waybill-fixture-cyc-a@1.0.0";
        const B: &str = "pkg:npm/waybill-fixture-cyc-b@1.0.0";
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Test),
            mk_component(A),
            mk_component(B),
        ];
        let relationships = vec![
            mk_rel(ROOT, DEVTOOL),
            mk_rel(DEVTOOL, A),
            mk_rel(A, B),
            mk_rel(B, A),
        ];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(scope_of(&components, A), Some(LifecycleScope::Test));
        assert_eq!(scope_of(&components, B), Some(LifecycleScope::Test));
    }

    /// FR-013: a document whose reader already marked every transitive
    /// has no unscoped component to assign, so the pass changes
    /// nothing and emits no annotation. This is why `npm-express` is
    /// byte-identical by construction rather than by test.
    #[test]
    fn an_already_correct_document_is_untouched() {
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            scoped(DEVDEP, LifecycleScope::Development),
        ];
        let before = components.clone();
        let relationships = vec![mk_rel(ROOT, DEVTOOL), mk_rel(DEVTOOL, DEVDEP)];

        let summary = propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(summary.assigned_total(), 0);
        for (a, b) in before.iter().zip(components.iter()) {
            assert_eq!(a.lifecycle_scope, b.lifecycle_scope);
            assert_eq!(a.extra_annotations, b.extra_annotations);
        }
    }

    /// FR-002b: an edge waybill inferred by matching names across
    /// ecosystems neither confers immunity nor carries a scope. Were
    /// propagation to travel edges that cannot protect, a heuristic
    /// name match could demote a component with no declared
    /// relationship to the dependency whose scope it inherited.
    #[test]
    fn a_cross_ecosystem_bridge_edge_neither_propagates_nor_immunizes() {
        const BRIDGED: &str = "pkg:generic/waybill-fixture-bridged@1.0.0";
        for data_type in INFERRED_EDGE_DATA_TYPES {
            let mut components = vec![
                mk_component(ROOT),
                scoped(DEVTOOL, LifecycleScope::Development),
                mk_component(BRIDGED),
            ];
            let mut bridge = mk_rel(DEVTOOL, BRIDGED);
            bridge.provenance = EnrichmentProvenance {
                // `source` holds the manifest path for these edges,
                // exactly as for a declared one — only `data_type`
                // discriminates.
                source: "/scan/package.json".to_string(),
                data_type: (*data_type).to_string(),
            };
            let relationships = vec![mk_rel(ROOT, DEVTOOL), bridge];

            propagate_non_runtime_scope(
                &mut components,
                &relationships,
                &seeds(&[ROOT]),
                &ecosystems(&["npm", "generic"]),
            );

            assert_eq!(
                scope_of(&components, BRIDGED),
                None,
                "data_type {data_type:?} is an inferred edge and must not carry a scope"
            );
        }
    }

    /// FR-002b, the other half: a declared edge whose `source` is a
    /// manifest path and whose `data_type` is an ordinary dependency
    /// graph IS traversed. Without this, the test above would pass
    /// against a pass that traverses nothing.
    #[test]
    fn a_declared_edge_is_traversed() {
        let r = mk_rel(DEVTOOL, DEVDEP);
        assert_eq!(r.provenance.data_type, "dependency-graph");
        assert!(is_declared_edge(&r));
    }

    /// FR-002a. Anchoring edges are synthesized inside the format
    /// builders, downstream of this pass, so none can appear in its
    /// input — and if one ever does, it must fail loudly rather than
    /// silently immunize the orphaned non-runtime closure.
    #[test]
    #[should_panic(expected = "scope propagation received an anchoring edge")]
    fn an_anchoring_edge_trips_the_assertion() {
        let mut components = vec![mk_component(ROOT), mk_component(DEVDEP)];
        let mut anchored = mk_rel(ROOT, DEVDEP);
        anchored.provenance = EnrichmentProvenance {
            source: ANCHORING_EDGE_SOURCES[0].to_string(),
            data_type: "dependency-graph".to_string(),
        };
        propagate_non_runtime_scope(
            &mut components,
            &[anchored],
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );
    }

    /// An edge already typed by the reader or by a previous
    /// edge-typing pass is still a dependency edge and is still
    /// traversed — every `RelationshipType` variant is a dependency
    /// relation today, and this is what says so.
    #[test]
    fn a_typed_dependency_edge_is_still_traversed() {
        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            mk_component(DEVDEP),
        ];
        let mut typed = mk_rel(DEVTOOL, DEVDEP);
        typed.relationship_type = RelationshipType::DevDependsOn;
        let relationships = vec![mk_rel(ROOT, DEVTOOL), typed];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );

        assert_eq!(
            scope_of(&components, DEVDEP),
            Some(LifecycleScope::Development)
        );
    }

    /// Degenerate inputs, from the algorithm contract: no components,
    /// no edges, and a document with no non-runtime component at all.
    #[test]
    fn degenerate_inputs_are_no_ops() {
        let mut empty: Vec<ResolvedComponent> = Vec::new();
        let s = propagate_non_runtime_scope(
            &mut empty,
            &[],
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );
        assert_eq!(s, PropagationSummary::default());

        let mut all_runtime = vec![mk_component(ROOT), mk_component(DEVDEP)];
        let s = propagate_non_runtime_scope(
            &mut all_runtime,
            &[mk_rel(ROOT, DEVDEP)],
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
        );
        assert_eq!(s.assigned_total(), 0);
        assert_eq!(s.seeded_from, 0);
    }
}
