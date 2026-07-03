# Contract: `mikebom:graph-completeness-reason` code vocabulary

**Milestone 158** • The closed 8-code vocabulary. Adding a new code is a spec/CHANGELOG event (SC-005); silent additions are prohibited.

## Values of `mikebom:graph-completeness`

| Value | Semantic |
|---|---|
| `complete` | BFS pass ran to completion; 100% of emitted components reachable from the multi-root seed set; no gap classes triggered. |
| `partial` | BFS pass ran; a gap was detected AND the gap class matches a documented reason-code below. |
| `unknown` | BFS could not run, produced an inconclusive result, or detected a gap that could not be classified. Under Q1 caution-first, this is the default fallback when in doubt. |

## Reason codes (milestone 158 ships with 8)

| Code | When emitted | Detail format |
|---|---|---|
| `workspace-peer-detection-degraded` | Root-selection identified N workspace peers but only linked M < N to the root | `root links to M of N detected workspace peers` |
| `root-selection-ambiguous` | Multiple candidate roots with no confident tiebreaker | `K candidate roots, no confident tiebreaker` |
| `root-selection-failed` | No root component could be selected | `no root component could be selected` |
| `edge-resolution-degraded` | Declared edges dropped by the graph resolver (e.g., issue #493 npm-alias syntax) | `K declared edge(s) dropped by graph resolver` |
| `go-transitive-coverage-degraded` | Go transitive-edge coverage <100% (issue #495) | `K transitive edge(s) not populated` |
| `go-workspace-mode-anomaly` | Go workspace-mode false edges detected (issue #494) | `K anomalous edge(s) detected in go.work mode` |
| `orphaned-components-detected` | Components emitted but not reachable from any per-ecosystem root (Q2) | `K component(s) not reachable from root` |
| `multi-ecosystem-partial-root` | Per-ecosystem root identification failed for one or more ecosystems (Q3) | `<comma-separated ecosystem names>` |

## Value combinations

- When multiple reason codes apply to the same scan, they MUST be joined by `; ` (semicolon + space) in a single reason-string per FR-012. Example:
  ```
  multi-ecosystem-partial-root: npm; orphaned-components-detected: 3
  ```

- When `mikebom:graph-completeness = complete`, NO reason-code annotation is emitted (FR-005).

- When `mikebom:graph-completeness = unknown`, a reason-code MAY be emitted if the "we know why we're unknown" case applies; otherwise the reason annotation is omitted.

- Under Q1 caution-first: mikebom MUST NOT emit `partial` with an undocumented code. If a gap can't be classified into one of the 8 codes above, emit `unknown` instead.

## Governance

- Adding a new code REQUIRES: a spec/CHANGELOG entry (spec update + CHANGELOG headline), a bump of this vocabulary file, and a new unit test asserting the new code emits its detail correctly.
- Deprecating a code (once the underlying issue is fixed): mark the code as `deprecated (fixed in milestone N)` in this table; keep the enum variant + emission logic for a full release cycle before removal.
- Never silently repurpose a code — consumers may have downstream tooling keyed to its exact wording.
