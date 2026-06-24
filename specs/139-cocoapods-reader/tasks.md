---

description: "Task list for milestone 139 — CocoaPods ecosystem reader"
---

# Tasks: CocoaPods ecosystem reader

**Input**: Design documents from `/specs/139-cocoapods-reader/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/cocoapods-component-purl.md ✓, quickstart.md ✓

**Tests**: Integration tests included — established convention for milestones 064 / 066 / 068 / 069 / 070 / 122 / 135 / 136 / 137 / 138 main-module-reader work. Synthetic-fixture pattern via `tempfile::tempdir()`.

**Organization**: Tasks grouped by user story (US1 = P1 MVP; US2 = P2 source-discriminator + subspec distinction; US3 = P3 design + deployed-tier fallback). Setup + Foundational phases are blocking prerequisites for ALL user stories.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps task to user story phase (US1 / US2 / US3)
- Setup / Foundational / Polish phases: no story label

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeleton + cyclonedx evidence-kind enum extension before any logic lands.

- [X] T001 Create `mikebom-cli/src/scan_fs/package_db/cocoapods.rs` with module-level docstring (mirrors composer.rs preamble: milestone reference, FR list, PURL shape summary), `use` block (`anyhow`, `serde`, `serde_yaml`, `serde_json`, `tracing`, `std::collections::{BTreeMap, HashSet, HashMap}`, `std::path::{Path, PathBuf}`, `regex::Regex`, `mikebom_common::types::purl::Purl`, `mikebom_common::types::hash::{ContentHash, HashAlgorithm}`, `mikebom_common::resolution::LifecycleScope`, the existing `PackageDbEntry` from `super`, `ExclusionSet` from `super::exclude_path`), and `pub fn read(rootfs: &Path, include_dev: bool, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>` stub returning `Vec::new()`.

- [X] T002 Add `pub mod cocoapods;` declaration to `mikebom-cli/src/scan_fs/package_db/mod.rs` (placed alphabetically between `pub mod cmake;` and `pub mod composer;`). No `read_all` integration yet — that lands in T010.

- [X] T002b Extend the cyclonedx evidence-kind allowlist in `mikebom-cli/src/generate/cyclonedx/builder.rs` to accept `"cocoapods-podfile-lock"`, `"cocoapods-podfile"`, and `"cocoapods-manifest-lock"`. The `debug_assert!` gate enumerates the evidence-kind enum; append the three new values per the milestone-135 / 136 / 137 / 138 T002b pattern. Without this, T012 + T013 + T021 + T022 would panic in debug builds.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Reader-private types + parsing + PURL helpers + dispatcher integration. MUST complete before ANY user story phase.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Define reader-private serde structs in `mikebom-cli/src/scan_fs/package_db/cocoapods.rs` per `data-model.md`: `PodfileLockDoc` (pods using `serde_yaml::Value` for heterogeneous shape, dependencies, external_sources, checkout_options, spec_checksums, podfile_checksum, cocoapods — all with appropriate `#[serde(rename = "...")]` for the YAML keys like `"PODS"`, `"SPEC CHECKSUMS"`, `"EXTERNAL SOURCES"`, `"CHECKOUT OPTIONS"`, `"PODFILE CHECKSUM"`, `"COCOAPODS"`), `PodsEntry` (name, version, transitive_deps) with `root_pod_name()` + `subpath()` accessor methods, `PodfileTargetInfo` (first_target_name, declared_pods) with inner `DeclaredPod` (name, constraint). All optional struct fields use `#[serde(default)]`. `#[allow(dead_code)]` on each struct.

- [X] T004 Implement `fn find_podfile_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` in `cocoapods.rs` walking via `scan_fs::walk::safe_walk` returning every absolute path to a `Podfile.lock` file. Skip descent into `.git`, `.svn`, `.hg`, `Pods`, `node_modules`, `build`, `DerivedData` (the canonical CocoaPods + Xcode build-output directories). Output lex-sorted for cross-platform deterministic discovery.

- [X] T005 Implement `fn find_manifest_locks(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` in `cocoapods.rs` walking via `safe_walk` returning every absolute path to `Pods/Manifest.lock` (per Q3 + R4 — multi-layer container support). Skip descent into `.git`, `.svn`, `.hg`, `node_modules`, `DerivedData`. Do NOT skip `Pods` here (it's the target directory). Match the canonical `<project>/Pods/Manifest.lock` layout — file name `Manifest.lock` whose immediate parent dir is `Pods`. Output lex-sorted.

- [X] T006 Implement `fn find_podfiles(rootfs: &Path, exclude_set: &ExclusionSet) -> Vec<PathBuf>` in `cocoapods.rs` walking via `safe_walk` returning every absolute path to a `Podfile` file (not `Podfile.lock`). Skip descent into the same set as T004. Output lex-sorted. Used in design-tier mode (T020) when no sibling `Podfile.lock` exists.

- [X] T007 Implement `fn parse_podfile_lock(path: &Path) -> Result<PodfileLockDoc>` in `cocoapods.rs` using `serde_yaml::from_slice` over `std::fs::read`. Errors propagate via `anyhow::Result` so callers can warn-and-skip per FR-007. Use `?` operator — no `.unwrap()` per Constitution Principle IV.

- [X] T008 Implement `fn parse_pods_entry(value: &serde_yaml::Value) -> Option<PodsEntry>` in `cocoapods.rs` per data-model.md dispatch logic: when `Value::String(s)`, parse via `parse_pod_spec_string`; when `Value::Mapping(m)` with exactly 1 key, treat the key as the pod-spec string and the value as the transitive-deps array; other shapes → log debug + None. Plus `fn parse_pod_spec_string(s: &str) -> Option<(String, String)>` returning `(name, version)` extracted via regex `^(?P<name>[^ ]+) \((?P<version>[^)]+)\)$`.

- [X] T009 Implement `fn parse_podfile(path: &Path) -> Result<PodfileTargetInfo>` in `cocoapods.rs` per R3: read file via `std::fs::read_to_string`, iterate lines, strip line comments via `line.split_once('#').map(|(s, _)| s).unwrap_or(line).trim_end()`. Use two `regex::Regex::new(...)` static-or-lazy regexes (lazy_static-free approach: use `std::sync::OnceLock`):
  - Target: `^\s*target\s+['"]([^'"]+)['"]\s+do\b` — capture FIRST match into `first_target_name`.
  - Pod: `^\s*pod\s+['"]([^'"]+)['"](?:\s*,\s*['"]([^'"]+)['"])?` — capture every match into `declared_pods` as `DeclaredPod { name, constraint }`.

- [X] T010 Implement `fn build_purl_for_pods_entry(entry: &PodsEntry, external_sources: &BTreeMap<String, serde_yaml::Value>, checkout_options: &BTreeMap<String, serde_yaml::Value>) -> Result<Purl, String>` in `cocoapods.rs` per FR-003 + contracts/cocoapods-component-purl.md:
  - Lookup `external_sources` for the pod (by `entry.name`, with fallback to `entry.root_pod_name()` for subspecs — EXTERNAL SOURCES is keyed by root pod, not per-subspec).
  - If EXTERNAL SOURCES entry has `:path` key → `pkg:generic/<flattened-name>@<version>` placeholder. The pod name MUST be flattened with `-` (e.g., `Firebase/Core` → `Firebase-Core`) to avoid the `pkg:generic/<namespace>/<name>` ambiguity per purl-spec base rules — `pkg:generic/Firebase/Core@1.0` would parse as `namespace=Firebase, name=Core` which is semantically wrong for a path-sourced pod. This matches the milestone-138 composer reader convention (composer.rs flattens `<vendor>-<package>` for the identical reason).
  - If EXTERNAL SOURCES entry has `:git` key → `pkg:cocoapods/<name>@<version>?vcs_url=git+<url>` (use the full name verbatim; subspec subpath would be ambiguous here since git-source identity is at the git-repo level).
  - Otherwise (trunk): if `entry.subpath()` is Some → `pkg:cocoapods/<root>@<version>#<subpath>`; else → `pkg:cocoapods/<name>@<version>`.
  - Names are case-preserved verbatim per purl-spec (no `.to_lowercase()` call — unlike milestone 138 Composer).
  - Use `minimal_qualifier_encode` helper from composer.rs precedent for URL encoding (or copy a local helper).
  - On error (e.g., malformed `:git` URL → empty after stripping; missing `:git` value entirely) → `Err(...)` so caller warns-and-skips per FR-007.

- [X] T011 Implement `fn classify_source_type(entry: &PodsEntry, external_sources: &BTreeMap<String, serde_yaml::Value>) -> &'static str` in `cocoapods.rs` returning the prefixed `mikebom:source-type` value per data-model.md. Lookup `external_sources` for the pod (by `entry.name`, fallback to `entry.root_pod_name()` for subspecs). When EXTERNAL SOURCES has `:git` → `"cocoapods-git"`. When `:path` → `"cocoapods-path"`. Otherwise (default trunk) → `"cocoapods-trunk"`.

- [X] T012 Wire `cocoapods::read(rootfs, include_dev, exclude_set)` into `read_all` in `mikebom-cli/src/scan_fs/package_db/mod.rs`. Place the call alphabetically between `cmake::read(...)` and `composer::read(...)` calls. Mirror the composer pattern (`out.extend(...)`) — CocoaPods's signature returns `Vec<PackageDbEntry>` directly. NO `collect_claimed_paths` integration — language readers don't claim binary paths.

**Checkpoint**: Foundation ready — `cocoapods::read` is callable from the dispatcher, returns empty Vec, and the cyclonedx evidence-kind gate accepts the new values. User story phases (US1 / US2 / US3) can now proceed in parallel.

---

## Phase 3: User Story 1 — Operator scans an iOS app with CocoaPods deps (Priority: P1) 🎯 MVP

**Goal**: Lockfile-driven SBOM emission for the canonical iOS-app case — one main-module per project root + one component per `PODS:` entry (including subspecs as distinct components with `#subpath` PURLs) + dep edges from main-module to direct deps + SHA-1 hashes from `SPEC CHECKSUMS:`.

**Independent Test (SC-001)**: Synthetic fixture with `Podfile` (target `'MyApp' do`, 3 direct deps) + `Podfile.lock` (5 PODS entries — 3 direct + 2 transitives, one of which is a subspec). Scan produces exactly 5 `pkg:cocoapods/*` lockfile-derived components + 1 main-module + main-module's `depends` lists the 3 direct deps by name.

### Implementation for User Story 1

- [X] T013 [US1] Implement `fn build_extra_annotations(entry: &PodsEntry, source_type_value: &str, external_sources: &BTreeMap<String, serde_yaml::Value>, checkout_options: &BTreeMap<String, serde_yaml::Value>) -> BTreeMap<String, serde_json::Value>` in `cocoapods.rs` per data-model.md per-source-type fields. Always sets `"mikebom:source-type"`. Source-specific extras:
  - For trunk subspec entries (`entry.subpath()` is Some AND source_type_value == "cocoapods-trunk"): `"mikebom:subspec": <subpath>` (informational; redundant with PURL `#subpath` but easier to query).
  - For git source: `"mikebom:vcs-ref": <resolved-sha-from-CHECKOUT-OPTIONS>` when `checkout_options.get(root_name).get(":commit")` returns a 40-char hex. Also `"mikebom:vcs-declared-ref": <operator-declared-ref>` when EXTERNAL SOURCES has `:branch`/`:tag`/`:commit` AND distinct from the resolved SHA. Use `lookup_yaml_ruby_symbol` helper (T014) to handle both `:git` and `git` key forms.
  - For path source: `"mikebom:path": <EXTERNAL-SOURCES-path-value>`.

- [X] T014 [US1] Implement `fn lookup_yaml_ruby_symbol<'a>(map: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value>` helper in `cocoapods.rs` that tries both Ruby-symbol-keyed (`:git`) and plain-string-keyed (`git`) lookups against a `serde_yaml::Value::Mapping`. Necessary because `serde_yaml` rendering of Ruby symbols varies depending on the lockfile writer's CocoaPods version (`:git` literal vs `git` string).

- [X] T015 [US1] Implement `fn emit_lockfile_components(lockfile_path: &Path, doc: &PodfileLockDoc, sbom_tier: &str, evidence_kind: &str) -> Vec<PackageDbEntry>` in `cocoapods.rs` per FR-002 + FR-003 + FR-008. Iterate `doc.pods`:
  - For each `serde_yaml::Value`, call `parse_pods_entry` (T008) → `Option<PodsEntry>`. Skip None with debug log.
  - Call `build_purl_for_pods_entry` (T010) — on `Err`, `tracing::warn!` with the pod name + lockfile path and `continue`.
  - Call `classify_source_type` (T011) → `source_type_value`.
  - Construct `extra_annotations` via T013 helper.
  - Build the `hashes` vector per FR-008: for `source_type_value == "cocoapods-trunk"` (NOT git/path which have no SPEC CHECKSUMS), look up `doc.spec_checksums.get(entry.root_pod_name().unwrap_or(&entry.name))` (ROOT-keyed per Phase 0 correction); if 40-char hex, construct `ContentHash::with_algorithm(HashAlgorithm::Sha1, hex)?` and push.
  - Construct `PackageDbEntry` with: `purl`, `name = entry.name.clone()` (case-preserved verbatim per Phase 0 correction), `version = entry.version.clone()`, `source_path = lockfile_path.to_string_lossy().into_owned()`, `lifecycle_scope = Some(LifecycleScope::Runtime)`, `evidence_kind = Some(evidence_kind.to_string())`, `sbom_tier = Some(sbom_tier.to_string())`, `source_type = Some(source_type_value.to_string())`, `extra_annotations`, `hashes`.

- [X] T016 [US1] Implement `fn emit_main_module(project_root: &Path, podfile_path: Option<&Path>, lockfile_path: Option<&Path>, doc: Option<&PodfileLockDoc>, podfile_info: Option<&PodfileTargetInfo>, sbom_tier: &str) -> Option<PackageDbEntry>` in `cocoapods.rs` per FR-012 + Q1 cascade. App-name derivation:
  - First: `podfile_info.and_then(|p| p.first_target_name.clone())` (Podfile target block).
  - Fallback: `project_root.file_name().and_then(|s| s.to_str()).map(String::from)` (parent-dir basename per Q1).
  - When both fail (e.g., scan root is `/`), return `None` after warn.

  Build a `PackageDbEntry` with: `purl = Purl::new(format!("pkg:cocoapods/{app_name}@0.0.0-unknown"))?`, `name = app_name.clone()`, `version = "0.0.0-unknown".to_string()`, `source_path` (Podfile path preferred, else lockfile path, else project_root.to_string_lossy()), `evidence_kind = Some("cocoapods-podfile".into())`, `sbom_tier = Some(sbom_tier.to_string())`, `source_type = Some("cocoapods-main-module".into())`, `extra_annotations = { "mikebom:component-role": "main-module", "mikebom:source-type": "cocoapods-main-module" }`. `depends` populated from:
  - Lockfile mode (`doc.is_some()`): `doc.unwrap().dependencies.iter()` mapped through `parse_dep_name` helper (strip parenthesized version constraint — `"AFNetworking (~> 4.0)"` → `"AFNetworking"`).
  - Design-tier mode (no doc): `podfile_info.unwrap().declared_pods.iter().map(|p| p.name.clone())`.

- [X] T017 [US1] Implement `fn parse_dep_name(dep_string: &str) -> String` helper in `cocoapods.rs` — strip the parenthesized constraint suffix from a DEPENDENCIES entry. `"AFNetworking (~> 4.0)"` → `"AFNetworking"`; `"Firebase/Core"` (no constraint) → `"Firebase/Core"`. Trim whitespace.

- [X] T018 [US1] Implement the `cocoapods::read` orchestrator body in `cocoapods.rs` per R7 three-pass algorithm. Maintain `seen_purls: HashSet<String>` for orchestrator dedup. Track `lockfile_dirs: HashSet<PathBuf>` (parent dirs of every parsed `Podfile.lock`) for use by the Manifest.lock pass per FR-011.
  - **Pass A** (T004): For each `Podfile.lock` found, parse via `parse_podfile_lock`. On error → `tracing::warn!`; check for sibling `Podfile`; if present → fall back to design-tier per pass C below; else skip project. On success:
    - Mark project dir in `lockfile_dirs`.
    - Look for sibling `Podfile` → parse via `parse_podfile` to get `PodfileTargetInfo` (None if absent or parse fails).
    - Emit main-module via T016 (sbom_tier = "source").
    - Emit lockfile components via T015 (sbom_tier = "source", evidence_kind = "cocoapods-podfile-lock"); dedupe through `seen_purls`.
  - **Pass B**: For each `Pods/Manifest.lock` found via T005, compute project root = parent-of-Pods. If project root is in `lockfile_dirs`, SKIP per FR-011. Else parse via `parse_podfile_lock` (same struct — Manifest.lock is byte-equivalent shape). On error → warn + skip. On success:
    - Look for sibling `Podfile` for main-module name (fall back to dir-basename per Q1).
    - Emit main-module via T016 (sbom_tier = "deployed" per Q3).
    - Emit lockfile components via T015 (sbom_tier = "deployed", evidence_kind = "cocoapods-manifest-lock" per Q3); dedupe.
  - **Pass C** (wired in T022, emitter in T021): For each `Podfile` found via T006, compute project dir. If project dir has a sibling `Podfile.lock` that was successfully parsed in Pass A, SKIP (lockfile already emitted). Otherwise emit design-tier via T021 (`emit_design_tier_components`).

- [X] T019 [US1] Write integration test file `mikebom-cli/tests/cocoapods_ios_app_baseline.rs` with `#[test]` functions covering:
  - `ios_app_baseline_emits_pods_count_plus_main_module` — SC-001 fixture (3 direct + 2 transitives including 1 subspec; 5 lockfile + 1 main-module = 6 total). Assert exact count + each expected PURL appears.
  - `main_module_emission_from_target_block` — SC-008: Podfile `target 'MyApp' do` produces `pkg:cocoapods/MyApp@0.0.0-unknown` with `mikebom:component-role = main-module`.
  - `main_module_depends_lists_direct_deps` — US1 acceptance scenario 4 equivalent: dependencies[] for main-module bom-ref targets each DEPENDENCIES bom-ref.
  - `sha1_hash_emitted_for_trunk_pods` — SC-007 / FR-008: a trunk pod with SPEC CHECKSUMS produces CDX `hashes[]` entry with `alg = SHA-1`.
  - `subspec_emits_distinct_component` — SC-009: `Firebase/Core` and `Firebase/Auth` in PODS produce `pkg:cocoapods/Firebase@10.20.0#Core` and `pkg:cocoapods/Firebase@10.20.0#Auth` as DISTINCT components (not collapsed); both carry the same SHA-1 (root-keyed lookup per FR-008).

  Use `tempfile::tempdir()` + helper functions to write synthetic `Podfile` + `Podfile.lock`. Invoke via `std::process::Command::new(env!("CARGO_BIN_EXE_mikebom"))`. Pattern matches `mikebom-cli/tests/composer_laravel_baseline.rs` (milestone 138). Guard `.unwrap()` calls with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md convention.

**Checkpoint**: US1 (Flutter app baseline + main-module + lockfile-driven components + dep edges + SHA-1 + subspec subpath form) is fully functional. SC-001 + SC-007 + SC-008 + SC-009 pass independently.

---

## Phase 4: User Story 2 — Source discriminators + git resolved SHA (Priority: P2)

**Goal**: Surface the trunk/git/path/subspec discriminator so downstream supply-chain tooling can correctly classify each dep's risk profile. Git-source pods include the resolved 40-char SHA from `CHECKOUT OPTIONS:` per Q2.

**Independent Test (SC-002)**: Synthetic fixture with one each of trunk / git / path / subspec in `Podfile.lock`. Scan. Assert correct PURL shape per FR-003 + correct `mikebom:source-type` annotation value + (for git) `mikebom:vcs-ref` matches the CHECKOUT OPTIONS resolved SHA.

### Implementation for User Story 2

US2's source-discriminator helpers (`build_purl_for_pods_entry`, `classify_source_type`, `build_extra_annotations`, `lookup_yaml_ruby_symbol`) are already implemented in foundational + US1 phases. This phase adds the end-to-end correctness validation.

- [X] T020 [US2] Write integration test file `mikebom-cli/tests/cocoapods_source_discriminators.rs` with `#[test]` functions covering SC-002 + the Phase 0 PURL-shape corrections:
  - `trunk_default_emits_bare_purl` — Standard pod → `pkg:cocoapods/AFNetworking@4.0.1` with `mikebom:source-type = cocoapods-trunk`.
  - `subspec_subpath_form` — `Firebase/Core (10.20.0)` → `pkg:cocoapods/Firebase@10.20.0#Core` (PURL `#subpath` form per Phase 0 correction — NOT `?subspec=`).
  - `multi_level_subspec_preserves_slashes` — `Firebase/Database/Realtime (10.20.0)` → `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (raw `/` between subpath segments per Phase 0 correction).
  - `git_source_emits_vcs_url_and_vcs_ref_from_checkout_options` — Q2: fixture with `EXTERNAL SOURCES: MyFork: {:git: 'https://github.com/foo/my-fork.git', :branch: 'main'}` + `CHECKOUT OPTIONS: MyFork: {:commit: 'eb39649...40chars'}` + `PODS: MyFork (1.5.0)`. Assert PURL = `pkg:cocoapods/MyFork@1.5.0?vcs_url=git+https://github.com/foo/my-fork.git`, `mikebom:source-type = cocoapods-git`, `mikebom:vcs-ref = eb39649...`, `mikebom:vcs-declared-ref = main`.
  - `path_source_emits_generic_placeholder` — fixture with `EXTERNAL SOURCES: LocalLib: {:path: '../packages/local-lib'}` + `PODS: LocalLib (0.1.0)`. Assert PURL = `pkg:generic/LocalLib@0.1.0` with `mikebom:source-type = cocoapods-path` + `mikebom:path = '../packages/local-lib'`.
  - `pod_name_case_preserved_in_purl` — fixture with `PODS: AFNetworking (4.0.1)` (mixed-case). Assert PURL preserves case exactly (per purl-spec: CocoaPods is case-sensitive, unlike Composer's lowercase requirement).
  - `subspec_shares_root_pod_sha1_hash` — SC-009 cross-check + FR-008 root-keyed lookup: fixture with `Firebase/Core (10.20.0)` + `Firebase/Auth (10.20.0)` + `SPEC CHECKSUMS: Firebase: <hash>`. Both subspec components carry the SAME `alg = SHA-1` hash content (the root pod's checksum); no separate per-subspec entry in SPEC CHECKSUMS.

**Checkpoint**: US1 + US2 both functional. Full source-discriminator coverage; subspec subpath form validated; git-source resolved-SHA from CHECKOUT OPTIONS surfaced.

---

## Phase 5: User Story 3 — Design + deployed tiers + Q1 dir-basename fallback (Priority: P3)

**Goal**: Design-tier emission for library projects (Podfile-only), deployed-tier emission for container scans (Manifest.lock-only per Q3), and the dir-basename main-module fallback for lockfile-only commits (Q1).

**Independent Test (SC-003 + Q1 + Q3)**: Three sub-fixtures: (1) Podfile only → design-tier with constraint preserved; (2) Manifest.lock only → deployed-tier per Q3; (3) Podfile.lock only (no Podfile) → main-module from dir-basename per Q1.

### Implementation for User Story 3

- [X] T021 [US3] Implement `fn emit_design_tier_components(podfile_path: &Path, podfile_info: &PodfileTargetInfo) -> Vec<PackageDbEntry>` in `cocoapods.rs` per FR-005. For each `DeclaredPod` in `podfile_info.declared_pods`:
  - Skip if pod name equals the main-module name (defensive — shouldn't happen).
  - Use `decl.constraint.clone().unwrap_or_else(|| "unspecified".to_string())` as the version string per FR-005 (`unspecified` placeholder when no constraint declared).
  - Build PURL `pkg:cocoapods/{decl.name}@{sanitize_purl_version(&constraint)}` — reuse a sanitize helper similar to dart.rs / composer.rs's `sanitize_purl_version` to neutralize `/`, `?`, `#`, ` ` for PURL safety; raw constraint preserved in `requirement_range`.
  - Construct `PackageDbEntry` with: `purl`, `name = decl.name.clone()`, `version = sanitized.clone()`, `source_path = podfile_path.to_string_lossy().into_owned()`, `lifecycle_scope = Some(LifecycleScope::Runtime)` (CocoaPods doesn't carry runtime/dev classification), `requirement_range = Some(decl.constraint.clone().unwrap_or_default())`, `evidence_kind = Some("cocoapods-podfile".into())`, `sbom_tier = Some("design".into())`, `source_type = Some("cocoapods-trunk".into())` (design-tier best-effort), `extra_annotations = { "mikebom:source-type": "cocoapods-trunk" }`.

- [X] T022 [US3] Wire design-tier (Pass C) into the `cocoapods::read` orchestrator per T018's Pass C bullet. Walk every `Podfile` via `find_podfiles` (T006); for each, compute project dir; if project dir has a sibling `Podfile.lock` that was successfully parsed in Pass A, SKIP. Else: parse via `parse_podfile`, emit main-module via T016 (sbom_tier = "design", evidence_kind = "cocoapods-podfile"), emit design-tier components via T021. Dedupe through `seen_purls`.

- [X] T023 [US3] Write integration test file `mikebom-cli/tests/cocoapods_tier_fallbacks.rs` with `#[test]` functions covering:
  - `design_tier_podfile_only_emits_constraints` — SC-003: fixture with `Podfile` declaring `pod 'AFNetworking', '~> 4.0'` + `pod 'SDWebImage', '~> 5.18'` (no `Podfile.lock`). Assert 2 components with `mikebom:sbom-tier = design` + `mikebom:requirement-range` matching the constraint string verbatim.
  - `design_tier_no_constraint_uses_unspecified_placeholder` — US3 acceptance scenario 3: `pod 'AFNetworking'` (no version) → PURL `pkg:cocoapods/AFNetworking@unspecified`.
  - `design_tier_no_transitive_deps` — US3 acceptance scenario 2: only declared direct deps emit; no transitives.
  - `deployed_tier_manifest_lock_only_emits_with_sbom_tier_deployed` — Q3 + R4: fixture with only `Pods/Manifest.lock` (no sibling `Podfile.lock`). Assert components carry `mikebom:sbom-tier = deployed` + `mikebom:evidence-kind = cocoapods-manifest-lock`.
  - `manifest_lock_skipped_when_podfile_lock_present` — FR-011: fixture with BOTH `Podfile.lock` (1 pod) AND `Pods/Manifest.lock` (same pod). Assert each PURL appears EXACTLY ONCE (Manifest.lock skipped, not double-counted). Verify via component-count assertion.
  - `lockfile_only_main_module_from_dir_basename` — Q1: fixture in `/tmp/<random>/MyContainerApp/` containing only `Podfile.lock` (no `Podfile`). Assert main-module = `pkg:cocoapods/MyContainerApp@0.0.0-unknown`.

**Checkpoint**: All three user stories functional. US3 enables both library-publisher scans (design-tier), container-image scans (deployed-tier), and lockfile-only commit scenarios (dir-basename fallback).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Edge-case coverage + invariant validation + pre-PR gate.

- [X] T024 [P] Write integration test file `mikebom-cli/tests/cocoapods_edge_cases.rs` covering spec Edge Cases + various failure modes:
  - `malformed_podfile_lock_falls_back_to_design_tier` — SC-005: fixture with malformed Podfile.lock + sibling Podfile. Scan succeeds (exit 0); design-tier components emit; warning fires for the malformed lockfile.
  - `multi_target_podfile_emits_first_target_as_main_module` — FR-010: Podfile with multiple `target 'A' do` / `target 'B' do` blocks. Assert first-target wins for main-module name; both targets' pods coalesce into the union per PODS.
  - `git_source_missing_checkout_options_emits_without_vcs_ref` — Q2 partial case: `EXTERNAL SOURCES: foo: {:git: '...', :tag: 'v1.0'}` but no `CHECKOUT OPTIONS:` entry (rare — happens for very-fresh installs). PURL still emits with `?vcs_url=` qualifier; `mikebom:vcs-ref` annotation absent; `mikebom:vcs-declared-ref = 'v1.0'` still present.
  - `pre_1_0_lockfile_warn_and_skip` — R7 + spec Out-of-Scope: synthetic lockfile lacking `SPEC CHECKSUMS:` section (the pre-1.0 sentinel). Scan still emits components (with empty hashes), no error — pre-1.0 detection is informational only since the PODS shape is unchanged.
  - `empty_pods_block_emits_only_main_module` — Edge case: `PODS: []` with valid Podfile. Only main-module emits; no warnings.
  - `pods_entry_malformed_string_warns_and_skips` — Edge case: a PODS entry that's neither a string nor a single-key map. Warn + skip that entry; other entries still emit.
  - `subspec_purl_subpath_does_not_include_subspec_qualifier` — Phase 0 correction regression: subspec PURL MUST use `#subpath`, NEVER include `?subspec=` (paranoia check against re-introduction of the original spec-guess form).
  - `manifest_lock_multi_layer_dedupes_via_seen_purls` — multi-layer container support (R4): fixture with two `Pods/Manifest.lock` files at different paths containing the same pod. Only ONE component emits per PURL via orchestrator dedup.
  - `path_sourced_subspec_flattens_slash_to_hyphen` — I2 remediation regression: fixture with `EXTERNAL SOURCES: Firebase/Core: {:path: '../firebase-core'}` + `PODS: Firebase/Core (10.20.0)`. Assert PURL = `pkg:generic/Firebase-Core@10.20.0` (slash flattened to hyphen per I2 + composer convention), `mikebom:source-type = "cocoapods-path"`, `mikebom:path = "../firebase-core"`, AND `mikebom:subspec = "Core"` annotation for original-form recovery. Defends against re-introduction of the `pkg:generic/Firebase/Core@1.0` ambiguous form (which would parse as `namespace=Firebase, name=Core` per purl-spec base rules).

- [X] T025 [P] Verify SC-004 no-CocoaPods-rootfs byte-identity invariant by running the existing CDX/SPDX 2.3/SPDX 3 regression test suites against a synthetic fixture containing zero CocoaPods files. Confirm SBOMs are byte-identical to a pre-feature baseline. Command: `cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression`. Document the invariant validation in the test file comments.

- [X] T026 Run `./scripts/pre-pr.sh` from repo root to confirm clippy + workspace test gates pass per CLAUDE.md MANDATORY pre-PR gate. Fix any clippy warnings (especially `unwrap_used` in test files — guard with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per convention) and any failing tests. Re-run until both lanes show `0 errors` / `N passed; 0 failed`.

- [X] T027 Run the quickstart.md SC-006 standard-PURL-filter check + the cross-format byte-equivalence diff (CDX vs SPDX 2.3 vs SPDX 3) on a synthetic iOS app fixture. The three formats' CocoaPods PURL sets MUST be identical when sorted. Document any divergences (none expected — CocoaPods components flow through the standard PackageDbEntry pipeline).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 → T002 (T002 imports the module declared in T001); T002b independent. T002b BLOCKS T015 + T016 + T021 (cyclonedx gate would panic in debug builds without T002b).
- **Foundational (Phase 2)**: All depend on Phase 1 completion. Within Phase 2: T003 → T007/T008 (parsers need struct definitions); T003 → T010/T011 (PURL + classification helpers need structs); T004 + T005 + T006 independent walker functions (parallel-friendly); T009 independent of struct work (regex extraction); T012 depends on `cocoapods::read` stub existing (T001).
- **User Story phases (3 / 4 / 5)**: ALL depend on Foundational completion. Within each phase, tasks are sequential unless marked `[P]`.
- **Polish (Phase 6)**: T024 + T025 marked `[P]` — independent files. T026 + T027 depend on all preceding phases.

### User Story Dependencies

- **US1 (P1) MVP**: Depends on Foundational. T019 (integration test) depends on T013–T018. T013 → T014 → T015 → T016 → T017 → T018 → T019 should be implemented in sequence (all in `cocoapods.rs`).
- **US2 (P2)**: Depends on Foundational (T010 + T011 + T013 already exist after US1). T020 is a pure integration-test addition.
- **US3 (P3)**: Depends on Foundational + T016 (main-module emission shared between lockfile + design-tier paths) + T018 (orchestrator). T021 → T022 → T023 sequential.

### Within Each User Story

- Models → services → integration tests (standard ordering).
- No TDD ordering imposed — tests follow implementation per the milestone-135/136/137/138 precedent.

### Parallel Opportunities

- **Phase 1**: T001 + T002b can run in parallel (different files). T002 sequential after T001.
- **Phase 2**: T004 + T005 + T006 + T009 can run in parallel (independent functions in same file; sequence-compatible). T007/T008/T010/T011 sequential after T003.
- **Phase 3**: T013 → T014 → T015 → T016 → T017 → T018 → T019 sequential (all touch `cocoapods.rs` orchestrator).
- **Phase 4**: T020 standalone (no impl changes needed; pure integration test).
- **Phase 5**: T021 → T022 → T023 sequential.
- **Phase 6**: T024 + T025 in parallel; T026 + T027 sequential after.

---

## Parallel Example: Phase 1 + Foundational kickoff

```bash
# Phase 1 — module + cyclonedx gate in parallel:
Task: "T001 Create mikebom-cli/src/scan_fs/package_db/cocoapods.rs skeleton"
Task: "T002b Extend evidence-kind enum in mikebom-cli/src/generate/cyclonedx/builder.rs"

# Then T002 (depends on T001):
Task: "T002 Add `pub mod cocoapods;` to mikebom-cli/src/scan_fs/package_db/mod.rs"

# Phase 2 — walkers + parsers + regex in parallel:
Task: "T003 Define PodfileLockDoc/PodsEntry/PodfileTargetInfo structs"
Task: "T004 Implement find_podfile_locks walker"
Task: "T005 Implement find_manifest_locks walker"
Task: "T006 Implement find_podfiles walker"
Task: "T009 Implement parse_podfile regex extractor"
```

---

## Implementation Strategy

### MVP First (US1 — P1)

1. Complete Phase 1: Setup (T001, T002, T002b).
2. Complete Phase 2: Foundational (T003–T012 — structs, walkers, parsers, PURL+classification helpers, dispatcher).
3. Complete Phase 3: US1 (T013–T019 — annotation helper, lookup helper, lockfile emission, main-module, orchestrator, integration test).
4. **STOP and VALIDATE**: Run `cargo +stable test -p mikebom --test cocoapods_ios_app_baseline` and confirm SC-001 + SC-007 + SC-008 + SC-009 all pass.
5. Deploy/demo if ready — the headline use case (iOS app scan with main-module + lockfile-driven pods + subspec subpath PURLs + SHA-1 hashes) ships independently.

### Incremental Delivery

1. Setup + Foundational → dispatch wired, empty Vec.
2. + US1 → MVP shippable: iOS app scans emit full pinned pod graph + main-module + SHA-1 hashes + subspec subpath PURLs.
3. + US2 → adds source-discriminator coverage validation + git-source resolved-SHA from CHECKOUT OPTIONS + case-preservation regression.
4. + US3 → adds library-project (design-tier) + container-image (deployed-tier) + lockfile-only-commit (dir-basename fallback) support.
5. + Polish → edge-case coverage + invariant validation + pre-PR gate green.

### Single-PR Pattern (per milestone-135/136/137/138 convention)

All phases land in ONE PR per the established language-reader milestone pattern. The single PR includes:
- Module: `mikebom-cli/src/scan_fs/package_db/cocoapods.rs` (~900–1100 LOC including doc-comments + unit tests)
- Dispatcher integration: `mikebom-cli/src/scan_fs/package_db/mod.rs` (≤10 LOC)
- Cyclonedx evidence-kind extension: `mikebom-cli/src/generate/cyclonedx/builder.rs` (≤6 LOC)
- Four integration test files: `mikebom-cli/tests/cocoapods_*.rs` (~800–1100 LOC total)
- Closes #424.

---

## Notes

- `[P]` tasks = different files, no dependencies on incomplete tasks.
- `[Story]` label maps task to specific user story for traceability.
- Each user story independently completable and testable.
- Commit after each logical group (e.g., T001–T002b together; T003–T012 together; T013–T019 together; etc.).
- Stop at any checkpoint to validate independently.
- **Pre-PR gate (CLAUDE.md MANDATORY)**: `./scripts/pre-pr.sh` MUST pass before opening PR.
- **No-op invariant (SC-004)**: every change MUST preserve byte-identical SBOM output on non-iOS source trees. T025 is the validation.
- **`--exclude-scope dev` is a TOP-LEVEL mikebom flag** (BEFORE `sbom scan` subcommand), per milestone-137 mid-stream discovery. CocoaPods doesn't carry runtime/dev classification at the Podfile.lock level so this matters less than milestone 138, but tests should still use the splice-before-subcommand helper if any `--exclude-scope` testing is added later.
- **Per-PR PR-friendly diff size estimate**: ~1,900–2,400 LOC total (similar to milestone 138 — three input artifacts + subspec subpath wire-format addition).
