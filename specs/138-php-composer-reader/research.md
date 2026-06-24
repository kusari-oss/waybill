# Research — milestone 138 PHP/Composer reader

Resolves the Phase 0 open items from `plan.md`'s Technical Context: Constitution Principle V audit + composer-spec authority audit, on-disk schemas (composer.lock + composer.json + vendor/composer/installed.json), purl-spec `composer` canonical form, integration site within `read_all`, the language-reader pattern selection, multi-project/layer handling, per-file error posture.

## R1: Constitution Principle V audit — `composer` PURL type is purl-spec-blessed

**Decision**: Emit `pkg:composer/<vendor>/<package>@<version>` per [purl-spec `composer-definition.md`](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md). No `mikebom:*` annotation introduced for identity. Source-discriminator surfaces via the existing `mikebom:source-type` annotation (parity-catalog C1, introduced in milestone 002) — same wire shape as cargo's `path` / `git` / `registry` discrimination + Dart's `pub-hosted` / `pub-git` etc.

**Rationale**:

The purl-spec defines `composer` explicitly:

- **Namespace**: required (the vendor — `<vendor>/<package>` form). Both segments MUST be lowercased per spec ("not case sensitive and must be lowercased").
- **Default repository URL**: `https://packagist.org` (canonical; the API host `https://repo.packagist.org` is a real-world variant treated as equivalent default for compatibility).
- **Canonical example**: `pkg:composer/laravel/laravel@5.5.0`.
- **No type-specific qualifier vocabulary**: the spec says "Use Repository: Yes" generically but doesn't bless `repository_url=` (or any other) qualifier name for composer.

This is similar to milestone 137's situation with `pub` — the type is upstream-blessed; only the qualifier names need parity-bridge treatment.

**Source-discriminator handling**:

For packagist / vcs / composer-plugin / metapackage: emit under `pkg:composer/` per spec. The `mikebom:source-type` annotation reuses the existing C1 parity-catalog row. Composer contributes new VALUES to that row (`composer-packagist`, `composer-vcs`, `composer-plugin`, `composer-metapackage`, `composer-main-module`) without altering wire shape — same pattern as cargo/dart.

For path-sourced deps: the purl-spec doesn't define an addressable PURL for filesystem-local packages. The reader emits `pkg:generic/<vendor>-<package>@<version>` as a placeholder (vendor + name flattened because `pkg:generic/` doesn't support namespace split) + `mikebom:source-type = "composer-path"` annotation as the discriminator. This is a **parity-bridge** per Principle V's escape clause.

**`repository_url=` qualifier** (per Phase 0 research correction): purl-spec's `composer-definition.md` doesn't bless this qualifier name. mikebom uses it as a **parity-bridge** because (a) purl-spec generically allows arbitrary qualifiers, (b) milestone 137 already uses the same name for the `pub-definition.md` self-hosted case (different type, same parity decision), and (c) no syft/trivy alternative exists. Documented in the parity catalog as a parity-bridge addition (deferred refresh — not blocking this milestone).

**`mikebom:lockfile-orphan` annotation** (per Q1 clarification): NEW property emission. Marks `installed.json` entries that exist on disk but don't appear in the sibling `composer.lock`. This is a NEW parity-catalog C-row candidate, but since orphan-detection is a v1 emission, it lives as `extra_annotations` data without requiring catalog refresh first.

**Alternatives considered**:

