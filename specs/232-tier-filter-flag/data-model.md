# Phase 1 Data Model: `--tier=<mode>` output-filter flag

**Feature**: 232-tier-filter-flag
**Date**: 2026-08-10

Everything below is scoped to one new enum and one new helper. No changes to existing types.

## New enum: `TierMode`

Encodes the operator's tier-filter mode. Lives in `waybill-cli/src/cli/scan_cmd.rs` alongside the existing `EnrichSource` / `SbomSourceMode` enums.

| Variant | CLI value | Predicate on `sbom_tier: Option<String>` |
|---|---|---|
| `TierMode::All` | `all` (default) | All components retained (no-op filter). |
| `TierMode::SourceOnly` | `source-only` | Retain iff `sbom_tier == Some("source")`. |
| `TierMode::DesignOnly` | `design-only` | Retain iff `sbom_tier == Some("design")`. |
| `TierMode::SourceAndBinary` | `source-and-binary` | Retain iff `sbom_tier == Some("source")` OR `sbom_tier == Some("binary")`. |

Derives: `ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq`. `#[default]` on `All` per FR-002 byte-parity guarantee.

## New field on `ScanArgs`

Existing struct at `waybill-cli/src/cli/scan_cmd.rs` around line 500+ (the `ScanArgs` derive block). Add:

```rust
/// Milestone 232 (#660) — filter the emitted SBOM's component set
/// by `sbom_tier`. Default `all` preserves pre-232 behavior byte-
/// for-byte. See `specs/232-tier-filter-flag/spec.md` for the
/// mode inventory.
#[arg(long, value_enum, default_value_t = TierMode::All)]
pub tier: TierMode,
```

## New helper: `apply_tier_filter`

Signature:

```rust
fn apply_tier_filter(
    components: &mut Vec<ResolvedComponent>,
    relationships: &mut Vec<Relationship>,
    mode: TierMode,
)
```

Location: same file, sibling to the existing `--exclude-scope` block. Logic:

1. If `mode == TierMode::All`: early return (no-op). This branch takes zero cost when the flag is not set.
2. Otherwise:
   - Compute the predicate `fn tier_matches(tier: Option<&str>, mode: TierMode) -> bool`:
     - `All` → true
     - `SourceOnly` → `tier == Some("source")`
     - `DesignOnly` → `tier == Some("design")`
     - `SourceAndBinary` → `tier == Some("source") || tier == Some("binary")`
   - Compute the drop set: `dropped_purls: HashSet<String>` = every `c.purl.as_str()` for components where `!tier_matches(c.sbom_tier.as_deref(), mode)`.
   - Retain components not in the drop set.
   - Retain relationships whose `from` AND `to` are not in the drop set.
   - Log an INFO line with the drop count and the mode: `"applied --tier filter dropped=<N> mode=<mode>"`.
   - If the post-filter `components` is empty, log an additional WARN line noting the outcome (FR-008).

## State transitions

None — the filter is a pure transformation on the `components` + `relationships` slices in place. No persistent state.

## Validation rules

- `TierMode` MUST derive `ValueEnum` so clap accepts the four kebab-case CLI values.
- `apply_tier_filter` MUST NOT touch any field on `ResolvedComponent` other than reading `purl` (for the drop-set key) and `sbom_tier` (for the predicate).
- The dropped-PURL HashSet MUST use `String` (owned) values, not `&str`, because the borrow-checker prohibits holding references to `components` while `retain` mutates it.
- `relationships` retention MUST check BOTH `from` and `to` — dropping only one direction would leave dangling edges (FR-006 explicitly prohibited).

## Interaction with existing pipeline

The filter runs at scan_cmd.rs:~3200 (immediately after the existing `--exclude-scope` filter block at 3175–3199, before the format-builder dispatch). Order:

```text
(existing) deduplicate → reconcile_design_source_tiers → --exclude-scope filter → NEW: --tier filter → format-builder dispatch
```

Because each format builder runs `compute_graph_completeness` INTERNALLY over the incoming components + relationships slice, filtering pre-dispatch means:

- CDX `waybill:graph-completeness` reflects the filtered set.
- SPDX 2.3 `waybill:graph-completeness` reflects the filtered set.
- SPDX 3 `waybill:graph-completeness` reflects the filtered set.
- Every other document-scope annotation that iterates `components` (`waybill:workspaces-detected`, `waybill:cisa-2026-lifecycle`, tier-based counters) reflects the filtered set.

FR-007 is satisfied structurally by the ordering, with no emitter-side changes.

## Out-of-scope

- Adding `--tier=analyzed-only`, `--tier=file-only`, or other tier-family modes. Per Clarifications §1, follow-up milestones can add these; not scoped here.
- Filtering by non-`sbom_tier` axes (e.g., `--tier=<ecosystem>` or `--tier=<confidence-band>`). Different filter semantics; different flag; different milestone.
- Mutual-exclusion enforcement between `--tier` and any other flag. Per Clarifications §2, no CLI-parse-level exclusions are added.
