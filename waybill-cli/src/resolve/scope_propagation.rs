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
/// rather than read from a declaration. Catalog row C62.
/// Catalog row **C62**, not a new row. The key has carried
/// `test-only-closure` and `go-mod-why` since the Go test-closure work;
/// `transitive` is a third value on the same open enum, which is what
/// that enum was documented as open for. See
/// `specs/1269-transitive-dev-scope/contracts/scope-derivation-annotation.md`,
/// including why the first draft of this feature wrongly specified a new
/// row for an already-live key.
pub(crate) const ANN_SCOPE_DERIVATION: &str = "waybill:lifecycle-scope-derivation";

/// The only value this feature adds to C62's open enum, which
/// already carries `test-only-closure` and `go-mod-why`. A
/// reader-declared scope is identified by the key's *absence*, so
/// there is deliberately no `"declared"` value to write: emitting one
/// would add a field to every already-correct component and break the
/// byte-identity this pass otherwise guarantees.
pub(crate) const DERIVATION_TRANSITIVE: &str = "transitive";

/// `provenance.data_type` values marking an edge this pass must not
/// traverse.
///
/// Two populations, for two different reasons.
///
/// **Inferred edges** (milestone 218,
/// `--experimental-cross-ecosystem-edges`): waybill matched names
/// across ecosystems rather than reading a manifest. Excluded from
/// both traversals (FR-002b), and the two directions hold the same
/// evidentiary standard on purpose — were propagation to travel edges
/// that cannot confer immunity, a heuristic name match could demote a
/// component with no declared relationship to the dependency whose
/// scope it inherited.
///
/// **Nix build-graph edges** (milestone 1280, #1346): read from nix's
/// own `inputs.drvs`, so *more* authoritative than a manifest rather
/// than less — but they answer a different question. `inputs.drvs`
/// records what the build **consumed**, and nixpkgs' Haskell builder
/// passes test dependencies through `buildInputs`, which C182's own
/// documentation names as the reason `artifact-input` does not mean
/// "ships". Traversing them makes a test library runtime-reachable.
///
/// Measured: when milestone 1280 first emitted these edges without
/// this exclusion, scope propagation reclassified **8** components on
/// `nix-closure-moat` — `HUnit`, `QuickCheck`, `call-stack`,
/// `haskell-lexer`, `hspec-core`, `hspec-expectations` and
/// `quickcheck-io` lost CycloneDX `scope: "excluded"`, and
/// `hspec-meta` gained it. Seven test libraries stopped asserting they
/// are absent from the runtime artifact, which for a test library was
/// the correct claim. Re-deciding scope for those eight is a separate
/// decision with its own reasoning; emitting the graph is this
/// milestone's.
///
/// Note this is `data_type`, not `source` — `source` holds the
/// manifest path for the inferred edges, exactly as it does for a
/// declared one, so it cannot discriminate.
const INFERRED_EDGE_DATA_TYPES: &[&str] = &[
    "cross-ecosystem-bridge",
    "cross-ecosystem-bridge-ambiguous",
    "nix-build-graph",
];

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
    /// Milestone 1276 (#1340) — components immune because the compiler ships
    /// them, counted separately from the runtime-reachable immunity.
    ///
    /// Separate because the two answer different questions: one says "a
    /// runtime path reaches this", the other "no path has to". Collapsing
    /// them would make the exemption invisible in the record that FR-007
    /// emits, and "the rule exempted nothing" would be indistinguishable
    /// from "the rule did not run".
    pub immune_compiler_supplied: usize,
    /// Size of the runtime-reachable immunity set.
    pub immune: usize,
    /// How many components were assigned each scope.
    pub assigned_by_scope: std::collections::BTreeMap<&'static str, usize>,
    /// Declared non-runtime components used as propagation seeds.
    pub seeded_from: usize,
    /// Reached by Phase B, not immune, unscoped — and skipped anyway
    /// because FR-012 found no root seed for their ecosystem.
    ///
    /// This exists because the pass used to be silent when it
    /// assigned nothing, and "the feature is a no-op here" and "the
    /// feature declined here, deliberately, for a stated reason" then
    /// looked identical from outside. Diagnosing three corpus targets
    /// took reading the readers' source; with this it is one log line.
    /// SC-001's exception (a) requires any such component be named, so
    /// the pass has to be able to say which they were.
    pub skipped_unrooted_ecosystem: usize,
    /// Ecosystems that had a resolvable root seed, sorted.
    pub rooted_ecosystems: Vec<String>,
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
/// Milestone 1276 (#1340) — packages the compiler ships, which no scope may
/// report as absent from the artifact.
///
/// # Why this is immunity rather than a correction
///
/// CycloneDX 1.6 defines `optional` as "not capable of being called due to
/// them **not being installed** or otherwise accessible by any means", and
/// states that a component which IS installed but is prohibited from being
/// called "must be scoped as `required`". It defines `excluded` as "not
/// reachable within a call graph at runtime". A GHC boot library is installed
/// — the compiler ships it in every build, whatever the flags say — and is in
/// the artifact's call graph. Both values are therefore false of it by the
/// schema's own text, which makes a propagated non-runtime scope a
/// conformance defect rather than a debatable reading.
///
/// So such a package belongs in the immune set beside the runtime-reachable
/// ones: the propagation should never reach it, rather than reaching it and
/// being corrected afterwards. The difference is visible in
/// `PropagationSummary` — immune, never assigned — and that is what C3
/// asserts.
///
/// `boot` is the nulled-name set from the pinned revision's own compiler
/// configuration, carried here by `EnrichmentSummary::boot_libraries`. It is
/// **empty** when that configuration could not be read, which exempts nothing
/// (FR-005): unearned immunity is the dangerous default.
fn compiler_supplied<'a>(
    components: &'a [ResolvedComponent],
    boot: &HashSet<String>,
) -> HashSet<&'a str> {
    if boot.is_empty() {
        return HashSet::new();
    }
    components
        .iter()
        .filter(|c| is_haskell_purl(c.purl.as_str()) && boot.contains(c.name.as_str()))
        .map(|c| c.purl.as_str())
        .collect()
}

