# Resolver Trait + Chain (m209)

The trace-mode resolution pipeline
(`waybill-cli/src/resolve/pipeline.rs::ResolutionPipeline::resolve`) is
composed of a **chain of resolvers**, each an implementation of the
`Resolver` trait at `waybill-cli/src/resolve/resolver_trait.rs`. This
document is the contributor's reference for adding a new resolver.

For the broader resolution flow, see `docs/architecture/resolution.md`.

## Chain composition

Locked in `waybill-cli/src/resolve/resolver_chain.rs::RESOLVER_REGISTRY`:

| Resolver | Priority | Technique | Confidence |
|---|---|---|---|
| `cargo` | 100 | `UrlPattern` | 0.95 |
| `pypi` | 99 | `UrlPattern` | 0.95 |
| `npm` | 98 | `UrlPattern` | 0.95 |
| `golang` | 97 | `UrlPattern` | 0.95 |
| `maven` | 96 | `UrlPattern` | 0.95 |
| `rubygems` | 95 | `UrlPattern` | 0.95 |
| `deb` | 94 | `UrlPattern` | 0.95 |
| `deps_dev_hash` | 90 | `HashMatch` | 0.90 |
| `path` | 70 | `FilePathPattern` | 0.70 |
| `hostname_fallback` | 40 | `HostnameHeuristic` | 0.40 |

Chain dispatch is **priority-descending, first-match-wins per input**:
the highest-priority resolver whose `handles()` returns `true` for
the given input runs first; if it returns `Ok(non-empty)`, subsequent
resolvers are skipped for that input.

## Adding a new ecosystem resolver

Two file edits + one line addition to the registry.

### Step 1 — create the resolver file

`waybill-cli/src/resolve/resolvers/<name>.rs`. Use `cargo.rs` as the
template:

```rust
use std::future::Future;
use std::pin::Pin;

use waybill_common::resolution::{ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::{encode_purl_segment, Purl};

use super::common::{build_url_component, hostname_and_path};
use crate::resolve::resolver_trait::{
    ResolveContext, ResolveInput, Resolver, ResolverError,
};

pub(crate) struct NugetResolver;

impl Resolver for NugetResolver {
    fn name(&self) -> &'static str { "nuget" }
    fn priority(&self) -> u32 { 93 }  // between deb (94) and deps_dev_hash (90)
    fn technique(&self) -> ResolutionTechnique { ResolutionTechnique::UrlPattern }
    fn confidence(&self) -> f64 { 0.95 }

    fn handles(&self, input: &ResolveInput<'_>, _ctx: &ResolveContext<'_>) -> bool {
        matches!(
            input,
            ResolveInput::Connection { connection, .. }
                if hostname_and_path(connection).0 == "api.nuget.org"
        )
    }

    fn resolve<'a>(
        &'a self,
        input: &'a ResolveInput<'a>,
        _ctx: &'a ResolveContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ResolvedComponent>, ResolverError>> + Send + 'a>>
    {
        Box::pin(async move {
            let ResolveInput::Connection { connection, basename_to_file_op } = input else {
                return Ok(Vec::new());
            };
            let (hostname, path) = hostname_and_path(connection);
            let Some(purl) = extract_nuget_purl(hostname, path) else {
                return Ok(Vec::new());
            };
            let component = build_url_component(
                purl, connection, path, basename_to_file_op,
                self.technique(), self.confidence(),
            );
            Ok(vec![component])
        })
    }
}

fn extract_nuget_purl(hostname: &str, path: &str) -> Option<Purl> {
    // ... ecosystem-specific extraction logic ...
}
```

### Step 2 — register the module + priority

Add one line to `waybill-cli/src/resolve/resolvers/mod.rs`:

```rust
pub(crate) mod nuget;
```

Add one line to `RESOLVER_REGISTRY` at
`waybill-cli/src/resolve/resolver_chain.rs` (in priority-descending
order):