- **Skip path deps entirely**. Rejected: they're real source-tree deps the operator's project relies on. Surfacing them with an annotated placeholder is more transparent than silently dropping (Principle VIII).
- **Custom `pkg:composer-path/` informal type**. Rejected: violates Principle V (don't invent type-names; the `pkg:generic/` + annotation pattern is the established mikebom convention for non-addressable identities).
- **Emit composer-plugin entries under `pkg:composer-plugin/` informal type**. Rejected: same reasoning. Plugins ARE Packagist-addressable; the `type:` field discrimination is a filtering hint, not an identity change.

## R2: `composer.lock` v2 schema (Composer 2.0+ — universal in 2026 production)

**Decision**: Parse a small typed subset. Polymorphic `license:` field handled via `#[serde(untagged)]` enum (string OR array per composer-schema.json). Required-in-modern-Composer-2 fields: `name`, `version`, `type`, `source`/`dist` (at least one — `metapackage` type has neither). Everything else `Option<T>` with `#[serde(default)]`.

**Schema** (per [composer/composer composer-schema.json](https://github.com/composer/composer/blob/main/res/composer-schema.json) + [Locker.php](https://github.com/composer/composer/blob/main/src/Composer/Package/Locker.php)):

Top-level keys (every Composer 2 lockfile):

```json
{
  "_readme": ["...", "..."],
  "content-hash": "<sha256-hex>",
  "packages": [ /* runtime deps */ ],
  "packages-dev": [ /* dev deps; can be null when omitted */ ],
  "aliases": [],
  "minimum-stability": "stable",
  "stability-flags": [],
  "prefer-stable": false,
  "prefer-lowest": false,
  "platform": {},
  "platform-dev": {},
  "platform-overrides": {},
  "plugin-api-version": "2.6.0"
}
```

Only `packages[]` + `packages-dev[]` are consumed by this reader. `plugin-api-version` is a useful Composer-2 sentinel but not required.

**Per-package entry** (typical fields — many more are passed through from the upstream package's `composer.json`):

```json
{
  "name": "symfony/console",
  "version": "v7.0.4",
  "source": {
    "type": "git",
    "url": "https://github.com/symfony/console.git",
    "reference": "abc123...def456"
  },
  "dist": {
    "type": "zip",
    "url": "https://api.github.com/repos/symfony/console/zipball/abc123",
    "reference": "abc123...def456",
    "shasum": "deadbeef..."
  },
  "type": "library",
  "require": {
    "php": ">=8.2",
    "psr/log": "^1.0"
  },
  "require-dev": {},
  "license": ["MIT"],
  "time": "2024-01-15T10:00:00+00:00",
  "description": "...",
  "keywords": [],
  "authors": [],
  "autoload": {}
}
```

Field consumed by mikebom v1:
- `name` (required) — `<vendor>/<package>` form; lowercase before PURL construction
- `version` (required) — verbatim (Composer preserves leading `v` for git-tag versions; PURL preserves it)
- `source.type` — `git` / `svn` / `hg` / `path` (no enum restriction — warn on unknown per FR-008)
- `source.url` — VCS remote URL or path-source path
- `source.reference` — resolved SHA for VCS sources
- `dist.url` — package distribution URL (Packagist or self-hosted mirror)
- `dist.shasum` — SHA-1 of the downloaded zip (per FR-013)
- `type` — `library` / `metapackage` / `composer-plugin` / `composer-installer` / custom (open-ended per composer-schema.json regex `^[a-z0-9-]+$`)
- `require` — informational; transitive edges deferred to v1.1

Fields NOT consumed v1: `description`, `keywords`, `authors`, `autoload`, `time`, `extra`, `notification-url`, `support` (all metadata; license deferred per spec Out-of-Scope).

**`license` polymorphism**: composer-schema.json declares `"license": { "type": ["string", "array"] }`. Real-world lockfiles emit array even for single licenses, but spec allows scalar. Handled via `#[serde(untagged)]` enum.

**`source.type` enum**: NOT restricted in schema. De-facto values:
- `git` (most common — GitHub/GitLab/Bitbucket)
- `svn` (rare in modern PHP)
- `hg` (rarer)
- `path` (filesystem-local; common in monorepos)
- `composer` (composer-shaped private repo — handled via `dist.url` discriminator since this implies Packagist-shape but custom URL)

Unknown source-types warn-and-skip per R7 (forward compat preservation).

**Alternatives considered**:

- **Shell out to `composer show --tree --format=json`**. Rejected: `composer` not guaranteed on scan host; cross-platform reader principle. Matches cargo/npm/pip posture.
- **Parse via the official PHP `composer/composer` library through some FFI/IPC shim**. Rejected: violates Principle I (Pure Rust). The JSON schema is stable and small enough to model in serde directly.
- **Strongly-typed `Source` enum with `tag: type`**. Rejected: the JSON's `source.type` field is a sibling of other fields, not a discriminator wrapper. Use a struct with optional `type` field + post-parse dispatch.

## R3: `vendor/composer/installed.json` schema

**Decision**: Parse the Composer 2 wrapper shape (`{"packages": [...], "dev": bool, "dev-package-names": [...]}`). Defensively detect the Composer 1 bare-array shape — on encountering a top-level array, log `tracing::warn!` and skip the file (spec Out-of-Scope: pre-Composer-2 formats).

**Composer 2 shape**:

```json
{
  "packages": [
    /* same per-entry shape as composer.lock packages[] */
  ],
  "dev": true,
  "dev-package-names": ["phpunit/phpunit", "vimeo/psalm", "..."]
}
```

The `packages[]` entries mirror the lockfile entry shape (FR-006 — see R2 for fields consumed). The `dev-package-names[]` array is the authoritative dev-classifier for installed-tier emission per FR-009.

**Composer 1 detection**: If the parsed JSON's root is an array (not an object), it's Composer 1 format. Per spec Out-of-Scope, warn-and-skip rather than emitting unclassified entries (we'd have no `dev-package-names` discriminator anyway).

**Multi-layer walker** (per Q2 clarification): the walker MUST discover every `vendor/composer/installed.json` under the scan root, regardless of sibling-manifest pairing. Cross-layer same-PURL duplicates collapse via the standard orchestrator `seen_purls` dedup — same posture as dpkg/apk multi-layer container scans.

## R4: purl-spec `composer` canonical form

**Decision**: Honor purl-spec verbatim. Specifically:

- **Base form**: `pkg:composer/<vendor>/<package>@<version>`.
- **Vendor + package**: lowercased per purl-spec ("not case sensitive and must be lowercased"). Lockfile preserves operator-authored case; mikebom lowercases on PURL construction.
- **Version**: verbatim from lockfile (Composer preserves the `v` prefix for git-tag versions like `v7.0.4`; we preserve it for round-trip fidelity).
- **Self-hosted Packagist mirror**: `?repository_url=<base-url-with-scheme>` qualifier (parity-bridge per R1). Omitted when `dist.url` base matches `https://packagist.org`, `https://repo.packagist.org`, or `https://api.github.com/repos/...` (the latter is what Packagist redirects to for default-tap dist; treated as default-Packagist for qualifier-omission purposes).
- **SHA-1 download hash**: surfaced via the `PackageDbEntry.hashes` field (existing milestone-002 convention), NOT as a PURL `checksums=` qualifier. PackageDbEntry.hashes flows to `components[].hashes[]` in CDX / `Package.checksums[]` in SPDX — the standards-native field. Bypassing it for a PURL qualifier would be Principle V regression. PHP/Composer is the only common-ecosystem reader to surface SHA-1 hashes natively per FR-013.
- **VCS source**: `pkg:composer/<vendor>/<package>@<version>?vcs_url=git+<git-url>`. The `git+` scheme prefix is the purl-spec git-source cross-type convention (`svn+`/`hg+` for svn/hg). The resolved SHA from `source.reference` is preserved as `mikebom:vcs-ref` evidence, not embedded in the version segment — Composer's lockfile records the upstream `version` field (typically the matching git tag) for VCS sources too, so version conveys real upstream identity.
- **Path source**: `pkg:generic/<vendor>-<package>@<version>` placeholder + `mikebom:source-type = "composer-path"` annotation (see R1). Vendor + name flattened with `-` because `pkg:generic/` doesn't support `<vendor>/<package>` namespace split.

**`mikebom:source-type` value set extension** (reuses parity-catalog C1):

| Source kind | `mikebom:source-type` value |
|---|---|
| Packagist default | `composer-packagist` |
| Packagist self-hosted | `composer-packagist` (the `repository_url=` PURL qualifier carries the distinguishing URL) |
| VCS (git/svn/hg) | `composer-vcs` |
| Path | `composer-path` |
| Composer plugin (`type: composer-plugin` or `composer-installer`) | `composer-plugin` |
| Metapackage (`type: metapackage`) | `composer-metapackage` |
| Main-module (FR-012) | `composer-main-module` |

The `composer-` prefix avoids collision with cargo's `git`/`path`/`registry` and Dart's `pub-` prefix. Per the established milestone-122 `kmp-` + milestone-137 `pub-` precedent.

## R5: Integration site within `read_all`

**Decision**: register `pub mod composer;` in `mikebom-cli/src/scan_fs/package_db/mod.rs` (placed alphabetically between `pub mod cmake;` and `pub mod conan;`) and add a call site in `read_all` alongside the existing language readers.

Existing pattern (cargo example, line 1499):

```rust
let cargo_out = cargo::read(rootfs, include_dev, exclude_set)?;
out.extend(cargo_out.entries);
diagnostics.divergence_records.extend(cargo_out.divergences);
```

Composer adds (placed alphabetically between `cmake::read` and `conan::read`):

```rust
out.extend(composer::read(rootfs, include_dev, exclude_set));
```

No `collect_claimed_paths` integration — language readers don't claim binary paths (file-claim is an OS-reader concern).

**`include_dev` flag**: per the milestone-137 discovery (CLI hardwires `include_dev = true` since milestone 052/part-3; dev-scope filtering is post-resolution via `--exclude-scope dev`), the reader accepts the parameter but doesn't filter on it — dev-scope is surfaced via `lifecycle_scope = Some(LifecycleScope::Development)` and the post-resolution filter handles `--exclude-scope dev`.

**`exclude_set`**: existing `ExclusionSet` (milestone 113) for the safe-walk filter. Standard plumbing.

## R6: Language-reader pattern selection

**Decision**: Use **dart.rs** (milestone 137) as the architectural template. Closest semantic match:

- Both parse a JSON / YAML lockfile in a source-tree project directory
- Both emit a main-module component per workspace member's manifest (`pubspec.yaml` ↔ `composer.json`)
- Both handle multi-source dep classification (dart: hosted/path/git/sdk; composer: packagist/vcs/path/plugin/metapackage)
- Both use the same prefixed `mikebom:source-type` convention (`pub-*` vs `composer-*`)
- Both ship 4 integration test files following the `<reader>_*.rs` naming convention
- Both reuse the milestone-114 `safe_walk` for project discovery

Composer adds one new capability over dart: the **deployed-tier path** via `vendor/composer/installed.json` (FR-006). This is the third tier (`design` < `source` < `deployed`) that no prior language reader has surfaced.

**`serde_json` precedent**: pervasive throughout the workspace (every existing reader). Confirms the dep is battle-tested for JSON parsing of arbitrary depth.

**Source-tree walker**: reuse `scan_fs::walk::safe_walk` (milestone 114). Pattern: walk for `composer.json` files; for each, look for sibling `composer.lock`; ALSO walk for `vendor/composer/installed.json` files at any depth (multi-layer container support per Q2).

**Alternatives considered**:

- **Use cargo.rs as template**. Rejected: cargo's workspace-Cargo.toml handling is more complex; composer doesn't have the same workspace-section construct.
- **Use maven.rs as template**. Rejected: maven's parent-POM walking + JAR archive descent is much more complex than composer's flat lockfile.

## R7: Multi-project / monorepo handling + per-file error posture

**Decision**: per Q1+Q2 clarifications + FR-010, one main-module per `composer.json` (when `name:` present); orphan installed.json entries surface with `mikebom:lockfile-orphan = true` annotation; per-file parse errors warn-and-skip; multi-layer `vendor/composer/installed.json` discovery widens the walker.

Algorithm:

1. **Walker pass A** — discover every `composer.json` under the scan root via `safe_walk`. For each:
   1. Parse via `serde_json` → `ComposerJson` struct. On error, `tracing::warn!` + skip the project entirely.
   2. Check for sibling `composer.lock`. If present and parses → source-tier emission per FR-002. If present but malforms → `tracing::warn!` + fall back to design-tier from `composer.json` per FR-005.
   3. If no lockfile → design-tier emission per FR-005.
   4. Emit main-module per FR-012 if `name:` field present; otherwise skip ONLY the main-module (deps still emit per Q3).

2. **Walker pass B** — discover every `vendor/composer/installed.json` under the scan root. For each:
   1. Parse via `serde_json` → `InstalledJson` struct. On root-is-array (Composer 1) → `tracing::warn!` + skip. On other parse errors → `tracing::warn!` + skip.
   2. For each entry: construct PURL via the same FR-003 helper. Set `sbom_tier = Some("deployed")`. Set `lifecycle_scope = Development` if `name` in `dev-package-names[]`.
   3. **Orphan detection**: if this `installed.json`'s sibling project (the parent of `vendor/`) ALSO has a `composer.lock` with the same package name+version, the lockfile entry wins (same-PURL dedup at orchestrator handles this). Otherwise (no sibling lockfile, OR sibling lockfile lacks this entry), set `extra_annotations["mikebom:lockfile-orphan"] = true`.

3. **Same-PURL dedup**: standard orchestrator-level `seen_purls` HashSet collapses cross-project + cross-layer duplicates. First entry wins (consistent with every other reader).

**Per-file error matrix** (per FR-008 + R7):

| Condition | Behavior | Justification |
|---|---|---|
| Source tree has no `composer.lock` / `composer.json` / `installed.json` | Return `Vec::new()` immediately | Clean no-op (FR-007) |
| `composer.json` exists, no `composer.lock` | Emit design-tier components per FR-005 | Library publishers / pre-`composer install` |
| `composer.json` malformed JSON | `tracing::warn!`, skip the project, continue walking other projects | FR-008 |
| `composer.json` missing `name:` field | `tracing::warn!`, skip ONLY main-module emission (deps still emit) | Q3 clarification |
| `composer.lock` malformed JSON | `tracing::warn!`, fall back to design-tier emission from sibling `composer.json` | FR-008 + R7 |
| Per-entry `version:` missing or empty | `tracing::warn!`, skip that single entry | Cannot synthesize PURL |
| Per-entry `name:` missing or not `<vendor>/<package>` form | `tracing::warn!`, skip that single entry | Cannot synthesize PURL namespace |
| Per-entry `source.type` unknown (not git/svn/hg/path) | `tracing::warn!`, skip that single entry | Forward compat preservation |
| `installed.json` root is array (Composer 1 format) | `tracing::warn!`, skip the file | Out-of-Scope per spec |
| `installed.json` malformed JSON | `tracing::warn!`, skip the file | FR-008 |
| Empty `packages[]` block | Emit just the main-module; no warning | Fresh `composer install` failure or library with zero deps |

## R8: Performance considerations

**Decision**: no performance budget violations expected; per-scan cost is bounded by JSON parse + walker traversal.

- Per-project: read `composer.json` (~2 KB) + `composer.lock` (50–300 KB typical for Laravel/Symfony), parse via `serde_json`. Estimated 2–10 ms per project on warm cache.
- Typical Laravel app (~100 deps): ~5 ms total. Heavy Symfony app (~250 deps): ~15 ms. Composer monorepo with 5 members (~400 deps total): ~30 ms.
- Source-tree walker discovery cost (find all `composer.json` + `installed.json` under scan root): same as cargo's existing walker — sub-millisecond on typical repos, ~100 ms on a heavy container scan with 50+ layers.

The no-Composer-detected fast path: walker finds no relevant files; reader returns empty Vec; statistically free.

---

## Summary of Phase 0 resolutions

| Unknown | Decision | Reference |
|---|---|---|
| Principle V audit | `composer` is purl-spec-blessed; reuse C1 for source-type discriminator; `repository_url=` is parity-bridge | R1 |
| `composer.lock` schema | Parse small typed subset; `#[serde(untagged)]` for license polymorphism | R2 |
| `installed.json` schema | Composer 2 wrapper shape; warn-and-skip Composer 1 bare array | R3 |
| purl-spec `composer` canonical form | Lowercase vendor+name; `repository_url=` for self-hosted; `vcs_url=git+...` for VCS | R4 |
| Integration site | `read_all` dispatcher alphabetically between cmake and conan | R5 |
| Reader pattern template | dart.rs (milestone 137) — closest semantic match | R6 |
| Multi-project handling | One main-module per `composer.json`; multi-layer `installed.json` walker | R7 |
| Per-file error posture | Warn-and-skip; never fail-the-scan | R7 |
| Performance | ~15 ms on heavy Symfony app; no budget concerns | R8 |

All Phase 0 unknowns resolved. Ready for Phase 1 (data-model + contracts + quickstart).
