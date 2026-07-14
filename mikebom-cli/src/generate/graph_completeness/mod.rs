//! Milestone 158 — graph-completeness signal (issue #492).
//!
//! Runs a multi-root BFS-reachability pass over the assembled dep-
//! graph at emit-time. Determines the three-value `mikebom:graph-
//! completeness` annotation per spec.md FR-006 + FR-008 + FR-012:
//!
//!   - `complete` iff 100% of emitted components are reachable
//!     from the multi-root seed set AND no gap class fired.
//!   - `partial` iff a gap was detected AND classified into one of
//!     the 8 documented reason codes.
//!   - `unknown` in all other cases (default fallback per Q1
//!     caution-first — prefer `unknown` over guessing).
//!
//! Public API: [`compute_graph_completeness`]. Called once per
//! scan at emit-time (after `select_root` has run + dependency
//! edges have been assembled) and BEFORE serialization to any of
//! the three formats. Result is threaded through to the CDX +
//! SPDX 2.3 + SPDX 3 emitters via new required parameters.

use std::collections::HashSet;

use mikebom_common::resolution::{
    EnrichmentProvenance, Relationship, RelationshipType, ResolvedComponent,
};

use crate::generate::root_selector::{ResolvedRootSubject, RootSelectionResult};

pub mod bfs;
pub mod reason_codes;
#[cfg(test)]
mod test_support;

pub use reason_codes::{join_reason_codes, ReasonCode};

/// The three-valued completeness domain per FR-006. Serialized as
/// lowercase kebab-case string via `as_str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphCompletenessValue {
    Complete,
    Partial,
    Unknown,
}

impl GraphCompletenessValue {
    /// Wire value for the `mikebom:graph-completeness` annotation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for GraphCompletenessValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The output of the graph-completeness pass. Consumed by the three
/// format emitters (CDX / SPDX 2.3 / SPDX 3) to produce the two
/// document-scope annotations per FR-003 + FR-004.
///
/// Field invariants (asserted at construction + in tests):
///
///   - `total_count == components.len()` at pass time.
///   - `reachable_count + orphan_count == total_count`.
///   - `orphan_count > 0` REQUIRES a `OrphanedComponentsDetected`
///     variant in `reason_codes` OR a `MultiEcosystemPartialRoot`
///     variant that accounts for those orphans (caution-first
///     unknown otherwise).
///   - `value == Complete` REQUIRES `reason_codes.is_empty()`.
#[derive(Debug, Clone)]
pub struct GraphCompletenessResult {
    pub value: GraphCompletenessValue,
    pub reason_codes: Vec<ReasonCode>,
    pub total_count: usize,
    pub reachable_count: usize,
    pub orphan_count: usize,
    // Milestone 167 (T002): expose the BFS-computed reachable set,
    // intersected with the emitted components' PURL keys, so the
    // emit-time orphan-reason classifier (`generate::orphan_reason`)
    // can decide per-component orphan status without recomputing BFS.
    //
    // Invariant: `reachable_set.len() == reachable_count`.
    //
    // `#[allow(dead_code)]` until T011 wires the call-site in
    // `scan_fs::mod.rs`; removed as part of T011.
    #[allow(dead_code)]
    pub reachable_set: HashSet<String>,
}

impl GraphCompletenessResult {
    /// Trivially-complete result — used for empty SBOMs and by
    /// tests. An empty SBOM is a well-defined complete graph
    /// (nothing to reach, nothing missing).
    pub fn trivially_complete() -> Self {
        Self {
            value: GraphCompletenessValue::Complete,
            reason_codes: Vec::new(),
            total_count: 0,
            reachable_count: 0,
            orphan_count: 0,
            reachable_set: HashSet::new(),
        }
    }

