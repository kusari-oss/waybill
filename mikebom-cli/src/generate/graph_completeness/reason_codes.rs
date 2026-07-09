//! Milestone 158 — `mikebom:graph-completeness-reason` code vocabulary.
//!
//! The closed 8-code vocabulary per spec.md SC-005 + contracts/
//! graph-completeness-vocabulary.md. Adding a new code is a spec/
//! CHANGELOG event — not a silent code change.
//!
//! Under Q1 caution-first: mikebom MUST NOT emit `partial` with a
//! reason-code outside this documented vocabulary. If a gap can't
//! be classified into one of these 8 variants, callers emit
//! `unknown` instead.

use std::fmt::Write as _;

/// The finite reason-code vocabulary. Each variant carries the
/// numeric or ecosystem-list detail required by the vocabulary
/// contract's detail-format column.
///
/// Under Q1 caution-first, ALL 8 variants MUST remain reachable
/// via `to_reason_string`, even if some (e.g., #494 / #495 / #496
/// deferred-milestone codes) aren't yet emitted at construction
/// time. SC-005 vocabulary stability treats the full closed set
/// as the public contract; individual emission call-sites will
/// wire the remaining variants in follow-on milestones.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ReasonCode {
    /// Root-selection identified N workspace peers but only linked
    /// M < N to the root. Detail: `root links to M of N detected
    /// workspace peers`.
    WorkspacePeerDetectionDegraded {
        linked: usize,
        detected: usize,
    },
    /// Multiple candidate roots with no confident tiebreaker.
    /// Detail: `K candidate roots, no confident tiebreaker`.
    RootSelectionAmbiguous {
        candidate_count: usize,
    },
    /// No root component could be selected. Detail: `no root
    /// component could be selected`.
    RootSelectionFailed,
    /// Declared edges dropped by the graph resolver (e.g., pnpm/
    /// yarn npm-alias syntax — issue #493). Detail: `K declared
    /// edge(s) dropped by graph resolver`.
    EdgeResolutionDegraded {
        dropped_count: usize,
    },
    /// Go transitive-edge coverage <100% (issue #495). Detail: `K
    /// transitive edge(s) not populated`.
    GoTransitiveCoverageDegraded {
        missing_count: usize,
    },
    /// Go workspace-mode false edges detected (issue #494). Detail:
    /// `K anomalous edge(s) detected in go.work mode`.
    GoWorkspaceModeAnomaly {
        anomaly_count: usize,
    },
    /// Components emitted but not reachable from any per-ecosystem
    /// root (Q2 clarification 2026-07-03). Detail: `K component(s)
    /// not reachable from root`.
    OrphanedComponentsDetected {
        orphan_count: usize,
    },
    /// Per-ecosystem root identification failed for one or more
    /// ecosystems (Q3 clarification 2026-07-03). Detail:
    /// `<comma-separated ecosystem names>`.
    MultiEcosystemPartialRoot {
        ecosystems: Vec<String>,
    },
    /// Milestone 177 — emitted when the scan produced ≥1 component
    /// at design-tier or analyzed-tier (`sbom_tier ∈ {"design",
    /// "analyzed"}`) that lacks a same-package source-tier-or-higher
    /// counterpart. Same-package identity is determined by PURL type
    /// plus name (version ignored — design-tier version is empty by
    /// definition).
    ///
    /// Signals to downstream reachability consumers that the
    /// transitive-edge closure past these components is unreliable:
    /// hash-match resolution (analyzed-tier) identifies components
    /// but doesn't emit transitive edges; constraint-only
    /// declarations (design-tier) have no version to resolve past.
    ///
    /// Detail: `<comma-separated PURL-type-canonical ecosystem
    /// list>`, alphabetically sorted, deduplicated. Format precedent:
    /// `MultiEcosystemPartialRoot`. Non-empty precondition — the
    /// classifier constructs this variant only when at least one
    /// affected ecosystem is present.
    TransitiveEdgesUnresolvable {
        ecosystems: Vec<String>,
    },
}

