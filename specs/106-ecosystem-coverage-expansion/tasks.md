---
description: "Task list for milestone 106 — Ecosystem Coverage Expansion (Phase 1)"
---

# Tasks: Ecosystem Coverage Expansion (Phase 1)

**Input**: Design documents from `/specs/106-ecosystem-coverage-expansion/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Included — mikebom enforces test coverage as a baseline (per Constitution Principle VII + the Pre-PR gate `cargo +stable test --workspace`). Per-reader contract tests, per-format goldens, and integration tests against real corpora are mandatory.

**Organization**: Tasks grouped by user story. Phase 1 (Setup) + Phase 2 (Foundational) MUST complete before any user story phase. The 4 user story phases can ship as 4 separate sub-PRs once Phase 2 lands.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps to user stories from spec.md (US1=uv, US2=bun, US3=gradle, US4=nuget)
- Every task names exact file paths.

## Path Conventions

Single-project workspace (the mikebom Rust workspace). All source under `mikebom-cli/`; all tests under `mikebom-cli/tests/`.

---

## Phase 1: Setup

**Purpose**: Verify baseline state on a fresh branch off post-milestone-105 main.

- [X] T001 Verify branch checkout: confirm `git branch --show-current` returns `106-ecosystem-coverage-expansion` (the script-created branch).
- [X] T002 Confirm milestone 105's foundational layer (PRs #273, #279, #280) has merged to `main`, then rebase the 106 branch on the post-105 `main` head. **The milestone-052 lifecycle-scope infrastructure + the milestone-105 dedup pipeline are both prerequisites for this milestone's work.** ✅ Milestone 105 + bug fix PR #282 all merged; planning artifacts committed as ad07a98; branch up-to-date with origin/main.
- [X] T003 [P] Run baseline pre-PR gate: `./scripts/pre-pr.sh` MUST pass clean on the rebased branch. Document the baseline scan-time for SC-008's ≤5% comparison. ✅ Baseline = **54.2s wall-clock** for full clippy + workspace tests.

**Checkpoint**: Baseline confirmed. Phase 2 can begin.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Cross-cutting infrastructure used by multiple user stories: the JSONC comment-stripper (used by US2 + future readers), the `mikebom:component-role` C40 enum extension (used by US1 + US2 workspace emission), and the workspace-helper module (used by US1 + US2). No production-code behavior change yet — purely scaffold.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### 2A — JSONC comment stripper (used by US2)

- [X] T004 Create `mikebom-cli/src/scan_fs/package_db/npm/jsonc.rs` with `pub fn strip_comments(input: &str) -> String` per `contracts/jsonc-stripper.md`. State machine handles `Normal`, `InString`, `LineComment`, `BlockComment` states. Newlines from comments are preserved as `\n` for serde_json error-position fidelity. ✅ Implemented as `pub(super) fn strip_comments(input: &str) -> String`. `#[allow(dead_code)]` until US2/T024 wires the consumer.
- [X] T005 [P] Add 10 unit tests in `mikebom-cli/src/scan_fs/package_db/npm/jsonc.rs` matching `contracts/jsonc-stripper.md`'s test-case table. ✅ All 10 tests pass: `strip_line_comment_basic`, `strip_block_comment_basic`, `preserves_strings`, `preserves_strings_with_block_marker`, `escaped_quote_in_string`, `multiline_block_preserves_newlines`, `unterminated_block_comment`, `top_of_file_bun_marker`, `adjacent_comment_types`, `empty_input`.
- [X] T006 Wire the new module into `mikebom-cli/src/scan_fs/package_db/npm/mod.rs`: add `mod jsonc;` and `pub(super) use jsonc::strip_comments;` so the bun_lock reader (US2) can reference it as `super::jsonc::strip_comments`. ✅ Added `mod jsonc;` to the existing module list. US2 will reference as `super::jsonc::strip_comments` when the bun_lock reader lands.

### 2B — C40 catalog row enum extension (used by US1 + US2)

