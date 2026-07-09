# Contract: `transitive-edges-unresolvable` reason-code wire format

**Feature**: 177-graph-reachability-signal
**Date**: 2026-07-09

Authoritative reference for the new reason code's wire shape, emission predicate, and consumer contract. Deviations in emitted output are grounds for review comment.

## Wire code name

**Kebab-case identifier**: `transitive-edges-unresolvable`

Fits the existing `mikebom-cli/src/generate/graph_completeness/reason_codes.rs` naming convention. Adjacent semantic peer: `edge-resolution-degraded` (also edge-related; different predicate — DROPPED vs UNRESOLVABLE).

## Detail template

`transitive-edges-unresolvable: <comma-separated ecosystem list>`

- Ecosystem list is alphabetically sorted, deduplicated.
- Names use PURL-type canonical form (`pypi`, `cargo`, `npm`, `gem`, `maven`, `composer`, `cocoapods`, `mix`, `rebar3`, `dart`, `haskell`, etc.) — the exact string returned by `Purl::ecosystem()`.
- Separator between ecosystems: `", "` (comma + single space). Matches the `MultiEcosystemPartialRoot` precedent.

## Composed with other codes

When emitted alongside other reason codes, `join_reason_codes` produces a `"; "`-separated value. Example:

```
root-selection-ambiguous: 3 candidate roots, no confident tiebreaker; transitive-edges-unresolvable: pypi
```

Order: BFS-orphan-derived codes appear FIRST (existing behavior); tier-based `TransitiveEdgesUnresolvable` appears LAST (per data-model.md §Entity 4 call-site placement).

## Emission predicate

**Fires**: `classify_transitive_edges_unresolvable(components).is_some()` where the classifier returns `Some(_)` iff:
- ≥1 component has `sbom_tier ∈ {Some("design"), Some("analyzed")}`, AND
- That component's `(purl.ecosystem(), purl.name())` key has NO other component in the slice with `sbom_tier ∈ {Some("source"), Some("deployed"), Some("build")}`.

**Suppressed**: when no design-tier or analyzed-tier components exist, OR every such component has a same-package source-tier-or-higher counterpart in the same scan.

**Offline behavior**: fires under `--offline` per FR-005 — the semantic is orthogonal to network state.

## Format-neutral emission

The code fires from a single site in `compute_graph_completeness` and threads through the m158 emission pipeline to all three formats:

- **CDX 1.6**: appears verbatim in `metadata.properties[]` entry `{name: "mikebom:graph-completeness-reason", value: "<joined>"}`.
- **SPDX 2.3**: appears in the `MikebomAnnotationCommentV1` envelope's `value` field on the doc-scope `SpdxDocument` annotations array (envelope-wrapped per m080).
- **SPDX 3.0.1**: appears in the typed `Annotation` graph element's `statement` field on the `SpdxDocument` root IRI (envelope-wrapped per m080).

Cross-format `SymmetricEqual` parity is inherited from the existing C111 parity extractor.

## Consumer contract

**Reachability tools** SHOULD gate their analysis on the presence of this code:

```jq
.metadata.properties[]?
  | select(.name == "mikebom:graph-completeness-reason")
  | .value
  | contains("transitive-edges-unresolvable")
```

Returns `true` when the graph is unreliable for reachability; `false` when safe.

**Response options** (documented in reading-a-mikebom-sbom.md):
1. **Refuse**: decline analysis, report "graph is not reliable for reachability — the scan input needs remediation."
2. **Downgrade**: proceed but flag results as low-confidence.
3. **Filter**: parse the ecosystem list, reachability-analyze only the ecosystems NOT named in the code's detail. This is the "partial reachability" pattern for polyglot scans.

**Extract affected ecosystems**:

```jq
.metadata.properties[]?
  | select(.name == "mikebom:graph-completeness-reason")
  | .value
  | capture("transitive-edges-unresolvable: (?<eco>[^;]+)")
  | .eco
  | split(", ")
```

Returns an array of ecosystem names, or empty array when the code isn't present.

## Stability guarantee

**Stable grep substring**: `"transitive-edges-unresolvable: "` (INCLUDES the trailing colon-space). CI dashboards + reachability-tool pre-flight scripts MAY `grep -F` this token to detect unreliable graphs.

**Wire-code name is closed**: changing `transitive-edges-unresolvable` to a different string is a SEMVER-breaking change and requires spec + CHANGELOG event per m158 governance.

## Non-goals

- No changes to the closed vocabulary's 8 existing codes.
- No new `mikebom:*` annotation.
- No new CLI flag.
- No reachability analysis performed by mikebom itself (mikebom emits the signal; consumers run analysis).
- No change to CDX / SPDX 2.3 / SPDX 3 native fields.

## Milestone context

- **m158** — introduced `mikebom:graph-completeness-reason` (C111) with 2 initial codes. Established the closed-vocabulary governance contract.
- **m167** — expanded vocabulary from 2 → 8 codes. Codes 4/5/6 are `#[allow(dead_code)]`-guarded pending emission wiring in deferred milestones (#494/#495/#496).
- **m170** — dedup document-scope emission. No vocabulary change.
- **m175** — introduced design-tier component visibility (advisory log + reading guide). Established the operator-UX signal for the same underlying scan-input state that m177 now surfaces as a machine-attestation.
- **m177 (this feature)** — extends vocabulary from 8 → 9 codes. `transitive-edges-unresolvable` is the ninth.
