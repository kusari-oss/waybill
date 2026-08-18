# Implementation Plan: Local-cache-probe resolver tier

**Branch**: `663-cache-probe-resolver` | **Date**: 2026-08-18 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/663-cache-probe-resolver/spec.md`

## Summary

Add a **cache-probe** resolver to `waybill-cli/src/resolve/` that slots between the URL-pattern resolvers (0.95) and the deps.dev-hash resolver (0.90) in the existing m209 `RESOLVER_REGISTRY`. Given a file path from an in-toto material/product entry, the resolver detects paths under six ecosystem-standard local cache locations (Maven `~/.m2/repository/`, Go `$GOMODCACHE`, Cargo `~/.cargo/registry/`, Ruby gem cache, npm/pnpm store, Python `site-packages` + wheel cache) and extracts a high-confidence PURL directly from the cache path structure (plus a bounded metadata read for npm + Python). Emits at confidence 0.92 with the `waybill:resolver-tier: "cache-probe"` annotation. Zero network. Zero new Cargo dependencies.

Per Q1 clarification: metadata-read failure → decline the match cleanly, log `tracing::warn!`, let downstream tiers (deps.dev) get their normal turn. Never emits at reduced confidence.

## Technical Context

**Language/Version**: Rust stable (workspace toolchain inherited from milestones 001–236; no nightly required).

**Primary Dependencies**: Existing only —

- **`std::env`** — env-var lookup for `GOMODCACHE`, `GOPATH`, `CARGO_HOME`, `GEM_HOME`, `PNPM_STORE_DIR`, `PIP_CACHE_DIR`, `M2_HOME`.
- **`std::path::Path` + `Path::starts_with`** — cache-prefix matching. NOT `canonicalize` (per spec Edge Cases — symlinked caches read verbatim).
- **`dirs` crate** — reused (already transitively in the workspace via `home`) for cross-platform home-dir lookup (`~/.m2` → `<home>/.m2` on Linux/macOS; `%USERPROFILE%\.m2` on Windows).
- **`serde_json`** — bounded metadata read for `package.json` (npm `"version"` field extraction).
- **`serde` + custom parser** — `dist-info/METADATA` is RFC 822-shape; parse the single `Version:` header.
- **Existing `Resolver` trait** at `waybill-cli/src/resolve/resolver_trait.rs` (m209).
- **`ResolutionTechnique`** enum extension in `waybill-common/src/resolution.rs` — add `LocalCacheHit` variant. **This is one small breaking change to `waybill-common`** (adds a variant); mitigated because the enum is `#[non_exhaustive]`-friendly OR by the fact that no external consumer imports it (`waybill-common` is workspace-internal per Principle VI).
- **`tracing`**, **`anyhow`**, **`thiserror`** — pervasive.

**Zero new Cargo dependencies.**

**Storage**: N/A — path-parsing + tiny metadata reads are stateless.

**Testing**: `cargo test --workspace`. Per-ecosystem unit tests use `tempfile::tempdir()` scaffolding of synthetic cache directory shapes (`waybill-fixture-*` names per project convention). Cross-ecosystem integration test at `waybill-cli/tests/cache_probe_universal.rs`.

**Target Platform**: Linux + macOS + Windows via CI matrix (m100 established). Every probe MUST work identically across the three. Windows path handling uses `PathBuf::components` semantics; env-var expansion uses `std::env::var_os` (portable).

**Project Type**: `waybill-cli` internal resolver. Zero CLI-flag additions. Zero `sbom scan` walker impact.

**Performance Goals**: ≤5 ms overhead per attested path (SC-006). Warm filesystem; single `Path::starts_with` per probe (7 probes → ≤7 O(prefix) comparisons ≤ 1 µs) + optional single small-file read (≤4 KB `package.json` / `METADATA`) per positive match.

**Constraints**:
- Zero network calls (FR-003)
- No `canonicalize` before matching (Edge Cases)
- No directory walking or binary-artifact reads
- FR-004: env-var overrides honored

**Scale/Scope**: 6 ecosystems, 7 probe implementations (Cargo has 2 sub-paths: `registry/cache/*.crate` + `registry/src/*/`). Per-probe implementation ≤ ~80 lines.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Applicable principles