```rust
pub const RESOLVER_REGISTRY: &[(&str, u32)] = &[
    ("cargo",             100),
    // ... existing entries ...
    ("deb",                94),
    ("nuget",              93),   // <-- new
    ("deps_dev_hash",      90),
    // ... rest ...
];
```

And add one line to `ResolverChain::new_default` at the same file:

```rust
Box::new(resolvers::nuget::NugetResolver),
```

### Step 3 — verify

```sh
cargo +stable build -p waybill
```

If you accidentally reused an existing priority, `cargo build` fails
with `E0080` at the const-eval of `assert_registry_priorities_unique`
and the panic message points at `resolver_chain.rs`. Pick an unused
priority; try again.

```sh
cargo +stable test -p waybill -- resolve::resolvers::nuget
```

Runs only your new resolver's unit tests in isolation.

## Trait contract

Locked in
`specs/209-resolver-trait-chain/contracts/resolver-trait.md` (C-1
through C-5). Summary:

- **`name()`** — stable, snake-case, matches `RESOLVER_REGISTRY`.
- **`priority()`** — unique across all registered resolvers (enforced
  at compile time by `assert_registry_priorities_unique`).
- **`technique()`** — one of `ResolutionTechnique` variants; drives
  the emitted `ResolutionEvidence.technique` signal (SC-005 preserved
  by `technique_mapping_matches_contract_c4` test).
- **`confidence()`** — per-resolver constant that populates
  `ResolutionEvidence.confidence`.
- **`handles(input, ctx)`** — cheap O(1) filter; called before every
  `.await`-invocation of `resolve`.
- **`resolve(input, ctx)`** — async; returns `Result<Vec<ResolvedComponent>, ResolverError>`.
  - `Ok(vec![])` = clean no-match (chain continues to next resolver).
  - `Ok(non-empty)` = match (chain short-circuits for this input).
  - `Err(...)` = transient/internal failure (pipeline logs WARN +
    continues to next resolver).

## Panic + error semantics (FR-013)

The chain catches `Err` returns cleanly — a WARN log names the
resolver + the failure, and the chain continues. Panics inside a
resolver's `resolve` future are NOT currently caught; a panicking
resolver aborts the process (matches pre-refactor pipeline behavior).
Constitution Principle IV's no-panic-in-production discipline is the
primary guard against this. Panic-catch via `tokio::task::spawn` was
deferred; a follow-up milestone may add it as defense-in-depth.

## Object-safety note (RPITIT vs Pin<Box<dyn Future>>)

The `Resolver` trait uses `fn resolve(...) -> Pin<Box<dyn Future + Send>>`
rather than the more ergonomic `async fn resolve(...) -> ...` (RPITIT).
Rationale: RPITIT traits aren't object-safe on stable Rust, but the
chain requires `Vec<Box<dyn Resolver>>` for uniform dispatch. Cost:
~50 ns per resolve call (well under SC-004's 5% perf ceiling). Zero
new Cargo dependencies (no `async-trait` crate).

## Testing patterns

- **Per-resolver unit tests**: live in each resolver file's `mod tests`
  block. Test the extraction function's happy paths + rejection paths
  + the metadata accessors. See `cargo.rs::tests` for the reference
  pattern.
- **SC-003 timing**: use `super::tests_common::exposes::assert_sc003_timing_ok(start)`
  to enforce the 100 ms per-test wall-clock budget.
- **Chain-behavior tests**: live in `resolver_chain.rs::tests`.
- **Byte-identity (SC-001) harness**: `pipeline.rs::tests::sample_attestation_byte_identity_vs_legacy_oracle`
  compares chain output vs. the `#[cfg(test)]`-gated legacy oracle
  at `pipeline_legacy_reference.rs`. Rerun via
  `cargo test -p waybill --bin waybill resolve::pipeline::tests`.

## Related documents

- Feature spec: `specs/209-resolver-trait-chain/spec.md`
- Plan: `specs/209-resolver-trait-chain/plan.md`
- Trait contract: `specs/209-resolver-trait-chain/contracts/resolver-trait.md`
- Broader resolution architecture: `docs/architecture/resolution.md`
