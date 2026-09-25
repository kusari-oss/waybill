//! Transitive runtime closure over the pinned nixpkgs package set
//! (milestone 985, issue #962).
//!
//! Milestone 926 resolves a Nix-built project's **declared** Haskell
//! dependencies against the nixpkgs revision its `flake.lock` pins. The
//! artifact it substitutes for — a `cabal.project.freeze` — carries the
//! transitive closure, so that substitution is partial. This module closes
//! the gap by walking the dependency relations already present in the
//! package-set file milestone 926 downloads and caches.
//!
//! # Scope: the runtime closure, and why only that
//!
//! Walks `libraryHaskellDepends` + `executableHaskellDepends`, not
//! `testHaskellDepends` or `benchmarkHaskellDepends` (FR-002).
//!
//! The reason is evidential rather than aesthetic. Parsing those two fields
//! reproduces what `nix eval` reports for `propagatedBuildInputs` — exactly,
//! at 167 components on one measured project — so an implementation can be
//! checked against something outside waybill's own assumptions. A test
//! closure has no such oracle, and it is far larger: measured multipliers are
//! 1.5–3.8× for the runtime closure against 7.3–9.8× including tests.
//! Committing to the larger number on a set nothing can verify is not
//! supportable. Deferred to issue #985.
//!
//! # Termination
//!
//! Hackage package sets contain mutually recursive relations. The walk marks
//! `seen` **before** enqueue, so a cycle revisits nothing (FR-010).
//!
//! # What this module must not repeat
//!
//! Milestone 980: version resolution rewrote component PURLs after the
//! dependency edges were built, and the PURL is the identity the graph keys
//! on — disconnecting every component it resolved, 66% of one project's SPDX
//! relationships. The closure multiplies both component and edge counts, so
//! the same mistake would be larger. Edges produced here name attributes; any
//! later identity rewrite must rewrite these endpoints in the same step
//! (`apply_renames`, added in #981).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Whether a component was named by the project or reached through another.
///
/// A typed enum rather than a bare string (Principle IV). Serialized only at
/// the emission boundary.
///
/// Deliberately NOT derived from the component's position in the dependency
/// graph. CycloneDX's primary-dependency fallback (milestone 894) synthesizes
/// a root edge to every unreferenced component when the root has no declared
/// outgoing edges; under that fallback every closure member would read as
/// declared. Graph position is unreliable here, so the origin is recorded
/// explicitly (FR-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ComponentOrigin {
    /// Named by the project's own manifest.
    Declared,
    /// Reached only through another package's runtime relations.
    Transitive,
}

impl ComponentOrigin {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Transitive => "transitive",
        }
    }
}

/// A package the walk reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosureMember {
    /// Attribute name in the pinned package set.
    pub(crate) name: String,
    pub(crate) origin: ComponentOrigin,
    /// Attributes whose relations named this one. Drives edge emission, and
    /// is a set so one package reached by several parents yields several
    /// edges and exactly one component (FR-012).
    ///
    /// `BTreeSet` rather than `HashSet` because output must be byte-identical
    /// across runs (FR-013); iteration order is part of the contract.
    pub(crate) reached_from: BTreeSet<String>,
}

/// A relation to emit into the dependency graph.
///
/// Endpoints are **attribute names** at construction. If component identities
/// are rewritten afterwards — which version assignment does — these must be
/// rewritten in the same step. See the module docs on milestone 980.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ClosureEdge {
    pub(crate) from: String,
    pub(crate) to: String,
}

/// What the walk did, for the document-scope record (FR-014).
///
/// Milestone 973 exists because milestone 926 computed exactly this kind of
/// record, logged it, and dropped it — leaving a document that could not
/// distinguish "the pass ran and resolved 97" from "the pass never ran". This
/// must reach the document, not a `tracing::info!`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClosureSummary {
    pub(crate) declared: usize,
    pub(crate) transitive: usize,
    /// Reason → count, reusing milestone 926's closed reason vocabulary.
    pub(crate) unresolved: BTreeMap<String, usize>,
    /// Relations traversed. Separates "few components because the project is
    /// small" from "few components because the walk stopped early" — two
    /// states with identical component counts and different causes (FR-014).
    pub(crate) relations_walked: usize,
}

impl ClosureSummary {
    /// Render as the C175 document-scope value (#962).
    ///
    /// Keys sort through `serde_json::Map`, which is a `BTreeMap` here (no
    /// `preserve_order` feature), so the string is stable and a golden can
    /// pin it.
    ///
    /// This must reach the DOCUMENT, not a log line. Milestone 973 exists
    /// because milestone 926 computed exactly this kind of record, logged it,
    /// and dropped it — leaving a document that could not distinguish "the
    /// pass ran and resolved 97" from "the pass never ran".
    pub(crate) fn to_document_value(&self) -> serde_json::Value {
        serde_json::json!({
            "declared": self.declared,
            "transitive": self.transitive,
            "unresolved": self.unresolved,
            "relations-walked": self.relations_walked,
        })
    }
}

