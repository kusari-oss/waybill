# Data Model: m192 Operator-Root Graph-Completeness Fix

**Date**: 2026-07-14
**Scope**: In-process types touched by the fix. All in `mikebom-cli/src/generate/graph_completeness/bfs.rs`; zero cross-crate type changes.

## Entity: EcosystemRootSet (existing, UNCHANGED)

```rust
pub struct EcosystemRootSet {
    pub roots: HashSet<String>,
    pub ecosystems_without_root: Vec<String>,
}
```

Located at `mikebom-cli/src/generate/graph_completeness/bfs.rs:24-27`. No struct-level change. The fix only alters HOW `roots` and `ecosystems_without_root` are POPULATED — the shape of the struct + its downstream consumers (the classifier at `mod.rs:229-296`) are untouched.

Post-fix invariants (asserted by unit tests):
- **Native-root path (MainModule)**: `roots` and `ecosystems_without_root` are computed identically to pre-m192. Byte-identity guarantee.
- **Operator-override path**: `ecosystems_without_root` is EMPTY for scans whose `components[]` has any PURL-typed entries (excluding `generic`). `roots` contains the operator's `target_ref` PURL (once) — deduplicated by HashSet semantics.

## Function: `build_ecosystem_root_set` (existing, EXTENDED)

**Existing signature**:

```rust
pub(super) fn build_ecosystem_root_set(
    components: &[ResolvedComponent],
    selection: &RootSelectionResult,
) -> EcosystemRootSet
```

**Post-m192 signature**:

```rust
pub(super) fn build_ecosystem_root_set(
    components: &[ResolvedComponent],
    selection: &RootSelectionResult,
    target_ref: &str,
) -> EcosystemRootSet
```

The new `target_ref: &str` parameter is the same value the caller (`mod.rs::compute_graph_completeness`) already receives at line 145 — pure thread-through, no new value construction.

**Extended behavior**: after the existing per-ecosystem-root loop (lines 89-116), before the `ecosystems_without_root` computation (line 118), execute:

```rust
// Milestone 192: operator-override synthesis. When the primary
// selection subject is NOT a MainModule (operator supplied
// --root-name, or synthetic-placeholder root), the pre-existing
// per-ecosystem loop above populates NOTHING (there are no
// mikebom:component-role=main-module components to iterate).
// That leaves `ecosystems_without_root` covering every ecosystem
// present in components[], and the classifier at mod.rs:250 fires
// MultiEcosystemPartialRoot on any single orphan.
//
// Fix (spec FR-001 / FR-002): for every ecosystem present in
// components[], seed per_ecosystem_root with (ecosystem, target_ref)
// so the classifier trusts the operator's root as authoritative.
// Per Q2 answer A: skip the ecosystem that matches the target_ref's
// own PURL type (avoid duplicate root).
let is_native_root = matches!(
    selection.subject,
    ResolvedRootSubject::MainModule(_)
);
if !is_native_root {
    let operator_root_ecosystem: Option<String> =
        mikebom_common::types::purl::Purl::new(target_ref)
            .ok()
            .map(|p| p.ecosystem().to_string())
            .filter(|e| e != "generic");
    let mut synthesized_count = 0usize;
    for c in components {
        let eco = c.purl.ecosystem().to_string();
        if per_ecosystem_root.contains_key(&eco) {
            continue; // already covered by a native main-module
        }
        if operator_root_ecosystem.as_deref() == Some(eco.as_str()) {
            continue; // covered by the operator's root PURL itself
        }
        per_ecosystem_root.insert(eco, target_ref.to_string());
        synthesized_count += 1;
    }
    if synthesized_count > 0 {
        tracing::info!(
            synthesized_ecosystems_count = synthesized_count,
            "synthesized per-ecosystem placeholder roots for operator-override scan"
        );
    }
}
```

**Invariants**:
- Idempotent: applying twice equals applying once (HashMap semantics).
- Byte-identity guard: when `is_native_root == true`, the entire block is skipped. Zero delta.
- No side effects beyond `per_ecosystem_root` mutation + one INFO log line.

## Function: `compute_graph_completeness` (existing, MINOR EDIT)

**Change**: pass `target_ref` through to `build_ecosystem_root_set`. At `mod.rs:156`:

```rust
// Before
let mut root_set = bfs::build_ecosystem_root_set(components, selection);

// After
let mut root_set = bfs::build_ecosystem_root_set(components, selection, target_ref);
```

One-line change. The value is already in scope as a `&str` parameter at line 144.

## Entity: Operator-override synthesis log line (NEW via `tracing::info!`)

```
INFO mikebom::generate::graph_completeness::bfs: synthesized per-ecosystem placeholder roots for operator-override scan synthesized_ecosystems_count=N
```

Fires ONCE per scan when synthesis occurred (`synthesized_count > 0`). Silent when the operator-override path yielded zero new synthesized ecosystems (already-covered case or empty components).

## Downstream classifier (existing, UNCHANGED — behavioral consequence only)

The `MultiEcosystemPartialRoot` classifier at `mod.rs:229-253` reads `ecosystems_without_root` and fires ONLY when it's non-empty AND there are orphans. Post-fix, operator-override scans produce `ecosystems_without_root = []`, so the classifier is silent for that path even when orphans exist — the operator-override synthesis has told BFS to trust the operator's chosen root, and the orphans (if any) get classified as `OrphanedComponentsDetected` instead, per FR-007.
