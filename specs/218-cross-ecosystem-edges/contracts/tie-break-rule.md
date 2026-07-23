# Contract: FR-003 tie-break rule (emit-all with ambiguous annotation)

**Feature**: 218-cross-ecosystem-edges | **Related**: FR-003, Clarification Q1

## Algorithm

Given:
- `dep_name` — the bare name from `entry.depends[]`.
- `source_purl` — the source main-module's PURL (source_purl.ecosystem() == "generic").
- `candidate_matches: Vec<(target_ecosystem, target_purl)>` — R2's cross-ecosystem search result set.
- `sibling_ecosystems: HashSet<String>` — precomputed set of ecosystems appearing in the scan's non-generic main-modules (E7).

Return `Vec<EdgeEmission>` where each `EdgeEmission` is either:
- `EdgeEmission::Resolved(target_purl, payload_C137)` — emit one edge with C137 only.
- `EdgeEmission::Ambiguous(target_purl, payload_C137, payload_C138)` — emit one edge with BOTH C137 AND C138.

### Pseudo-code

```rust
fn resolve_cross_ecosystem(
    dep_name: &str,
    source_purl: &str,
    candidate_matches: Vec<(String, String)>,   // (to_eco, target_purl)
    sibling_ecosystems: &HashSet<String>,
    lookup_via: &str,
) -> Vec<EdgeEmission> {
    // Fast path: exactly one candidate → resolved, no ambiguity possible.
    if candidate_matches.len() == 1 {
        let (to_eco, target_purl) = candidate_matches.into_iter().next().unwrap();
        return vec![EdgeEmission::Resolved(target_purl.clone(), CrossEcosystemInferencePayload {
            from_eco: "generic".to_string(),
            lookup_via: lookup_via.to_string(),
            target_purl,
            to_eco,
        })];
    }

    // Multi-match: try to narrow via sibling-ecosystem intersection.
    let sibling_matches: Vec<_> = candidate_matches
        .iter()
        .filter(|(to_eco, _)| sibling_ecosystems.contains(to_eco))
        .cloned()
        .collect();

    if sibling_matches.len() == 1 {
        // Tie-break succeeded — single sibling match wins.
        let (to_eco, target_purl) = sibling_matches.into_iter().next().unwrap();
        return vec![EdgeEmission::Resolved(target_purl.clone(), CrossEcosystemInferencePayload {
            from_eco: "generic".to_string(),
            lookup_via: lookup_via.to_string(),
            target_purl,
            to_eco,
        })];
    }

    // Tie-break did NOT narrow to exactly one (either 0 sibling matches OR ≥2).
    // Per Q1 clarification: emit ALL candidate edges, each with C138.
    let alternates_full: Vec<AlternateMatch> = candidate_matches
        .iter()
        .map(|(to_eco, target_purl)| AlternateMatch {
            target_purl: target_purl.clone(),
            to_eco: to_eco.clone(),
        })
        .collect();

    candidate_matches
        .into_iter()
        .map(|(to_eco, target_purl)| {
            let mut alternates = alternates_full.clone();
            // Remove self from the alternates list for this edge.
            alternates.retain(|a| a.target_purl != target_purl);
            alternates.sort_by(|a, b| a.target_purl.cmp(&b.target_purl));

            let base = CrossEcosystemInferencePayload {
                from_eco: "generic".to_string(),
                lookup_via: lookup_via.to_string(),
                target_purl: target_purl.clone(),
                to_eco: to_eco.clone(),
            };
            let ambiguous = CrossEcosystemInferenceAmbiguousPayload {
                alternates,
                from_eco: base.from_eco.clone(),
                lookup_via: base.lookup_via.clone(),
                target_purl: base.target_purl.clone(),
                to_eco: base.to_eco.clone(),
            };
            EdgeEmission::Ambiguous(target_purl, base, ambiguous)
        })
        .collect()
}
```

## Determinism guarantees

- Iteration order of `candidate_matches`: SORT once at R2's return by `(to_eco, target_purl)` lex before feeding into this function. Ensures deterministic emission order across scan runs.
- `alternates` sort: lex by `target_purl` (matches C138 payload validation rule).
- The `sibling_ecosystems.contains(to_eco)` check is O(1); the whole function is O(N) where N = candidate count (typically ≤10).

## Test coverage matrix

Standalone unit tests in `waybill-cli/src/generate/cross_ecosystem_edges/tie_break.rs::tests`:

| Test scenario                                                       | Candidates | Sibling ecos | Expected emissions        |
|---------------------------------------------------------------------|------------|--------------|---------------------------|
| Single candidate (fast path)                                        | `[gem]`    | `{gem}`      | 1 × Resolved(gem)         |
| Single candidate, no siblings                                       | `[gem]`    | `{}`         | 1 × Resolved(gem)         |
| Multi-candidate, one sibling match                                  | `[gem, npm, pypi]` | `{gem}` | 1 × Resolved(gem)     |
| Multi-candidate, two sibling matches                                | `[gem, npm, pypi]` | `{gem, npm}` | 3 × Ambiguous (all) |
| Multi-candidate, zero sibling matches                               | `[gem, npm, pypi]` | `{}`   | 3 × Ambiguous (all)   |
| Alternates self-exclusion                                           | `[gem, npm]` | `{}`      | 2 × Ambiguous, each with alternates.len() == 1 |
| Empty candidate list                                                | `[]`         | `{}`      | 0 emissions (goes to FR-004 unresolved path, NOT this function) |

Each row is an independent `#[test]` function. Total unit-test count for tie_break.rs: 7.

## Interaction with FR-004 (unresolved)

This function is called ONLY when `candidate_matches.len() >= 1`. When R2 returns zero matches, the caller MUST short-circuit and record `unresolved_name` into `CrossEcosystemEdgesReport.unresolved` per FR-004. The tie-break function is not reached in that case.
