# Phase 1 Data Model: Graph-completeness reachability signal (m177)

**Feature**: 177-graph-reachability-signal
**Date**: 2026-07-09

Four entities. Zero new types on `mikebom-common` (per Constitution VI — this is a `mikebom-cli`-only milestone). One new `ReasonCode` enum variant. One new classifier function. One extended docs subsection. One new C111 catalog vocabulary entry.

## Entity 1 — `ReasonCode::TransitiveEdgesUnresolvable` enum variant

**Location**: `mikebom-cli/src/generate/graph_completeness/reason_codes.rs`, inserted alphabetically-adjacent to `MultiEcosystemPartialRoot` (which has the closest structural precedent — `Vec<String>` of ecosystems).

**Shape**:

```rust
/// Emitted when the scan produced ≥1 component at design-tier or
/// analyzed-tier (`sbom_tier ∈ {"design", "analyzed"}`) that lacks
/// a same-package source-tier-or-higher counterpart. Same-package
/// identity is determined by PURL type + name (version ignored per
/// research §R1 — design-tier version is empty by definition).
///
/// Signals to downstream reachability consumers that the transitive-
/// edge closure past these components is unreliable: hash-match
/// resolution (analyzed-tier) identifies components but doesn't emit
/// transitive edges; constraint-only declarations (design-tier) have
/// no version to resolve past.
///
/// Detail: `<comma-separated PURL-type-canonical ecosystem list>`,
/// alphabetically sorted, deduplicated. Format precedent:
/// `MultiEcosystemPartialRoot`.
///
/// Milestone 177 vocabulary extension (spec §FR-001).
TransitiveEdgesUnresolvable {
    ecosystems: Vec<String>,
},
```

**Contract**:
- **Sorted-deduplicated on construction**: callers MUST pass `ecosystems` in alphabetical order with no duplicates. The classifier function enforces this via `sort()` + collection through `HashSet`.
- **Non-empty precondition**: callers MUST NOT construct this variant with an empty `ecosystems` vec. The classifier function checks non-empty before pushing.
- **PURL-type-canonical names**: `"pypi"`, `"cargo"`, `"npm"`, `"gem"`, `"maven"`, `"composer"`, `"cocoapods"`, `"mix"`, `"rebar3"`, `"dart"`, `"haskell"`, etc. — the exact string returned by `Purl::ecosystem()`.

## Entity 2 — `to_reason_string` arm

**Location**: `mikebom-cli/src/generate/graph_completeness/reason_codes.rs::to_reason_string`, appended to the existing `match` block.

**Shape**:

```rust
Self::TransitiveEdgesUnresolvable { ecosystems } => format!(
    "transitive-edges-unresolvable: {}",
    ecosystems.join(", ")
),
```

**Wire output example (single ecosystem)**: `transitive-edges-unresolvable: pypi`
**Wire output example (multi-ecosystem)**: `transitive-edges-unresolvable: composer, pypi`
**Wire output when composed with other codes** (via `join_reason_codes`): `root-selection-ambiguous: 3 candidate roots, no confident tiebreaker; transitive-edges-unresolvable: pypi`

**Stable grep substring per SC-005**: `"transitive-edges-unresolvable: "` (includes the trailing colon-space, matching the m175 `"design-tier components detected: "` precedent).

## Entity 3 — Classifier function `classify_transitive_edges_unresolvable`

**Location**: `mikebom-cli/src/generate/graph_completeness/mod.rs`, private helper adjacent to the existing classification logic in `compute_graph_completeness`.

**Signature**:

```rust
/// Milestone 177 classifier — identify ecosystems where the
/// transitive-edge closure is unwalkable due to design-tier or
/// analyzed-tier components without a same-package source-tier-
/// or-higher counterpart.
///
/// Returns `Some(ReasonCode::TransitiveEdgesUnresolvable { ecosystems })`
/// when non-empty; `None` when no triggering components exist.
///
/// Complexity: O(N) over components; O(N) auxiliary space for the
/// same-package lookup table. Runs once per scan at emit-time.
fn classify_transitive_edges_unresolvable(
    components: &[ResolvedComponent],
) -> Option<ReasonCode>;
```

**Contract**:
- **Pure function** — no I/O, deterministic on the input slice.
- **Idempotent** — repeated calls with the same input return the same output.
- **PURL-type identity** — uses `component.purl.ecosystem()` and `component.purl.name()` per research §R1.
- **Tier-set membership** — uses `matches!(c.sbom_tier.as_deref(), Some("source") | Some("deployed") | Some("build"))` per research §R2.

**Algorithm**:

```rust
fn classify_transitive_edges_unresolvable(
    components: &[ResolvedComponent],
) -> Option<ReasonCode> {
    use std::collections::{HashMap, HashSet};

    // Pass 1: build a same-package tier-safe lookup table.
    // Key: (purl_type, purl_name). Value: true iff any component with
    // that key has sbom_tier ∈ safe-set.
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

    // Pass 2: identify triggering components + collect affected ecosystems.
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
    Some(ReasonCode::TransitiveEdgesUnresolvable {
        ecosystems: sorted,
    })
}
```