/// The walk's output.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClosureResult {
    pub(crate) members: Vec<ClosureMember>,
    pub(crate) edges: Vec<ClosureEdge>,
    pub(crate) summary: ClosureSummary,
}

/// What the walk needs to know about one attribute, supplied by the caller so
/// this module does not depend on the package-set representation.
pub(crate) struct RelationLookup<'a> {
    /// Runtime relations of an attribute: library + executable, already
    /// merged. Returns `None` when the attribute is absent from the set.
    pub(crate) relations_of: &'a dyn Fn(&str) -> Option<Vec<String>>,
    /// Is this name supplied by the compiler rather than built from Hackage?
    pub(crate) is_boot: &'a dyn Fn(&str) -> bool,
}

/// Walk the runtime closure of `declared`.
///
/// Breadth-first, `seen` checked before enqueue so cycles terminate. A boot
/// library is recorded but **not traversed**: its dependencies are a property
/// of the compiler, not of the nixpkgs entry, so walking its relation list
/// would attribute relations the build does not have (FR-011).
///
/// A name absent from the package set is recorded as a member — it is a
/// dependency that genuinely exists, having been read from another package's
/// relation list — but contributes no relations of its own. The caller
/// classifies it and emits it versionless with a reason (FR-005a).
pub(crate) fn walk(declared: &[String], lookup: &RelationLookup<'_>) -> ClosureResult {
    let declared_set: BTreeSet<&str> = declared.iter().map(String::as_str).collect();

    let mut reached_from: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut edges: BTreeSet<ClosureEdge> = BTreeSet::new();
    let mut relations_walked = 0usize;

    for name in declared {
        if seen.insert(name.clone()) {
            queue.push_back(name.clone());
        }
        reached_from.entry(name.clone()).or_default();
    }

    while let Some(current) = queue.pop_front() {
        // FR-011 — a compiler-supplied package is a leaf here.
        if (lookup.is_boot)(&current) {
            continue;
        }
        let Some(relations) = (lookup.relations_of)(&current) else {
            // Absent from the package set: a real dependency we cannot expand.
            continue;
        };
        for dep in relations {
            if dep == current {
                // A self-reference adds no information and no edge.
                continue;
            }
            relations_walked += 1;
            edges.insert(ClosureEdge { from: current.clone(), to: dep.clone() });
            reached_from.entry(dep.clone()).or_default().insert(current.clone());
            // `seen` before enqueue: this is what terminates a cycle (FR-010).
            if seen.insert(dep.clone()) {
                queue.push_back(dep);
            }
        }
    }

    let members: Vec<ClosureMember> = reached_from
        .into_iter()
        .map(|(name, parents)| {
            // FR-007 — reachable both ways is Declared; the stronger claim wins.
            let origin = if declared_set.contains(name.as_str()) {
                ComponentOrigin::Declared
            } else {
                ComponentOrigin::Transitive
            };
            ClosureMember { name, origin, reached_from: parents }
        })
        .collect();

    let declared_count = members.iter().filter(|m| m.origin == ComponentOrigin::Declared).count();
    let transitive_count = members.len() - declared_count;

    ClosureResult {
        summary: ClosureSummary {
            declared: declared_count,
            transitive: transitive_count,
            unresolved: BTreeMap::new(), // filled by the caller, which classifies
            relations_walked,
        },
        members,
        edges: edges.into_iter().collect(),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Drive the walk over a package set shaped like the real one, small
    /// enough to reason about.
    fn run(
        declared: &[&str],
        relations: &[(&str, Vec<&str>)],
        boot: &[&str],
    ) -> ClosureResult {
        let map: HashMap<&str, Vec<&str>> = relations.iter().cloned().collect();
        let rel = |n: &str| {
            map.get(n)
                .map(|v| v.iter().map(|s| (*s).to_string()).collect::<Vec<String>>())
        };
        let is_boot = |n: &str| boot.contains(&n);
        let l = RelationLookup { relations_of: &rel, is_boot: &is_boot };
        let d: Vec<String> = declared.iter().map(|s| (*s).to_string()).collect();
        walk(&d, &l)
    }

    fn names(r: &ClosureResult) -> Vec<&str> {
        r.members.iter().map(|m| m.name.as_str()).collect()
    }

    /// The feature in one assertion: a package the project never declares
    /// appears because something it declares depends on it.
    #[test]
    fn m985_a_transitive_dependency_is_reached() {
        let r = run(&["app"], &[("app", vec!["mid"]), ("mid", vec!["leaf"])], &[]);
        assert!(names(&r).contains(&"leaf"), "got {:?}", names(&r));
        assert_eq!(
            r.members.iter().find(|m| m.name == "leaf").unwrap().origin,
            ComponentOrigin::Transitive
        );
    }

    /// FR-010. A mutually recursive pair must not spin.
    ///
    /// `seen` is checked before enqueue, which is the whole mechanism. Without
    /// it this test does not fail — it hangs, which is why the assertion is on
    /// the result rather than on a flag.
    #[test]
    fn m985_the_walk_terminates_on_a_cycle() {
        let r = run(
            &["app"],
            &[("app", vec!["a"]), ("a", vec!["b"]), ("b", vec!["a"])],
            &[],
        );
        assert_eq!(names(&r), vec!["a", "app", "b"]);
    }

    /// FR-012 / E2.2. Two parents, two edges, one component.
    #[test]
    fn m985_a_package_reached_twice_appears_once_with_two_edges() {
        let r = run(
            &["app"],
            &[("app", vec!["x", "y"]), ("x", vec!["shared"]), ("y", vec!["shared"])],
            &[],
        );
        assert_eq!(names(&r).iter().filter(|n| **n == "shared").count(), 1);
        let shared = r.members.iter().find(|m| m.name == "shared").unwrap();
        assert_eq!(
            shared.reached_from.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["x", "y"]
        );
    }

    /// FR-011. A boot library is recorded but never traversed: its relations
    /// are a property of the compiler, not of the nixpkgs entry, so walking
    /// them would attribute relations the build does not have.
    #[test]
    fn m985_a_boot_library_is_recorded_but_not_traversed() {
        let r = run(
            &["app"],
            &[("app", vec!["base"]), ("base", vec!["never-visited"])],
            &["base"],
        );
        assert!(names(&r).contains(&"base"), "boot lib must still be recorded");
        assert!(
            !names(&r).contains(&"never-visited"),
            "must not walk THROUGH a boot library; got {:?}",
            names(&r)
        );
    }

    /// FR-005a. A name absent from the package set is still a real dependency
    /// — it was read from another package's relation list — so it is recorded
    /// and left for the caller to classify, never dropped.
    #[test]
    fn m985_a_name_absent_from_the_set_is_recorded_not_dropped() {
        let r = run(&["app"], &[("app", vec!["ghost"])], &[]);
        assert!(names(&r).contains(&"ghost"), "got {:?}", names(&r));
    }

    /// FR-007 / E3.2. Reachable both ways is Declared; the stronger claim wins.
    #[test]
    fn m985_a_package_reachable_both_ways_is_declared() {
        let r = run(
            &["app", "both"],
            &[("app", vec!["both"]), ("both", vec![])],
            &[],
        );
        let both = r.members.iter().find(|m| m.name == "both").unwrap();
        assert_eq!(both.origin, ComponentOrigin::Declared);
        // ...and it keeps its inbound edge, so the graph still explains it.
        assert!(both.reached_from.contains("app"));
    }

    /// FR-013. Byte-identical output requires deterministic ordering, and the
    /// walk's order reaches the document.
    #[test]
    fn m985_the_walk_is_deterministic() {
        let spec: &[(&str, Vec<&str>)] =
            &[("app", vec!["z", "a", "m"]), ("z", vec!["deep"]), ("a", vec!["deep"])];
        let first = run(&["app"], spec, &[]);
        for _ in 0..8 {
            assert_eq!(names(&run(&["app"], spec, &[])), names(&first));
            assert_eq!(run(&["app"], spec, &[]).edges, first.edges);
        }
    }

    /// E5.2 / FR-009. An edge names the actual parent, not the root.
    #[test]
    fn m985_an_edge_comes_from_the_actual_parent() {
        let r = run(&["app"], &[("app", vec!["mid"]), ("mid", vec!["leaf"])], &[]);
        assert!(
            r.edges.contains(&ClosureEdge { from: "mid".into(), to: "leaf".into() }),
            "expected mid -> leaf; got {:?}",
            r.edges
        );
        assert!(
            !r.edges.contains(&ClosureEdge { from: "app".into(), to: "leaf".into() }),
            "must not synthesize a root edge to a depth-2 package"
        );
    }

    /// A self-referential relation contributes nothing and no edge.
    #[test]
    fn m985_a_self_reference_yields_no_edge() {
        let r = run(&["app"], &[("app", vec!["app", "real"])], &[]);
        assert!(!r.edges.iter().any(|e| e.from == e.to));
        assert!(names(&r).contains(&"real"));
    }

    /// FR-014. The counts distinguish a small project from a stalled walk.
    #[test]
    fn m985_the_summary_counts_what_the_walk_did() {
        let r = run(&["app"], &[("app", vec!["mid"]), ("mid", vec!["leaf"])], &[]);
        assert_eq!(r.summary.declared, 1);
        assert_eq!(r.summary.transitive, 2);
        assert_eq!(r.summary.relations_walked, 2);
    }
}