    /// Caution-first `unknown` result — used when the pass can't
    /// run (e.g. root-selection failed) or when a gap was detected
    /// but couldn't be classified. Currently unused inside
    /// `compute_graph_completeness` (all paths classify), but
    /// retained as public API for external callers who need to
    /// signal `unknown` without running BFS.
    #[allow(dead_code)]
    pub fn unknown(total_count: usize, reason_codes: Vec<ReasonCode>) -> Self {
        Self {
            value: GraphCompletenessValue::Unknown,
            reason_codes,
            total_count,
            reachable_count: 0,
            orphan_count: total_count,
            reachable_set: HashSet::new(),
        }
    }
}

/// Compute graph-completeness for the emitted SBOM. Runs the full
/// pass per contracts/reachability-algorithm.md:
///
///   Step 1: build the seed set (primary root + per-ecosystem tops).
///   Step 2: (in `bfs::pick_ecosystem_top`) pick each ecosystem's
///           top main-module.
///   Step 3: multi-source BFS from the seed set.
///   Step 4: classify — count reachable vs orphans, fire the
///           applicable reason codes, apply Q1 caution-first
///           fallback.
///
/// O(V + E) — meets FR-008.
pub fn compute_graph_completeness(
    components: &[ResolvedComponent],
    relationships: &[Relationship],
    selection: &RootSelectionResult,
    target_ref: &str,
) -> GraphCompletenessResult {
    // Milestone 194 US3: file-tier components (m133) carry
    // `mikebom:component-tier: file` and have no dep-graph edges by
    // design — they represent unattributed file inventory (SHA-256-
    // hashed blobs), not package-graph participants. Excluding them
    // from reachability accounting prevents them from perma-triggering
    // OrphanedComponentsDetected on scans that emit file-tier
    // components (per SC-005: pico corpus must report `complete`).
    // File-tier components remain in the emitted SBOM verbatim; this
    // filter only affects the classifier's total/reachable counts.
    let non_file_tier: Vec<ResolvedComponent> = components
        .iter()
        .filter(|c| {
            c.extra_annotations
                .get(crate::scan_fs::file_tier::COMPONENT_TIER_KEY)
                .and_then(|v| v.as_str())
                != Some(crate::scan_fs::file_tier::COMPONENT_TIER_FILE_VALUE)
        })
        .cloned()
        .collect();
    let components: &[ResolvedComponent] = non_file_tier.as_slice();
    let total_count = components.len();

    // Empty SBOM = trivially complete (nothing to reach, nothing
    // missing). Also handles the pathological "no components emitted"
    // path.
    if total_count == 0 {
        return GraphCompletenessResult::trivially_complete();
    }

    // Step 1 + Step 2 — seed set.
    // Milestone 192: `target_ref` is threaded through so
    // `build_ecosystem_root_set` can synthesize per-ecosystem
    // placeholder roots when the primary selection subject is NOT
    // a MainModule (operator-override / synthetic-placeholder /
    // maven-coord roots). Prevents the MultiEcosystemPartialRoot
    // classifier below from over-firing on operator-supplied roots.
    let mut root_set = bfs::build_ecosystem_root_set(components, selection, target_ref);
    // Also seed with the EMITTED `metadata.component` / SPDX root ref
    // (target_ref) — for the SyntheticPlaceholder / MavenCoord /
    // OperatorOverride cases where the emitted root doesn't map back
    // to a component in `components[]`. The CDX + SPDX emitters
    // guarantee this identity appears in the wire output as the BOM
    // subject, and their primary-dep-fallback logic (cyclonedx/
    // dependencies.rs:74-99, v3_document.rs:637-662) synthesizes
    // edges from it to every graph-top when it has no explicit
    // outbound edges. We mirror that here to keep BFS-reachability
    // aligned with the emitted graph.
    if !target_ref.is_empty() {
        root_set.roots.insert(target_ref.to_string());
        root_set
            .ecosystems_without_root
            .retain(|e| e != "generic");
    }

    // Step 3 — multi-source BFS. First build the base adjacency, then
    // mirror the emitter's primary-dep-fallback: if `target_ref` has
    // no outbound edges in the relationships, synthesize edges from
    // it to every "graph-top" (component NOT depended-on by anything
    // else). This matches how the emitters build the wire-format
    // dep-graph and keeps BFS reachability aligned with what the
    // consumer sees.
    let mut edges = bfs::build_edge_adjacency(relationships);
    let target_has_outbound = edges
        .get(target_ref)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !target_ref.is_empty() && !target_has_outbound && !components.is_empty() {
        // Collect graph-tops: components not depended on by anything.
        let depended_on: HashSet<&str> = relationships
            .iter()
            .map(|r| r.to.as_str())
            .collect();
        let graph_tops: Vec<String> = components
            .iter()
            .map(|c| c.purl.as_str().to_string())
            .filter(|purl| !depended_on.contains(purl.as_str()) && purl != target_ref)
            .collect();
        if !graph_tops.is_empty() {
            edges.insert(target_ref.to_string(), graph_tops);
        }
    }

    let visited = bfs::multi_source_bfs(&root_set.roots, &edges);

    // Step 4 — classify.
    // A component "counts" toward reachability only if it appears in
    // the emitted `components[]` (not the edge closure). Compute the
    // component-key set once for the intersection.
    let component_keys: HashSet<String> = components
        .iter()
        .map(|c| c.purl.as_str().to_string())
        .collect();
    // Milestone 167 (T002): materialize the reachable-set (intersected
    // with emitted component PURL keys) so the orphan-reason classifier
    // can key on it without recomputing BFS. `reachable_component_count`
    // == `reachable_set.len()` invariant asserted by construction below.
    let reachable_set: HashSet<String> = visited
        .intersection(&component_keys)
        .cloned()
        .collect();
    let reachable_component_count = reachable_set.len();
    let orphan_count = total_count.saturating_sub(reachable_component_count);

    let mut reason_codes: Vec<ReasonCode> = Vec::new();

    // Q2/Q3 classification — only trigger when there ARE actual
    // unreachable components. If BFS reached everything (e.g., because
    // the primary-dep-fallback covered the graph), the ecosystems-
    // without-root aren't a gap in practice.
    if orphan_count > 0 {
        let ecos_without_root: HashSet<String> = root_set
            .ecosystems_without_root
            .iter()
            .cloned()
            .collect();

        // Ecosystems whose orphans are ATTRIBUTABLE to a missing per-
        // ecosystem root (Q3 `multi-ecosystem-partial-root`). Only
        // include an ecosystem if it has at least one unreachable
        // component in ecos_without_root.
        let mut q3_ecosystems: Vec<String> = components
            .iter()
            .filter(|c| !visited.contains(c.purl.as_str()))
            .map(|c| c.purl.ecosystem().to_string())
            .filter(|eco| ecos_without_root.contains(eco))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        q3_ecosystems.sort();
        if !q3_ecosystems.is_empty() {
            reason_codes.push(ReasonCode::MultiEcosystemPartialRoot {
                ecosystems: q3_ecosystems,
            });
        }

        // Residual orphans — components NOT in a q3 ecosystem AND not
        // reachable. Q2 `orphaned-components-detected`.
        let residual_orphan_count = components
            .iter()
            .filter(|c| !visited.contains(c.purl.as_str()))
            .filter(|c| !ecos_without_root.contains(c.purl.ecosystem()))
            .count();
        if residual_orphan_count > 0 {
            reason_codes.push(ReasonCode::OrphanedComponentsDetected {
                orphan_count: residual_orphan_count,
            });
        }
    }

    // Q1 caution-first classification:
    // - `Complete` iff no reason codes AND all components reachable.
    // - `Partial` iff reason codes are present.
    // - `Unknown` in the pathological case: components not reachable
    //   AND no classifier fired. Should be impossible given the
    //   coverage above, but caution-first says emit `unknown` rather
    //   than lying.
    // Milestone 177 — classify tier-based reachability gaps
    // (design-tier or analyzed-tier components without same-package
    // source-tier-or-higher counterpart). Orthogonal to BFS-orphan
    // classification above — CAN fire even when orphan_count == 0.
    // Placement (after MultiEcosystemPartialRoot + OrphanedComponentsDetected,
    // before the final value computation) matters for reason-string
    // ordering: BFS-orphan-derived codes appear FIRST, m177 code
    // appears LAST when both fire.
    if let Some(code) = classify_transitive_edges_unresolvable(components) {
        reason_codes.push(code);
    }

    let value = if reason_codes.is_empty() && orphan_count == 0 {
        GraphCompletenessValue::Complete
    } else if !reason_codes.is_empty() {
        GraphCompletenessValue::Partial
    } else {
        // Defensive: orphans exist but no reason code fired. Caution-
        // first fallback per Q1.
        GraphCompletenessValue::Unknown
    };

    GraphCompletenessResult {
        value,
        reason_codes,
        total_count,
        reachable_count: reachable_component_count,
        orphan_count,
        reachable_set,
    }
}

/// Milestone 177 classifier — identify ecosystems where the
/// transitive-edge closure is unwalkable due to design-tier or
/// analyzed-tier components lacking a same-package source-tier-or-
/// higher counterpart.
///
/// Two-pass algorithm per data-model.md §Entity 3:
///
///   1. Build a same-package lookup table keyed by `(purl.ecosystem(),
///      purl.name())`. Value: `true` iff any component with that key
///      has `sbom_tier ∈ {"source", "deployed", "build"}` (the "safe"
///      set per spec Q2).
///   2. Iterate components filtered to `sbom_tier ∈ {"design",
///      "analyzed"}` (the "triggering" set per spec Q2). For each
///      triggering component whose same-package key has no safe
///      counterpart in the lookup, add its PURL type to the affected-
///      ecosystems set.
///
/// Returns `Some(TransitiveEdgesUnresolvable { ecosystems })` when
/// the affected-ecosystems set is non-empty (sorted-deduplicated);
/// `None` otherwise.
///
/// Complexity: O(N) time, O(N) auxiliary space. Pure function.
fn classify_transitive_edges_unresolvable(
    components: &[ResolvedComponent],
) -> Option<ReasonCode> {
    use std::collections::HashMap;

    // Pass 1: same-package safety lookup.
    let mut safe_packages: HashMap<(String, String), bool> = HashMap::new();
    for c in components {
        let key = (c.purl.ecosystem().to_string(), c.purl.name().to_string());
        let is_safe = matches!(
            c.sbom_tier.as_deref(),
            Some("source") | Some("deployed") | Some("build")
        );
        let entry = safe_packages.entry(key).or_insert(false);
        *entry = *entry || is_safe;
    }

    // Pass 2: collect affected ecosystems.
    let mut affected_ecosystems: HashSet<String> = HashSet::new();
    for c in components {
        let is_triggering_tier = matches!(
            c.sbom_tier.as_deref(),
            Some("design") | Some("analyzed")
        );
        if !is_triggering_tier {
            continue;
        }
        let key = (c.purl.ecosystem().to_string(), c.purl.name().to_string());
        if !safe_packages.get(&key).copied().unwrap_or(false) {
            affected_ecosystems.insert(c.purl.ecosystem().to_string());
        }
    }

    if affected_ecosystems.is_empty() {
        return None;
    }
    let mut sorted: Vec<String> = affected_ecosystems.into_iter().collect();
    sorted.sort();
    Some(ReasonCode::TransitiveEdgesUnresolvable { ecosystems: sorted })
}

/// Milestone 158 FR-002 — construct the synthetic `root → loser`
/// dependency edges that link workspace peers into the primary
/// root's `dependsOn`. Returns an empty vec when there are no
/// losers OR when the selection subject doesn't map to a component
/// (operator-override, maven-coord, synthetic-placeholder — those
/// paths already have no losers per milestone-127 root_selector
/// contract, so this defensively degrades to empty).
///
/// Callers append the returned edges to the existing
/// `Relationship[]` list. Both `build_dependencies` (CDX) and
/// `build_relationships` (SPDX 2.3 + SPDX 3) naturally emit the
/// resulting root → loser edges via their existing infrastructure.
pub fn build_workspace_peer_edges(
    selection: &RootSelectionResult,
    components: &[ResolvedComponent],
) -> Vec<Relationship> {
    if selection.losers.is_empty() {
        return Vec::new();
    }
    let root_purl = match &selection.subject {
        ResolvedRootSubject::MainModule(idx) => {
            match components.get(*idx) {
                Some(c) => c.purl.as_str().to_string(),
                None => return Vec::new(),
            }
        }
        // The other 3 subject variants (MavenCoord, SyntheticPlaceholder,
        // OperatorOverride) all produce empty `losers` per milestone-127
        // FR-006 — reaching this arm with non-empty losers would be a
        // root_selector bug, not a 158 concern. Degrade to empty.
        _ => return Vec::new(),
    };
    selection
        .losers
        .iter()
        .map(|loser| Relationship {
            from: root_purl.clone(),
            to: loser.as_str().to_string(),
            relationship_type: RelationshipType::DependsOn,
            provenance: EnrichmentProvenance {
                source: "milestone-158-workspace-peer-linkage".to_string(),
                data_type: "dependency-graph".to_string(),
            },
        })
        .collect()
}

/// Milestone 192 pre-rewrite: re-anchor DependsOn edges whose `.from`
/// is a dropped-main-module PURL onto `target_ref`. Called from the
/// three format emitters BEFORE `compute_graph_completeness` runs, so
/// the classifier sees the same edge topology that the emitters will
/// later serialize (via the m086 rewrite at build_dependencies /
/// build_relationships time).
///
/// Without this, operator-override scans (`--root-name X`) that drop
/// a native main-module leave stale `.from = <dropped-purl>` edges in
/// the relationships vec; BFS from the synthetic target_ref then
/// can't reach the transitive deps that used to hang off the dropped
/// mainmod, and the classifier over-fires
/// `partial: orphaned-components-detected: N`.
///
/// Extracted in milestone 194 US4 from `cyclonedx/builder.rs` (added
/// by #570) so SPDX 2.3 + SPDX 3 emitters get the same pre-rewrite,
/// closing the format-parity gap for graph-completeness on operator-
/// override scans (SC-005: pico corpus SBOMs → `complete` across all
/// three formats).
pub fn rewrite_dropped_mainmod_edges(
    relationships: &[Relationship],
    dropped_main_module_purls: &[String],
    target_ref: &str,
) -> Vec<Relationship> {
    if dropped_main_module_purls.is_empty() {
        return relationships.to_vec();
    }
    let dropped: HashSet<&str> = dropped_main_module_purls
        .iter()
        .map(|s| s.as_str())
        .collect();
    relationships
        .iter()
        .map(|r| {
            if dropped.contains(r.from.as_str()) {
                Relationship {
                    from: target_ref.to_string(),
                    to: r.to.clone(),
                    relationship_type: r.relationship_type.clone(),
                    provenance: r.provenance.clone(),
                }
            } else {
                r.clone()
            }
        })
        .collect()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::graph_completeness::test_support::{
        mk_component, mk_main_module, mk_rel, selection_with_main_module,
    };
    use crate::generate::root_selector::RootSelectionResult;

    #[test]
    fn empty_components_is_trivially_complete() {
        let result = compute_graph_completeness(&[], &[], &selection_with_main_module(0), "");
        assert_eq!(result.value, GraphCompletenessValue::Complete);
        assert_eq!(result.total_count, 0);
        assert_eq!(result.reachable_count, 0);
        assert_eq!(result.orphan_count, 0);
        assert!(result.reason_codes.is_empty());
    }

    #[test]
    fn single_package_no_peers_is_complete_sc007a() {
        // T017a / SC-007(a) — single-package repo, root + 3 direct
        // deps, no losers. Value MUST be Complete.
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_component("pkg:npm/a@1.0.0"),
            mk_component("pkg:npm/b@1.0.0"),
            mk_component("pkg:npm/c@1.0.0"),
        ];
        let relationships = vec![
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/a@1.0.0"),
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/b@1.0.0"),
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/c@1.0.0"),
        ];
        let result = compute_graph_completeness(
            &components,
            &relationships,
            &selection_with_main_module(0),
            "",
        );
        assert_eq!(result.value, GraphCompletenessValue::Complete);
        assert!(result.reason_codes.is_empty());
        assert_eq!(result.total_count, 4);
        assert_eq!(result.reachable_count, 4);
        assert_eq!(result.orphan_count, 0);
    }