impl ReasonCode {
    /// Render this reason-code as `<code>: <detail>` per the spec
    /// reason-string grammar. The `<detail>` portion MUST NOT
    /// contain `;` so that `join_reason_codes` remains reversible.
    pub fn to_reason_string(&self) -> String {
        match self {
            Self::WorkspacePeerDetectionDegraded { linked, detected } => format!(
                "workspace-peer-detection-degraded: root links to {linked} of {detected} detected workspace peers"
            ),
            Self::RootSelectionAmbiguous { candidate_count } => format!(
                "root-selection-ambiguous: {candidate_count} candidate roots, no confident tiebreaker"
            ),
            Self::RootSelectionFailed => {
                "root-selection-failed: no root component could be selected".to_string()
            }
            Self::EdgeResolutionDegraded { dropped_count } => format!(
                "edge-resolution-degraded: {dropped_count} declared edge(s) dropped by graph resolver"
            ),
            Self::GoTransitiveCoverageDegraded { missing_count } => format!(
                "go-transitive-coverage-degraded: {missing_count} transitive edge(s) not populated"
            ),
            Self::GoWorkspaceModeAnomaly { anomaly_count } => format!(
                "go-workspace-mode-anomaly: {anomaly_count} anomalous edge(s) detected in go.work mode"
            ),
            Self::OrphanedComponentsDetected { orphan_count } => format!(
                "orphaned-components-detected: {orphan_count} component(s) not reachable from root"
            ),
            Self::MultiEcosystemPartialRoot { ecosystems } => format!(
                "multi-ecosystem-partial-root: {}",
                ecosystems.join(", ")
            ),
            Self::TransitiveEdgesUnresolvable { ecosystems } => format!(
                "transitive-edges-unresolvable: {}",
                ecosystems.join(", ")
            ),
        }
    }
}