/// Whether a PURL names a Hackage package.
///
/// The nulled set holds bare package names, which are not unique across
/// ecosystems — `binary`, `array`, `unix`, `directory` and `pretty` are all in
/// it, and all are plausible names elsewhere. Matching on the name alone
/// would exempt an npm or Go package that happens to share one, which is the
/// over-exemption failure mode C4 exists to catch. This is the same
/// two-condition discipline milestone 1275 used for closure identity, where a
/// name match alone would have minted `pkg:hackage/bash@5.3p9` for GNU bash.
fn is_haskell_purl(purl: &str) -> bool {
    purl.starts_with("pkg:hackage/")
}

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
    // Milestone 1276 — the packages the pinned revision's compiler supplies.
    // Empty when unknown, which exempts nothing (FR-005).
    boot_libraries: &HashSet<String>,
) -> PropagationSummary {
    assert_no_anchoring_edges(relationships);

    let adj = declared_adjacency(relationships);

    // Phase A, and the borrow of `components` ends with it: the
    // assignment map below is keyed by owned PURL strings so the
    // mutation pass can take `&mut`.
    // Milestone 1276 (#1340) — packages the compiler ships are immune.
    //
    // Computed before the loop and merged into the runtime-reachable immunity
    // rather than filtered at assignment time, so the two are one concept at
    // the point of decision: the propagation does not reach these components.
    // The counts stay separate in the summary, because "a runtime path
    // reaches this" and "no path has to" answer different questions.
    let exempt = compiler_supplied(components, boot_libraries);
    let immune_compiler_supplied = exempt.len();

    let (immune, assigned, skipped_unrooted): (usize, HashMap<String, LifecycleScope>, usize) = {
        let mut immune = runtime_reachable(components, &adj, seeds);
        immune.extend(exempt.iter().copied());

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
        let mut skipped_unrooted: HashSet<&str> = HashSet::new();
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
                    skipped_unrooted.insert(purl);
                    continue; // FR-012
                }
                assigned
                    .entry((*purl).to_string())
                    .or_insert_with(|| *scope); // FR-004, by iteration order
            }
        }
        (immune.len(), assigned, skipped_unrooted.len())
    };

    let mut rooted: Vec<String> = ecosystems_with_root.iter().cloned().collect();
    rooted.sort();
    let mut summary = PropagationSummary {
        immune_compiler_supplied,
        immune,
        seeded_from: components
            .iter()
            .filter(|c| c.lifecycle_scope.is_some_and(|s| s.is_non_runtime()))
            .count(),
        skipped_unrooted_ecosystem: skipped_unrooted,
        rooted_ecosystems: rooted,
        ..Default::default()
    };
    if assigned.is_empty() {
        // FR-013: nothing to assign means nothing CHANGES — no scope,
        // no annotation, byte-identical output. It does not mean
        // nothing is worth saying. A pass that is silent when it
        // declines is indistinguishable from a pass that found
        // nothing to do, and the difference is the whole of a
        // diagnosis: three corpus targets were declined under FR-012
        // and the only way to learn that was to read the readers.
        tracing::info!(
            immune = summary.immune,
            seeded_from = summary.seeded_from,
            skipped_unrooted_ecosystem = summary.skipped_unrooted_ecosystem,
            rooted_ecosystems = ?summary.rooted_ecosystems,
            "non-runtime lifecycle scope propagation assigned nothing",
        );
        return summary;
    }

    for component in components.iter_mut() {
        if let Some(scope) = assigned.get(component.purl.as_str()) {
            // C62's enum is shared with the Go readers' two derivation
            // values. They can never collide: both of those writers set
            // the scope and the derivation together, and `assigned`
            // holds only components whose `lifecycle_scope` was `None`.
            // Asserted rather than reasoned about, because the argument
            // lives in another module and would not travel with a
            // change to it.
            debug_assert!(
                !component.extra_annotations.contains_key(ANN_SCOPE_DERIVATION),
                "{}: would overwrite an existing scope derivation \
                 ({:?}) — a reader assigned a derivation without a \
                 scope, which breaks C62's one-value-per-component \
                 contract",
                component.purl.as_str(),
                component.extra_annotations.get(ANN_SCOPE_DERIVATION),
            );
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
        skipped_unrooted_ecosystem = summary.skipped_unrooted_ecosystem,
        rooted_ecosystems = ?summary.rooted_ecosystems,
        "propagated non-runtime lifecycle scope to transitive dependencies",
    );
    summary
}