    #[test]
    fn orphan_present_produces_partial_with_orphaned_reason_q2() {
        // T027 / SC-007(g) — Q2 orphan classification.
        let components = vec![
            mk_main_module("pkg:npm/root@1.0.0"),
            mk_component("pkg:npm/reachable-a@1.0.0"),
            mk_component("pkg:npm/reachable-b@1.0.0"),
            mk_component("pkg:npm/reachable-c@1.0.0"),
            mk_component("pkg:npm/orphan@1.0.0"),
        ];
        let relationships = vec![
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/reachable-a@1.0.0"),
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/reachable-b@1.0.0"),
            mk_rel("pkg:npm/root@1.0.0", "pkg:npm/reachable-c@1.0.0"),
        ];
        let result = compute_graph_completeness(
            &components,
            &relationships,
            &selection_with_main_module(0),
            "",
        );
        assert_eq!(result.value, GraphCompletenessValue::Partial);
        assert_eq!(result.orphan_count, 1);
        assert_eq!(result.reason_codes.len(), 1);
        assert!(matches!(
            result.reason_codes[0],
            ReasonCode::OrphanedComponentsDetected { orphan_count: 1 }
        ));
        assert_eq!(
            join_reason_codes(&result.reason_codes),
            "orphaned-components-detected: 1 component(s) not reachable from root"
        );
    }