/// Join a slice of reason-codes into a single reason-string per
/// FR-012, separated by `; ` (semicolon + space). Returns the empty
/// string when `codes` is empty (caller decides whether to emit the
/// annotation at all).
pub fn join_reason_codes(codes: &[ReasonCode]) -> String {
    let mut out = String::new();
    for (i, c) in codes.iter().enumerate() {
        if i > 0 {
            out.push_str("; ");
        }
        let _ = write!(&mut out, "{}", c.to_reason_string());
    }
    out
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    // SC-005 — the 8 documented codes emit their exact
    // vocabulary-contract detail strings. Byte-precise.

    #[test]
    fn workspace_peer_detection_degraded_detail() {
        let c = ReasonCode::WorkspacePeerDetectionDegraded {
            linked: 3,
            detected: 12,
        };
        assert_eq!(
            c.to_reason_string(),
            "workspace-peer-detection-degraded: root links to 3 of 12 detected workspace peers"
        );
    }

    #[test]
    fn root_selection_ambiguous_detail() {
        let c = ReasonCode::RootSelectionAmbiguous { candidate_count: 4 };
        assert_eq!(
            c.to_reason_string(),
            "root-selection-ambiguous: 4 candidate roots, no confident tiebreaker"
        );
    }

    #[test]
    fn root_selection_failed_detail() {
        let c = ReasonCode::RootSelectionFailed;
        assert_eq!(
            c.to_reason_string(),
            "root-selection-failed: no root component could be selected"
        );
    }

    #[test]
    fn edge_resolution_degraded_detail() {
        let c = ReasonCode::EdgeResolutionDegraded { dropped_count: 6 };
        assert_eq!(
            c.to_reason_string(),
            "edge-resolution-degraded: 6 declared edge(s) dropped by graph resolver"
        );
    }

    #[test]
    fn go_transitive_coverage_degraded_detail() {
        let c = ReasonCode::GoTransitiveCoverageDegraded { missing_count: 87 };
        assert_eq!(
            c.to_reason_string(),
            "go-transitive-coverage-degraded: 87 transitive edge(s) not populated"
        );
    }

    #[test]
    fn go_workspace_mode_anomaly_detail() {
        let c = ReasonCode::GoWorkspaceModeAnomaly { anomaly_count: 57 };
        assert_eq!(
            c.to_reason_string(),
            "go-workspace-mode-anomaly: 57 anomalous edge(s) detected in go.work mode"
        );
    }

    #[test]
    fn orphaned_components_detected_detail() {
        let c = ReasonCode::OrphanedComponentsDetected { orphan_count: 3 };
        assert_eq!(
            c.to_reason_string(),
            "orphaned-components-detected: 3 component(s) not reachable from root"
        );
    }

    #[test]
    fn multi_ecosystem_partial_root_detail_single_ecosystem() {
        let c = ReasonCode::MultiEcosystemPartialRoot {
            ecosystems: vec!["npm".to_string()],
        };
        assert_eq!(c.to_reason_string(), "multi-ecosystem-partial-root: npm");
    }

    #[test]
    fn multi_ecosystem_partial_root_detail_multi_ecosystem() {
        let c = ReasonCode::MultiEcosystemPartialRoot {
            ecosystems: vec!["npm".to_string(), "gem".to_string()],
        };
        assert_eq!(
            c.to_reason_string(),
            "multi-ecosystem-partial-root: npm, gem"
        );
    }

    #[test]
    fn join_reason_codes_empty() {
        assert_eq!(join_reason_codes(&[]), "");
    }

    #[test]
    fn join_reason_codes_single() {
        let codes = vec![ReasonCode::OrphanedComponentsDetected { orphan_count: 2 }];
        assert_eq!(
            join_reason_codes(&codes),
            "orphaned-components-detected: 2 component(s) not reachable from root"
        );
    }

    #[test]
    fn join_reason_codes_multi_uses_semicolon_space() {
        // FR-012 — joined by "; " (semicolon + space).
        let codes = vec![
            ReasonCode::MultiEcosystemPartialRoot {
                ecosystems: vec!["npm".to_string()],
            },
            ReasonCode::OrphanedComponentsDetected { orphan_count: 3 },
        ];
        assert_eq!(
            join_reason_codes(&codes),
            "multi-ecosystem-partial-root: npm; orphaned-components-detected: 3 component(s) not reachable from root"
        );
    }

    // ------------------------------------------------------------
    // Milestone 177 — TransitiveEdgesUnresolvable wire-format tests.
    // ------------------------------------------------------------

    #[test]
    fn transitive_edges_unresolvable_single_ecosystem() {
        let c = ReasonCode::TransitiveEdgesUnresolvable {
            ecosystems: vec!["pypi".to_string()],
        };
        assert_eq!(
            c.to_reason_string(),
            "transitive-edges-unresolvable: pypi"
        );
    }

    #[test]
    fn transitive_edges_unresolvable_multi_ecosystem() {
        // Ecosystems arrive alphabetically-sorted from the classifier.
        let c = ReasonCode::TransitiveEdgesUnresolvable {
            ecosystems: vec!["composer".to_string(), "pypi".to_string()],
        };
        assert_eq!(
            c.to_reason_string(),
            "transitive-edges-unresolvable: composer, pypi"
        );
    }

    #[test]
    fn transitive_edges_unresolvable_composes_with_orphaned() {
        // FR-004: joined by "; " alongside existing codes. Order
        // follows Vec insertion — orphaned appears before the new
        // m177 code per the classifier placement in
        // compute_graph_completeness (m177 classifier runs AFTER
        // BFS-orphan classification).
        let codes = vec![
            ReasonCode::OrphanedComponentsDetected { orphan_count: 3 },
            ReasonCode::TransitiveEdgesUnresolvable {
                ecosystems: vec!["pypi".to_string()],
            },
        ];
        assert_eq!(
            join_reason_codes(&codes),
            "orphaned-components-detected: 3 component(s) not reachable from root; transitive-edges-unresolvable: pypi"
        );
    }
}
