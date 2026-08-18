# Probe interface contract

**Milestone**: 663

## Per-probe function signature

Every `EcosystemProbe` variant implements the same shape:

```rust
impl EcosystemProbe {
    pub(super) fn try_match(&self, path: &Path) -> Option<Purl> {
        match self {
            Self::Maven => try_match_maven(path),
            Self::Golang => try_match_golang(path),
            Self::Cargo => try_match_cargo(path),
            Self::RubyGems => try_match_rubygems(path),
            Self::NpmPnpm => try_match_npm_pnpm(path),
            Self::PyPi => try_match_pypi(path),
        }
    }
}
```

Each `try_match_*` fn returns `Some(Purl)` on a successful cache-hit extraction; `None` on decline (either "not my ecosystem" or Q1-clarification metadata-read failure).

## Decision tree per probe

```text
try_match_<ecosystem>(path):
    1. Compute the cache root(s) for this ecosystem (env-var lookup + fallback to dirs::home_dir()).
    2. If path does NOT start_with any cache root → return None (declines, next probe tries).
    3. If path DOES match:
        a. Try to extract the PURL from the path structure alone (Maven, Go, Cargo, Ruby).
        b. If the ecosystem needs a metadata file (npm, PyPi):
             i. Locate the metadata file path relative to the matched cache entry.
             ii. Read at most 64 KiB.
             iii. Parse for the version field.
             iv. If any step fails → log tracing::warn! + return None (Q1 clarification).
        c. Construct Purl. If Purl::new() rejects (invalid segments) → log warn + return None.
        d. Return Some(Purl).
```

**Invariant**: If a probe returns `Some(_)`, it MUST be a fully-formed PURL with valid `type`, `namespace` (if applicable), `name`, and `version`. No versionless or partial PURLs at confidence 0.92.

## Dispatch order (locked)

`CacheProbeResolver.probes` is populated in this order, iterated first-match-wins:

1. Maven
2. Golang
3. Cargo
4. RubyGems
5. NpmPnpm
6. PyPi

The order is deliberately stable — changing it may change which probe wins for adversarial edge-case paths. Any reorder requires an explicit spec change.

## Emit contract

When any probe returns `Some(purl)`:

- The resolver constructs a `ResolvedComponent` with:
  - `purl` = the returned PURL
  - `confidence` = 0.92 (from `CacheProbeResolver::confidence()`)
  - `evidence.technique` = `ResolutionTechnique::LocalCacheHit`
  - `extra_annotations.insert("waybill:resolver-tier", "local_cache_hit")`

## FR-011 cross-resolver deps.dev-skip guarantee

When cache-probe emits, the resolver chain SHORT-CIRCUITS for that input path — deps.dev is not called for it. This is FR-006 + SC-002: cache hits don't consume deps.dev API budget.

Enforced structurally by the existing m209 chain: the chain returns after the first resolver whose `resolve()` produces an entry. Cache-probe (priority 92) runs before deps.dev (priority 90), so a cache-probe hit exits the chain before deps.dev is invoked.