- **I. Pure Rust, Zero C** — ✅ Rust-only; zero new deps.
- **III. Fail Closed** — ✅ Per Q1 clarification: decline on metadata failure, log warn, pipeline continues. No silent success at reduced confidence.
- **IV. Type-Driven Correctness** — ✅ `EcosystemProbe` newtype-style per-probe struct (not raw closures). Cache prefix is a `PathBuf` (not raw `&str`). New `ResolutionTechnique::LocalCacheHit` enum variant (not a magic string).
- **V. Specification Compliance (Standards-Native Audit)** — ✅ Per-component `waybill:resolver-tier` annotation. Standards-native audit: **KEEP-NO-NATIVE** — no CDX/SPDX carrier for "which resolver tier produced this component's identity"; similar to C104/C118/C147 doc-scope resolver-tier signals but scoped per-component here since resolution is per-input-path.
- **VI. Three-Crate Architecture** — ✅ New `cache_probe_resolver` module in `waybill-cli/src/resolve/`. Adds `LocalCacheHit` to `ResolutionTechnique` in `waybill-common` (single-variant addition; `waybill-common` is workspace-internal — no external breakage).
- **VII. Test Isolation** — ✅ Per-probe unit tests use `tempfile::tempdir()`; cross-probe integration test uses `apply_fake_home_env` pattern from m235 precedent.
- **VIII. Completeness** — ✅ Extends the attestation-consumer-side resolution coverage.
- **IX. Accuracy** — ✅ 0.92 is HONEST — reflects the confidence gap between "URL was captured in a network trace" (0.95) and "cache path structure names the coord unambiguously" (still very high, but one less signal than a network event).
- **X. Transparency** — ✅ New per-component annotation surfaces the resolver-tier decision.

### Verdict

**Gate passes.** No principle-level tension.

## Project Structure

### Documentation (this feature)

```text
specs/663-cache-probe-resolver/
├── plan.md              # This file
├── research.md          # Phase 0: R1-R4
├── data-model.md        # Phase 1: probe structs + registry
├── contracts/           # Phase 1: probe interface + per-ecosystem cache-path specs
├── quickstart.md        # Phase 1: verify + add-new-ecosystem recipe
└── tasks.md             # Phase 2: /speckit.tasks output
```

### Source Code (repository root)

```text
waybill-cli/
├── src/
│   └── resolve/
│       ├── resolvers/
│       │   ├── cache_probe.rs               # NEW — the resolver + registry
│       │   └── cache_probe/                 # NEW — per-ecosystem probe modules
│       │       ├── mod.rs
│       │       ├── maven.rs                 # US1
│       │       ├── golang.rs                # US1
│       │       ├── cargo.rs                 # US2
│       │       ├── rubygems.rs              # US2
│       │       ├── npm.rs                   # US3 (npm + pnpm)
│       │       └── pypi.rs                  # US3
│       ├── resolver_chain.rs                # MODIFY: add "cache_probe" to RESOLVER_REGISTRY at priority 92
│       └── (untouched: hash_resolver.rs, path_resolver.rs, hostname_resolver.rs, resolver_trait.rs, pipeline.rs)
├── tests/
│   └── cache_probe_universal.rs             # NEW: SC-001..SC-005 integration coverage
└── (m071 parity extractors — extend for the new per-component annotation)

waybill-common/
└── src/
    └── resolution.rs                        # MODIFY: add ResolutionTechnique::LocalCacheHit
```

**Structure Decision**: Follow the existing per-ecosystem-file pattern in `waybill-cli/src/resolve/resolvers/`. The `cache_probe.rs` module is a SINGLE resolver in the registry (priority 92); it internally dispatches to per-ecosystem probe functions organized as sibling files under `cache_probe/`. This gives one registry entry with 6 focused probes — cleaner than 6 separate registry entries competing for u32 priority slots between 90 and 95.

## Phase 0: Outline & Research

### R1 — Registry insertion strategy: single entry vs six per-ecosystem entries

**Question**: Should the m663 resolver register as ONE entry in `RESOLVER_REGISTRY` or SIX (one per ecosystem, matching the existing cargo/pypi/npm/… pattern)?

**Decision**: **Single `cache_probe` entry at priority 92.** Internally dispatches to per-ecosystem probes via a first-match-wins loop.

**Rationale**:
- Existing 7 per-ecosystem URL-pattern resolvers already occupy priorities 94-100. Only 3 integer priorities (91, 92, 93) are free between them and deps.dev (90). Six new entries won't fit.
- A single entry keeps the registry compact and preserves the priority ladder's readability.
- Per-ecosystem probes remain code-organized as sibling files under `cache_probe/`, so per-ecosystem debuggability is preserved.
- The `EcosystemProbe` enum + dispatch pattern mirrors how the existing `PackageDbEntry` readers organize per-ecosystem logic in one dispatch.

**Alternatives considered**:
- (a) Six per-ecosystem registry entries with priorities 91-96 (would collide with existing rubygems=95, deb=94; would require renumbering the 7 URL-pattern resolvers, cascading breaking changes to test expectations).
- (b) Priority space refactor to open a wider gap (large scope; deferred).

### R2 — `ResolutionTechnique::LocalCacheHit` variant addition

**Question**: What's the right `ResolutionTechnique` for cache-probe? Reuse `FilePathPattern` (0.70), or add a new variant?