## Entity 4 — Call-site edit in `compute_graph_completeness`

**Location**: `mikebom-cli/src/generate/graph_completeness/mod.rs::compute_graph_completeness`, inserted between the BFS-orphan classification block (lines 229–267) and the final `value` computation (lines 276+).

**Shape**:

```rust
    }  // end of `if orphan_count > 0 { ... }` block

    // Milestone 177 — classify tier-based reachability gaps
    // (design-tier or analyzed-tier components without same-package
    // source-tier-or-higher counterpart). Orthogonal to BFS-orphan
    // classification above — can fire even when orphan_count == 0.
    if let Some(code) = classify_transitive_edges_unresolvable(components) {
        reason_codes.push(code);
    }

    // Q1 caution-first classification (unchanged from m158):
    let value = if reason_codes.is_empty() && orphan_count == 0 {
        GraphCompletenessValue::Complete
    } else if !reason_codes.is_empty() {
        GraphCompletenessValue::Partial
    } else {
        GraphCompletenessValue::Unknown
    };
```

**Contract**:
- **Composability**: the new classifier's output is appended to `reason_codes`; the final `value` computation naturally treats `Partial` as fired when EITHER BFS-orphan OR tier-based classifier produced a code.
- **No mutation of BFS-derived state**: `orphan_count`, `reachable_count`, `total_count`, `reachable_set` are UNCHANGED by the new classifier.
- **Ordering of codes in the final reason string**: BFS-orphan codes appear FIRST (as they do today), then `TransitiveEdgesUnresolvable`. Joined via `join_reason_codes` with `"; "` separator.

## Entity 5 — Docs subsection extension in `reading-a-mikebom-sbom.md`

**Location**: `docs/reference/reading-a-mikebom-sbom.md` §3.4, the existing `mikebom:graph-completeness + mikebom:graph-completeness-reason` subsection at line ~494.

**Change shape**: append a new sub-paragraph after the existing reason-code enumeration:

```markdown
> **Milestone 177 — reachability-consumer contract**: post-m177 mikebom fires the reason code `transitive-edges-unresolvable: <ecosystem-list>` whenever the scan emits ≥1 design-tier or analyzed-tier component without a same-package source-tier-or-higher counterpart. Reachability tools consuming the SBOM SHOULD machine-check this signal before running: if the value is `"partial"` AND the reason contains this code, the affected-ecosystem transitive-edge closure is unwalkable, and reachability analysis in those ecosystems will produce false negatives. Reachability tools have three options: (a) refuse to run and instruct the operator to remediate the scan input, (b) run with results flagged as low-confidence, (c) filter to reachability-analyze only the ecosystems NOT named in the reason detail.
>
> **Composes orthogonally with milestone 175** (design-tier component visibility): m175 emits an INFO-level advisory log at scan time (`"design-tier components detected: "`) for the operator; m177 emits this machine-readable signal for downstream reachability consumers. Both fire on the same scan condition (design-tier components exist) but serve different audiences.
>
> **jq recipe** — machine-check whether reachability analysis is safe:
>
> ```jq
> .metadata.properties[]?
>     | select(.name == "mikebom:graph-completeness-reason")
>     | .value
>     | contains("transitive-edges-unresolvable")
> ```
>
> Returns `true` when the graph is unreliable for reachability; `false` otherwise. Reachability tools can wire this directly into their pre-flight gating.
```

## Entity 6 — `sbom-format-mapping.md` C111 row extension

**Location**: `docs/reference/sbom-format-mapping.md`, Section C, existing C111 row.

**Change shape**: extend the "closed vocabulary" enumeration in the Justification column to include the new code. The row's structural shape (KEEP-NO-NATIVE, native-alternative rejections) is UNCHANGED — m177 is a vocabulary extension, not an audit-shape change.

Concretely: the C111 row's Justification column currently enumerates 8 codes; extend to 9. Add the sentence: `"Milestone 177 extension: added ninth code `transitive-edges-unresolvable: <ecosystem-list>` for tier-based reachability gaps (design-tier or analyzed-tier components without same-package source-tier-or-higher counterpart). Closed vocabulary is additive — pre-177 consumers who don't recognize this code treat it as opaque diagnostic detail per Constitution Principle X."`

## Cross-entity invariants (post-177)

1. **Vocabulary is closed and enumerated**: 9 codes total in `reason_codes::ReasonCode`, all reachable via `to_reason_string`. Verified by test coverage.
2. **Value transition is deterministic**: `graph-completeness = "partial"` iff `reason_codes.len() > 0`. `graph-completeness = "complete"` iff `reason_codes.is_empty() && orphan_count == 0`. New classifier maintains this invariant.
3. **FR-002 predicate is same-package granular**: a mixed-tier ecosystem with N-1 source-tier + 1 design-tier component fires the code with THAT ecosystem in the list. Verified by acceptance test.
4. **Stable grep substring**: `"transitive-edges-unresolvable: "` appears verbatim in the emitted reason value whenever the code fires. Verified by contract test.

## State transitions

None. Pure emission-time classification. No lifecycle events.
