# Feature Specification: PHP/Composer ecosystem reader

**Feature Branch**: `138-php-composer-reader`
**Created**: 2026-06-23
**Status**: Draft
**Input**: User description: "418"

## Background

PHP is a top-5 web ecosystem powering ≈ 78% of the public web (WordPress, Drupal, Magento, Laravel, Symfony, Joomla, MediaWiki, Composer-managed CMS-and-framework deployments). Composer is its dominant package manager — every modern PHP project (Composer 2.0+ since 2020) commits a `composer.lock` for reproducible installs, mirroring the npm / yarn / pip lockfile pattern.

mikebom currently emits **zero** PHP components when scanning a Laravel app, a Symfony service, a WordPress site, or any source tree containing `composer.lock`. Every package pulled from Packagist — plus the typically-larger graph of dev-deps (PHPUnit, PHPStan, Psalm, friendsofphp/php-cs-fixer) — is invisible to the scan.

The Composer ecosystem has four discrimination surfaces a reader must handle:

- **Packagist deps** (the common case): from `packagist.org`. PURL: `pkg:composer/<vendor>/<package>@<version>`.
- **VCS deps**: directly-pinned git/svn/hg URLs (`{"type":"vcs","url":"https://github.com/foo/bar.git"}` in `composer.json` repositories block, with `source.reference` SHA in lockfile). Identity is the resolved VCS reference.
- **Path deps**: local-filesystem deps (`{"type":"path","url":"../my-lib"}`) typically used for monorepos. Identity is the path; the dep doesn't carry a Packagist version.
- **Composer plugins + metapackages**: lockfile entries with `"type": "composer-plugin"` / `"composer-installer"` / `"metapackage"`. These ARE addressable via the same `pkg:composer/` PURL but operators need to filter them differently than runtime application deps.

Additionally Composer's lockfile carries two distinct package sets — `packages[]` (runtime deps from `require:` in composer.json) and `packages-dev[]` (dev-only deps from `require-dev:`). Tagging the second set with `mikebom:lifecycle-scope = "development"` is the symmetric pattern to milestones 137 (Dart), 069 (gem), and 052 (cross-ecosystem dev/build/test classification).

This feature closes the gap so an operator scanning any PHP/Composer project gets a complete SBOM with every Composer-managed dep represented, the right provenance distinction surfaced, and dev-deps correctly classified.

## Clarifications

### Session 2026-06-23

