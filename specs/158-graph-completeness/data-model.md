# Data Model: Milestone 158

**Date**: 2026-07-03
**Feature**: [spec.md](./spec.md)

Phase-1 data structures. All types live in `mikebom-cli/src/generate/graph_completeness/` (new submodule).

## Entities

### `GraphCompletenessResult` (public struct)

The output of the multi-root BFS pass. Consumed by the three format emitters (CDX / SPDX 2.3 / SPDX 3) to produce the two document-scope annotations + drive the workspace-peer linkage.

```rust
pub struct GraphCompletenessResult {
    /// The three-value domain per FR-006. Serialized as lowercase string.
    pub value: GraphCompletenessValue,
    /// Set of reason codes when value != Complete. Empty when Complete.
    /// Multiple codes are joined by `; ` at annotation-emission time
    /// per FR-012.
    pub reason_codes: Vec<ReasonCode>,
    /// Total components in the SBOM at pass time. Used for the "N of M"
    /// component of reason strings (e.g. `orphaned-components-detected: 3`
    /// says N=3).
    pub total_count: usize,
    /// Components reachable from the multi-root BFS seed set.
    pub reachable_count: usize,
    /// Components emitted but NOT in the reachable set. Used to
    /// populate the `orphaned-components-detected: <N>` reason string.
    pub orphan_count: usize,
}
```

### `GraphCompletenessValue` (enum)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphCompletenessValue {
    Complete,   // BFS pass ran, 100% reachability, no gap classes
    Partial,    // BFS pass ran, gap detected, all gaps classified via reason codes
    Unknown,    // BFS couldn't run, or produced inconclusive result, or gap couldn't be classified
}

impl GraphCompletenessValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
        }
    }
}
```

### `ReasonCode` (enum) — the SC-005 vocabulary

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasonCode {
    WorkspacePeerDetectionDegraded { linked: usize, detected: usize },
    RootSelectionAmbiguous { candidate_count: usize },
    RootSelectionFailed,
    EdgeResolutionDegraded { dropped_count: usize },
    GoTransitiveCoverageDegraded { missing_count: usize },
    GoWorkspaceModeAnomaly { anomaly_count: usize },
    OrphanedComponentsDetected { orphan_count: usize },
    MultiEcosystemPartialRoot { ecosystems: Vec<String> },
}

impl ReasonCode {
    /// Emit as `<code>: <detail>` per the reason-string grammar (R4).
    pub fn to_reason_string(&self) -> String {
        match self {
            Self::WorkspacePeerDetectionDegraded { linked, detected } =>
                format!("workspace-peer-detection-degraded: root links to {linked} of {detected} detected workspace peers"),
            Self::RootSelectionAmbiguous { candidate_count } =>
                format!("root-selection-ambiguous: {candidate_count} candidate roots, no confident tiebreaker"),
            Self::RootSelectionFailed =>
                "root-selection-failed: no root component could be selected".to_string(),
            Self::EdgeResolutionDegraded { dropped_count } =>
                format!("edge-resolution-degraded: {dropped_count} declared edge(s) dropped by graph resolver"),
            Self::GoTransitiveCoverageDegraded { missing_count } =>
                format!("go-transitive-coverage-degraded: {missing_count} transitive edge(s) not populated"),
            Self::GoWorkspaceModeAnomaly { anomaly_count } =>
                format!("go-workspace-mode-anomaly: {anomaly_count} anomalous edge(s) detected in go.work mode"),
            Self::OrphanedComponentsDetected { orphan_count } =>
                format!("orphaned-components-detected: {orphan_count} component(s) not reachable from root"),
            Self::MultiEcosystemPartialRoot { ecosystems } =>
                format!("multi-ecosystem-partial-root: {}", ecosystems.join(", ")),
        }
    }
}
```

### `EcosystemRootSet` (internal helper)

The seed set for the multi-root BFS pass. Populated by walking `components[]` and picking a top per-ecosystem main-module using the existing ladder.

```rust
pub(crate) struct EcosystemRootSet {
    /// Purl keys of the seed set. Includes the primary root
    /// (`RootSelectionResult.subject`) UNION the per-ecosystem tops.
    pub roots: HashSet<PurlKey>,
    /// Per-ecosystem breakdown — used for FR-012 diagnostics when
    /// per-ecosystem root identification fails.
    pub per_ecosystem: HashMap<String, PurlKey>,
    /// Ecosystems where an emitted component exists but no confident
    /// root could be picked. Drives `multi-ecosystem-partial-root`
    /// reason code.
    pub ecosystems_without_root: Vec<String>,
}
```

## Public API

```rust
// mikebom-cli/src/generate/graph_completeness/mod.rs
pub fn compute_graph_completeness(
    components: &[ResolvedComponent],
    dependency_edges: &HashMap<PurlKey, Vec<PurlKey>>,
    selection: &RootSelectionResult,
) -> GraphCompletenessResult;
```

Called once per scan at emit-time (after `select_root` has run + dependency edges have been assembled), BEFORE serialization to any of the three formats.

## Validation Rules

- `total_count` MUST equal `components.len()` at pass time.
- `reachable_count + orphan_count` MUST equal `total_count`.
- `orphan_count > 0` REQUIRES `reason_codes` to include `OrphanedComponentsDetected { orphan_count }` with the same count.
- When `value == Complete`, `reason_codes` MUST be empty.
- When `value == Unknown`, `reason_codes` MAY be empty OR contain codes; the annotation-emitter treats `unknown` values as "reason is optional" per FR-005 exception.
- The reason-string join uses `; ` (semicolon + space); the `;` character MUST NOT appear inside any individual `<detail>` value (validated at emission by the reason-code enum's own to-string implementations, which never emit `;`).

## Relationships

```text
ResolvedComponent[]  ─┐
                     │
RootSelectionResult ─┼─► compute_graph_completeness ─► GraphCompletenessResult
                     │                                       │
Dependency edges  ───┘                                       ▼
                                       ┌─────────────────────┴─────────────────────┐
                                       │                                           │
                                       ▼                                           ▼
                       [CDX emitter]                                  [SPDX 2.3 / SPDX 3 emitters]
                       metadata.properties[]                          document-scope Annotation
                       (2 entries per FR-007)                         (2 entries per FR-007)
```

## State Transitions

Not applicable — `GraphCompletenessResult` is a value type produced once per scan and never mutated. No lifecycle.