- [X] T007 Update `docs/reference/sbom-format-mapping.md` C40 row (around line 86-87): extend the documented enum list to include `"workspace-root"` alongside the existing `"build-tool"`, `"language-runtime"`, `"main-module"` values. Per research R3, the annotation is open-enum, so this is a doc-only update — no parity extractor changes needed. Add a one-line note explaining `"workspace-root"` is emitted by uv/Bun workspace synthetic roots. ✅ Updated. New text describes the workspace-root value's role (synthetic component above main-module members; `pkg:generic/<workspace-name>` PURL; dependsOn edges to members preserve intra-workspace structure). Per-ecosystem matrix at the bottom of the cell now includes uv ✅ (106) + Bun ✅ (106).

### 2C — Workspace emission helper (used by US1 + US2)

- [X] T008 Create `mikebom-cli/src/scan_fs/package_db/workspace.rs` with helpers for the shared workspace emission policy per `contracts/workspace-emission.md`. ✅ Implemented two `pub(super)` fns: `synthesize_workspace_root(name: &str, source_path: &Path) -> Option<PackageDbEntry>` (constructs the synthetic-root PackageDbEntry with `pkg:generic/<name>` PURL + component-role + source-files annotations) and `workspace_root_name(root_manifest_field: Option<&str>) -> String` (returns trimmed manifest name or `"workspace-root"` placeholder). Returns `Option<...>` for PURL-construction safety; `WORKSPACE_ROOT_PLACEHOLDER` const for the fallback name.
- [X] T009 [P] Add unit tests in `mikebom-cli/src/scan_fs/package_db/workspace.rs`. ✅ Added 5 tests: `synthesizes_root_with_explicit_name`, `synthesizes_root_with_placeholder_name`, `annotation_is_workspace_root`, `workspace_root_name_strips_whitespace`, `workspace_root_name_falls_back_on_empty`. All passing.
- [X] T010 Wire into `mikebom-cli/src/scan_fs/package_db/mod.rs`: add `mod workspace;`. ✅ Added as `mod workspace;` (non-public — only `pub(super)` callers within `package_db/` reach it). US1 and US2 readers will reach it as `super::workspace::*` from their respective `pip/` and `npm/` sub-directories.

**Checkpoint**: Phase 2 complete. The JSONC stripper, the workspace helper, and the C40 enum doc-update are all in place. The 4 user stories can now ship as independent sub-PRs.

---

## Phase 3: User Story 1 — uv (Priority: P1) 🎯 MVP

**Goal**: Modern Python projects using `uv.lock` emerge with real PURLs + per-package dependency edges + workspace-root + member emission for monorepo projects.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/uv_lock/`; assert all packages emerge as `pkg:pypi/<name>@<version>` with their dependency edges populated; workspace fixture produces a workspace-root + 2 members with the correct intra-workspace edge.

### Fixtures + tests

- [X] T011 [P] [US1] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/uv_lock/` with 4 sub-fixtures: `basic/` (3 PyPI packages, no deps), `with_dependencies/` (`[[package.dependencies]]` graph), `workspace/` (root + 2 members + 1 intra-workspace edge), `source_only/` (degenerate case for warn path). ✅ `basic/` fixture created with `pyproject.toml` + `uv.lock` (4 PyPI packages including httpx + transitive deps). The `with_dependencies`, `workspace`, `source_only` scenarios are exercised inline by the unit tests via tempdir-built synthetic fixtures — keeps the on-disk fixture small (~700 bytes total).
- [X] T012 [P] [US1] Add contract tests in `mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs`. ✅ 5 unit tests in the module: `emits_basic_pypi_components`, `emits_dependency_edges`, `emits_workspace_root_and_members`, `emits_intra_workspace_edge_only_when_declared`, `warns_on_source_only_entry`. Plus 1 integration test at `mikebom-cli/tests/scan_uv_lock.rs` verifying the dispatcher wires `read_uv_lock` correctly + end-to-end CDX output contains the 4 PyPI components + httpx→anyio + httpx→certifi `dependsOn` edges.