**Decision**: Add `ResolutionTechnique::LocalCacheHit` variant with docstring documenting confidence 0.92.

**Rationale**:
- `FilePathPattern` (0.70) is documented at that confidence in `waybill-common/src/resolution.rs:315`. Bumping its associated confidence to 0.92 would break the existing generic `path` resolver's tier.
- A new variant is semantically honest: "this file lives under an ecosystem-authoritative cache path with structured coord extraction," distinct from "this file matches a generic file-path regex."
- `waybill-common` is workspace-internal (Constitution Principle VI) — variant addition is not an external breaking change.

**Alternatives considered**:
- Reuse `FilePathPattern` with a confidence override at the resolver level: rejected — resolver `confidence()` is fn-level, not per-emission; would require adding a variant anyway to distinguish downstream.

### R3 — Per-ecosystem cache-path shapes + version-extraction locations

**Question**: For each of the 6 ecosystems, what's the exact cache-path structure and where does the version come from?

**Decision**: Locked table below. See `contracts/per-ecosystem-cache-shapes.md` for the full spec.

| Ecosystem | Default cache root | Env-var override | Path shape → PURL | Version source |
|---|---|---|---|---|
| Maven | `<home>/.m2/repository` | `M2_HOME` (POSIX only; Windows uses default) | `.../g1/g2/.../artifact/version/artifact-version.jar` | Path segments (penultimate = version) |
| Go | `<home>/go/pkg/mod` | `GOMODCACHE`, `GOPATH/pkg/mod` | `.../namespace/pkg@vX.Y.Z/...` | `@vX.Y.Z` suffix on the last coord dir |
| Cargo | `<home>/.cargo/registry` | `CARGO_HOME` | `.../cache/<registry-hash>/name-version.crate` OR `.../src/<registry-hash>/name-version/` | Filename stem `name-version` (regex-split on last `-`) |
| Ruby gems | `<home>/.gem/specs/rubygems.org%443` OR `<bundler>/vendor/bundle/ruby/*/gems` | `GEM_HOME` | `.../name-version.gemspec` or `.../name-version/` | Filename stem (same regex-split as cargo) |
| npm/pnpm | `<home>/.local/share/pnpm/store/*/files` OR `<node_modules>/name/package.json` OR `<node_modules>/@scope/name/package.json` | `PNPM_STORE_DIR` | Path → name; **`package.json` bounded read** → version | `package.json`'s `"version"` field |
| Python | `<site-packages>/name-version.dist-info/METADATA` OR `<pip_cache>/wheels/.../name-version-py3-none-any.whl` | `PIP_CACHE_DIR` | Filename stem OR dist-info dir name → name+version | Filename stem OR `METADATA`'s `Version:` header |

**Rationale**: Verified against SBOMit's per-ecosystem source files (referenced in the issue). All 6 shapes are stable and well-documented in each ecosystem's tooling docs.

**Alternatives considered**: None — these shapes are canonical.

### R4 — Windows tilde + env-var portability

**Question**: How does the resolver expand `~/.m2/repository` on Windows where there's no `$HOME`?

**Decision**: Use `dirs::home_dir()` (already transitively in the workspace via `home`) which returns `%USERPROFILE%` on Windows. All cache-root construction routes through `dirs::home_dir()` + fixed suffix. Env-var overrides use `std::env::var_os()` which is portable.

**Rationale**: `dirs` is the de-facto Rust crate for cross-platform home-dir lookup. Waybill uses `home` transitively; `dirs` is compatible.

**Alternatives considered**:
- Manual `%USERPROFILE%` handling: rejected — reinvents `dirs` poorly.
- `std::env::var("HOME")`: broken on Windows.

## Phase 1: Design & Contracts

### 1. Data model

See `data-model.md` — the `EcosystemProbe` enum, cache-root construction, and per-probe dispatch pattern.

### 2. Contracts

- **`contracts/probe-interface.md`** — the per-probe function signature, the "decline vs emit" decision tree (per Q1 clarification), and the annotation contract.
- **`contracts/per-ecosystem-cache-shapes.md`** — the R3 locked table with concrete example paths + expected PURLs per ecosystem.

### 3. Quickstart

See `quickstart.md` — how to verify shipped behavior + add-new-ecosystem recipe.

### 4. Agent context

Run `.specify/scripts/bash/update-agent-context.sh claude` after this plan is committed to add the milestone-663 line to `CLAUDE.md` Active Technologies.

## Complexity Tracking

*None.* Adds one `ResolutionTechnique` variant (workspace-internal), one `RESOLVER_REGISTRY` entry, six per-ecosystem probe modules, one integration-test file, one parity-extractor row. Zero new Cargo deps. Zero new CLI flags. Zero subprocess calls. Zero network access.