    #[test]
    fn multi_root_bfs_two_ecosystems_complete_q3() {
        // T028 / SC-007(i) — Q3 multi-root BFS with 2 ecosystems, each
        // with its own main-module and reachable set.
        let components = vec![
            mk_main_module("pkg:npm/npm-root@1.0.0"),
            mk_component("pkg:npm/npm-a@1.0.0"),
            mk_component("pkg:npm/npm-b@1.0.0"),
            mk_component("pkg:npm/npm-c@1.0.0"),
            mk_main_module("pkg:gem/gem-root@1.0.0"),
            mk_component("pkg:gem/gem-a@1.0.0"),
            mk_component("pkg:gem/gem-b@1.0.0"),
        ];
        let relationships = vec![
            mk_rel("pkg:npm/npm-root@1.0.0", "pkg:npm/npm-a@1.0.0"),
            mk_rel("pkg:npm/npm-root@1.0.0", "pkg:npm/npm-b@1.0.0"),
            mk_rel("pkg:npm/npm-root@1.0.0", "pkg:npm/npm-c@1.0.0"),
            mk_rel("pkg:gem/gem-root@1.0.0", "pkg:gem/gem-a@1.0.0"),
            mk_rel("pkg:gem/gem-root@1.0.0", "pkg:gem/gem-b@1.0.0"),
        ];
        let result = compute_graph_completeness(
            &components,
            &relationships,
            &selection_with_main_module(0),
            "",
        );
        assert_eq!(result.value, GraphCompletenessValue::Complete);
        assert!(result.reason_codes.is_empty());
        assert_eq!(result.total_count, 7);
        assert_eq!(result.reachable_count, 7);
    }

