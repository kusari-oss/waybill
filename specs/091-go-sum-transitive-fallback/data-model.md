# Data Model — milestone 091 go.sum-fallback step 5

This is an internal-library extension with no new domain types beyond the typed enum variant the new ladder step needs. The "model" lives entirely in the milestone-055 `GraphResolver` types at `mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`.

## Entities

### `ResolutionStep` enum (extended)

Existing enum at `graph_resolver.rs:64`. This milestone adds one variant:

```rust
pub enum ResolutionStep {
    GoModGraph,         // step 1 (existing)
    GoModCache,         // step 2 (existing)
    Proxy,              // step 3 (existing)
    GoSumFallback,      // NEW — step 5 (per milestone 091)
    None,               // step 6 (was step 4) — empty fallthrough
}
```

**Validation rules**:
- VR-091-001: `ResolutionStep::GoSumFallback` MUST appear in the enum BEFORE `None` (so the `Display` impl + match-exhaustiveness ordering is the same as the ladder execution order).
- VR-091-002: every existing usage of `ResolutionStep::None` MUST continue to compile + behave identically (FR-005 invariant).

### `LadderSummary` struct (extended)

Existing struct at `graph_resolver.rs:154`. This milestone adds one counter field:

```rust
pub struct LadderSummary {
    pub graph_count: usize,           // step 1 hits
    pub cache_count: usize,           // step 2 hits
    pub proxy_count: usize,           // step 3 hits
    pub gosum_fallback_count: usize,  // NEW — step 5 hits
    pub missing_count: usize,         // step 6 (formerly step 4) hits
}
```

**Validation rules**:
- VR-091-003: `Display` impl for `LadderSummary` MUST include the new `gosum_fallback_count` in the same `field=value field=value` format as the existing counters. The post-resolver `tracing::info!` line at `graph_resolver.rs:368` prints the summary; new field shows up there.
- VR-091-004: `Default` impl MUST initialize `gosum_fallback_count = 0` (existing pattern; trivially derived).
- VR-091-005: `gosum_fallback_count + missing_count + graph_count + cache_count + proxy_count` MUST equal the total module count emitted into the resolver map post-resolve.

### `step5_go_sum_fallback` method (new)

Added to `impl GraphResolver` at `graph_resolver.rs`. Inserted between `step3_proxy_fetch` and `step4_empty_fallthrough` in the call chain at `GraphResolver::resolve`.

**Signature**:
```rust
fn step5_go_sum_fallback(&self, map: &mut ModuleGraphMap, ctx: &WorkspaceContext);
```

**Behavior contract**:
- Iterates `ctx.go_sum_modules` (existing field of type `&[ModuleId]`).
- For each module already present in `map` (via steps 1, 2, or 3): no-op (preserves the higher-fidelity entry).
- For each module NOT present: insert a new `ModuleGraphEntry` with `module = <iter-item>`, `requires = vec![]`, `source = ResolutionStep::GoSumFallback`. Increment `map.summary_mut().gosum_fallback_count`.
- After the iteration: insert a synthetic root-module entry. Build a `ModuleId` from `ctx.root_module_path` (existing field) + the workspace's resolved version (or empty if not declared). Set `requires = ctx.go_sum_modules.iter().cloned().collect()` (the full closure).
- The root entry's `source = ResolutionStep::GoSumFallback` so the per-component provenance discriminator carries through to the root if it's emitted as a component (note: for typical workspace projects, the root IS the project being scanned and isn't double-emitted; for transitives that ARE discovered via step 5 only, they get the discriminator).

**Validation rules**:
- VR-091-006: step 5 MUST be a no-op (zero entries inserted, zero counter increment) when `ctx.go_sum_modules.is_empty()`.
- VR-091-007: step 5 MUST NOT modify entries already in `map` from steps 1–3. Verified by per-module `if map.contains(module) { continue; }` guard.
- VR-091-008: step 5 MUST run BEFORE `step4_empty_fallthrough` (renamed conceptually to "step 6"). The `step4_empty_fallthrough` call is unchanged but its method name MAY stay `step4_empty_fallthrough` in the source for git-blame stability — the docstring updates to clarify it's now the post-step-5 fallback.
- VR-091-009: step 5 MUST emit edges from the root module to every entry in `ctx.go_sum_modules`, NOT just the ones it newly inserted. The root entry's `requires` list is the full deduped closure regardless of which step claimed each transitive.

### Per-component provenance carrier (existing milestone-084 mechanism)

No schema change here. Reuses the existing milestone-084 `mikebom:resolver-step` field across CDX 1.6 / SPDX 2.3 / SPDX 3, extending the value enum:

| Format | Field | Existing values | New value |
|--------|-------|-----------------|-----------|
| CDX 1.6 | `Component.evidence.identity[].methods[].technique` + `confidence` | `manifest-analysis` (0.85) | `manifest-analysis` (0.50) for step-5 components |
| CDX 1.6 | `Component.properties[].name = "mikebom:resolver-step"` | `go-mod-graph`, `go-mod-cache`, `proxy`, `none` | `go-sum-fallback` |
| SPDX 2.3 | `package.annotations[].comment` | `mikebom:resolver-step=<step>` with same value space | `mikebom:resolver-step=go-sum-fallback` |
| SPDX 3 | `Annotation.statement` | same | `mikebom:resolver-step=go-sum-fallback` |

**Validation rules**:
- VR-091-010: every component reached via step 5 (i.e., every `ModuleGraphEntry` with `source = ResolutionStep::GoSumFallback`) MUST emit `mikebom:resolver-step = go-sum-fallback` in its CDX `Component.properties[]`, SPDX 2.3 `package.annotations[]`, and SPDX 3 `Annotation.statement`.
- VR-091-011: the CDX `Component.evidence.identity[].methods[].confidence` for step-5 components MUST be ≤ 0.50 (signal of lower fidelity per CDX 1.6 §6.4 guidance).
- VR-091-012: components reached via steps 1–3 MUST NOT carry the `go-sum-fallback` value (so the discriminator's presence is meaningful).

### Test fixture (no schema change; plumbing unchanged)

The audit fixture lives in the `mikebom-test-fixtures` repo at `transitive_parity/go/` per milestone 090. mikebom's test code resolves it via `MIKEBOM_FIXTURES_DIR` (set by `mikebom-cli/build.rs`).

**Validation rules**:
- VR-091-013: `mikebom-cli/tests/transitive_parity_go.rs` baseline `EXPECTED_MIKEBOM_EDGE_COUNT` bumps from 31 to ≥130 (exact value derived from the post-091 smoke test). Standard milestone-083 baseline-bump pattern.
- VR-091-014: at least one new entry in `EXPECTED_REPRESENTATIVE_EDGES` MUST be a root-to-go-sum-only-transitive edge that exercises step 5. PURL-prefix matching per the existing convention.
