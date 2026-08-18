# Feature Specification: Local-cache-probe resolver tier

**Feature Branch**: `663-cache-probe-resolver`
**Created**: 2026-08-17
**Status**: Draft
**Input**: User description: Add a new resolver tier between URL and deps.dev that probes local ecosystem caches (`~/.m2`, `$GOMODCACHE`, `~/.cargo`, `~/.gem`, npm/pnpm stores, Python site-packages) for high-confidence PURL extraction without network. Attestation-consumer-side only. Closes GitHub issue #605.

## Context (informational)

Waybill's attestation-resolution pipeline at `waybill-cli/src/resolve/pipeline.rs` resolves file paths from in-toto material/product entries into components via a four-tier confidence chain:

| Order | Resolver | Confidence | Network | Notes |
|---|---|---|---|---|
| 1 | URL resolver | 0.95 | ✓ | URL pattern match from attested network events |
| 2 | Hash resolver (deps.dev) | 0.90 | ✓ | Blocked in air-gapped CI |
| 3 | Path resolver | 0.70 | — | Generic path pattern match |
| 4 | Hostname fallback | 0.40 | ✓ | Coarse |

The gap: `cargo build --offline`, `mvn --offline`, `go build` with a pre-warmed Go module cache produce no network events, so those components silently drop out of the attestation and downstream resolution falls to path-pattern (0.70) or hostname (0.40) instead of finding the coord that IS present in the cache itself. Even with waybill's `--offline` flag set, no code path today reads the local ecosystem caches.

Precedent is SBOMit's file-path-first offline-only resolver model: per-ecosystem resolvers extract identity from cache paths directly (Maven GAV from `~/.m2/repository/g/a/v/…`, Go coord from `$GOMODCACHE/…@v…`, etc.). Zero network. High confidence from the cache path structure. Waybill can adopt the same technique.

## Clarifications

### Session 2026-08-18

- Q1: When metadata extraction fails for ecosystems that need a bounded metadata read (npm `package.json`, Python `dist-info/METADATA`), what does the cache-probe resolver do? → A: **Decline the match cleanly.** Log `tracing::warn!` naming the path + failure reason and fall through to the next resolver (deps.dev). Preserves the "0.92 = high-confidence cache hit" invariant — the resolver never emits at 0.92 without full confidence in both name AND version. Downstream tiers get their normal turn on the path.

- Q2: Should the `waybill:resolver-tier` per-component annotation be scoped to cache-probe-emitted components only, or emitted universally by every resolver? → A: **Universal per-component emission.** Every resolver (URL-pattern per-ecosystem resolvers, cache-probe, deps.dev-hash, generic path, hostname-fallback) tags its emitted components with `waybill:resolver-tier: <technique>` where `<technique>` matches `ResolutionTechnique::as_wire_str()`. Broader operator visibility ("which tier produced each component") at negligible cost — one call site in the emit path, no per-resolver branching.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Maven + Go cache-hit resolution (Priority: P1) 🎯 MVP

**Description**: When an in-toto attestation names a file path under `~/.m2/repository/` or `$GOMODCACHE`, the resolver extracts the exact Maven GAV or Go module coord from the cache path structure and emits the component at confidence 0.92 (higher than the deps.dev tier below it, lower than the URL tier above it). Runs in the pipeline between URL resolution and deps.dev.

**Why this priority**: Maven and Go are the two highest-volume ecosystems in enterprise builds. Landing them first gives immediate operator value in the air-gapped CI case.

**Independent Test**: Feed a fixture attestation naming paths under a synthetic `~/.m2/repository/com/example/waybill-fixture-lib/1.0.0/waybill-fixture-lib-1.0.0.jar` and `$GOMODCACHE/example.com/waybill/fixture@v2.0.0/`. Run `waybill trace verify` (or the equivalent resolution entry point). Assert the emitted components carry the expected PURLs (`pkg:maven/com.example/waybill-fixture-lib@1.0.0` and `pkg:golang/example.com/waybill/fixture@v2.0.0`) at confidence 0.92 from the cache-probe resolver.

**Acceptance Scenarios**:

