---

description: "Task list for milestone 138 — PHP/Composer ecosystem reader"
---

# Tasks: PHP/Composer ecosystem reader

**Input**: Design documents from `/specs/138-php-composer-reader/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/composer-component-purl.md ✓, quickstart.md ✓

**Tests**: Integration tests included — established convention for milestones 064 / 066 / 068 / 069 / 070 / 122 / 135 / 136 / 137 main-module-reader work. Synthetic-fixture pattern via `tempfile::tempdir()`.

**Organization**: Tasks grouped by user story (US1 = P1 MVP; US2 = P2 source-discriminator distinction; US3 = P3 design + deployed-tier fallback). Setup + Foundational phases are blocking prerequisites for ALL user stories.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps task to user story phase (US1 / US2 / US3)
- Setup / Foundational / Polish phases: no story label

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeleton + cyclonedx evidence-kind enum extension before any logic lands.

- [X] T001 Create `mikebom-cli/src/scan_fs/package_db/composer.rs` with module-level docstring (mirrors dart.rs preamble: milestone reference, FR list, PURL shape summary), `use` block (`anyhow`, `serde`, `serde_json`, `tracing`, `std::collections::{BTreeMap, HashSet}`, `std::path::{Path, PathBuf}`, `mikebom_common::types::purl::Purl`, `mikebom_common::types::hash::ContentHash`, the existing `PackageDbEntry` from `super`, `ExclusionSet` from `super::exclude_path`), and `pub fn read(rootfs: &Path, include_dev: bool, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>` stub returning `Vec::new()`.

- [X] T002 Add `pub mod composer;` declaration to `mikebom-cli/src/scan_fs/package_db/mod.rs` (placed alphabetically between `pub mod cmake;` and `pub mod conan;`). No `read_all` integration yet — that lands in T009.

- [X] T002b Extend the cyclonedx evidence-kind allowlist in `mikebom-cli/src/generate/cyclonedx/builder.rs` to accept `"composer-lock"`, `"composer-json"`, and `"composer-installed-json"`. The `debug_assert!` gate currently enumerates {rpm-file, rpmdb-sqlite, rpmdb-bdb, dynamic-linkage, elf-note-package, embedded-version-string, symbol-fingerprint, python-stdlib-collapsed, jdk-runtime-collapsed, alpm-local-db, brew-install-receipt, brew-cask-metadata, pubspec-lock, pubspec-yaml} — add the three new values per the milestone-135 / 136 / 137 T002b pattern. Without this, T010 + T011 + T020 would panic in debug builds.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Reader-private types + parsing + PURL helpers + dispatcher integration. MUST complete before ANY user story phase.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Define reader-private serde structs in `mikebom-cli/src/scan_fs/package_db/composer.rs` per `data-model.md`: `ComposerJson` (name, version, type, description, require, require_dev — with `#[serde(rename = "require-dev")]`), `ComposerLock` (packages, packages_dev with `#[serde(rename = "packages-dev")]`, plugin_api_version), `LockfilePackage` (name, version, r#type with `default_type()` returning `"library"`, source, dist, require, license), `LockfileSource` (r#type, url, reference), `LockfileDist` (r#type, url, reference, shasum), `LockfileLicense` enum with `#[serde(untagged)]` covering `Single(String)` and `List(Vec<String>)` variants, and `InstalledJson` (packages, dev, dev_package_names with `#[serde(rename = "dev-package-names")]`). All optional fields use `#[serde(default)]`. `#[allow(dead_code)]` on each struct.

- [X] T004 Implement `fn find_composer_manifests(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` in `composer.rs` that walks via `scan_fs::walk::safe_walk` returning every absolute path containing a `composer.json` file (one per directory). Skip descent into `.git`, `.svn`, `.hg`, `vendor`, `node_modules` directories via `should_skip_descent` helper. Output lex-sorted for cross-platform deterministic discovery.

- [X] T005 Implement `fn find_installed_jsons(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` in `composer.rs` per Q2 clarification: walks via `safe_walk` returning every absolute path matching `vendor/composer/installed.json` regardless of sibling-manifest pairing (multi-layer container support). Skip descent into `.git`, `.svn`, `.hg`, `node_modules`. DO NOT skip `vendor` here (it's the target directory). Output lex-sorted.

- [X] T006 Implement `fn parse_composer_json(path: &Path) -> Result<ComposerJson>`, `fn parse_composer_lock(path: &Path) -> Result<ComposerLock>`, and `fn parse_installed_json(path: &Path) -> Result<InstalledJson>` in `composer.rs` using `serde_json::from_slice` over `std::fs::read`. The `parse_installed_json` helper MUST defensively check root-is-array (Composer 1 format) and return `Err` with a "Composer 1 format not supported" message so callers can warn-and-skip per R3. Errors propagate via `anyhow::Result`. Use `?` operator — no `.unwrap()` per Constitution Principle IV.

- [X] T007 Implement `fn build_purl_for_package(name: &str, version: &str, source: Option<&LockfileSource>, dist: Option<&LockfileDist>, package_type: &str) -> Result<Purl, String>` in `composer.rs` per FR-003 + contracts/composer-component-purl.md:
  - Validate `name` contains exactly one `/` (vendor/package form); error otherwise.
  - Lowercase the vendor + package segments via `name.to_lowercase()` per purl-spec canonical form (research §R4).
  - For `source.type == "git" | "svn" | "hg"` → `pkg:composer/<lc-vendor>/<lc-package>@<version>?vcs_url=<scheme>+<url>` where scheme matches the source type.
  - For `source.type == "path"` → `pkg:generic/<lc-vendor>-<lc-package>@<version>` (vendor+name flattened with `-`).
  - For all other cases (default Packagist OR self-hosted) → `pkg:composer/<lc-vendor>/<lc-package>@<version>`; if `dist.url` is present AND its base (scheme + host) is NOT one of `https://packagist.org`, `https://repo.packagist.org`, `https://api.github.com` (the redirect target), append `?repository_url=<base-with-scheme>` qualifier with minimal-encoding per the dart.rs `minimal_qualifier_encode` precedent.
  - Unknown `source.type` value (not git/svn/hg/path) → `Err(...)` so caller warns-and-skips per FR-008.

- [X] T008 Implement `fn classify_source_type(package: &LockfilePackage) -> &'static str` in `composer.rs` returning the prefixed `mikebom:source-type` value per the per-source-type table in data-model.md: `"composer-packagist"` (default), `"composer-vcs"` (when source.type is git/svn/hg), `"composer-path"` (when source.type is path), `"composer-plugin"` (when package.type is `composer-plugin` or `composer-installer`), `"composer-metapackage"` (when package.type is `metapackage`). Plugin/metapackage classification takes precedence over source-type when both apply (plugin packages are typically Packagist-hosted; the operator-mental-model bucket discrimination matters more).

- [X] T009 Wire `composer::read(rootfs, include_dev, exclude_set)` into `read_all` in `mikebom-cli/src/scan_fs/package_db/mod.rs`. Place the call alphabetically between `cmake::read(...)` and `conan::read(...)` calls. Mirror the dart pattern (`out.extend(...)`) — Composer's signature is simpler (returns `Vec<PackageDbEntry>` directly, no separate divergence record set). NO `collect_claimed_paths` integration — language readers don't claim binary paths.

**Checkpoint**: Foundation ready — `composer::read` is callable from the dispatcher, returns empty Vec, and the cyclonedx evidence-kind gate accepts the new values. User story phases (US1 / US2 / US3) can now proceed in parallel.

---

## Phase 3: User Story 1 — Operator scans a Laravel/Symfony PHP application (Priority: P1) 🎯 MVP

**Goal**: Lockfile-driven SBOM emission for the canonical Laravel/Symfony case — one main-module per `composer.json` + one component per lockfile entry + dep edges from the main-module to its direct deps.

**Independent Test (SC-001)**: Synthetic fixture with `composer.json` (name=`acme/my-app`, version=`1.2.3`, 3 direct deps) + `composer.lock` (5 packages — 3 direct + 2 transitives). Scan produces exactly 5 `pkg:composer/*` lockfile-derived components + 1 main-module + main-module's `depends` lists the 3 direct deps by name.

### Implementation for User Story 1

- [X] T010 [US1] Implement `fn emit_main_module(composer_json_path: &Path, manifest: &ComposerJson, parsed_lockfile: Option<&ComposerLock>) -> Option<PackageDbEntry>` in `composer.rs` per FR-012. Returns `None` when `manifest.name` is `None` or fails the vendor/package validation (warn-and-skip per Q3). Builds a `PackageDbEntry` with: `purl = Purl::new(format!("pkg:composer/{lc_name}@{version}", ..., version = manifest.version.clone().unwrap_or("0.0.0-unknown".into())))?` (lowercase the name segments), `name = manifest.name.clone().unwrap_or_default()`, `version = manifest.version.clone().unwrap_or_else(|| "0.0.0-unknown".into())`, `source_path = composer_json_path.to_string_lossy().into_owned()`, `evidence_kind = Some("composer-json".into())`, `sbom_tier = Some("source".into())`, `source_type = Some("composer-main-module".into())`, `extra_annotations` containing `"mikebom:component-role": "main-module"` + `"mikebom:source-type": "composer-main-module"`. `depends` populated from `manifest.require` + `manifest.require_dev` keys (post-resolution filter handles dev-scope per milestone-052/part-3).

- [X] T011 [US1] Implement `fn emit_lockfile_packages(lockfile_path: &Path, parsed_lockfile: &ComposerLock) -> Vec<PackageDbEntry>` in `composer.rs` per FR-002 + FR-003 + FR-009. For each entry in `parsed_lockfile.packages` (with `lifecycle_scope = Runtime`) followed by `parsed_lockfile.packages_dev.unwrap_or_default()` (with `lifecycle_scope = Development`):
  - Call `build_purl_for_package` (T007) — on `Err`, `tracing::warn!` with the package name + lockfile path and `continue`.
  - Call `classify_source_type` (T008) → `source_type_value`.
  - Construct `extra_annotations` via `build_extra_annotations` (T012).
  - Build the `hashes` vector: if `dist.shasum` is `Some(hex)` AND 40-char hex (SHA-1 length) AND classify_source_type isn't `composer-metapackage` (metapackages have no dist), construct `ContentHash::sha1(hex)?` and push (FR-013). Else empty.
  - Construct `PackageDbEntry` with `purl`, `name = entry.name.clone()` (NOT lowercased — name field preserves source case; PURL is the lowercased form per data-model.md), `version = entry.version.clone()`, `source_path = lockfile_path.to_string_lossy().into_owned()`, `lifecycle_scope`, `evidence_kind = Some("composer-lock".into())`, `sbom_tier = Some("source".into())`, `source_type = Some(source_type_value.to_string())`, `extra_annotations`, `hashes`.

- [X] T012 [US1] Implement `fn build_extra_annotations(package: &LockfilePackage, source_type_value: &str) -> BTreeMap<String, serde_json::Value>` in `composer.rs` per data-model.md per-source-type fields. Always sets `"mikebom:source-type"` to the prefixed value. Source-specific extras:
  - VCS source (`source.type` is `git`/`svn`/`hg`): `"mikebom:vcs-ref": <source.reference>` when reference is present.
  - Path source: `"mikebom:path": <source.url>` when url is present.
  - Plugin classification (when `source_type_value == "composer-plugin"`): `"mikebom:composer-type": <package.type-verbatim>` so consumers can distinguish modern `composer-plugin` from legacy `composer-installer`.

- [X] T013 [US1] Implement the `composer::read` orchestrator body in `composer.rs` per R7: walk for `composer.json` files via `find_composer_manifests`; for each `composer_json_path`, attempt `parse_composer_json`; on error → `tracing::warn!` + skip. Compute sibling `composer.lock` path. If lockfile present and parses → emit main-module via T010 (with `depends` populated from `manifest.require` + `manifest.require_dev` keys) + emit lockfile entries via T011. If lockfile present but fails to parse → `tracing::warn!` and fall back to design-tier (T020). If lockfile absent → fall back to design-tier. Then walk for `vendor/composer/installed.json` files via `find_installed_jsons` (T005) and emit deployed-tier entries via T021. Maintain `seen_purls: HashSet<String>` for orchestrator-level same-PURL dedup. Append all `PackageDbEntry`s to the output Vec.

- [X] T014 [US1] Write integration test file `mikebom-cli/tests/composer_laravel_baseline.rs` with `#[test]` functions covering:
  - `laravel_app_baseline_emits_lockfile_count_plus_main_module` — SC-001 5-component count assertion (3 direct + 2 transitive + 1 main-module = 6 composer components total in the emitted CDX).
  - `main_module_emission` — SC-008 PURL `pkg:composer/acme/my-app@1.2.3` exists in `components[]` (or `metadata.component`) with `mikebom:component-role = main-module` property.
  - `main_module_depends_lists_direct_deps` — US1 acceptance scenario 4: the SBOM's `dependencies[]` entry for the main-module's `bom-ref` carries `dependsOn` targeting each direct dep's bom-ref.
  - `sha1_hash_emitted_for_packagist_entries` — FR-013: a Packagist entry with `dist.shasum` set produces a CDX `hashes[]` entry with `alg = SHA-1`.
  - `dev_scope_filterability` — SC-007: a fixture with a `packages-dev[]` entry produces a component with `mikebom:lifecycle-scope = development`; running with `--exclude-scope dev` (top-level flag BEFORE `sbom scan` subcommand) suppresses it.

  Use `tempfile::tempdir()` + helper functions to write synthetic `composer.json` + `composer.lock` files; invoke via `std::process::Command::new(env!("CARGO_BIN_EXE_mikebom"))`. Pattern matches `mikebom-cli/tests/dart_flutter_app_baseline.rs` (milestone 137) — including the splice-flags-before-subcommand helper for `--exclude-scope`. Guard `.unwrap()` calls with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md convention.

**Checkpoint**: At this point, US1 (the headline MVP — Laravel/Symfony app scan with main-module + lockfile-driven components + dep edges + SHA-1 hash emission) is fully functional and SC-001, SC-007, SC-008 + FR-013 hash emission all pass independently.

---

## Phase 4: User Story 2 — Operator distinguishes Packagist vs VCS / path / plugin deps (Priority: P2)

**Goal**: Surface the discriminator so downstream supply-chain tooling can correctly classify each dep's risk profile.

**Independent Test (SC-002)**: Synthetic fixture with one each of Packagist / VCS / path / composer-plugin in `composer.lock`. Scan. Assert correct PURL shape per FR-003 for each + correct `mikebom:source-type` annotation value.

### Implementation for User Story 2

US2's source-discriminator helpers (`build_purl_for_package` + `classify_source_type` + `build_extra_annotations`) are already implemented in foundational phase (T007 + T008 + T012). This phase adds end-to-end correctness validation + the self-hosted Packagist qualifier branch + the metapackage branch.

- [X] T015 [US2] Augment `build_purl_for_package` (T007) with the self-hosted Packagist qualifier branch: when `dist.url` is `Some(u)` AND `u`'s scheme+host is NOT one of the three default hosts (per T007 description), append `?repository_url=<base-scheme-and-host>` qualifier. Reuse the dart.rs `minimal_qualifier_encode` helper pattern (PURL `pchar` rule allows `:` / `/` / `@` in qualifier values; encode only ` `, `?`, `#`, `&`). Extract base scheme + host via simple `&str` slicing (find third `/` after the scheme delimiter; substring up to that point) — no `url` crate dep needed.

- [X] T016 [US2] Augment `build_purl_for_package` (T007) with explicit VCS branch handling for `source.type: git` → `?vcs_url=git+<url>`, `source.type: svn` → `?vcs_url=svn+<url>`, `source.type: hg` → `?vcs_url=hg+<url>`. The `source.reference` SHA is NOT embedded in PURL (per R4 — Composer's lockfile-recorded `version` field IS the upstream identity even for VCS); it's preserved via the `mikebom:vcs-ref` annotation in T012. Validate `source.reference` is present (warn-and-skip the single entry if absent per R7 Edge Cases).

- [X] T017 [US2] Augment `classify_source_type` (T008) with the plugin precedence rule: when `package.type` is `composer-plugin` OR `composer-installer`, return `"composer-plugin"` REGARDLESS of `source.type` (plugins are typically Packagist-hosted but the operator-mental-model bucket discrimination matters more). When `package.type` is `metapackage`, return `"composer-metapackage"`. Otherwise dispatch on `source.type`.

- [X] T018 [US2] Write integration test file `mikebom-cli/tests/composer_source_discriminators.rs` with `#[test]` functions covering SC-002 per the contracts/composer-component-purl.md example table:
  - `packagist_default_emits_bare_purl` — `pkg:composer/symfony/console@v7.0.4` with `mikebom:source-type = composer-packagist`.
  - `packagist_self_hosted_emits_repository_url_qualifier` — `pkg:composer/acme/internal_lib@2.0.0?repository_url=https://repo.acme.example.com`.
  - `vcs_source_emits_vcs_url_and_vcs_ref` — `pkg:composer/acme/my-fork@dev-main?vcs_url=git+https://github.com/acme/my-fork.git` with `mikebom:vcs-ref = "eb39649..."` annotation.
  - `path_source_emits_generic_placeholder` — `pkg:generic/acme-local-lib@0.1.0` with `mikebom:source-type = composer-path` and `mikebom:path = "../packages/local-lib"`.
  - `composer_plugin_emits_packagist_purl_with_plugin_annotation` — `pkg:composer/composer/installers@v2.3.0` with `mikebom:source-type = composer-plugin` AND `mikebom:composer-type = composer-plugin`.
  - `composer_metapackage_emits_packagist_purl_with_metapackage_annotation` — `pkg:composer/symfony/symfony@v7.0.4` with `mikebom:source-type = composer-metapackage` and empty `hashes` (metapackages have no dist).
  - `vendor_name_lowercased_in_purl` — fixture with `"ACME/MyLib"` in composer.json/lock emits PURL `pkg:composer/acme/mylib@...` per purl-spec canonical form; the `name` field preserves source case.

**Checkpoint**: US1 + US2 both functional. The full lockfile-driven SBOM (Packagist + VCS + path + plugin + metapackage) is correctly classified, addressable via standard `purl` and `properties[].name = mikebom:source-type` filters, with vendor/name properly lowercased.

---

## Phase 5: User Story 3 — Operator scans WITHOUT lockfile OR with installed.json only (Priority: P3)

**Goal**: Design-tier emission for library projects + deployed-tier emission for container scans + orphan-installed.json detection for drift scenarios.

**Independent Test (SC-003 + SC-009 + SC-010)**: Three sub-fixtures: (1) composer.json only → design-tier; (2) installed.json only → deployed-tier; (3) both lockfile + installed.json with one orphan → lockfile-orphan annotation.

### Implementation for User Story 3

- [X] T019 [US3] Define `should_skip_descent(name: &str) -> bool` helper in `composer.rs` skipping `.git`, `.svn`, `.hg`, `vendor`, `node_modules` for the manifest walker; the installed.json walker uses a DIFFERENT skip-set (does NOT skip `vendor`). Wire both `find_composer_manifests` (T004) and `find_installed_jsons` (T005) to their respective skip-sets.

- [X] T020 [US3] Implement `fn emit_design_tier_components(composer_json_path: &Path, manifest: &ComposerJson) -> Vec<PackageDbEntry>` in `composer.rs` per FR-005. For each `(name, constraint)` in `manifest.require` (with `lifecycle_scope = Runtime`) followed by `manifest.require_dev` (with `lifecycle_scope = Development`):
  - Validate `name` is `<vendor>/<package>` form; warn-and-skip on malformed.
  - Skip the entry if `name` matches `manifest.name` (defensive — shouldn't happen but composer.json is operator-authored).
  - Build PURL `pkg:composer/<lc-vendor>/<lc-package>@<sanitized-constraint>` (reuse a sanitize helper similar to dart.rs's `sanitize_purl_version` to neutralize `/`, `?`, `#`, ` ` in the constraint string for PURL safety; the raw constraint is preserved verbatim in `requirement_range`).
  - Construct `PackageDbEntry` with: `purl`, `name = name.clone()`, `version = sanitized-constraint`, `source_path = composer_json_path.to_string_lossy().into_owned()`, `lifecycle_scope`, `evidence_kind = Some("composer-json".into())`, `sbom_tier = Some("design".into())`, `requirement_range = Some(constraint.clone())`, `source_type = Some("composer-packagist".into())` (design-tier is best-effort default), `extra_annotations = { "mikebom:source-type": "composer-packagist" }`.

- [X] T021 [US3] Implement `fn emit_installed_json_components(installed_json_path: &Path, parsed: &InstalledJson, sibling_lockfile_purls: Option<&HashSet<String>>) -> Vec<PackageDbEntry>` in `composer.rs` per FR-006 + Q1 clarification. For each entry in `parsed.packages`:
  - Build the PURL + source-type + hashes via the same helpers (T007/T008/T011 building blocks). Set `lifecycle_scope = Development` if `name` is in `parsed.dev_package_names`, else `Runtime`. Set `sbom_tier = Some("deployed")` and `evidence_kind = Some("composer-installed-json")`.
  - **Orphan detection** (per Q1 + I3 + C1 remediation): the `sibling_lockfile_purls` parameter is `Option<&HashSet<String>>` — `Some(set)` when a sibling `composer.lock` was found and parsed; `None` when no sibling lockfile exists (deployed-tier-only scan; container layer stripped of manifests). Only emit the orphan annotation when `Some(set)` AND `!set.contains(&purl_str)` — i.e., a lockfile EXISTS but doesn't contain this entry. In the `None` case (no lockfile), do NOT emit the annotation (the lockfile-vs-disk comparison is undefined). The annotation value MUST be the string `"true"` (NOT a boolean) per CDX 1.6 `componentProperty.value` wire-format constraint — add via `extra_annotations.insert("mikebom:lockfile-orphan".to_string(), serde_json::Value::String("true".to_string()))`.

- [X] T022 [US3] Wire design-tier + deployed-tier paths into the `composer::read` orchestrator (T013):
  - **Manifest pass**: when sibling `composer.lock` does NOT exist OR fails to parse, fall back to `emit_design_tier_components(composer_json_path, &manifest)` per R7.
  - **Installed.json pass**: after all manifest-discovered projects are emitted, walk for every `vendor/composer/installed.json` under the scan root via `find_installed_jsons` (T005). For each `installed_json_path`, compute the sibling project root (parent of `vendor/`) and look up its lockfile's PURL set in a `HashMap<PathBuf, HashSet<String>>` (built once during the manifest pass, keyed by the project root path = parent of `composer.lock`, populated only when a sibling lockfile parsed successfully). Pass `lookup.get(&project_root)` (which returns `Option<&HashSet<String>>`) directly to `emit_installed_json_components` per T021's updated signature — `None` correctly signals "no sibling lockfile" so the orphan annotation is suppressed for deployed-only scans. Append results (orchestrator-level `seen_purls` handles cross-source dedup).

- [X] T023 [US3] Write integration test file `mikebom-cli/tests/composer_tier_fallbacks.rs` with `#[test]` functions covering:
  - `design_tier_no_lockfile_emits_constraints` — SC-003: fixture with `composer.json` declaring `symfony/console: ^7.0` + `monolog/monolog: ^3.5`; assert 2 components emit each with `mikebom:sbom-tier = design` and `mikebom:requirement-range = ^7.0` (and `^3.5`).
  - `design_tier_no_transitive_deps` — US3 acceptance scenario 2: assert NO components emit for packages not declared in composer.json (transitive deps require lockfile).
  - `design_tier_dev_deps_carry_lifecycle_scope` — US3 acceptance scenario 3: fixture with `require-dev: { "phpunit/phpunit": "^11.0" }` → emitted `phpunit/phpunit` component carries `mikebom:lifecycle-scope = development`; rerun with `--exclude-scope dev` suppresses it.
  - `deployed_tier_installed_json_only_emits_with_sbom_tier_deployed` — SC-009: fixture with only `vendor/composer/installed.json` (3 packages, 1 in dev-package-names) → 3 components emit with `mikebom:sbom-tier = deployed`; the dev one carries `mikebom:lifecycle-scope = development`.
  - `lockfile_orphan_drift_detection` — SC-010: fixture with `composer.lock` (1 package) + `vendor/composer/installed.json` (2 packages, 1 matching lockfile + 1 orphan); assert lockfile entry emits without `mikebom:lockfile-orphan` property AND orphan entry emits WITH `mikebom:lockfile-orphan = "true"` (string value per I3 remediation — CDX 1.6 `componentProperty.value` is string-typed).
  - `deployed_tier_only_no_orphan_annotation` — C1 remediation: fixture with `vendor/composer/installed.json` ONLY (no sibling `composer.lock`); assert NO component carries the `mikebom:lockfile-orphan` annotation (the lockfile-vs-disk comparison is undefined when no lockfile exists).

**Checkpoint**: All three user stories independently functional. US3 enables both library-publisher scans (design-tier) and container-image scans (deployed-tier) plus operator drift detection (orphan annotation).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Edge-case coverage + invariant validation + pre-PR gate.

- [X] T024 [P] Write integration test file `mikebom-cli/tests/composer_edge_cases.rs` covering the spec Edge Cases section + SC-005 + various failure modes:
  - `malformed_lockfile_falls_back_to_design_tier` — SC-005: 4-project fixture where one lockfile has corrupted JSON; scan succeeds (exit 0); the 3 valid projects emit normally + the 4th falls back to design-tier from its composer.json; `tracing::warn!` fires.
  - `monorepo_emits_one_main_module_per_composer_json` — FR-010: fixture with 3 packages under `packages/<member>/composer.json` each with own lockfile; assert 3 main-module components emit; assert NO synthetic monorepo-root component.
  - `missing_name_skips_only_main_module` — Q3: composer.json without `name:` field emits warn-and-skip for main-module but lockfile deps still emit per Q3.
  - `missing_version_falls_back_to_unknown_placeholder` — FR-012: composer.json without `version:` field emits main-module with PURL `pkg:composer/<vendor>/<package>@0.0.0-unknown`.
  - `license_polymorphism_string_or_array` — composer-schema.json: license can be string OR array; parser handles both without panic. Synthetic lockfile entries with each shape.
  - `composer_1_installed_json_warns_and_skips` — R3: a `vendor/composer/installed.json` whose root is a JSON array (Composer 1 format) triggers warn-and-skip with NO components emitted from that file; scan still succeeds.
  - `multi_layer_installed_json_dedupes_via_seen_purls` — Q2: a fixture with two `vendor/composer/installed.json` files at different paths (simulating container layers) containing same packages → unique components emit per PURL (no duplicates) via orchestrator dedup.
  - `git_source_missing_reference_warns_and_skips_entry` — Edge Cases: lockfile entry for git source lacking `source.reference` triggers warn-and-skip for that single entry; other lockfile entries still emit.
  - `vendor_name_uppercase_in_lockfile_lowercased_in_purl` — research §R4: confirm `"ACME/MyLib"` round-trips to `pkg:composer/acme/mylib@...`.

- [X] T025 [P] Verify SC-004 no-Composer-rootfs byte-identity invariant by running the existing CDX/SPDX 2.3/SPDX 3 regression test suites against a synthetic fixture containing zero Composer files. Confirm the emitted SBOMs are byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline. Command: `cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression`. Document the invariant validation in the test file comments.

- [X] T026 Run `./scripts/pre-pr.sh` from repo root to confirm clippy + workspace test gates pass per CLAUDE.md MANDATORY pre-PR gate. Fix any clippy warnings (especially `unwrap_used` in test files — guard with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per convention) and any failing tests. Re-run until both lanes show `0 errors` / `N passed; 0 failed`.

- [X] T027 Run the quickstart.md SC-006 standard-PURL-filter check + the cross-format byte-equivalence diff (CDX vs SPDX 2.3 vs SPDX 3) on a synthetic Laravel-app fixture. The three formats' Composer-component PURL sets MUST be identical when sorted. Document any divergences (none expected — Composer components flow through the standard PackageDbEntry pipeline).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002 (T002 imports the module declared in T001); T002b independent. T002b BLOCKS T010 + T011 + T020 (cyclonedx gate would panic in debug builds without T002b).
- **Foundational (Phase 2)**: All foundational tasks depend on Phase 1 completion. Within Phase 2: T003 → T006 (parser needs struct definitions); T003 → T007 + T008 (PURL + classification helpers need structs); T004 + T005 independent of struct work; T009 depends on `composer::read` stub existing (T001). T019 (`should_skip_descent` helper) is conceptually foundational but referenced in T004 + T005 — best ordered before T004/T005 OR inlined into them.
- **User Story phases (Phase 3 / 4 / 5)**: ALL depend on Foundational completion. Within each phase, tasks are sequential by default unless marked `[P]`.
- **Polish (Phase 6)**: T024 + T025 marked `[P]` — independent files. T026 + T027 depend on all preceding phases.

### User Story Dependencies

- **US1 (P1) MVP**: Depends on Foundational. T014 (integration test) depends on T010 + T011 + T012 + T013. T010 / T011 / T012 / T013 should be implemented in sequence (T010 main-module first, T011 lockfile entries, T012 annotations helper, T013 orchestrator wiring).
- **US2 (P2)**: Depends on Foundational (T007 + T008 + T012 already exist). T015–T017 augment those in-place. T018 (integration test) depends on T015–T017 + T011 (US2 builds atop US1's emit_lockfile_packages).
- **US3 (P3)**: Depends on Foundational + T010 (main-module emission is shared between lockfile + design-tier paths) + T013 (orchestrator). T020 → T021 → T022 → T023.

### Within Each User Story

- Models → services → integration tests (standard ordering).
- No TDD ordering imposed for this milestone — tests follow implementation per the milestone-135/136/137 precedent.

### Parallel Opportunities

- **Phase 1**: T001 + T002b can run in parallel (different files). T002 sequential after T001.
- **Phase 2**: T004 + T005 can run in parallel (different functions in same file but independent logic); T007 + T008 sequential after T003 since both depend on struct definitions but can be edited in same dev sitting.
- **Phase 3**: T010 + T011 + T012 + T013 all modify `composer.rs` — sequentialize.
- **Phase 4**: T015/T016/T017 all modify `build_purl_for_package` or `classify_source_type` in `composer.rs` — sequential.
- **Phase 5**: T020 → T021 → T022 sequential.
- **Phase 6**: T024 + T025 in parallel (different test files); T026 + T027 sequential after.

---

## Parallel Example: Phase 1 + Foundational kickoff

```bash
# Phase 1 — module + cyclonedx gate extension in parallel:
Task: "T001 Create mikebom-cli/src/scan_fs/package_db/composer.rs skeleton"
Task: "T002b Extend evidence-kind enum in mikebom-cli/src/generate/cyclonedx/builder.rs"

# Then T002 (depends on T001):
Task: "T002 Add `pub mod composer;` to mikebom-cli/src/scan_fs/package_db/mod.rs"

# Phase 2 — walkers + struct definitions in parallel:
Task: "T003 Define ComposerJson/ComposerLock/LockfilePackage/InstalledJson structs"
Task: "T004 Implement find_composer_manifests walker"
Task: "T005 Implement find_installed_jsons walker"
```

---

## Implementation Strategy

### MVP First (US1 — P1)

1. Complete Phase 1: Setup (T001, T002, T002b — module skeleton + cyclonedx gate extension).
2. Complete Phase 2: Foundational (T003–T009 — structs, walkers, parsers, PURL+source-type helpers, dispatcher integration).
3. Complete Phase 3: US1 (T010–T014 — main-module + lockfile-driven emission + dep edges + integration test).
4. **STOP and VALIDATE**: Run `cargo +stable test -p mikebom --test composer_laravel_baseline` and confirm SC-001 + SC-007 + SC-008 + FR-013 hash emission all pass.
5. Deploy/demo if ready — the headline use case (Laravel/Symfony app scan with PURLs + dep edges + SHA-1 hashes) ships independently.

### Incremental Delivery

1. Setup + Foundational → dispatch wired, empty Vec.
2. + US1 → MVP shippable: Laravel/Symfony app scans emit full pinned dep graph + main-module + SHA-1 hashes.
3. + US2 → adds source-discriminator filterability (Packagist vs VCS vs path vs plugin vs metapackage) + self-hosted Packagist support + lowercase canonicalization.
4. + US3 → adds library-project (design-tier) + container-image (deployed-tier) + drift-detection (orphan annotation) support.
5. + Polish → edge-case coverage + invariant validation + pre-PR gate green.

### Single-PR Pattern (per milestone-135/136/137 convention)

All phases land in ONE PR per the established language-reader milestone pattern. The single PR includes:
- Module: `mikebom-cli/src/scan_fs/package_db/composer.rs` (~700–900 LOC including doc-comments + unit tests)
- Dispatcher integration: `mikebom-cli/src/scan_fs/package_db/mod.rs` (≤10 LOC)
- Cyclonedx evidence-kind extension: `mikebom-cli/src/generate/cyclonedx/builder.rs` (≤6 LOC)
- Four integration test files: `mikebom-cli/tests/composer_*.rs` (~800–1100 LOC total)
- Closes #418.

---

## Notes

- `[P]` tasks = different files, no dependencies on incomplete tasks.
- `[Story]` label maps task to specific user story for traceability.
- Each user story independently completable and testable.
- Commit after each logical group (e.g., T001–T002b together; T003–T009 together; T010–T014 together; etc.).
- Stop at any checkpoint to validate independently.
- Avoid: cross-story dependencies that break independence (US2 + US3 can ship after US1 in either order).
- **Pre-PR gate (CLAUDE.md MANDATORY)**: `./scripts/pre-pr.sh` MUST pass before opening PR; per-crate `cargo test -p mikebom` is insufficient (clippy `--all-targets` enforces `unwrap_used` inside test mods).
- **No-op invariant (SC-004)**: every change MUST preserve byte-identical SBOM output on non-PHP source trees. T025 is the validation.
- **`--exclude-scope dev` is a TOP-LEVEL mikebom flag** (BEFORE `sbom scan` subcommand), NOT a subcommand flag — per milestone-137 mid-stream discovery. Tests use a splice-before-subcommand helper.
- **Per-PR PR-friendly diff size estimate**: ~1,800–2,400 LOC total (slightly larger than milestone 137 because of the third tier — installed.json).