- Q: When `composer.lock` and `vendor/composer/installed.json` are both present in the same project, what should happen to `installed.json` entries that DO NOT appear in the lockfile (lockfile-vs-disk drift — e.g., post-CI `composer require` without committing the updated lockfile)? → A: Emit orphan `installed.json` entries as `deployed`-tier components AND tag them with `mikebom:lockfile-orphan = "true"` (string value, per CycloneDX `componentProperty.value` wire-format constraint — CDX 1.6 requires string-valued properties) so consumers can detect drift. Lockfile wins for same-PURL dedup; truly-orphan entries surface (Principle VIII completeness over silent omission). The orphan annotation MUST only fire when a sibling `composer.lock` EXISTS but doesn't contain the entry — deployed-tier scans with no sibling lockfile (container images stripped of manifests) emit normally WITHOUT the orphan annotation (the lockfile-vs-disk comparison is undefined when there's no lockfile).
- Q: How wide should the `vendor/composer/installed.json` walker cast its net? Specifically, should it discover every `installed.json` under the scan root regardless of sibling manifests, or only ones paired with a `composer.json` / `composer.lock`? → A: Discover every `vendor/composer/installed.json` under the scan root with no sibling-manifest requirement. Container layers with their own `vendor/` ship their own deps; standard `seen_purls` dedup collapses cross-layer same-PURL duplicates. Matches the dpkg / apk multi-layer discovery posture (Principle VIII).
- Q: When a `composer.json` lacks the `name:` field (legal for private apps that won't be published to Packagist), how should emission scope be handled given that dep edges normally flow from main-module to direct deps? → A: Emit lockfile components normally; skip ONLY the main-module + its dep-edge attribution. The deps remain real Packagist-identifiable packages with surviving identity even without a project root. Synthesizing a placeholder main-module from the directory path would invent a non-Packagist-addressable identity (Principle V violation risk).

#### Phase 0 research corrections (post-clarification)

Plan-phase research against the [purl-spec `composer-definition.md`](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md) and the canonical [composer-schema.json](https://github.com/composer/composer/blob/main/res/composer-schema.json) surfaced three corrections to initial guesses. These are CORRECTIONS to align with the authority, not scope changes:

- **Default Packagist URL**: the canonical default per purl-spec is `https://packagist.org`, NOT `https://repo.packagist.org` (the latter is the API host but is NOT what purl-spec records as the default). The `repository_url=` qualifier is omitted when `dist.url` (or `source.url`) base matches `https://packagist.org` OR `https://repo.packagist.org` (both treated as default for compatibility with real-world lockfiles).
- **Namespace + name case-normalization**: purl-spec REQUIRES `<vendor>` and `<package>` segments to be lowercased in the canonical PURL form. The lockfile preserves operator-authored case; mikebom MUST lowercase both segments when constructing the PURL. (The Composer ecosystem treats vendor/name case-insensitively for resolution; the autoloader's PSR-4 namespace mapping is separate and orthogonal.) This OVERRIDES the spec Assumption that said "preserve case" — the assumption was wrong vs the standard.
- **`repository_url=` qualifier is NOT spec-blessed for `composer`**: the purl-spec's `composer-definition.md` declares "Use Repository: Yes" generically but defines NO composer-specific qualifier vocabulary. Per Principle V, this means `repository_url=` is a **parity-bridge** (the standard accepts arbitrary qualifiers via its generic qualifier mechanism, but doesn't bless the name). Use is justified by symmetry with the milestone-137 Dart precedent (which uses `repository_url=` per the purl-spec `pub-definition.md`) AND by the fact that no syft/trivy alternative exists. Documented in the parity catalog.

FR-003 + spec Assumptions are updated below to reflect these corrections.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scans a Laravel/Symfony PHP application (Priority: P1) 🎯 MVP

A PHP backend developer runs `mikebom sbom scan --path .` on their Laravel or Symfony app source tree. They receive an SBOM containing one component per package pinned in `composer.lock`. Each component carries a `pkg:composer/<vendor>/<package>@<version>` PURL identity and a dependsOn edge from the app's root component to each of its direct deps.

**Why this priority**: The headline use case. Without it, the entire feature has no operator value — every Laravel/Symfony app is the canonical target, and `composer.lock` is the universal artifact present in modern PHP projects.

**Independent Test** (SC-001): Synthetic fixture with `composer.json` declaring 3 direct deps + `composer.lock` pinning those + their 2 transitive deps (5 total in `packages[]`). Run `mikebom sbom scan --path <tmp>`. Assert exactly 5 `pkg:composer/*` components emit with correct vendor/name/version triples and the project's direct-dep edges target the correct bom-refs.

**Acceptance Scenarios**:

1. **Given** a Laravel project with `composer.lock` pinning `symfony/console 7.0.4`, `monolog/monolog 3.5.0`, `guzzlehttp/guzzle 7.8.1`, **When** the operator runs `mikebom sbom scan --path <project>`, **Then** the emitted SBOM contains components for each pinned dep with PURL `pkg:composer/<vendor>/<package>@<version>`.
2. **Given** the same project, **When** the operator inspects the emitted SBOM, **Then** transitive deps pinned in `composer.lock` (e.g., `psr/log`, `symfony/polyfill-mbstring`) also appear as components — the lockfile is the authoritative dep set, not just direct deps.
3. **Given** a source tree WITHOUT `composer.lock` or `composer.json`, **When** the operator scans, **Then** no Composer components or annotations appear AND no warning fires (clean no-op).
4. **Given** a project whose `composer.json` declares `"name": "acme/my-app"` and `"version": "1.2.3"`, **When** the operator scans, **Then** a main-module component emits with PURL `pkg:composer/acme/my-app@1.2.3`, `mikebom:component-role = "main-module"`, and `mikebom:sbom-tier = "source"` annotations; dep edges flow from this main-module bom-ref to each direct dep's bom-ref.

---

### User Story 2 — Operator distinguishes Packagist vs VCS / path / plugin deps (Priority: P2)

The operator's PHP application's `composer.lock` mixes Packagist deps (the default), a `type: vcs` dep pinning a fork on GitHub, a `type: path` dep pointing to a shared in-monorepo package, and a `composer-plugin` meta-package (e.g., `composer/installers`). The SBOM must distinguish these sources so downstream supply-chain risk tooling can correctly assess each (path-deps + VCS-deps + plugin meta-packages have meaningfully different risk profiles than Packagist-hosted application deps).

**Why this priority**: Important for supply-chain risk assessment but the headline value (US1) ships independently. Path/VCS/plugin handling is a refinement layered on top of the baseline Packagist-dep extraction.

**Independent Test** (SC-002): Synthetic fixture with one each of packagist / vcs / path dep + one Composer plugin meta-package in `composer.lock`. Scan. Assert that:
- The Packagist dep emits as `pkg:composer/<vendor>/<package>@<version>` (standard) with `mikebom:source-type = "composer-packagist"` evidence.
- The VCS dep emits with `mikebom:source-type = "composer-vcs"` evidence; PURL carries `?vcs_url=git+<url>` qualifier per the purl-spec cross-type git-source convention; the resolved SHA is preserved as `mikebom:vcs-ref` evidence.
- The path dep emits with `mikebom:source-type = "composer-path"` evidence and a `pkg:generic/` PURL (path-deps have no Packagist-addressable identity).
- The Composer plugin meta-package emits with the standard `pkg:composer/<vendor>/<package>@<version>` PURL AND `mikebom:source-type = "composer-plugin"` evidence so downstream consumers can filter it out of "runtime application deps" views.

**Acceptance Scenarios**:

1. **Given** a `composer.lock` containing a `type: path` dep entry, **When** the operator scans, **Then** that dep emits with `mikebom:source-type = "composer-path"` evidence; downstream filtering on the property surfaces only path-sourced deps.
2. **Given** a `composer.lock` containing a `type: vcs` dep with a `source.reference:` SHA, **When** the operator scans, **Then** the emitted PURL carries `?vcs_url=git+<url>` qualifier AND `mikebom:source-type = "composer-vcs"` evidence; the resolved SHA appears as `mikebom:vcs-ref`.
3. **Given** a `composer.lock` containing a `"type": "composer-plugin"` entry (e.g., `composer/installers`), **When** the operator scans, **Then** the emitted component carries the standard `pkg:composer/composer/installers@<version>` PURL AND `mikebom:source-type = "composer-plugin"` annotation so consumers can distinguish toolchain plugins from application deps via the standard property filter.

---

### User Story 3 — Operator scans a PHP project WITHOUT a committed lockfile, OR scans a deployed container with `vendor/` installed (Priority: P3)

Some PHP projects (especially libraries published to Packagist) deliberately do NOT commit `composer.lock` — only `composer.json` with version constraints. Conversely, on deployed PHP containers the source tree may have only the post-install evidence under `vendor/composer/installed.json` (the post-`composer install` manifest written by Composer itself). Scanning such trees should produce SOME inventory rather than empty output:

- `composer.json` only → `design`-tier emission with declared constraints.
- `vendor/composer/installed.json` present → `deployed`-tier emission.
- Both lockfile + installed.json → lockfile wins; installed.json is informational.

**Why this priority**: Important for libraries-and-packages publishers (smaller user base than app authors) AND for image-scan workflows where the source tree is the running container's filesystem. The lockfile-driven slice in US1 is the headline.

**Independent Test** (SC-003): Three sub-fixtures —

1. `composer.json` only, declaring 2 direct deps. Assert: 2 components emit with `mikebom:sbom-tier = "design"` annotation, each carries the declared constraint string as a `mikebom:requirement-range` annotation.
2. `vendor/composer/installed.json` only, with 3 installed packages. Assert: 3 components emit with `mikebom:sbom-tier = "deployed"` annotation, each with the installed version.
3. Both `composer.lock` AND `vendor/composer/installed.json` (the common case). Assert: lockfile is the source of truth; component count matches lockfile package count (no duplicate emission from installed.json).

**Acceptance Scenarios**:

1. **Given** a PHP library project with `composer.json` only (no `composer.lock`), **When** the operator scans, **Then** components emit for declared direct deps from BOTH `require:` and `require-dev:` blocks, each with `mikebom:sbom-tier = "design"` and the original version constraint preserved as evidence.
2. **Given** a deployed PHP container with `vendor/composer/installed.json` only (no `composer.json`/`composer.lock` in the same directory tree — common when only the build output ships), **When** the operator scans, **Then** components emit for each entry in `installed.json` with `mikebom:sbom-tier = "deployed"`.
3. **Given** a project with `require-dev:` declaring `phpunit/phpunit: ^11.0`, **When** the operator scans in design-tier mode, **Then** the emitted `phpunit/phpunit` component carries `mikebom:lifecycle-scope = "development"` annotation; downstream `--exclude-scope dev` filtering on that property successfully suppresses it.

---

### Edge Cases

- **Mixed Packagist/VCS/path/plugin in one project**: a Laravel app commonly has all four source kinds in one `composer.lock`. Each MUST surface with the correct `source-type` evidence; none MUST silently masquerade as a Packagist hosted dep.
- **Private Packagist (satis / private-packagist.com)**: some organizations run an internal Composer mirror. The lockfile's `dist.url` (or `source.url` for VCS-style private repos) points at a non-`https://packagist.org` host. When the URL doesn't match the default Packagist host, emit a `repository_url=<base-url-with-scheme>` qualifier on the PURL per the purl-spec cross-type convention.
- **Composer 1 lockfile format**: pre-2020 lockfiles have a slightly different field shape (notably `hash` vs `content-hash`, and `installed.json` was a bare JSON array instead of `{packages: [...]}`). Out of scope for v1 — modern Composer 2.0+ (released 2020) is universal in 2026 production.
- **`packages-dev[]` vs `packages[]` classification**: lockfile entries in `packages-dev[]` SHOULD be tagged with `mikebom:lifecycle-scope = "development"` so the standard `--exclude-scope dev` filtering works (matches Dart/npm/maven precedent).
- **Malformed `composer.lock`**: skip-the-file with `tracing::warn!`; when a sibling `composer.json` exists, fall back to design-tier emission from the manifest rather than producing zero components.
- **`installed.json` schema variants**: Composer 2 wraps it in `{"packages": [...], "dev": true|false, "dev-package-names": [...]}`. Bare-array Composer 1 shape is out of scope (see above). The Composer 2 `dev-package-names` array is the authoritative dev-classifier for installed-tier emission.
- **Empty `packages[]` block**: a project with `composer.lock` but zero packages section (fresh `composer install` failure). Skip silently — only the main-module emits, no warnings.
- **`composer.json` lacking `name:` field**: rare but legal (private apps that won't be published to Packagist). Per FR-012 + Q3 clarification, skip ONLY the main-module emission for that project with `tracing::warn!`; lockfile deps + installed.json deps still emit (deps remain Packagist-identifiable). The only loss is dep-edge attribution from a project root — transitive edges between siblings still work in v1.1.
- **Vendor/name with case differences**: Packagist treats vendor/name as case-insensitive and the lockfile preserves the operator's literal case. PURLs MUST lowercase both `<vendor>` and `<package>` segments per purl-spec `composer-definition.md` canonical form ("not case sensitive and must be lowercased"). The `name` field on the emitted component preserves the lockfile's literal case for display purposes; only the PURL identity is lowercased. (See Phase 0 corrections.)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect Composer projects by the presence of `composer.lock`, `composer.json`, OR `vendor/composer/installed.json` files anywhere under the scan root. Any of the three triggers reader activation. The walker MUST discover every matching file under the scan root regardless of whether sibling manifests exist alongside — container images often carry multiple layered `vendor/composer/installed.json` files (base PHP image + app layer + intermediate build stages), each representing a real install state on disk. Cross-layer same-PURL duplicates collapse via the standard orchestrator `seen_purls` dedup, mirroring the dpkg/apk multi-layer posture.
- **FR-002**: System MUST parse `composer.lock` (JSON, Composer 2.x schema) and extract from BOTH `packages[]` and `packages-dev[]` arrays: each entry's `name` (`<vendor>/<package>`), `version`, `source.type` discriminator (`git`/`svn`/`hg`/`path`/null), `source.url`, `source.reference` (resolved SHA for VCS), `dist.url`, `dist.shasum` (SHA-1 of the downloaded zip), `type` field (`library`/`metapackage`/`composer-plugin`/etc.), and `require:` map (transitive dep names — surfaced as informational evidence; transitive edges deferred to v1.1 per FR-004).
- **FR-003**: System MUST emit one component per parsed lockfile entry with PURL according to the source/type discriminator (shapes per the [purl-spec `composer` definition](https://github.com/package-url/purl-spec/blob/main/types-doc/composer-definition.md)):
  - **packagist** (default — `source.type` absent or `dist.url` points at the default Packagist host): `pkg:composer/<vendor>/<package>@<version>[?repository_url=<base-url-with-scheme>]` (`repository_url=` qualifier omitted when `dist.url`'s scheme+host matches any of the three default URLs: `https://packagist.org` (purl-spec canonical default), `https://repo.packagist.org` (the API host, treated as equivalent default for compatibility with real-world lockfiles), OR `https://api.github.com` (Packagist's redirect target for dist downloads — emitted by Composer itself in the lockfile's `dist.url` field). Present for private mirrors).
  - **vcs** (`source.type: git`/`svn`/`hg`): `pkg:composer/<vendor>/<package>@<version>?vcs_url=git+<url>` (the `git+` prefix per purl-spec cross-type convention; svn/hg use their scheme prefixes accordingly). The lockfile's `source.reference` SHA is preserved as `mikebom:vcs-ref` evidence rather than embedded in the version segment (the lockfile-recorded `version` field IS the meaningful upstream identity even for VCS sources).
  - **path** (`source.type: path`): `pkg:generic/<vendor>-<package>@<version>` placeholder (vendor/name flattened with `-` because `pkg:generic/` lacks the namespace split) + `mikebom:source-type = "composer-path"` evidence — path-deps have no Packagist-addressable identity, so the placeholder + annotation surface the discriminator while preserving a usable bom-ref for dep-graph wiring.
  - **composer-plugin / metapackage** (lockfile `type:` field, distinct from `source.type`): standard `pkg:composer/<vendor>/<package>@<version>` PURL + `mikebom:source-type = "composer-plugin"` (for `type: composer-plugin` / `composer-installer`) or `"composer-metapackage"` (for `type: metapackage`) annotation; these are addressable via Packagist but operators filter them separately from runtime application deps.
- **FR-004**: System MUST emit dependency edges from each project's main-module (per FR-012) to each direct dep declared in `composer.json`'s `require:` (+ `require-dev:` when dev-scope is not excluded) OR — when a `composer.lock` is present — to each lockfile entry name that appears in the manifest's `require:` / `require-dev:` blocks. Transitive components (lockfile entries not declared in any manifest) surface as standalone components but their inter-package dependency edges (per-entry `require:` arrays from the lockfile) are deferred to v1.1 — scope-aligned with the milestone-064 / 066 / 068 / 069 / 070 / 137 v1 convention.
- **FR-005**: When `composer.lock` is absent but `composer.json` is present, system MUST emit components for direct deps declared in BOTH `require:` and `require-dev:` blocks, with `mikebom:sbom-tier = "design"` annotation and the original constraint string as `mikebom:requirement-range` evidence. Components from `require-dev:` MUST additionally carry `mikebom:lifecycle-scope = "development"` annotation so `--exclude-scope dev` filtering applies uniformly. No transitive deps emit in this design-tier mode.
- **FR-006**: When `vendor/composer/installed.json` is present (matching the standard `<project>/vendor/composer/installed.json` layout), system MUST parse the Composer 2 wrapper (`{"packages": [...], "dev": bool, "dev-package-names": [...]}`) and emit one component per entry with `mikebom:sbom-tier = "deployed"` annotation. Entries whose `name` appears in `dev-package-names[]` MUST carry `mikebom:lifecycle-scope = "development"` annotation. When BOTH lockfile and installed.json are present in the same project, the lockfile wins for entries that appear in both (same-PURL dedup at the orchestrator level handles this naturally). Entries that appear ONLY in installed.json (lockfile-vs-disk drift — post-CI `composer require` without committing the updated lockfile) MUST emit as `deployed`-tier components AND carry a `mikebom:lockfile-orphan = "true"` annotation (string value per the CycloneDX wire-format constraint — CDX 1.6 `componentProperty.value` is string-typed) so consumers can detect the drift; suppressing them would create a false negative against Principle VIII (Completeness). The orphan annotation MUST only fire when a sibling `composer.lock` EXISTS but doesn't contain the entry — deployed-tier-only scans where the project root has no `composer.lock` (container images stripped of manifests) emit normally WITHOUT the orphan annotation.
- **FR-007**: System MUST treat a source tree containing none of `composer.lock` / `composer.json` / `vendor/composer/installed.json` as a clean no-op — no components emitted, no warnings logged. Existing scans on non-PHP projects MUST stay byte-identical pre/post this feature.
- **FR-008**: System MUST tolerate per-file parse errors (malformed JSON, missing required fields, encoding issues) without aborting the whole scan — log a structured warning naming the affected file path and continue. When a lockfile is malformed AND a sibling `composer.json` exists, fall back to design-tier emission from the manifest per FR-005.
- **FR-009**: System MUST tag `packages-dev[]` lockfile entries AND `installed.json::dev-package-names[]` entries with `mikebom:lifecycle-scope = "development"` so the standard `--exclude-scope dev` filtering works (matches Dart milestone 137 + npm + maven precedent).
- **FR-010**: System MUST handle Composer monorepo projects (a top-level `composer.json`/`composer.lock` plus N member packages each with their own `composer.json` under `packages/<member>/composer.json`, common in the Symfony / Doctrine ecosystems) by emitting **one main-module component per `composer.json`** regardless of monorepo membership. Each member's lockfile (when present) is parsed independently for the dep edges attributed to that member's main-module. Same-PURL deps across multiple lockfiles collapse via the standard cross-component `seen_purls` dedup at the orchestrator level. No synthetic monorepo-root component is emitted. Mirrors the cargo (milestone 064) + Dart (milestone 137) workspace pattern.
- **FR-011**: System MUST NOT make any network calls during the scan — `composer.lock` is fully self-contained. Resolving a Packagist API call for richer metadata (license, homepage, authors) is out of scope.
- **FR-012**: For each `composer.json` encountered under the scan root with a non-empty `name:` field, system MUST emit one **main-module component** with PURL `pkg:composer/<vendor>/<package>@<version>` (where `<vendor>/<package>` and `<version>` come from the manifest's `name:` and `version:` fields). The component MUST carry `mikebom:component-role = "main-module"` and `mikebom:sbom-tier = "source"` annotations. Dep edges MUST flow from this main-module to each direct dep declared in `composer.json` (or, when `composer.lock` is present, to each direct-dep entry per FR-004). When the `composer.json` lacks a `version:` field (common for application projects — Composer infers the version from VCS tags at install time and many apps simply don't declare one), use `0.0.0-unknown` as the placeholder per the cargo/Dart/gem main-module convention. When the `composer.json` lacks a `name:` field (legal for private apps that won't be published to Packagist), skip ONLY the main-module emission for that project with `tracing::warn!` rather than synthesize a placeholder name — but DO NOT suppress lockfile- or installed.json-derived component emission for the same project (deps remain Packagist-identifiable; only the dep-edge attribution from a project root is lost). Synthesizing a placeholder main-module from the directory path would invent a non-Packagist-addressable identity, violating Principle V. Mirrors the established milestone-064 (cargo) / 066 (npm) / 068 (pip) / 069 (gem) / 070 (maven) / 137 (Dart) main-module emission pattern.
- **FR-013**: System MUST preserve content-addressable hashes when present in the lockfile: `dist.shasum` (SHA-1 of the downloaded zip from Packagist) flows into `PackageDbEntry.hashes` as a `ContentHash::sha1(<hex>)` entry. Composer's lockfile carries SHA-1 hashes inline for every Packagist-hosted entry; preserving them as evidence (even though SHA-1 is cryptographically deprecated for new constructions) honors the spec-fidelity principle — the standards-native CDX `hashes[]` array and SPDX `Package.checksums[]` array both carry arbitrary algorithms.

### Key Entities

- **composer.lock**: JSON lockfile pinning each direct + transitive dep of a PHP project to a specific version. Top-level structure: `packages: [...]` (runtime), `packages-dev: [...]` (dev-only), `aliases: []`, `minimum-stability`, `content-hash`. Each `packages[]` entry carries `name`, `version`, `source` (map with `type`/`url`/`reference`), `dist` (map with `type`/`url`/`reference`/`shasum`), `type`, `require` (map of dep-name → constraint), `require-dev`, `license`, `authors`, `description`, `keywords`, `time`.
- **composer.json**: Declared dep manifest. Required `name` (when published to Packagist; optional for private apps), optional `version`, required `require` and optional `require-dev` blocks. Lower fidelity than lockfile (constraints not pinned versions).
- **vendor/composer/installed.json**: Post-install manifest written by Composer 2. Wrapper `{packages: [...], dev: bool, dev-package-names: [...]}` shape; the inner `packages[]` entries mirror the lockfile entry shape.
- **Package source**: Discriminator for where a dep came from. Implicit from absent `source.type` (default Packagist) or explicit: `git`/`svn`/`hg` (VCS), `path` (filesystem-local), `composer` (private composer-shaped repo).
- **Package type**: Composer's own `type:` field on each lockfile entry — `library` (default), `metapackage`, `composer-plugin`, `composer-installer`, `composer-script`, `php-ext`, `wordpress-plugin`, etc. Distinct from source-type; surfaces as the `mikebom:source-type` value for plugin/metapackage discrimination.
- **Composer plugin meta-package**: A `type: composer-plugin` or `type: composer-installer` package that modifies Composer's behavior at install time (e.g., `composer/installers` for WordPress plugin install paths, `composer/package-versions-deprecated` for legacy compatibility). Has Packagist provenance but lives in a different operator-mental-model bucket than runtime application deps.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a synthetic Laravel/Symfony project with `composer.json` (3 direct deps) + `composer.lock` (those 3 plus 2 transitives = 5 total in `packages[]`) produces a CDX SBOM whose Composer component count matches the lockfile package count exactly (5) plus 1 main-module (= 6 total Composer-derived components) and direct-dep edges target real bom-refs.
- **SC-002**: A scan of a fixture mixing one Packagist, one VCS, one path, and one composer-plugin meta-package produces correct PURLs for each per FR-003: Packagist as `pkg:composer/<vendor>/<package>@<version>` (or with `?repository_url=` for private), VCS as `pkg:composer/<vendor>/<package>@<version>?vcs_url=git+<url>`, path as `pkg:generic/<vendor>-<package>@<version>` (placeholder), plugin as `pkg:composer/<vendor>/<package>@<version>` (standard). Each carries the correct `mikebom:source-type` evidence (`composer-packagist` / `composer-vcs` / `composer-path` / `composer-plugin`).
- **SC-003**: A scan of a PHP library project with `composer.json` only (no `composer.lock`) produces components for declared direct deps with `mikebom:sbom-tier = "design"` annotation and the constraint string preserved as `mikebom:requirement-range` evidence.
- **SC-004**: A source tree containing no PHP files produces an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan. (No-op preservation invariant — protects every non-PHP scan.)
- **SC-005**: A scan completes successfully (exit code 0, valid SBOM) on a fixture where one `composer.lock` has corrupted JSON alongside three valid PHP project subdirectories. The output contains components from the three valid projects plus a warning naming the corrupted lockfile path; the corrupted project falls back to design-tier emission from its sibling `composer.json`.
- **SC-006**: An external SBOM consumer reading the emitted CDX JSON can enumerate every Composer-managed dep via the standard `components[]` array filtered on `purl =~ "^pkg:composer/"`. No PHP-specific consumer code is required — the standard PURL filter works.
- **SC-007**: A scan of a fixture with one `packages-dev[]` entry (e.g., `phpunit/phpunit`) produces a component carrying `mikebom:lifecycle-scope = "development"` annotation; downstream `--exclude-scope dev` filtering on that property successfully suppresses the component.
- **SC-008**: A scan of a project whose `composer.json` declares `"name": "acme/my-app"` and `"version": "1.2.3"` produces a main-module component with PURL `pkg:composer/acme/my-app@1.2.3` carrying `mikebom:component-role = "main-module"` annotation; the SBOM's `dependencies[]` block contains an entry for the main-module's bom-ref with `dependsOn` targeting every direct dep's bom-ref.
- **SC-009**: A scan of a deployed-tier fixture (only `vendor/composer/installed.json` present, no lockfile/manifest) produces components carrying `mikebom:sbom-tier = "deployed"` annotation; the Composer 2 wrapper's `dev-package-names[]` array correctly classifies dev entries with `mikebom:lifecycle-scope = "development"`.
- **SC-010**: A scan of a project with BOTH `composer.lock` AND `vendor/composer/installed.json`, where installed.json contains one package NOT present in the lockfile (drift scenario), produces a component for that orphan package carrying both `mikebom:sbom-tier = "deployed"` and `mikebom:lockfile-orphan = "true"` (string value) annotations. Lockfile-pinned packages emit normally without the orphan annotation. A scan of a deployed-tier-only project (installed.json present, no sibling `composer.lock`) does NOT carry the orphan annotation on any component (the orphan-vs-lockfile comparison is undefined when no lockfile exists).

## Assumptions

- **Modern Composer 2.0+ format only**: pre-2020 Composer 1 lockfiles + bare-array `installed.json` are out of scope. Composer 2 is universal in 2026 production.
- **`composer.lock` is the authoritative source when present**: the reader prefers lockfile over `composer.json` (design-tier fallback) over `vendor/composer/installed.json` (deployed-tier fallback). When both lockfile + installed.json are present in the same project, the lockfile wins.
- **No live `composer` invocation**: the reader parses on-disk metadata directly. It does NOT shell out to `composer show` or `composer install` — the `composer` binary isn't guaranteed to exist on the scan host (mikebom is host-portable; scanned target may be a PHP container on a Linux server scanned from a macOS host).
- **The `composer` PURL type IS purl-spec-blessed**: the [purl-spec PURL-TYPES.rst](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst) defines the `composer` type explicitly with `<namespace>/<name>` shape. mikebom emits per the spec — no informal-type follow-up needed.
- **Existing milestone-002 language-reader pattern is the template**: the reader will share architectural shape with `cargo.rs` / `dart.rs` / `npm/` (closest siblings — all parse lockfiles in source-tree-walked project directories). NOT the milestones-002/004/107/135/136 OS-reader pattern (those parse system-installed package DBs).
- **JSON parsing**: `serde_json` is already a pervasive workspace dep; zero new Cargo deps required.
- **VCS-source PURL convention**: per purl-spec, VCS-sourced packages use the `vcs_url=git+<repo>` qualifier; the resolved Git SHA from `source.reference` is preserved separately as `mikebom:vcs-ref` evidence rather than embedded in the version segment — Composer's lockfile carries an upstream-recorded `version` field even for VCS sources (typically the matching tag), so PURL version conveys real upstream identity.
- **Path-deps use `pkg:generic/` placeholders with flattened vendor/name**: these don't have Packagist provenance so emitting under `pkg:composer/` would be wrong. `pkg:generic/` doesn't support the `vendor/name` namespace split, so we flatten with `<vendor>-<package>` to preserve identity readability.
- **`mikebom:source-type` value set**: uses the `composer-` prefix (`composer-packagist` / `composer-vcs` / `composer-path` / `composer-plugin` / `composer-metapackage` / `composer-main-module`) to avoid collision with cargo's existing C1 values (`git` / `path` / `registry`) and Dart's `pub-` prefixed values. Per the established milestone-122 `kmp-` + milestone-137 `pub-` precedent.
- **Vendor/name case-normalization**: PURLs lowercase the `<vendor>` and `<package>` segments per purl-spec `composer-definition.md` ("not case sensitive and must be lowercased"). The lockfile preserves operator-authored case, so mikebom lowercases on construction. Packagist treats them case-insensitively for resolution; PSR-4 autoloader namespace mapping is orthogonal (autoloader resolves the FQCN, not the package coordinate).

## Out of Scope

- **Live invocation of `composer` or any PHP toolchain binary**: read-only metadata parse only.
- **Composer 1 lockfile / installed.json format**: deferred indefinitely (exceptionally rare in 2026 production).
- **Packagist API enrichment**: when only `composer.json` is present (no lockfile), we emit at design-tier with the raw constraints preserved. We do NOT resolve constraints (`^7.4`, `>=2.0 <3.0`, etc.) into pinned versions ourselves nor fetch license/author data from Packagist.
- **Private Packagist authentication**: scanning a project whose lockfile points at a private Composer registry that requires auth is out of scope — we read the registry URL from the lockfile but never contact the registry.
- **`composer.json::autoload.psr-4` / `psr-0` namespace mapping emission**: out of scope as evidence properties for v1. The autoloader configuration is interesting for static-analysis tooling but adds noise to the SBOM without informing dep identity.
- **WordPress plugin/theme discovery via `wp-content/`**: tracked separately as #437. This feature ONLY discovers Composer-managed deps; WordPress installs that bypass Composer (the majority before ≈2020) are out of scope.
- **License extraction**: `composer.lock` DOES carry a `license:` array per package entry — but extracting it requires plumbing through the `licenses` field on `PackageDbEntry` end-to-end and verifying SPDX-expression canonicalization across all real-world Composer license strings (many are bare SPDX IDs, some are arbitrary free-text). Same shape as milestone-135 FR-012 / milestone-136 FR-011 / milestone-137 deferrals — out of scope for v1, tracked as cross-reader follow-up.
- **Transitive dep edges from individual lockfile entries' `require:` maps**: v1 emits main-module → direct deps only; transitive components surface but their inter-edges are deferred to v1.1 (same scope as milestone 137).
- **Plugin-rewritten install paths**: `composer/installers` rewrites where `wordpress-plugin` type packages land on disk (`wp-content/plugins/<name>` instead of `vendor/<vendor>/<name>`). File-claim integration is a separate concern.

## Dependencies and Constraints

- **Builds on milestone 002** (initial language-reader architecture — cargo, npm, pip, etc.).
- **Builds on milestone 137** (Dart pub reader — most recent main-module-per-manifest precedent + prefixed `mikebom:source-type` convention).
- **Reuses the existing source-tree walker** (`scan_fs::walk::safe_walk`) — no new walker logic.
- **Does NOT touch existing language readers** — Composer support is strictly additive.
- **Does NOT introduce new external dependencies** — `serde_json` is already pervasive in the workspace.

## Related

- Closes: #418 (Add PHP/Composer ecosystem support (composer.lock + installed.json))
- Adjacent: #424 (CocoaPods reader — sibling lockfile language ecosystem)
- Adjacent: #422 (Elixir/Mix reader — another lockfile language ecosystem)
- Foundational reference: milestone 002 (cargo + npm + pip lockfile readers), milestone 137 (Dart pub reader — most recent main-module convention)
