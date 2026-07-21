//! Milestone 158 — multi-root BFS + ecosystem-root-set derivation.
//!
//! Per contracts/reachability-algorithm.md:
//!
//!   Step 1: build the seed set (primary root + per-ecosystem tops).
//!   Step 2: pick each ecosystem's "top" main-module.
//!   Step 3: multi-source BFS from the seed set.
//!   Step 4: classify (in mod.rs — this file provides Steps 1–3).

use std::collections::{HashMap, HashSet, VecDeque};

use waybill_common::resolution::{Relationship, ResolvedComponent};

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
///
/// Milestone 192 (FR-001, FR-002): when the primary selection subject
/// is NOT a `MainModule` variant (i.e., the operator supplied
/// `--root-name` producing an `OperatorOverride` root, or the root is
/// a `SyntheticPlaceholder`, or a `MavenCoord`), synthesize per-
/// ecosystem placeholder roots so `ecosystems_without_root` becomes
/// empty. Prevents the `MultiEcosystemPartialRoot` classifier at
/// `mod.rs:250` from over-firing on every operator-override scan
/// where a native main-module wasn't picked as the primary root.
///
/// `target_ref` is the emitted BOM subject identity (CDX
/// `metadata.component.bom-ref`, SPDX 2.3 root Package SPDXID, SPDX 3
/// root `software_Package.spdxId`). Passed in from
/// `compute_graph_completeness` at `mod.rs:156`.
pub(super) fn build_ecosystem_root_set(
    components: &[ResolvedComponent],
    selection: &RootSelectionResult,
    target_ref: &str,
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

    // Milestone 192 (spec FR-001 / FR-002): operator-override
    // synthesis. When the primary selection subject is NOT a
    // MainModule, seed per_ecosystem_root with the operator's
    // target_ref for every ecosystem present in components[] that
    // doesn't already have an entry — so the downstream
    // MultiEcosystemPartialRoot classifier at mod.rs:250 doesn't
    // fire spuriously. Per Q2 answer A: skip the ecosystem that
    // matches the target_ref's own PURL type (avoids duplicate root
    // when the operator passed `--root-purl-type <eco>`).
    //
    // Byte-identity guard: on the native-root (MainModule) path this
    // block is a no-op — the existing per-ecosystem-main-module loop
    // above already populates per_ecosystem_root correctly. Zero
    // delta on any golden generated from a native-root scan.
    let is_native_root =
        matches!(selection.subject, ResolvedRootSubject::MainModule(_));
    if !is_native_root {
        // Per Q2 answer A: parse the target_ref's PURL ecosystem so
        // when the operator picked `--root-purl-type <eco>`, we can
        // recognize that ecosystem as "covered by the operator's own
        // root PURL" rather than by our synthesis. The distinction
        // matters ONLY for the observability count — the operator's
        // root IS the per-ecosystem root for its own ecosystem, so
        // we still insert it into `per_ecosystem_root` (otherwise
        // `ecosystems_without_root` at the end of this function would
        // spuriously include the operator's own ecosystem).
        let operator_root_ecosystem: Option<String> =
            waybill_common::types::purl::Purl::new(target_ref)
                .ok()
                .map(|p| p.ecosystem().to_string())
                .filter(|e| e != "generic");
        let mut synthesized_count = 0usize;
        for c in components {
            let eco = c.purl.ecosystem().to_string();
            if per_ecosystem_root.contains_key(&eco) {
                continue;
            }
            let is_operators_own_ecosystem =
                operator_root_ecosystem.as_deref() == Some(eco.as_str());
            per_ecosystem_root.insert(eco, target_ref.to_string());
            if !is_operators_own_ecosystem {
                synthesized_count += 1;
            }
        }
        if synthesized_count > 0 {
            tracing::info!(
                synthesized_ecosystems_count = synthesized_count,
                "synthesized per-ecosystem placeholder roots for operator-override scan"
            );
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
    use waybill_common::resolution::RelationshipType as RT;
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
        let set = build_ecosystem_root_set(&components, &selection, "");
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
        let set = build_ecosystem_root_set(&components, &selection, "");
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
        let set = build_ecosystem_root_set(&components, &selection, "");
        assert!(set.roots.contains("pkg:npm/root@1.0.0"));
        assert!(set.roots.contains("pkg:gem/gemroot@1.0.0"));
        assert!(set.ecosystems_without_root.is_empty());
    }

    // ── Milestone 192 — operator-override placeholder-root synthesis ──

    use crate::generate::graph_completeness::test_support::selection_with_operator_override;

    #[test]
    fn m192_fixture_o1_operator_override_single_ecosystem_no_orphan_root_flag() {
        // Fixture O1 per contracts/classifier-input.md: operator supplied
        // `--root-name pico --root-version abc123` → target_ref is a
        // pkg:generic root; components are all Go. Pre-m192 this left
        // `ecosystems_without_root = ["golang"]`; post-m192 synthesis
        // seeds a placeholder golang root pointing at target_ref.
        let components = vec![
            mk_component("pkg:golang/foo/bar@v1.0.0"),
            mk_component("pkg:golang/foo/baz@v2.0.0"),
        ];
        let selection = selection_with_operator_override();
        let target_ref = "pkg:generic/pico@abc123";
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        assert!(
            set.ecosystems_without_root.is_empty(),
            "post-m192 synthesis must empty ecosystems_without_root on the operator-override path; got: {:?}",
            set.ecosystems_without_root
        );
    }

    #[test]
    fn m192_fixture_o2_operator_override_multi_ecosystem_all_synthesized() {
        // Fixture O2: mixed golang + npm + pypi; operator-supplied
        // pkg:generic root. Synthesis fires for all three ecosystems.
        let components = vec![
            mk_component("pkg:golang/foo@v1.0.0"),
            mk_component("pkg:npm/bar@1.0.0"),
            mk_component("pkg:pypi/baz@1.0.0"),
        ];
        let selection = selection_with_operator_override();
        let target_ref = "pkg:generic/mixed@1.0";
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        assert!(
            set.ecosystems_without_root.is_empty(),
            "all three ecosystems must be covered by synthesis"
        );
    }

    #[test]
    fn m192_fixture_o3_root_purl_type_ecosystem_skipped_from_synthesis() {
        // Fixture O3 per Q2 answer A: operator passed
        // `--root-purl-type golang --root-name X` so target_ref is a
        // pkg:golang/... — the golang ecosystem is ALREADY covered by
        // the operator's chosen root PURL type. Synthesis MUST skip
        // that ecosystem to avoid a duplicate root, and MUST still
        // fire for other ecosystems present.
        let components = vec![
            mk_component("pkg:golang/svc@v1.0.0"),
            mk_component("pkg:npm/dep@1.0.0"),
        ];
        let selection = selection_with_operator_override();
        let target_ref = "pkg:golang/github.com/example/svc@1.0";
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        assert!(
            set.ecosystems_without_root.is_empty(),
            "npm placeholder synthesis must still fire even when golang is already the operator's root type"
        );
    }

    #[test]
    fn m192_fixture_n_native_root_synthesis_no_op() {
        // Fixture N: byte-identity guard. Native-root MainModule
        // scan — synthesis block MUST NOT execute. Output MUST be
        // byte-identical to the pre-m192 shape (mirrors the existing
        // `ecosystem_root_set_uses_primary_from_selection` test but
        // exercises the m192 code path with a non-empty target_ref to
        // prove the guard fires).
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_component("pkg:npm/dep@1.0.0"),
        ];
        let selection = selection_with_main_module(0);
        let target_ref = "pkg:npm/root@1.0.0";
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        // Same expectations as pre-m192 native-root behavior — the
        // primary main-module fills the npm slot; no synthesis.
        assert!(set.roots.contains("pkg:npm/root@1.0.0"));
        assert!(set.ecosystems_without_root.is_empty());
    }

    #[test]
    fn m192_operator_override_with_no_components_is_noop() {
        // Empty components + operator-override subject — synthesis
        // loop executes zero times; ecosystems_without_root stays
        // empty; no INFO log fires (synthesized_count == 0).
        let components: Vec<ResolvedComponent> = Vec::new();
        let selection = selection_with_operator_override();
        let target_ref = "pkg:generic/pico@abc123";
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        assert!(set.roots.is_empty());
        assert!(set.ecosystems_without_root.is_empty());
    }

    #[test]
    fn m192_operator_override_empty_target_ref_falls_back_to_generic_semantics() {
        // Pre-m084 shape: target_ref is not a full PURL (legacy
        // `name@version` short-form). `Purl::new(target_ref)` fails,
        // so `operator_root_ecosystem` is None; synthesis fires for
        // every ecosystem present in components[].
        let components = vec![
            mk_component("pkg:golang/foo@v1.0.0"),
            mk_component("pkg:npm/bar@1.0.0"),
        ];
        let selection = selection_with_operator_override();
        let target_ref = "some-legacy-name@0.0.0"; // NOT a valid pkg: PURL
        let set = build_ecosystem_root_set(&components, &selection, target_ref);
        assert!(
            set.ecosystems_without_root.is_empty(),
            "fallback: when target_ref isn't a valid PURL, synthesize for every ecosystem"
        );
    }
}