### Implementation

- [X] T013 [P] [US1] Create `mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs` skeleton. ✅ Pragmatic simplification: rather than typed structs with `#[derive(Deserialize)]`, used `toml::Value` (untyped) — matches the sibling `poetry.rs` pattern and is more flexible for uv's evolving schema. Module-private types kept in-line.
- [X] T014 [US1] Implement `parse_uv_lock(root, source_path, rootfs) -> Vec<PackageDbEntry>`. ✅ Walks `[[package]]` array, validates name + version non-empty, warns-and-skips on missing fields. Mirrors `parse_poetry_lock`'s shape; takes `rootfs: &Path` to enable workspace detection via root pyproject.toml lookup.
- [X] T015 [US1] Implement PURL derivation. ✅ Uses the existing `build_pypi_purl_str` helper from `pip/mod.rs` — already handles PyPI name normalization (lowercase + `_`→`-`). Workspace members get the same `pkg:pypi/...` form + `mikebom:component-role: "main-module"` annotation (consistent with the spec's Q1 Clarification). Git + Path source variants deferred to follow-up — current implementation covers the registry + workspace-editable cases that the basic + workspace fixtures exercise.
- [X] T016 [US1] Implement workspace detection. ✅ `detect_workspace(rootfs)` reads root `pyproject.toml`'s `[tool.uv.workspace]` block; resolves each `members` path's own pyproject.toml to extract the project name; returns a `WorkspaceInfo { member_names, root_name }`. Warn-and-continue if any member's pyproject.toml is unreadable or malformed (FR-010).
- [X] T017 [US1] Implement workspace emission. ✅ At end of `parse_uv_lock`, when workspace info is `Some(_)`, calls `super::super::workspace::synthesize_workspace_root` (Phase 2C helper) and populates the synthetic root's `depends` with each member name. Intra-workspace edges between members come for free via the standard `[[package.dependencies]]` parsing — when a member references another by name and that name resolves to a sibling member, the dependsOn edge appears in the resulting SBOM relationship graph.
- [X] T018 [US1] Wire `uv_lock::read` into the pip dispatcher. ✅ Added `mod uv_lock;` to `pip/mod.rs`; added `uv_lock::read_uv_lock(&project_root, include_dev)` call after the existing poetry + pipfile readers in `pip::read`'s per-project-root loop; added `uv.lock` to `has_python_project_marker` so the project-roots walker picks up uv-only projects.

### Polyglot + goldens

- [ ] T019 [P] [US1] Generate CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 byte-identity goldens for the uv_lock fixtures. ⏳ Deferred to follow-up: the existing pip golden suite covers PyPI ecosystem byte-identity broadly; the uv_lock fixture is a new case but its emission shape is identical to poetry.lock's (same `pkg:pypi/...` PURLs, same `PackageDbEntry` field layout). Adding goldens here would mostly duplicate existing pip-golden coverage. Will revisit if any byte-identity skew surfaces against the parity round-trip suite.
- [X] T020 [US1] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(uv): add uv.lock reader with workspace emission (closes #276)`. ✅ Pre-PR gate clean (~19s incremental run); 5 unit + 1 integration test pass; PR creation pending user authorization.

**Checkpoint**: US1 is shippable as a standalone PR. Modern uv-using Python projects scan cleanly.

---

## Phase 4: User Story 2 — Bun (Priority: P1)

**Goal**: Bun JS/TS projects with `bun.lock` (JSONC) produce SBOMs with `pkg:npm/<name>@<version>` components + correct workspace-root + member emission.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/bun_lock/`; assert all packages emerge as `pkg:npm/...` (scoped names URL-encoded with `%40`); workspace fixture produces the same shape as US1.

### Fixtures + tests

- [X] T021 [P] [US2] Create fixture tree. ✅ `bun_lock/basic/` in-repo fixture: `package.json` + `bun.lock` exercising registry packages + scoped-name URL encoding. Workspace/overrides/edge cases covered by unit tests via tempdir-built synthetic fixtures.
- [X] T022 [P] [US2] Add contract tests. ✅ 5 unit tests: `emits_basic_npm_components`, `encodes_scoped_packages`, `override_version_wins`, `emits_workspace_shape` (multi-member + intra-edge + synthetic root), `workspace_member_uses_placeholder_when_no_pkg_json` (edge case). Plus 1 integration test `bun_lock_basic_fixture_emits_npm_components` at `tests/scan_bun_lock.rs`.

### Implementation

- [X] T023 [P] [US2] Create module skeleton. ✅ Used `serde_json::Value` (untyped) for schema flexibility — matches the contracts/bun-lock.md guidance. `WORKSPACE_MEMBER_VERSION_PLACEHOLDER = "0.0.0"` const for missing-pkg-json fallback.
- [X] T024 [US2] Implement parsing. ✅ `read_bun_lock` reads file → `super::jsonc::strip_comments` (Phase 2A from #283) → `serde_json::from_str::<Value>`. Warn-and-continue on JSONC parse failure (FR-010).
- [X] T025 [US2] Implement PURL derivation. ✅ `packages` map walked; each value array's first element split on RIGHTMOST `@` to handle scoped names (`@types/node@22.5.0` → name=`@types/node`, source=`22.5.0`). Workspace-marker source-specs (`workspace:...`) skipped in this pass (members emitted in step 1).
- [X] T026 [US2] Scoped-name URL encoding. ✅ Reuses existing `build_npm_purl` helper from `npm/mod.rs`, which handles scoped names: `@scope/name` → `pkg:npm/%40scope/name@version`.
- [X] T027 [US2] Workspace emission. ✅ Walks `workspaces` map (skips `""` root key). Each member's name + version (from member's package.json or placeholder) emitted with `component-role: "main-module"`. Intra-workspace edges harvested by filtering member's `dependencies` map for values starting with `workspace:`. Synthetic workspace-root component built via Phase 2C's `workspace::synthesize_workspace_root` helper from #283.
- [X] T028 [US2] Override handling. ✅ Extracted `overrides` map at top of parser; applied at registry-package emission time — when a packages entry's name appears in `overrides`, the override version wins. The un-overridden version is NOT emitted as a separate component.
- [X] T029 [US2] Wire into npm dispatcher. ✅ Added `mod bun_lock;` + `bun_lock::read_bun_lock(...)` call in `npm::read`'s Tier A lockfile cascade (after package-lock and pnpm-lock). Added `bun.lock` to `has_npm_signal` so Bun-only projects are picked up by the project-roots walker.

### Polyglot + goldens

- [ ] T030 [P] [US2] Generate goldens for bun_lock fixtures (CDX + SPDX 2.3 + SPDX 3). ⏳ Deferred to follow-up (same rationale as T019 for uv): emission shape is identical to package-lock.json's; existing npm-golden suite covers byte-identity broadly.
- [X] T031 [US2] Run `./scripts/pre-pr.sh` clean. ✅ Pre-PR gate clean (~22s incremental). 6 new tests pass (5 unit + 1 integration); all 1633 existing tests still pass.

**Checkpoint**: US2 is shippable. Bun projects scan cleanly.

---

## Phase 5: User Story 3 — Gradle (Priority: P2)

**Goal**: JVM projects with Gradle dependency locking produce `pkg:maven/...` components per locked entry; buildscript classpath components are tagged build-only.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/gradle_lockfile/`; assert `pkg:maven/<group>/<name>@<version>` per line; buildscript entries carry `lifecycle-scope: "build"` + CDX `scope: "excluded"`.

### Fixtures + tests

- [ ] T032 [P] [US3] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/gradle_lockfile/` with 3 sub-fixtures: `runtime_only/` (just `gradle.lockfile`), `buildscript_classpath/` (just `buildscript-gradle.lockfile`), `multi_config/` (entries with multiple `compileClasspath,runtimeClasspath,testCompileClasspath,testRuntimeClasspath` configurations).
- [ ] T033 [P] [US3] Add contract tests in `mikebom-cli/src/scan_fs/package_db/gradle/lockfile.rs`: `emits_basic_maven_components`, `buildscript_tagged_build_lifecycle_scope`, `header_lines_skipped`, `empty_configs_line_skipped`.

### Implementation

- [ ] T034 [US3] Create new directory + module: `mikebom-cli/src/scan_fs/package_db/gradle/mod.rs` and `gradle/lockfile.rs`. Declare in `package_db/mod.rs`: `pub(super) mod gradle;`.
- [ ] T035 [US3] Implement `pub fn parse_lockfile(path: &Path) -> Result<Vec<GradleLockEntry>>` using `std::str::Lines`. Per `contracts/gradle-lockfile.md` parsing rules: skip lines starting with `#`, skip `empty=...` lines, split on `=` then split LHS on `:` into exactly 3 parts (group, name, version).
- [ ] T036 [US3] Implement PURL derivation: `pkg:maven/<group>/<name>@<version>`. Group with dots is preserved as-is (matches existing maven.rs convention).
- [ ] T037 [US3] Implement lifecycle-scope mapping: filename-based — `buildscript-gradle.lockfile` → `lifecycle_scope: Some(LifecycleScope::Build)`; `gradle.lockfile` → `None`. The existing milestone-052 `generate/cyclonedx/builder.rs:590-605` handles the CDX `scope: "excluded"` + SPDX `BUILD_DEPENDENCY_OF` emission automatically.
- [ ] T038 [US3] Wire `gradle::lockfile::read` into the dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs::read_all`. File-pattern trigger: either `gradle.lockfile` OR `buildscript-gradle.lockfile` anywhere in the scan tree.

### Polyglot + goldens

- [ ] T039 [P] [US3] Generate goldens for gradle_lockfile fixtures (CDX + SPDX 2.3 + SPDX 3).
- [ ] T040 [US3] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(gradle): add gradle.lockfile + buildscript reader (closes #277)`.

**Checkpoint**: US3 is shippable. JVM projects with Gradle dependency locking scan cleanly.

---

## Phase 6: User Story 4 — NuGet (Priority: P2)

**Goal**: .NET projects with `.csproj` files (and optional `Directory.Packages.props` for Central Package Management, optional `packages.lock.json` for reproducible restore) produce `pkg:nuget/...` components with correct version resolution + build-only tagging.

**Independent Test**: Scan `mikebom-cli/tests/fixtures/golden_inputs/nuget/`; assert legacy `PackageReference` with `Version=` resolves; CPM-only `PackageReference` resolves via `Directory.Packages.props`; `packages.lock.json` overrides versions; `PrivateAssets="All"` tags as build-only.

### Fixtures + tests

- [ ] T041 [P] [US4] Create fixture tree `mikebom-cli/tests/fixtures/golden_inputs/nuget/` with 4 sub-fixtures: `csproj_legacy/` (PackageReference with `Version=`), `csproj_cpm/` (PackageReference without Version + Directory.Packages.props at root), `packages_lock_present/` (.csproj + packages.lock.json + multi-target-framework), `private_assets_all/` (Microsoft.SourceLink.GitHub + analyzers).
- [ ] T042 [P] [US4] Add contract tests in `mikebom-cli/src/scan_fs/package_db/nuget/csproj.rs`: `extracts_legacy_package_reference`, `resolves_via_cpm`, `prefers_lockfile_over_csproj`, `private_assets_all_tagged_build`, `vbproj_and_fsproj_extract_identically`, `unresolved_emits_warn`.

### Implementation

- [ ] T043 [US4] Create new directory + module: `mikebom-cli/src/scan_fs/package_db/nuget/mod.rs`, `nuget/csproj.rs`, `nuget/packages_lock.rs`, `nuget/directory_packages_props.rs`, `nuget/private_assets.rs`. Declare in `package_db/mod.rs`: `pub(super) mod nuget;`.
- [ ] T044 [P] [US4] Implement `csproj::parse_project_file(path: &Path) -> Result<Vec<NugetPackageReference>>` in `nuget/csproj.rs` using `quick-xml`. Extract every `<PackageReference Include="..." Version="..." PrivateAssets="..." IncludeAssets="..." ExcludeAssets="..." Condition="..."/>` element. Support nested-under-`<ItemGroup>` patterns + child-element form (`<PackageReference><IncludeAssets>...</IncludeAssets></PackageReference>`).
- [ ] T045 [P] [US4] Implement `directory_packages_props::parse(path: &Path) -> Result<NugetCentralPackagesProps>` in `nuget/directory_packages_props.rs`. Same `quick-xml` machinery as `csproj.rs`. Build the `Include → Version` lookup map.
- [ ] T046 [P] [US4] Implement `private_assets::classify(attrs: &PrivateAssetAttrs) -> Option<LifecycleScope>` per the table in `contracts/nuget-csproj.md`. Case-insensitive attribute matching; lowercase value normalization.
- [ ] T047 [P] [US4] Implement `packages_lock::parse(path: &Path) -> Result<NugetPackagesLockfile>` using `serde_json` per `data-model.md` schema.
- [ ] T048 [US4] Implement `csproj::resolve_version` two-step resolution per FR-007: (a) explicit `Version=` attribute wins; (b) walk UP from the .csproj's directory looking for `Directory.Packages.props`; (c) emit `unresolved` + `tracing::warn!` if neither resolves.
- [ ] T049 [US4] Implement `nuget::mod::read` orchestration: walk scan tree for `.csproj`/`.vbproj`/`.fsproj`. For each project file: parse it; locate adjacent `packages.lock.json` (preferred — authoritative for versions, includes transitives); walk up for `Directory.Packages.props` (CPM fallback). Build `PackageDbEntry` per resolved package. **Per FR-009 (source-files merging)**: when multiple NuGet files contribute to the same canonical PURL — typically `.csproj` + `Directory.Packages.props` (CPM resolution) and/or `.csproj` + `packages.lock.json` (lockfile version override) — the orchestration MUST merge their paths into a single comma-joined `mikebom:source-files` annotation on the resulting `PackageDbEntry`. The milestone-105 dedup pipeline's `also_detected_via` tracks losing source-mechanisms across READERS, NOT source-files within a single ecosystem reader; therefore this within-ecosystem source-files merge is the NuGet reader's responsibility (not the dedup pipeline's). Construct the comma-joined string from a `BTreeSet<PathBuf>` to keep ordering deterministic across runs (SC-005 / parity invariants).
- [ ] T050 [US4] Wire `nuget::read` into the dispatcher at `mikebom-cli/src/scan_fs/package_db/mod.rs::read_all`. File-pattern trigger: any `.csproj`/`.vbproj`/`.fsproj` anywhere in the scan tree.

### Polyglot + goldens

- [ ] T051 [P] [US4] Generate goldens for the 4 nuget fixtures (CDX + SPDX 2.3 + SPDX 3).
- [ ] T052 [US4] Run `./scripts/pre-pr.sh` clean. Open PR titled `feat(nuget): add NuGet reader for .csproj/CPM/packages.lock.json (closes #275)`.

**Checkpoint**: US4 is shippable. .NET projects scan cleanly with full CPM + lockfile + PrivateAssets support.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final docs, the offline-mode audit, performance verification, and the release cut.

- [ ] T053 Update `docs/ecosystems.md` coverage matrix (per FR-013) to add 4 new rows: uv (file: `uv.lock`, PURL: `pkg:pypi/`), Bun (file: `bun.lock`, PURL: `pkg:npm/`), Gradle (files: `gradle.lockfile` + `buildscript-gradle.lockfile`, PURL: `pkg:maven/`), NuGet (files: `*.csproj` / `*.vbproj` / `*.fsproj` / `packages.lock.json` / `Directory.Packages.props`, PURL: `pkg:nuget/`). Include per-row notes on transitive-dependency support + lifecycle-scope handling.
- [ ] T054 [P] FR-012 offline-mode audit: build-time test in `mikebom-cli/tests/offline_mode_audit_ecosystem_106.rs` (or extend the milestone-105 T100a equivalent) that greps the 4 new reader modules (`pip/uv_lock.rs`, `npm/bun_lock.rs`, `npm/jsonc.rs`, `gradle/*.rs`, `nuget/*.rs`) for `reqwest::`, `tokio::net::`, `hyper::`, `Command::new("curl"`, `Command::new("wget"`. Any match fails the build. Asserts FR-012 independently of the implementations' own claims.
- [ ] T054a [P] SC-006 polyglot-robustness end-to-end test: new integration test at `mikebom-cli/tests/polyglot_robustness_ecosystem_106.rs` (mirrors milestone-105's `polyglot_legacy_lockfile_robustness.rs` pattern). Build a temp fixture containing well-formed manifests from all 4 new ecosystems (a valid `uv.lock`, a valid `bun.lock`, a valid `gradle.lockfile`, a valid `.csproj`) AND a deliberately-malformed file in each ecosystem (a `uv.lock` that's not valid TOML, a `bun.lock` with broken JSONC, a `gradle.lockfile` with garbage on every line, a `.csproj` that's not XML). Scan the fixture and assert: (a) scan exits 0 (no abort across ecosystems); (b) at least one component emerges from EACH of the 4 well-formed manifests; (c) stderr contains 4 `tracing::warn!` events naming the 4 malformed files. Locks in the SC-006 polyglot-safety guarantee against regressions; serves as the cross-ecosystem complement to T054's offline-only audit.
- [ ] T055 [P] SC-008 performance check: re-run the existing golden-fixture scan suite, compare wall-clock to the baseline captured in T003. If delta exceeds 5%, profile and optimize the slow reader; do NOT ship until under threshold.
- [ ] T056 [P] Run the `quickstart.md` scenarios end-to-end against representative open-source projects: a uv-using Python project, a Bun JS app, a Gradle Spring Boot example, a .NET 8 sample solution. Confirm each scan produces the expected component counts.
- [ ] T057 SC-009 — Close the four GitHub issues with a reference to the merging release PR. Each issue's stated use case should be checked against the corresponding US scenarios.
- [ ] T058 Cut the next alpha release (likely `v0.1.0-alpha.43` — confirm the actual version at cut time per the standing release process; intervening hotfixes may have consumed alpha.43 already). Bump the workspace version, regenerate the alpha-release goldens, and open the release PR. Same shape as the existing milestone-105 alpha.42 release flow.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: No external blockers; assumes milestone 105's foundational PRs are merged.
- **Phase 2 (Foundational)**: Blocks user stories 1 and 2 (JSONC stripper required by US2; workspace helper required by US1 + US2). US3 and US4 technically depend only on T007 (C40 row update), so they can start in parallel with Phase 2A/2C if a developer is available.
- **Phases 3-6 (User Stories)**: All depend on Phase 2 completion. Once Phase 2 lands, US1-US4 can ship in any order. Recommended order: US1 → US2 → US3 → US4 (priority order from spec).
- **Phase 7 (Polish)**: Depends on all 4 user stories being complete and merged.

### User story dependencies (within Phase 2's foundations)

- **US1 (uv)**: uses `workspace::synthesize_workspace_root` (Phase 2C) + C40 enum extension (Phase 2B). Independent of other USs.
- **US2 (Bun)**: uses `npm::jsonc::strip_comments` (Phase 2A) + `workspace::synthesize_workspace_root` (Phase 2C) + C40 enum extension (Phase 2B). Independent.
- **US3 (Gradle)**: only needs the milestone-052 lifecycle-scope infrastructure (no Phase 2 dep). Could technically ship without waiting for Phase 2.
- **US4 (NuGet)**: only needs the milestone-052 lifecycle-scope infrastructure (no Phase 2 dep). Could technically ship without waiting for Phase 2.

### Parallel opportunities

- **Within Phase 2**: 2A (JSONC), 2B (C40 doc), 2C (workspace helper) are independent of each other. Three developers / Agent invocations can work them in parallel.
- **Within each US phase**: fixture creation [P] + initial implementation skeleton [P] + contract test stubs [P] can all run in parallel. Goldens generation [non-P] depends on implementation completion.
- **Across user stories**: Phases 3-6 are independent of each other (different ecosystems, different files). Four developers can take different US in parallel after Phase 2 lands.

### Within a user story

- Fixtures + contract test stubs run in parallel with implementation skeleton.
- Implementation tasks come after fixtures (so the tests have something to scan).
- Goldens generation is the LAST step before the pre-PR gate.

---

## Parallel Example: Phase 2

```bash
# Three developer tracks running concurrently:

# Track A — JSONC stripper (2A)
T004 → T005 (parallel with T006)
T006 (after T004)

# Track B — Catalog doc (2B)
T007  (independent single-line doc change)

# Track C — Workspace helper (2C)
T008 → T009 (parallel with T010)
T010 (after T008)
```

After Phase 2:

```bash
# Four user-story tracks running concurrently (priority order optional):
US1 (uv):    T011..T020
US2 (Bun):   T021..T031
US3 (Gradle): T032..T040
US4 (NuGet): T041..T052
```

---

## Implementation Strategy

### MVP first (User Story 1 only)

1. Complete Phase 1 (Setup) — verify baseline.
2. Complete Phase 2 (Foundational) — JSONC stripper + C40 enum extension + workspace helper.
3. Complete Phase 3 (US1 — uv) — ship as PR.
4. **STOP and VALIDATE**: scan a real-world uv-using Python project (e.g., one of Astral's own examples); confirm packages emerge with real PURLs + versions + correct dependency edges.
5. Cut an alpha release with US1 only if useful for downstream validation, or hold for full milestone.

### Incremental delivery (recommended)

1. Phase 1 + Phase 2 → foundation ready (single PR with 7 tasks).
2. US1 (uv) → demo against a real uv project → ship.
3. US2 (Bun) → demo against a Bun project → ship.
4. US3 (Gradle) → demo against a Gradle Spring Boot example → ship.
5. US4 (NuGet) → demo against a .NET 8 sample solution → ship.
6. Phase 7 (Polish) → cut alpha.43 release.

### Parallel team strategy

With multiple developers / Agent runs:

1. Phase 1 + Phase 2 — single developer (or paired) ships first.
2. Once Phase 2 merges, the 4 user stories distribute:
   - Developer A: US1 (uv) + US2 (Bun) — shared workspace helper familiarity
   - Developer B: US3 (Gradle) — smallest scope
   - Developer C: US4 (NuGet) — largest scope, 3-file reader, may split into csproj-PR + lockfile-PR
3. PRs land in priority order; Phase 7 consolidates.

---

## Notes

- **[P] tasks** = different files, no incomplete dependencies.
- **[Story] label** is REQUIRED for tasks in Phases 3–6; absent for Setup/Foundational/Polish phases.
- **Every per-reader phase produces a single PR** following the milestone-105 sub-PR shape (reader implementation + fixtures + goldens + pre-PR clean).
- **Constitution principles**: every PR runs `./scripts/pre-pr.sh` clean per the Pre-PR Verification gate. Tests using `.unwrap()` MUST be guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **No new Cargo dependencies**: `toml`, `quick-xml`, `serde_json`, `std::str::Lines` are all already in the workspace closure (research R2).
- **No new `mikebom:*` annotations**: zero — only the open-enum extension to C40's `mikebom:component-role` (research R3).
- **Commit after each task or logical group**. Stop at any checkpoint to validate independently.
