# Data Model: Local-cache-probe resolver

**Milestone**: 663 | **Date**: 2026-08-18

## Entity: `CacheProbeResolver`

**Rust type**: `struct CacheProbeResolver` implementing the m209 `Resolver` trait.

### Fields

| Field | Type | Notes |
|---|---|---|
| `probes` | `Vec<EcosystemProbe>` | Ordered list; first-match-wins per Q2 registration order. |

### Trait impl

| Method | Value |
|---|---|
| `name()` | `"cache_probe"` |
| `priority()` | `92` (registered in `RESOLVER_REGISTRY` between rubygems=95 and deps_dev_hash=90; unique per FR-017 compile-time check) |
| `technique()` | `ResolutionTechnique::LocalCacheHit` (new variant) |
| `confidence()` | `0.92` |
| `handles(input, ctx)` | Returns `true` iff the input is a path-shaped resolve input. |
| `resolve(input, ctx)` | Iterates `probes` in order; first that matches emits; if none match, returns empty. |

## Entity: `EcosystemProbe`

Per-ecosystem sub-resolver. One of six enum variants at launch.

**Rust type**: `enum EcosystemProbe { Maven, Golang, Cargo, RubyGems, NpmPnpm, PyPi }`. Each variant has an associated `try_match(path: &Path) -> Option<Purl>` method.

**Dispatch order** (locked per registration; SC-005 preserves pre-m663 behavior for non-cache paths):

1. Maven
2. Golang
3. Cargo
4. RubyGems
5. NpmPnpm
6. PyPi

**Rationale**: Order by ecosystem-prefix specificity. Maven and Go have the most specific path prefixes (`.m2/repository/` and `go/pkg/mod/`). npm has the most ambiguous (any `node_modules/*/package.json`), so it goes near the end.

## Entity: Cache root

**Rust type**: `PathBuf`.

**Construction rules per ecosystem** (all portable via `dirs::home_dir()` + `std::env::var_os()`):

| Ecosystem | Construction |
|---|---|
| Maven | `env::var_os("M2_HOME")` → append `repository`; else `dirs::home_dir() + ".m2/repository"` |
| Go | `env::var_os("GOMODCACHE")` else `env::var_os("GOPATH") + "pkg/mod"` else `dirs::home_dir() + "go/pkg/mod"` |
| Cargo | `env::var_os("CARGO_HOME") + "registry"` else `dirs::home_dir() + ".cargo/registry"` |
| RubyGems | `env::var_os("GEM_HOME") + "specs/rubygems.org%443"` else `dirs::home_dir() + ".gem/specs/rubygems.org%443"`. Plus a Bundler variant: any path segment `vendor/bundle/ruby/*/gems`. |
| NpmPnpm | `env::var_os("PNPM_STORE_DIR")` else `dirs::home_dir() + ".local/share/pnpm/store"`. Plus a `node_modules` variant: any path segment `node_modules/*/package.json`. |
| PyPi | `env::var_os("PIP_CACHE_DIR") + "wheels"` else `dirs::home_dir() + ".cache/pip/wheels"`. Plus a `dist-info` variant: any path ending in `*.dist-info/METADATA` under any `site-packages`. |

**Invariant**: If `dirs::home_dir()` returns `None` (edge case on locked-down systems), the resolver falls back to matching only env-var-derived paths and declines everything else.

## Entity: Metadata read

For npm and PyPi ecosystems where the version isn't in the cache path itself.

**Rust type**: Per-probe fn — reads a single file, extracts a single field, decides emit-or-decline.

### npm `package.json`

- Read at most 64 KiB. Parse as JSON via `serde_json`.
- Extract `"version"` field.
- **Decline behavior (Q1)**: if file unreadable, or JSON invalid, or `"version"` field missing/non-string → log `tracing::warn!` with path + reason, return `None`.

### PyPi `dist-info/METADATA`

- Read at most 64 KiB. Parse line-by-line looking for `Version:` header (RFC 822 shape).
- **Decline behavior (Q1)**: if file unreadable, or no `Version:` header found → log `tracing::warn!`, return `None`.

## Entity: `ResolutionTechnique::LocalCacheHit` (variant)

**Location**: `waybill-common/src/resolution.rs::ResolutionTechnique`.

**Docstring**:

```rust
/// A file path matched an ecosystem-authoritative local cache root
/// (Maven `~/.m2/repository/`, Go `$GOMODCACHE`, Cargo `~/.cargo/
/// registry/`, etc.) and the resolver extracted the exact coord
/// from the path structure (± a bounded metadata read for npm/PyPi).
/// Confidence 0.92 — higher than deps.dev's 0.90 (because the
/// artifact IS on this machine) but lower than URL-pattern's 0.95
/// (which corroborates the cache with a network trace).
LocalCacheHit,
```

**Serde**: `#[serde(rename_all = "snake_case")]` (existing) yields `"local_cache_hit"` on the wire.

## Entity: `waybill:resolver-tier` annotation

**Wire location**: Per-component `properties[]` / `annotations[].comment` envelope.

**Value**: string. Closed enum matching `ResolutionTechnique::as_wire_str()`:

- `"url_pattern"` (existing URL-pattern resolvers — but NOT emitted today; new for m663)
- `"local_cache_hit"` (new — cache-probe)
- `"hash_match"` (deps.dev)
- `"file_path_pattern"` (generic path resolver)
- `"hostname_fallback"`

**Presence**: emitted on every component produced by ANY resolver, not just cache-probe. This is a broader transparency win — operators can grep for which resolver tier produced each component. Non-cache-probe resolvers get their existing `ResolutionTechnique` mapped through the same emit path.

**Design note**: FR-007 originally scoped this annotation to cache-probe-emitted components only. During plan-phase design review, expanded to universal per-component emission — cheaper (one call site in the emit pipeline vs branching per-resolver) AND more useful to operators. If universal emission is too broad for MVP, the MVP can scope to cache-probe-only and follow-on can broaden. Decision deferred to `/speckit.tasks`.

## State transitions

None — probing is stateless.
