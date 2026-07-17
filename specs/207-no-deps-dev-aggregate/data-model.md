# Data Model: Fix `--no-deps-dev` Flag UX

**Date**: 2026-07-17
**Purpose**: Document the 1 new flag field + 1 modified pure-function branch + 1 new WARN log site. No new types.

## E1: `ScanArgs.no_deps_dev_license` (NEW field)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs` — adjacent to existing `no_deps_dev` (line 599) and `no_deps_dev_graph` (line 636).

**Shape**:

```rust
/// Milestone 207 (#596) — skip deps.dev LICENSE enrichment only.
/// Keeps the deps.dev transitive dep-graph enrichment active. This
/// is the pre-m207 semantic of `--no-deps-dev` (which now disables
/// BOTH deps.dev paths). Scripts that relied on the pre-m207
/// behavior can migrate by renaming `--no-deps-dev` →
/// `--no-deps-dev-license` in their invocations.
///
/// Has no effect when `--offline` is set (offline suppresses all
/// enrichment paths). Overridden by `--enrich-sources` allowlist
/// mode when the operator supplies that flag.
#[arg(long)]
pub no_deps_dev_license: bool,
```

**Validation rules**:
- Boolean flag; default `false` (enrichment on).
- Composes cleanly with other `--no-*` flags: setting both `--no-deps-dev-license` and `--no-deps-dev-graph` produces the same effect as `--no-deps-dev` (aggregate disable).
- `--enrich-sources <list>` overrides this flag entirely per FR-004.

## E2: `resolve_enrich_sources` semantic change (MODIFIED function)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs:1631-1645`.

**Pre-m207 body** (default-mode branch at lines 1638-1644):

```rust
} else {
    EnrichConfig {
        deps_dev: !args.no_deps_dev,
        clearly_defined: !args.no_clearly_defined,
        deps_dev_graph: !args.no_deps_dev_graph,
    }
}
```

**Post-m207 body**:

```rust
} else {
    // Milestone 207 (#596): `--no-deps-dev` becomes an aggregate
    // disable (both license and dep-graph paths). Fine-grained
    // "license only" moves to the new `--no-deps-dev-license` flag;
    // fine-grained "graph only" remains `--no-deps-dev-graph`.
    // Composition: any of these flags being set suppresses its
    // respective path.
    EnrichConfig {
        deps_dev: !args.no_deps_dev && !args.no_deps_dev_license,
        clearly_defined: !args.no_clearly_defined,
        deps_dev_graph: !args.no_deps_dev && !args.no_deps_dev_graph,
    }
}
```

**Change semantics**:
- `deps_dev` (license path): OFF when `no_deps_dev` OR `no_deps_dev_license` is set. `no_deps_dev` alone suffices to disable it (aggregate); `no_deps_dev_license` alone also suffices to disable it (fine-grained "license only").
- `deps_dev_graph`: OFF when `no_deps_dev` OR `no_deps_dev_graph` is set. `no_deps_dev` alone suffices (aggregate); `no_deps_dev_graph` alone also suffices (fine-grained "graph only").
- `clearly_defined`: UNCHANGED per FR-004.
- Allowlist mode branch (lines 1632-1637): UNCHANGED per FR-004.

**Truth table** (default-mode branch):

| no_deps_dev | no_deps_dev_license | no_deps_dev_graph | deps_dev (license) | deps_dev_graph |
|---|---|---|---|---|
| false | false | false | true | true |
| **true** | false | false | **false** | **false** |
| false | true | false | false | true |
| false | false | true | true | false |
| true | true | false | false | false |
| true | false | true | false | false |
| false | true | true | false | false |
| true | true | true | false | false |

Row 2 is the m207 behavior change (bolded); pre-m207 it was `[false, true]`.

## E3: FR-006 migration-signal INFO log (NEW)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs` — at the scan entry point after `resolve_enrich_sources` runs (around line 2714 today; adjust to actual line at implement time).

**Emit condition** (R2 Option B): fires ONLY when the operator is using the aggregate flag alone (no fine-grained escape hatch also set):

```rust
if args.no_deps_dev && !args.no_deps_dev_license && !args.no_deps_dev_graph {
    tracing::info!(
        "--no-deps-dev now disables ALL deps.dev enrichment paths \
         (m207 aggregate semantic per #596). For the pre-m207 \"license \
         only\" behavior, use --no-deps-dev-license instead."
    );
}
```

**Validation rules**:
- Fires ONCE per scan (not per-component).
- Log message text pinned per research.md R2 — integration test greps stderr for the substring `"m207 aggregate semantic"`.
- INFO level (not WARN) — this is an advisory, not a failure.
- Suppressed by `RUST_LOG=warn` filter (operator's choice — they opted into a stricter filter, and the message is informational not corrective).

**Rationale**: only operators using `--no-deps-dev` alone are affected by the semantic change; adding the log for the fine-grained-aware operators would be pure noise.

## E4: `--help` text updates (MODIFIED doc-comments)

**Location**: `mikebom-cli/src/cli/scan_cmd.rs:587-593` (existing `--no-deps-dev` doc-comment).

**Pre-m207 text** (per recon at scan_cmd.rs:587):

> "Skip deps.dev license enrichment. Keeps ClearlyDefined and dep-graph enrichment active. This is the fastest enrichment source and rarely needs skipping; the allowlist via `--enrich-sources` is the alternative for full control. Has no effect when `--offline` is set."

**Post-m207 text**:

```rust
/// Milestone 207 (#596): AGGREGATE disable — skip BOTH the deps.dev
/// license enrichment AND the deps.dev transitive dep-graph
/// enrichment. Combines `--no-deps-dev-license` and
/// `--no-deps-dev-graph` semantics per operator expectation.
///
/// Pre-m207 this flag disabled only the license path. Scripts that
/// relied on that behavior can migrate by renaming to
/// `--no-deps-dev-license`.
///
/// Composition: `--no-deps-dev` (aggregate) OR `--no-deps-dev-license`
/// (license only) OR `--no-deps-dev-graph` (graph only). Fine-grained
/// flags allow surgical control. `--enrich-sources <list>` (allowlist
/// mode) overrides all `--no-*` flags. `--offline` suppresses all
/// enrichment paths regardless of `--no-*` flags.
#[arg(long)]
pub no_deps_dev: bool,
```

**Also**: update the existing `--no-deps-dev-graph` doc-comment at line 625-630 to add a note about the new companion flag:

```rust
/// Skip the deps.dev transitive dep-graph enrichment step ONLY.
/// Keeps deps.dev license enrichment and ClearlyDefined active.
///
/// Companion to `--no-deps-dev-license` (m207 #596) which does the
/// reverse (skip license, keep graph). Use `--no-deps-dev` for the
/// aggregate "skip both" semantic.
///
/// Has no effect when `--offline` is set.
#[arg(long)]
pub no_deps_dev_graph: bool,
```

## Cross-cutting: FR-004 (`--enrich-sources` allowlist unchanged)

**Guarantee**: The allowlist-mode branch (`resolve_enrich_sources` lines 1632-1637) is untouched. When the operator supplies `--enrich-sources <list>`, none of the `--no-*` flags apply — including the new `--no-deps-dev-license`. This mirrors the existing behavior for `--no-deps-dev-graph` and preserves the documented "allowlist wins" invariant.

**Test**: unit test `enrich_sources_allowlist_overrides_no_deps_dev` asserts that `--enrich-sources deps-dev --no-deps-dev` still enables the deps.dev license path (allowlist wins).