/// #1283 (invariant I9) — withdraw a non-runtime scope a reader assigned
/// directly to a package the pinned compiler supplies.
///
/// Milestone 1276 made such a package immune to *inheriting* a scope and left
/// the direct origin without a suppression path, because its population was
/// zero; invariant I9 was written to say where the mark should be suppressed
/// when one appeared. On the 1283 corpus run it was one: slack-web's test
/// suite declares `template-haskell`, so the Haskell reader scoped it
/// `development`, and CycloneDX called a package GHC ships in every build
/// `excluded`. The reasoning is the propagation's own (see
/// [`compiler_supplied`]): CycloneDX 1.6 makes `excluded` and `optional` false
/// of an installed package, whichever stanza named it.
///
/// Runs **before** the propagation, so a withdrawn mark seeds nothing. The
/// scope becomes absent, not `Runtime`, matching the boot libraries that
/// never carried one (C2). The reader-side edge typing has already run, so
/// the edges into the component are returned to `DependsOn`; otherwise SPDX
/// 2.3 would keep `DEV_DEPENDENCY_OF` beside a CycloneDX `required`, and the
/// formats would disagree about one component.
///
/// The same `boot` set as the propagation's immunity and the closure-role
/// exemption, read from the pinned revision's compiler configuration; empty
/// when unknown, which withdraws nothing (FR-005). Returns how many marks it
/// withdrew.
pub(crate) fn withdraw_compiler_supplied_scope(
    components: &mut [ResolvedComponent],
    relationships: &mut [Relationship],
    boot_libraries: &HashSet<String>,
) -> usize {
    let exempt: HashSet<String> = compiler_supplied(components, boot_libraries)
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut withdrawn: HashSet<String> = HashSet::new();
    for c in components.iter_mut() {
        if exempt.contains(c.purl.as_str()) && c.lifecycle_scope.is_some_and(|s| s.is_non_runtime()) {
            c.lifecycle_scope = None;
            // C62 describes a scope; with none there is nothing to derive.
            c.extra_annotations.remove(ANN_SCOPE_DERIVATION);
            withdrawn.insert(c.purl.as_str().to_string());
        }
    }
    if withdrawn.is_empty() {
        return 0;
    }
    use waybill_common::resolution::RelationshipType;
    for r in relationships.iter_mut().filter(|r| withdrawn.contains(&r.to)) {
        if matches!(
            r.relationship_type,
            RelationshipType::DevDependsOn
                | RelationshipType::BuildDependsOn
                | RelationshipType::TestDependsOn
                | RelationshipType::OptionalDependsOn
        ) {
            r.relationship_type = RelationshipType::DependsOn;
        }
    }
    tracing::info!(
        withdrawn = withdrawn.len(),
        "withdrew a reader-assigned non-runtime scope from packages the compiler supplies",
    );
    withdrawn.len()
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

    fn boot_set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    // ---- milestone 1276 (#1340): the propagation ----

    /// C2 / FR-001 — a compiler-supplied component reachable only through
    /// non-runtime edges is not assigned a non-runtime scope, and carries no
    /// scope at all.
    ///
    /// Both halves. "No non-runtime scope" is also satisfied by an explicit
    /// `required`, which would pass every other contract while making these
    /// components inconsistent with the 68 boot libraries that already carry
    /// none; the spec's Assumptions choose absence and this enforces it.
    #[test]
    fn m1276_a_compiler_supplied_component_is_not_scoped() {
        let mut comps = vec![
            mk_component("pkg:hackage/root@1.0.0"),
            scoped("pkg:hackage/testdep@1.0.0", LifecycleScope::Test),
            mk_component("pkg:hackage/pretty@1.1.3.6"),
        ];
        let rels = vec![
            mk_rel("pkg:hackage/root@1.0.0", "pkg:hackage/testdep@1.0.0"),
            mk_rel("pkg:hackage/testdep@1.0.0", "pkg:hackage/pretty@1.1.3.6"),
        ];
        let summary = propagate_non_runtime_scope(
            &mut comps,
            &rels,
            &seeds(&["pkg:hackage/root@1.0.0"]),
            &ecosystems(&["hackage"]),
            &boot_set(&["pretty"]),
        );
        assert_eq!(
            scope_of(&comps, "pkg:hackage/pretty@1.1.3.6"),
            None,
            "the compiler-supplied component carries a scope"
        );
        assert!(
            summary.immune_compiler_supplied >= 1,
            "the component was not counted as immune; summary={summary:?}"
        );
    }

    /// C3 / FR-002 — the exemption is immunity, not repair.
    ///
    /// A pass that assigned the scope and then removed it would produce the
    /// same document and a different summary, and the summary is the only
    /// thing that distinguishes them.
    #[test]
    fn m1276_the_exemption_is_immunity_not_repair() {
        let mut comps = vec![
            mk_component("pkg:hackage/root@1.0.0"),
            scoped("pkg:hackage/testdep@1.0.0", LifecycleScope::Test),
            mk_component("pkg:hackage/pretty@1.1.3.6"),
        ];
        let rels = vec![
            mk_rel("pkg:hackage/root@1.0.0", "pkg:hackage/testdep@1.0.0"),
            mk_rel("pkg:hackage/testdep@1.0.0", "pkg:hackage/pretty@1.1.3.6"),
        ];
        let summary = propagate_non_runtime_scope(
            &mut comps,
            &rels,
            &seeds(&["pkg:hackage/root@1.0.0"]),
            &ecosystems(&["hackage"]),
            &boot_set(&["pretty"]),
        );
        assert_eq!(summary.immune_compiler_supplied, 1);
        assert_eq!(
            summary.assigned_by_scope.values().sum::<usize>(),
            0,
            "a scope was assigned and then removed rather than never assigned"
        );
    }

    /// C4 / FR-003 — the control. A component that is NOT compiler-supplied,
    /// reachable only through non-runtime edges, still receives the scope.
    ///
    /// This is the test for the mirror-image defect. An over-exempting
    /// implementation makes every component below a disabled stanza look
    /// runtime, which is this feature's failure mode rather than a
    /// hypothetical one: measured, 8 such components on `slack-web` must keep
    /// their scope.
    #[test]
    fn m1276_a_non_boot_component_still_receives_the_scope() {
        let mut comps = vec![
            mk_component("pkg:hackage/root@1.0.0"),
            scoped("pkg:hackage/testdep@1.0.0", LifecycleScope::Test),
            mk_component("pkg:hackage/clock@0.8.4"),
        ];
        let rels = vec![
            mk_rel("pkg:hackage/root@1.0.0", "pkg:hackage/testdep@1.0.0"),
            mk_rel("pkg:hackage/testdep@1.0.0", "pkg:hackage/clock@0.8.4"),
        ];
        propagate_non_runtime_scope(
            &mut comps,
            &rels,
            &seeds(&["pkg:hackage/root@1.0.0"]),
            &ecosystems(&["hackage"]),
            &boot_set(&["pretty"]),
        );
        assert!(
            scope_of(&comps, "pkg:hackage/clock@0.8.4")
                .is_some_and(|s| s.is_non_runtime()),
            "a non-boot component lost its non-runtime scope: over-exemption"
        );
    }

    /// C5 / FR-006 — every exempted component is one C170 explains.
    ///
    /// No new property: the exemption's basis is already on the wire. Both
    /// the exempt class and C170 derive from the same nulled-name set through
    /// `classify()`, so they coincide — and research R3 argued that while the
    /// plan recorded that an argument is not a measurement. T007 measured it
    /// and found the apparent counterexamples were an artefact of probing a
    /// set the code never forms (findings F2). This asserts the property the
    /// predicate actually guarantees: an exempted component is a Hackage
    /// component whose name the revision nulls, which is exactly the
    /// condition under which the reader emits C170.
    #[test]
    fn m1276_the_exempt_class_is_the_class_c170_describes() {
        let comps = vec![
            mk_component("pkg:hackage/pretty@1.1.3.6"),
            mk_component("pkg:hackage/aeson@2.2.4.1"),
            mk_component("pkg:npm/pretty@2.0.0"),
        ];
        let exempt = compiler_supplied(&comps, &boot_set(&["pretty"]));
        // Exactly the Hackage component whose name is nulled: the same two
        // conditions `classify()` uses before emitting `compiler-supplied`.
        assert_eq!(exempt.len(), 1);
        assert!(exempt.contains("pkg:hackage/pretty@1.1.3.6"));
        assert!(
            !exempt.contains("pkg:npm/pretty@2.0.0"),
            "a component the reader would never mark compiler-supplied was exempted"
        );
    }

    /// C7 / FR-005 — a scan with no compiler configuration exempts nothing,
    /// **even where names collide**.
    ///
    /// Asserted against a real corpus shape rather than an invented one
    /// (findings F2). `haskell-aeson` carries **zero** C170 components,
    /// because the nixpkgs-haskell pass never ran for it, while 11 of its
    /// components share names with a nulled set and one of them —
    /// `directory` — is non-runtime. A predicate fed a set assembled any
    /// other way than from the scan's own pass would wrongly exempt it.
    ///
    /// This is the fail-closed direction for this feature: unearned immunity
    /// is the dangerous default, because it reports a genuinely test-only
    /// component as present in the artifact.
    #[test]
    fn m1276_no_compiler_configuration_exempts_nothing_even_on_name_collisions() {
        let comps = vec![
            // haskell-aeson's shape: Hackage components whose names are in
            // some revision's nulled set, in a scan that learned nothing
            // about any revision.
            mk_component("pkg:hackage/directory@1.3.8.5"),
            mk_component("pkg:hackage/binary@0.8.9.2"),
            mk_component("pkg:hackage/deepseq@1.5.1.0"),
        ];
        let exempt = compiler_supplied(&comps, &HashSet::new());
        assert!(
            exempt.is_empty(),
            "a scan with no compiler configuration exempted {} component(s)",
            exempt.len()
        );
    }

    // ---- milestone 1276 (#1340): the exempt class ----

    #[test]
    fn m1276_a_nulled_hackage_package_is_compiler_supplied() {
        let comps = vec![mk_component("pkg:hackage/pretty@1.1.3.6")];
        let exempt = compiler_supplied(&comps, &boot_set(&["pretty"]));
        assert_eq!(exempt.len(), 1);
        assert!(exempt.contains("pkg:hackage/pretty@1.1.3.6"));
    }

    #[test]
    fn m1276_a_hackage_package_the_revision_does_not_null_is_not() {
        let comps = vec![mk_component("pkg:hackage/aeson@2.2.4.1")];
        assert!(compiler_supplied(&comps, &boot_set(&["pretty"])).is_empty());
    }

    #[test]
    fn m1276_a_same_named_package_in_another_ecosystem_is_not() {
        // The nulled set holds bare names, and `binary`, `array`, `unix`,
        // `directory` and `pretty` are all in it at the measured revision --
        // every one a plausible name elsewhere. Matching on the name alone
        // would exempt these, which is the over-exemption failure mode C4
        // catches at document scale and this catches at the predicate.
        let comps = vec![
            mk_component("pkg:npm/pretty@2.0.0"),
            mk_component("pkg:golang/directory@1.0.0"),
            mk_component("pkg:cargo/binary@0.1.0"),
        ];
        assert!(
            compiler_supplied(&comps, &boot_set(&["pretty", "directory", "binary"])).is_empty(),
            "a bare-name match exempted a package from another ecosystem"
        );
    }

    #[test]
    fn m1276_an_empty_boot_set_exempts_nothing() {
        // FR-005. The compiler configuration could not be read, so nothing is
        // known to be compiler-supplied. Unearned immunity is the dangerous
        // default, and this is the arm that refuses it.
        let comps = vec![mk_component("pkg:hackage/pretty@1.1.3.6")];
        assert!(compiler_supplied(&comps, &HashSet::new()).is_empty());
    }

    #[test]
    fn m1276_the_versionless_boot_shape_is_recognised() {
        // A boot library is routinely emitted versionless, because the
        // revision gives it no version to resolve -- `pkg:hackage/directory`
        // with no `@`. That is the shape `nix-closure-moat` carries, so a
        // predicate keyed on a versioned PURL would miss the real population.
        let comps = vec![mk_component("pkg:hackage/directory")];
        let exempt = compiler_supplied(&comps, &boot_set(&["directory"]));
        assert_eq!(exempt.len(), 1, "the versionless boot shape was not matched");
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
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
            &HashSet::new(),
        );
        assert_eq!(
            s,
            PropagationSummary {
                rooted_ecosystems: vec!["npm".to_string()],
                ..Default::default()
            }
        );

        let mut all_runtime = vec![mk_component(ROOT), mk_component(DEVDEP)];
        let s = propagate_non_runtime_scope(
            &mut all_runtime,
            &[mk_rel(ROOT, DEVDEP)],
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
            &HashSet::new(),
        );
        assert_eq!(s.assigned_total(), 0);
        assert_eq!(s.seeded_from, 0);
    }

    /// FR-007, FR-008, SC-006: the C62 marker's domain is exactly the
    /// set this pass assigned.
    ///
    /// Absence is the load-bearing half of the contract. A consumer
    /// reads a non-runtime scope with no marker as "a reader declared
    /// this", so a marker that leaked onto a declared component would
    /// not merely be noise — it would assert the opposite of the
    /// truth. The three populations are checked in one test because
    /// the claim is about the partition, not about any one of them:
    /// declared non-runtime, runtime, and inferred non-runtime, all in
    /// a single document so no population can be checked in isolation
    /// and pass vacuously.
    #[test]
    fn the_derivation_marker_marks_only_what_this_pass_assigned() {
        const DECLARED_DEV: &str = "pkg:npm/waybill-fixture-declared-dev@1.0.0";
        const INFERRED_DEV: &str = "pkg:npm/waybill-fixture-inferred-dev@1.0.0";

        let mut components = vec![
            mk_component(ROOT),
            // runtime by reachability from the root
            mk_component(SHARED),
            // non-runtime because a reader said so
            scoped(DECLARED_DEV, LifecycleScope::Development),
            // non-runtime because this pass infers it
            mk_component(INFERRED_DEV),
        ];
        let relationships = vec![
            mk_rel(ROOT, SHARED),
            mk_rel(ROOT, DECLARED_DEV),
            mk_rel(DECLARED_DEV, INFERRED_DEV),
        ];

        let summary = propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm"]),
            &HashSet::new(),
        );

        assert_eq!(summary.assigned_total(), 1, "exactly one inference");

        assert_eq!(
            derivation_of(&components, INFERRED_DEV),
            Some(DERIVATION_TRANSITIVE.to_string()),
            "the inferred component carries the marker"
        );
        assert_eq!(
            derivation_of(&components, DECLARED_DEV),
            None,
            "a reader-declared non-runtime scope carries NO marker — its              absence is what identifies it as declared (FR-008)"
        );
        assert_eq!(
            derivation_of(&components, SHARED),
            None,
            "a runtime component is outside the marker's domain"
        );
        assert_eq!(
            derivation_of(&components, ROOT),
            None,
            "the subject is outside the marker's domain"
        );

        // Stated as a set rather than per-component, so a future
        // population cannot be added without this failing.
        let marked: Vec<&str> = components
            .iter()
            .filter(|c| c.extra_annotations.contains_key(ANN_SCOPE_DERIVATION))
            .map(|c| c.purl.as_str())
            .collect();
        assert_eq!(marked, vec![INFERRED_DEV]);
    }

    /// C62's enum is shared: the Go readers write `test-only-closure`
    /// and `go-mod-why` into the same key. A component may carry at
    /// most one value, and this pass must not be the one that breaks
    /// that.
    ///
    /// It cannot, because both Go writers set the scope and the
    /// derivation together and this pass assigns only where the scope
    /// is absent. That argument lives in another module, so it is
    /// pinned here instead of trusted.
    #[test]
    fn a_component_with_an_existing_derivation_is_not_touched() {
        const GO_TEST_ONLY: &str = "pkg:golang/waybill-fixture/gotest@v1.0.0";

        let mut go_component = mk_component(GO_TEST_ONLY);
        go_component.lifecycle_scope = Some(LifecycleScope::Test);
        go_component.extra_annotations.insert(
            ANN_SCOPE_DERIVATION.to_string(),
            serde_json::Value::String("test-only-closure".to_string()),
        );

        let mut components = vec![
            mk_component(ROOT),
            scoped(DEVTOOL, LifecycleScope::Development),
            go_component,
        ];
        // The Go component is reached from a Development seed, so
        // without the scope guard Phase B would claim it.
        let relationships = vec![mk_rel(ROOT, DEVTOOL), mk_rel(DEVTOOL, GO_TEST_ONLY)];

        propagate_non_runtime_scope(
            &mut components,
            &relationships,
            &seeds(&[ROOT]),
            &ecosystems(&["npm", "golang"]),
            &HashSet::new(),
        );

        assert_eq!(
            scope_of(&components, GO_TEST_ONLY),
            Some(LifecycleScope::Test),
            "FR-003: the reader's Test scope survives a Development seed"
        );
        assert_eq!(
            derivation_of(&components, GO_TEST_ONLY).as_deref(),
            Some("test-only-closure"),
            "and so does the reader's C62 value — overwriting it with              `transitive` would relabel a Go test-closure inference as              this pass's work"
        );
    }

    // ---- #1283 (invariant I9): a reader's own mark on a compiler-supplied package ----

    fn typed(from: &str, to: &str, ty: RelationshipType) -> Relationship {
        let mut r = mk_rel(from, to);
        r.relationship_type = ty;
        r
    }

    const HS_ROOT: &str = "pkg:hackage/root@1.0.0";
    const TH: &str = "pkg:hackage/template-haskell";
    const HSPEC: &str = "pkg:hackage/hspec@2.11.9";

    /// slack-web's shape: its test suite declares `template-haskell` and
    /// `hspec`, and only the first is supplied by the compiler.
    fn test_suite_declares_boot_library() -> (Vec<ResolvedComponent>, Vec<Relationship>) {
        (
            vec![
                mk_component(HS_ROOT),
                scoped(TH, LifecycleScope::Development),
                scoped(HSPEC, LifecycleScope::Development),
            ],
            vec![
                typed(HS_ROOT, TH, RelationshipType::DevDependsOn),
                typed(HS_ROOT, HSPEC, RelationshipType::DevDependsOn),
            ],
        )
    }

    #[test]
    fn m1283_a_reader_scope_on_a_compiler_supplied_package_is_withdrawn() {
        let (mut comps, mut rels) = test_suite_declares_boot_library();
        let n = withdraw_compiler_supplied_scope(&mut comps, &mut rels, &boot_set(&["template-haskell"]));
        assert_eq!(n, 1);
        assert_eq!(scope_of(&comps, TH), None, "absent, like every boot library (C2)");
        assert_eq!(
            rels[0].relationship_type,
            RelationshipType::DependsOn,
            "the edge the reader typed agrees with the component, or SPDX 2.3 \
             keeps DEV_DEPENDENCY_OF beside a CycloneDX `required`"
        );
        // CONTROL: a test dependency the compiler does not supply keeps both.
        assert_eq!(scope_of(&comps, HSPEC), Some(LifecycleScope::Development));
        assert_eq!(rels[1].relationship_type, RelationshipType::DevDependsOn);
    }

    /// FR-005: an unknown compiler configuration withdraws nothing.
    #[test]
    fn m1283_an_unknown_boot_set_withdraws_nothing() {
        let (mut comps, mut rels) = test_suite_declares_boot_library();
        assert_eq!(withdraw_compiler_supplied_scope(&mut comps, &mut rels, &boot_set(&[])), 0);
        assert_eq!(scope_of(&comps, TH), Some(LifecycleScope::Development));
        assert_eq!(rels[0].relationship_type, RelationshipType::DevDependsOn);
    }

    /// A boot library's name outside Hackage is a different package (C4).
    #[test]
    fn m1283_a_boot_name_outside_hackage_is_not_withdrawn() {
        let npm = "pkg:npm/template-haskell@1.0.0";
        let mut comps = vec![mk_component(HS_ROOT), scoped(npm, LifecycleScope::Development)];
        let mut rels = vec![typed(HS_ROOT, npm, RelationshipType::DevDependsOn)];
        assert_eq!(
            withdraw_compiler_supplied_scope(&mut comps, &mut rels, &boot_set(&["template-haskell"])),
            0
        );
        assert_eq!(scope_of(&comps, npm), Some(LifecycleScope::Development));
    }
}