    #[test]
    fn combined_reason_multi_eco_plus_orphan_q3() {
        // T029 / SC-007(j) — Q3 combined-reason. npm root fine + gem
        // ecosystem has emitted components but no main-module → drives
        // multi-ecosystem-partial-root: gem. Plus a floating orphan
        // in a THIRD ecosystem that CAN pick a main-module but the
        // orphan isn't reachable from it.
        let components = vec![
            mk_main_module("pkg:npm/npm-root@1.0.0"),
            mk_component("pkg:npm/npm-a@1.0.0"),
            // gem has an emitted component but NO main-module →
            // multi-ecosystem-partial-root: gem.
            mk_component("pkg:gem/gem-orphan@1.0.0"),
            // cargo has a main-module but a residual orphan not
            // reachable from it.
            mk_main_module("pkg:cargo/cargo-root@1.0.0"),
            mk_component("pkg:cargo/cargo-orphan@1.0.0"),
        ];
        let relationships = vec![
            mk_rel("pkg:npm/npm-root@1.0.0", "pkg:npm/npm-a@1.0.0"),
            // Note: NO edge from cargo-root to cargo-orphan.
        ];
        let result = compute_graph_completeness(
            &components,
            &relationships,
            &selection_with_main_module(0),
            "",
        );
        assert_eq!(result.value, GraphCompletenessValue::Partial);
        // Should have BOTH reason codes.
        let has_multi = result.reason_codes.iter().any(|c| {
            matches!(c, ReasonCode::MultiEcosystemPartialRoot { ecosystems }
                     if ecosystems == &vec!["gem".to_string()])
        });
        let has_orphan = result.reason_codes.iter().any(|c| {
            matches!(c, ReasonCode::OrphanedComponentsDetected { orphan_count: 1 })
        });
        assert!(has_multi, "expected multi-ecosystem-partial-root: gem");
        assert!(has_orphan, "expected orphaned-components-detected: 1");
        // Joined string format.
        let joined = join_reason_codes(&result.reason_codes);
        assert_eq!(
            joined,
            "multi-ecosystem-partial-root: gem; orphaned-components-detected: 1 component(s) not reachable from root"
        );
    }

    #[test]
    fn caution_first_partial_with_multi_ecosystem_when_no_seed_q1() {
        // T026 / SC-007(h) — Q1 caution-first. selection.subject
        // points to an index that doesn't exist in components[], and
        // target_ref is empty. The seed set is empty AND the classifier
        // detects an npm ecosystem with no root → emits `partial` with
        // `multi-ecosystem-partial-root: npm`. This IS the caution-first
        // behavior: no positive claim of `complete`, and we CAN name the
        // gap class, so `partial` is honest (not `unknown`).
        let components = vec![mk_component("pkg:npm/orphan@1.0.0")];
        let selection = RootSelectionResult {
            subject: ResolvedRootSubject::MainModule(99), // invalid idx
            heuristic: None,
            losers: Vec::new(),
        };
        let result = compute_graph_completeness(&components, &[], &selection, "");
        assert_eq!(result.value, GraphCompletenessValue::Partial);
        assert!(result.reason_codes.iter().any(|c| matches!(
            c,
            ReasonCode::MultiEcosystemPartialRoot { ecosystems } if ecosystems == &vec!["npm".to_string()]
        )));
    }
}
