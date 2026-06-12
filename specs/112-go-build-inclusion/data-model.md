# Data Model: Go Build-Inclusion Clarity

**Feature**: 112-go-build-inclusion | **Date**: 2026-06-11

## New types

### `BuildInclusion` (mikebom-common, `resolution.rs`)

```rust
/// Build-inclusion status for a source-tier component whose
/// participation in a production build was either ruled out by
/// package-level analysis or could not be determined.
/// Absence (`None`) means production participation is confirmed or
/// assumed (pre-feature semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildInclusion {
    /// Discovered only via go.sum fallback / orphan flat-attach and
    /// confirmed by no higher-fidelity signal.
    Unknown,
    /// Package-level analysis determined the main module's production
    /// build does not need this module.
    NotNeeded,
}
```

Constitution IV: typed enum, never raw strings across boundaries.
String forms (`"unknown"`, `"not-needed"`) appear only at emission.

### Field additions

- `PackageDbEntry.build_inclusion: Option<BuildInclusion>` (default `None`)
- `ResolvedComponent.build_inclusion: Option<BuildInclusion>` (flows through
  the existing PackageDbEntry → ResolvedComponent mapping, same as
  `lifecycle_scope`)

`None` everywhere ⇒ byte-identical pre-feature emission (FR-008).

### `GoModWhyVerdict` (mikebom-cli, new module `golang/mod_why.rs`)

```rust
pub(crate) enum GoModWhyVerdict {
    /// Non-empty import chain with no test node.
    ProdNeeded,
    /// Shortest chain passes through a `.test`-suffixed package.
    TestOnly,
    /// "(main module does not need module X)".
    NotNeeded,
    /// Per-module parse/query failure — leaves entry for the
    /// unknown-marker pass.
    Unresolved,
}
```

### `GoModWhyOutcome` (per-scan analysis result, for FR-013 logging)

| Field | Type | Meaning |
|---|---|---|
| `analyzed` | usize | modules queried |
| `prod_needed` | usize | verdict ProdNeeded |
| `test_only` | usize | verdict TestOnly → `LifecycleScope::Test` |
| `not_needed` | usize | verdict NotNeeded → `BuildInclusion::NotNeeded` |
| `unresolved` | usize | per-module failures |
| `skipped_reason` | Option<SkipReason> | whole-analysis skip (no toolchain / disabled / offline-failure / budget-exhausted / subprocess-error / unresolvable-packages — `go list all` reliability-preflight failure per contracts/go-toolchain-invocation.md; NotNeeded verdicts are never accepted without a passing preflight) |

## State transitions (per golang source-tier entry)

```text
                       ┌────────────────────────────────────────────┐
                       │ entry after existing G3/G4 filters          │
                       └────────────────────────────────────────────┘
                                         │
              ┌──────────────────────────┼──────────────────────────┐
              ▼                          ▼                          ▼
   BuildInfo-confirmed          go-mod-why verdict           no verdict AND
   (binary present, no          (toolchain available)        fallback-discovered
   mikebom:not-linked)               │                            │
              │              ┌───────┼────────┐                   ▼
              ▼              ▼       ▼        ▼          BuildInclusion::Unknown
        FR-010: never   ProdNeeded TestOnly NotNeeded    + derivation annotation
        Unknown or       (no-op)   scope=   BuildInclusion::
        NotNeeded                  Test     NotNeeded
                                   + deriv  + deriv
```

Precedence (evidence hierarchy, spec Key Entities):
`BuildInfo > go-mod-why > module-graph reachability > fallback`.
Rules:

1. BuildInfo-confirmed ⇒ `build_inclusion` stays `None` regardless of
   toolchain verdict (FR-010).
2. `mikebom:not-linked: true` MAY coexist with `NotNeeded` (consistent).
3. Existing `LifecycleScope::Test` (direct import / #332 closure) is
   never downgraded by a ProdNeeded verdict; a TestOnly verdict updates
   the derivation annotation to `go-mod-why`.
4. A classified entry (any verdict except Unresolved) is never marked
   `Unknown`.
5. Main-module entries (`mikebom:component-role: main-module`) are
   exempt from all passes.

## Emission mapping (see contracts/annotations.md for full table)

| `build_inclusion` | CDX scope | CDX property | SPDX 2.3 / SPDX 3 annotation |
|---|---|---|---|
| `None` | unchanged (pre-feature) | — | — |
| `Unknown` | absent | `mikebom:build-inclusion: unknown` | same key/value |
| `NotNeeded` | `excluded` (unconditional — bypasses the include-dev gate at builder.rs:599) | `mikebom:build-inclusion: not-needed` + `mikebom:build-inclusion-derivation: go-mod-why` | same keys/values |

## Validation rules

- `BuildInclusion::NotNeeded` requires a derivation annotation
  (`go-mod-why` is the only producer in this milestone).
- `BuildInclusion::Unknown` and `LifecycleScope::Test` are mutually
  exclusive on one entry (a test verdict is a classification).
- `NotNeeded` entries are never removed by scope filtering
  (clarification 2026-06-11); test-tagged entries follow pre-existing
  drop semantics.
- Component count: unknown/not-needed transitions never add or remove
  entries (FR-011).
