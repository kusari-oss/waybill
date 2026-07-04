//! Milestone 158 — multi-root BFS + ecosystem-root-set derivation.
//!
//! Per contracts/reachability-algorithm.md:
//!
//!   Step 1: build the seed set (primary root + per-ecosystem tops).
//!   Step 2: pick each ecosystem's "top" main-module.
//!   Step 3: multi-source BFS from the seed set.
//!   Step 4: classify (in mod.rs — this file provides Steps 1–3).

use std::collections::{HashMap, HashSet, VecDeque};

use mikebom_common::resolution::{Relationship, ResolvedComponent};

use crate::generate::root_selector::{ResolvedRootSubject, RootSelectionResult};

/// Per contracts/reachability-algorithm.md, the seed set is derived
/// from the primary root (from `select_root`) UNION per-ecosystem
/// tops. Ecosystems where an emitted component exists but no
/// confident root could be picked are recorded separately — they
/// drive the `multi-ecosystem-partial-root` reason code.
pub(super) struct EcosystemRootSet {
    /// The BFS seed set: canonical PURL strings.
    pub roots: HashSet<String>,
    /// Ecosystems represented by at least one emitted component
    /// where mikebom could NOT identify a root.
    pub ecosystems_without_root: Vec<String>,
}

/// Step 2 — pick an ecosystem's "top" main-module. Reuses the
/// milestone-127 workspace-root-first + LCP-fallback ladder in a
/// per-ecosystem scope.
///
/// Deterministic tiebreak on `pkg:` canonical string (lex sort) if
/// both heuristics tie — matches milestone-127's convention.
fn pick_ecosystem_top<'a>(
    candidates: &'a [&'a ResolvedComponent],
) -> Option<&'a ResolvedComponent> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }

    // (a) prefer `mikebom:is-workspace-root == true`. The annotation
    // is stored as `Value::Bool` per `root_selector::is_workspace_root`;
    // absent or non-bool degrades to false.
    let workspace_roots: Vec<&ResolvedComponent> = candidates
        .iter()
        .copied()
        .filter(|c| {
            c.extra_annotations
                .get("mikebom:is-workspace-root")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .collect();
    if workspace_roots.len() == 1 {
        return Some(workspace_roots[0]);
    }
    if workspace_roots.len() > 1 {
        // Deterministic tiebreak among workspace-roots.
        return workspace_roots.into_iter().min_by_key(|c| c.purl.as_str());
    }

    // (b) fall back to deterministic sort of ALL candidates — matches
    // the milestone-127 stable-tiebreak convention when LCP can't
    // distinguish.
    candidates.iter().copied().min_by_key(|c| c.purl.as_str())
}

/// Step 1 — derive the seed set from `components[]` + `selection`.
pub(super) fn build_ecosystem_root_set(
    components: &[ResolvedComponent],
    selection: &RootSelectionResult,
) -> EcosystemRootSet {
    let mut roots: HashSet<String> = HashSet::new();
    let mut per_ecosystem_root: HashMap<String, String> = HashMap::new();

    // Primary root (from milestone-127 select_root).
    if let ResolvedRootSubject::MainModule(idx) = &selection.subject {
        if let Some(c) = components.get(*idx) {
            let key = c.purl.as_str().to_string();
            roots.insert(key.clone());
            per_ecosystem_root.insert(c.purl.ecosystem().to_string(), key);
        }
    }

    // Per-ecosystem main-modules — group by ecosystem.
    let main_modules: Vec<&ResolvedComponent> = components
        .iter()
        .filter(|c| {
            c.extra_annotations
                .get("mikebom:component-role")
                .and_then(|v| v.as_str())
                == Some("main-module")
        })
        .collect();
    let mut by_ecosystem: HashMap<String, Vec<&ResolvedComponent>> = HashMap::new();
    for c in main_modules {
        by_ecosystem
            .entry(c.purl.ecosystem().to_string())
            .or_default()
            .push(c);
    }

    for (ecosystem, mods) in &by_ecosystem {
        if per_ecosystem_root.contains_key(ecosystem) {
            continue;
        }
        if let Some(top) = pick_ecosystem_top(mods) {
            let key = top.purl.as_str().to_string();
            roots.insert(key.clone());
            per_ecosystem_root.insert(ecosystem.clone(), key);
        }
    }

    // Determine ecosystems that have emitted components but no root.
    // Deterministic order via sort.
    let mut ecosystems_without_root: Vec<String> = components
        .iter()
        .map(|c| c.purl.ecosystem().to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|e| !per_ecosystem_root.contains_key(e))
        .collect();
    ecosystems_without_root.sort();

    EcosystemRootSet {
        roots,
        ecosystems_without_root,
    }
}

/// Step 3 — multi-source BFS. Starts from `seeds` and walks
/// `edges` (outbound adjacency). Returns the visited set.
///
/// O(V + E). Cycles are handled naturally by the visited HashSet.
/// Components in `seeds` that don't appear in `edges` are still
/// counted as visited (they have zero outbound edges).
pub(super) fn multi_source_bfs(
    seeds: &HashSet<String>,
    edges: &HashMap<String, Vec<String>>,
) -> HashSet<String> {
    let mut visited: HashSet<String> = seeds.clone();
    let mut queue: VecDeque<String> = seeds.iter().cloned().collect();

    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = edges.get(&node) {
            for neighbor in neighbors {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    visited
}

/// Build an adjacency map from `Relationship[]` — filters to
/// dependency-type edges (`DependsOn` variants) and keys by
/// `from`-PURL.
pub(super) fn build_edge_adjacency(relationships: &[Relationship]) -> HashMap<String, Vec<String>> {
    use mikebom_common::resolution::RelationshipType as RT;
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for rel in relationships {
        // Only DependsOn (and its lifecycle-tagged variants) count as
        // graph-completeness edges. Other relationship types (Contains,
        // Describes, GeneratedFrom, ...) don't participate in the
        // dep-graph BFS.
        let is_dep_edge = matches!(
            rel.relationship_type,
            RT::DependsOn | RT::DevDependsOn | RT::BuildDependsOn | RT::TestDependsOn
        );
        if !is_dep_edge {
            continue;
        }
        out.entry(rel.from.clone())
            .or_default()
            .push(rel.to.clone());
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::{
        mk_component, mk_main_module, mk_workspace_root, selection_with_main_module,
    };

    #[test]
    fn bfs_empty_seeds_returns_empty() {
        let seeds: HashSet<String> = HashSet::new();
        let edges: HashMap<String, Vec<String>> = HashMap::new();
        let visited = multi_source_bfs(&seeds, &edges);
        assert!(visited.is_empty());
    }

    #[test]
    fn bfs_single_root_single_ecosystem_matches_naive_traversal() {
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        edges.insert(
            "pkg:npm/root@1.0.0".to_string(),
            vec!["pkg:npm/a@1.0.0".to_string(), "pkg:npm/b@1.0.0".to_string()],
        );
        edges.insert(
            "pkg:npm/a@1.0.0".to_string(),
            vec!["pkg:npm/leaf@1.0.0".to_string()],
        );
        let mut seeds = HashSet::new();
        seeds.insert("pkg:npm/root@1.0.0".to_string());

        let visited = multi_source_bfs(&seeds, &edges);
        assert_eq!(visited.len(), 4);
        assert!(visited.contains("pkg:npm/root@1.0.0"));
        assert!(visited.contains("pkg:npm/a@1.0.0"));
        assert!(visited.contains("pkg:npm/b@1.0.0"));
        assert!(visited.contains("pkg:npm/leaf@1.0.0"));
    }

    #[test]
    fn bfs_two_ecosystem_seeds_return_union() {
        // Q3 multi-root — npm-root reaches 2 npm components, gem-root
        // reaches 1 gem component. Union covers everything.
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        edges.insert(
            "pkg:npm/root@1.0.0".to_string(),
            vec!["pkg:npm/a@1.0.0".to_string()],
        );
        edges.insert(
            "pkg:gem/gemroot@1.0.0".to_string(),
            vec!["pkg:gem/x@1.0.0".to_string()],
        );

        let mut seeds = HashSet::new();
        seeds.insert("pkg:npm/root@1.0.0".to_string());
        seeds.insert("pkg:gem/gemroot@1.0.0".to_string());

        let visited = multi_source_bfs(&seeds, &edges);
        assert_eq!(visited.len(), 4);
    }

    #[test]
    fn pick_ecosystem_top_prefers_workspace_root_marker_over_lcp() {
        let ws_root = mk_workspace_root("pkg:npm/z-last@1.0.0");
        let plain = mk_main_module("pkg:npm/a-first@1.0.0");
        let candidates: Vec<&ResolvedComponent> = vec![&plain, &ws_root];
        let top = pick_ecosystem_top(&candidates).expect("has top");
        // Even though `plain` sorts lex-first, workspace-root marker wins.
        assert_eq!(top.purl.as_str(), "pkg:npm/z-last@1.0.0");
    }

    #[test]
    fn ecosystem_root_set_uses_primary_from_selection() {
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_component("pkg:npm/dep@1.0.0"),
        ];
        let selection = selection_with_main_module(0);
        let set = build_ecosystem_root_set(&components, &selection);
        assert!(set.roots.contains("pkg:npm/root@1.0.0"));
        assert!(set.ecosystems_without_root.is_empty());
    }

    #[test]
    fn ecosystem_root_set_flags_ecosystem_with_no_main_module() {
        // Two ecosystems, but only npm has a main-module component.
        // The gem ecosystem has emitted components but no main-module
        // → gets flagged as ecosystems_without_root.
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_component("pkg:gem/orphan-gem@1.0.0"),
        ];
        let selection = selection_with_main_module(0);
        let set = build_ecosystem_root_set(&components, &selection);
        assert!(set.roots.contains("pkg:npm/root@1.0.0"));
        assert_eq!(set.ecosystems_without_root, vec!["gem".to_string()]);
    }

    #[test]
    fn ecosystem_root_set_adds_per_ecosystem_top_when_no_primary_match() {
        // Primary root is npm; gem has its own main-module — that
        // becomes an additional seed via per-ecosystem lookup.
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_main_module("pkg:gem/gemroot@1.0.0"),
        ];
        let selection = selection_with_main_module(0);
        let set = build_ecosystem_root_set(&components, &selection);
        assert!(set.roots.contains("pkg:npm/root@1.0.0"));
        assert!(set.roots.contains("pkg:gem/gemroot@1.0.0"));
        assert!(set.ecosystems_without_root.is_empty());
    }
}