1. **Given** an in-toto material entry naming `<mvn_cache>/repository/org/apache/commons/commons-lang3/3.12.0/commons-lang3-3.12.0.jar`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:maven/org.apache.commons/commons-lang3@3.12.0"` and `confidence = 0.92`, and the deps.dev resolver is NOT called for this path.

2. **Given** an in-toto material entry naming `<gomodcache>/github.com/user/pkg@v1.2.3/main.go`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:golang/github.com/user/pkg@v1.2.3"` and `confidence = 0.92`.

3. **Given** the attestation contains a path that does NOT match any ecosystem cache prefix, **When** the resolver pipeline runs, **Then** the cache-probe resolver declines this path and the deps.dev resolver is called instead (existing pre-m663 behavior preserved).

4. **Given** the `$GOMODCACHE` env var is set to a non-default location (e.g., `/opt/go-cache/pkg/mod`), **When** the resolver pipeline runs, **Then** the cache-probe resolver honors the env var and correctly extracts coords from that location.

---

### User Story 2 — Cargo + Ruby cache-hit resolution (Priority: P2)

**Description**: Extend the resolver to Cargo's crate cache (`~/.cargo/registry/cache/…` + `~/.cargo/registry/src/…`) and Ruby's gem cache (`~/.gem/specs/…` + Bundler's `vendor/bundle/…`).

**Why this priority**: Second-tier volume ecosystems in enterprise builds. Cargo is critical for security-focused Rust projects; Ruby is critical for Rails / legacy Ruby CI.

**Independent Test**: For each of Cargo and Ruby, fixture attestations naming paths under a synthetic cache. Assert emitted components' PURLs match the expected shape at confidence 0.92.

**Acceptance Scenarios**:

1. **Given** an in-toto material entry naming `<cargo_home>/registry/cache/github.com-1ecc6299db9ec823/serde-1.0.100.crate`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:cargo/serde@1.0.100"` and `confidence = 0.92`.

2. **Given** an in-toto material entry naming `<gem_home>/specs/rubygems.org%443/waybill-fixture-gem-1.2.3.gemspec` OR `<bundler>/vendor/bundle/ruby/3.1.0/gems/waybill-fixture-gem-1.2.3/`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:gem/waybill-fixture-gem@1.2.3"` and `confidence = 0.92`.

3. **Given** the `CARGO_HOME` env var is set to a non-default location, **When** the resolver pipeline runs, **Then** the cache-probe resolver honors the env var.

---

### User Story 3 — npm/pnpm + Python cache-hit resolution (Priority: P3)

**Description**: Complete ecosystem coverage with npm/pnpm store (`~/.local/share/pnpm/store/…` and `node_modules/` correlation) and Python (`site-packages/*.dist-info/` and `~/.cache/pip/wheels/…`).

**Why this priority**: Third-tier ecosystems in cache-friendly enterprise pipelines. npm's cache format has multiple variants; Python has both the wheel cache and installed-package dist-info paths.

**Independent Test**: For each of npm/pnpm and Python, fixture attestations with paths under synthetic caches. Assert emitted PURLs and confidence.

**Acceptance Scenarios**:

1. **Given** an in-toto material entry naming `<node_modules>/waybill-fixture-npm/package.json`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:npm/waybill-fixture-npm@<version>"` at confidence 0.92 (version derived from the `package.json`'s `"version"` field).

2. **Given** an in-toto material entry naming a Python wheel cache path like `~/.cache/pip/wheels/…/waybill_fixture_pip-1.0.0-py3-none-any.whl`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:pypi/waybill-fixture-pip@1.0.0"` at confidence 0.92.

3. **Given** an in-toto material entry naming `<site_packages>/waybill_fixture_pip-1.0.0.dist-info/METADATA`, **When** the resolver pipeline runs, **Then** the emitted component carries `purl = "pkg:pypi/waybill-fixture-pip@1.0.0"` at confidence 0.92.

4. **Given** an in-toto material entry naming `<node_modules>/waybill-fixture-npm/package.json` where the `package.json` is unreadable OR missing the `"version"` field, **When** the resolver pipeline runs, **Then** the cache-probe resolver logs a warning and DECLINES the match; the deps.dev resolver receives this path and produces its own attempt (Q1 clarification — no partial-confidence emission).

---

### Edge Cases

- **Non-standard cache locations via env vars** — `M2_HOME`, `GOPATH`, `GOMODCACHE`, `CARGO_HOME`, `GEM_HOME`, `PNPM_STORE_DIR`, `PIP_CACHE_DIR` all override defaults. The resolver MUST honor them.

- **Symlinked caches** — Docker layer builds often symlink `~/.m2` to a mounted volume. The resolver reads the attested path verbatim (no `canonicalize` before matching, to preserve the operator's declared path structure).

- **Partial cache — coordinate present but artifact bytes missing** — a stale metadata-only entry MUST still successfully extract the coord if the path structure encodes it. The resolver is a path-parser, not a content-integrity check.

- **Multiple ecosystem prefixes match** — an unusual path like `~/.m2/repository/npm-registry/…` (someone mirroring npm under the Maven layout) is not expected in practice; if it happens, first-registered-ecosystem-wins semantics kick in. Document but don't fight.

- **Version-less coord in the cache path** — some cache layouts embed the version in a sibling metadata file, not the path itself (e.g., npm `node_modules/*/package.json` requires reading the file to extract version). For such ecosystems, the resolver MAY do a bounded metadata read (single small JSON/TOML file per cache-hit path); it MUST NOT walk directories or read binary artifacts.

- **Attested path lies OUTSIDE any known cache** — the resolver declines cleanly and the pipeline continues to deps.dev (default pre-m663 behavior).

- **Confidence conflict with the URL resolver** — if both URL (0.95) and cache-probe (0.92) resolve the same path, URL wins because it runs first.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The resolver MUST slot into the pipeline between the URL resolver and the hash (deps.dev) resolver. Order: URL (0.95) → **cache-probe (0.92)** → hash/deps.dev (0.90) → path (0.70) → hostname (0.40).

- **FR-002**: The resolver MUST emit each resolved component at confidence 0.92.

- **FR-003**: The resolver MUST make ZERO network calls. It reads only filesystem paths and, for ecosystems where cache-path structure alone doesn't yield a version (Python `dist-info/METADATA`, npm `package.json`), the specific single small metadata file per candidate path. Per Q1 clarification, if the metadata file is unreadable, malformed, or missing the version field, the resolver MUST decline the match (see FR-009) rather than emit at reduced confidence.

- **FR-004**: The resolver MUST honor standard ecosystem env vars that override default cache locations. At minimum: `GOMODCACHE`, `GOPATH`, `CARGO_HOME`, `GEM_HOME`, `PNPM_STORE_DIR`, `PIP_CACHE_DIR`, `M2_HOME` — plan phase enumerates the final set.

- **FR-005**: The resolver MUST run when the operator invokes attestation-consumer resolution (`waybill trace verify` and equivalents). It MUST NOT alter `sbom scan` filesystem-walker behavior.

- **FR-006**: When the resolver declines to match a path, the pipeline MUST continue to the next resolver (deps.dev) — no behavior change from pre-m663 for paths outside known caches.

- **FR-007**: Per Q2 clarification, EVERY resolver (URL-pattern, cache-probe, deps.dev-hash, path, hostname-fallback) MUST emit a `waybill:resolver-tier: <technique>` per-component annotation on every component it produces, where `<technique>` is the snake-case wire form of `ResolutionTechnique` (e.g., `"url_pattern"`, `"local_cache_hit"`, `"hash_match"`, `"file_path_pattern"`, `"hostname_fallback"`). This universal emission gives downstream tools a per-component signal for the resolver tier that produced each component's identity.

- **FR-008**: The resolver MUST produce byte-stable PURLs across scans — the same input path on the same waybill build produces the same PURL.

- **FR-009**: The resolver MUST NOT crash on malformed cache paths (truncated Maven GAV, missing `@v` segment on Go paths, `package.json` with no `"version"` field, unreadable `METADATA`, etc.). It MUST log a `tracing::warn!` naming the offending path + failure reason (e.g., `"missing version field"`, `"unreadable metadata file"`, `"malformed path structure"`) and decline the match. Downstream resolvers (deps.dev, path, hostname) then get their normal turn per FR-006. Per Q1 clarification, the resolver never emits at reduced confidence — a partial match is always a decline.

- **FR-010**: A parity extractor MUST be registered for the FR-007 annotation as `SymmetricEqual` at component scope in the m071 catalog.

- **FR-011**: A cross-resolver integration test MUST verify that a mixed-ecosystem attestation (paths from ≥3 different ecosystems) resolves every cached component via the cache-probe tier at confidence 0.92, with none flowing through to deps.dev.

- **FR-012**: The resolver MUST work identically on Linux, macOS, and Windows. Path separators, tilde expansion, and env-var lookup MUST be portable.

### Key Entities

- **`CacheProbeResolver`** — Rust struct implementing the existing `Resolver` trait (or slotted directly into `pipeline.rs` if the trait refactor of #601 hasn't landed). Owns per-ecosystem probe functions.

- **`EcosystemProbe`** — Per-ecosystem sub-component: `(prefix_matcher, path_to_purl_extractor, optional_metadata_reader)`. Six implementations at launch: Maven, Go, Cargo, Ruby, npm/pnpm, Python.

- **Cache prefix** — Absolute path root a probe matches (e.g., `<m2_repo_root>/repository/`). Derived from env vars + platform defaults.

- **Resolved component from cache-probe** — Emitted `ResolvedComponent` with `confidence = 0.92`, PURL derived from the cache path (+ optional metadata file), and the `waybill:resolver-tier: "cache-probe"` annotation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a fixture attestation naming ≥1 Maven + ≥1 Go path, the cache-probe resolver emits both components at confidence 0.92 with correct PURLs (verified by per-ecosystem integration test).

- **SC-002**: For the same fixture, the deps.dev resolver is NOT invoked for the cache-hit paths (verified by mock/spy assertion — deps.dev call count for those paths is zero).

- **SC-003**: A cross-ecosystem integration test using a synthetic fixture with paths from all 6 supported ecosystems emits 6 components (one per ecosystem), each with confidence 0.92 and the `waybill:resolver-tier: "cache-probe"` annotation.

- **SC-004**: A fixture with paths under non-default env-var-overridden cache locations (e.g., `GOMODCACHE=/opt/gomod`) still resolves those paths correctly via the cache-probe resolver.

- **SC-005**: A path that lies outside all known caches falls through to deps.dev (verified by pre-m663 behavior byte-equivalence test — a fixture with only non-cache paths produces identical resolved components pre/post merge).

- **SC-006**: The cache-probe resolver's per-path overhead is **p95 ≤ 5 ms across ≥100k warm-filesystem paths** (verified by microbenchmark). Byte-equivalent output for non-cache paths.

- **SC-007**: The resolver runs cleanly on Linux, macOS, and Windows in CI (all three platform lanes green).

## Assumptions

- The existing `Resolver` chain in `waybill-cli/src/resolve/pipeline.rs` accepts insertion of a new tier without a spec-level refactor. If issue #601's resolver-trait refactor lands first, the cache-probe resolver plugs in as a trait impl; if not, direct insertion at the pipeline's URL→hash boundary is fine.

- Confidence value 0.92 is between the URL tier (0.95) and the deps.dev tier (0.90); this is the natural insertion point per the issue text and matches operator expectations ("cache hit is more trustworthy than a deps.dev lookup because the artifact IS on this machine").

- Zero new Cargo dependencies. The path-prefix matching, env-var lookup, and small-file JSON/TOML reads reuse existing workspace crates (`serde_json`, `toml`, `std::env`, `dirs` transitively).

- Six ecosystems is the launch scope. Others (Nix, opam, LuaRocks) are follow-on issues; the resolver design is open-ended enough to accept new probes.

- The resolver reads paths verbatim (no `canonicalize`) so operator-declared symlinks are honored.

- No new CLI flag. The resolver runs unconditionally on every attestation-consumer resolution invocation. There is no operator opt-out (matches the URL / hash / path / hostname tiers — they all run unconditionally today).

- Attestation-consumer path only. Zero impact on `sbom scan` filesystem walkers. Zero impact on `sbom generate` when there's no attestation input.

- Tests use synthetic fixture caches per the `feedback_fixture_synthetic_package_names` project convention. Never real coord names.
