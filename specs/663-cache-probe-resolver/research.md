# Research: Local-cache-probe resolver

**Milestone**: 663 | **Date**: 2026-08-18

## R1 — Registry insertion strategy

**Decision**: Single `cache_probe` entry at priority 92 in `RESOLVER_REGISTRY`; internally dispatches to 6 per-ecosystem probes.

**Rationale**: Only 3 integer priorities (91, 92, 93) are free between the URL-pattern resolvers (94-100) and deps.dev (90). Six new entries won't fit. Single entry with internal per-ecosystem dispatch mirrors the `PackageDbEntry` reader pattern.

**Alternatives**: Six per-ecosystem entries (requires renumbering existing URL-pattern resolvers; large scope). Priority-space refactor (deferred).

## R2 — `ResolutionTechnique::LocalCacheHit` variant

**Decision**: Add new `LocalCacheHit` variant (documented confidence 0.92).

**Rationale**: `FilePathPattern` (0.70) already documents that confidence for the generic path resolver; bumping it breaks the existing tier. Cache-probe is semantically distinct ("ecosystem-authoritative cache path with structured coord extraction").

**Alternatives**: Reuse `FilePathPattern` with a resolver-level confidence override (rejected — requires distinguishing variant downstream anyway).

## R3 — Per-ecosystem cache-path shapes

**Decision**: Locked contract in `contracts/per-ecosystem-cache-shapes.md`. Summary:

| Ecosystem | Cache root default | Env-var override | Path → coord |
|---|---|---|---|
| Maven | `~/.m2/repository` | `M2_HOME` | `g/a/v/artifact-v.jar` |
| Go | `~/go/pkg/mod` | `GOMODCACHE`, `GOPATH` | `.../name@vX.Y.Z/...` |
| Cargo | `~/.cargo/registry` | `CARGO_HOME` | `cache/*/name-version.crate` |
| Ruby | `~/.gem/specs/rubygems.org%443` or `<bundler>/vendor/bundle` | `GEM_HOME` | `name-version.gemspec` |
| npm/pnpm | `~/.local/share/pnpm/store` or `<node_modules>/name/package.json` | `PNPM_STORE_DIR` | Path + bounded `package.json` read for version |
| Python | `<site-packages>/name-version.dist-info/METADATA` or `~/.cache/pip/wheels/*/name-version-py3-none-any.whl` | `PIP_CACHE_DIR` | Filename stem OR `METADATA`'s `Version:` header |

**Rationale**: Verified against SBOMit's per-ecosystem source files (per issue #605 references).

## R4 — Windows portability

**Decision**: Use `dirs::home_dir()` (in-workspace via `home` transitively) for `~` expansion. `std::env::var_os()` for env-var overrides. Both are portable.

**Rationale**: `dirs` is the de-facto Rust crate for cross-platform home-dir lookup; returns `%USERPROFILE%` on Windows.

**Alternatives**: Manual `%USERPROFILE%` handling (reinvents `dirs` poorly). `std::env::var("HOME")` (broken on Windows).
