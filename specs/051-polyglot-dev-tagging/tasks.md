---
description: "Task list — milestone 051 polyglot dev/test tagging (cargo + gem + maven regression)"
---

# Tasks: Polyglot dev/test tagging — `mikebom:dev-dependency` for cargo and gem

**Input**: spec.md ✅, plan.md ✅, checklists/requirements.md ✅. (No
research.md / data-model.md / contracts/ / quickstart.md — same
4-file tighter template milestones 047/048/049/050 use; plan
resolves the integration-point lookups inline in §Phase 0.)

**Tests**: explicitly requested in the spec (SC-008 — at least one
integration test per ecosystem). Inline unit tests for the new
helpers; integration tests in `tests/scan_cargo.rs`,
`tests/scan_gem.rs`, `tests/scan_maven.rs`.

**Organization**: Three user stories, three commits (one per
ecosystem). US1 cargo (P1) and US2 gem (P1) ship behavior change;
US3 maven (P2) ships a regression-guard test only.

## Format: `[ID] [P?] [Story?] Description`

---

## Phase 1: Setup

- [X] T001 Branch `051-polyglot-dev-tagging` created (via /speckit.specify auto-allocation; rebased onto main `9f8302c` after milestone 050 merge).
- [X] T002 Spec.md + plan.md authored, /speckit.clarify resolved gem source-of-truth (Q1 → Option C, lock + Gemfile + gemspec).
- [X] T003 Baseline `./scripts/pre-pr.sh` clean from a fresh shell (should pass since no edits yet).

---

## Phase 2: Foundational

(No foundational tasks — every change in this milestone lives in
the per-ecosystem readers and their integration tests. The shared
infrastructure — `is_dev: Option<bool>` field + `--include-dev`
flag + C6 catalog row + parity-extractor wiring + CDX/SPDX
emission paths — is already in place per Phase 0 R5. This
milestone is pure additive population on existing infrastructure.)

---

## Phase 3: Commit `feat(051/us1)` — Cargo dev/build dep tagging

**Goal**: Crates declared in `[dev-dependencies]` or
`[build-dependencies]` of any `Cargo.toml` in the workspace get
tagged `is_dev = Some(true)`. When `--include-dev=off`, tagged
entries are dropped (mirrors maven.rs:1786-1823 + milestone 049
Go pattern).

**Independent test**: SC-001 (≥5 fewer cargo components on the
mikebom workspace default scan), SC-002 (≥5 carrying
`mikebom:dev-dependency = true` with `--include-dev`).

### Cargo reader extensions

- [X] T004 [US1] Add a `CargoTomlSections` struct to `mikebom-cli/src/scan_fs/package_db/cargo.rs` holding three fields: `prod_deps: HashSet<String>`, `dev_deps: HashSet<String>`, `build_deps: HashSet<String>`. Each set holds the crate names declared in the corresponding `Cargo.toml` section.
- [X] T005 [US1] Add `parse_cargo_toml(path: &Path) -> Option<CargoTomlSections>` to `cargo.rs`. Use the existing `toml` crate (already in deps) to parse the file. Iterate the `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` tables AND every `target.<cfg>.{dependencies,dev-dependencies,build-dependencies}` table by walking the parsed `toml::Table`. Per R3: warn-and-skip on parse error (return `None` rather than abort the scan).
- [X] T006 [P] [US- [ ] T006 [P] [US1]1] Add inline unit tests in `cargo.rs::tests` for `parse_cargo_toml`: (a) basic three-section split correctly populates the three HashSets; (b) `target."cfg(unix)".dev-dependencies` entries land in `dev_deps`; (c) malformed TOML returns `None`; (d) absent sections produce empty sets, not failure.
- [X] T007 [US1] Add `discover_workspace_manifests(rootfs: &Path) -> Vec<PathBuf>` to `cargo.rs`. For each `Cargo.lock` already discovered by `find_cargo_lockfiles`, find its sibling `Cargo.toml`. If the root `Cargo.toml` declares `[workspace] members = [...]`, resolve each member entry (handle simple glob patterns like `crates/*` via the existing path-walk; no new crate dep) and include each member's `Cargo.toml`. Per R1: fallback to "scan every `Cargo.toml` reachable under rootfs" if glob handling escalates.
- [X] T008 [US1] Add `compute_cargo_prod_set(lock: &CargoLock, direct_prod: HashSet<String>) -> HashSet<(String, String)>` to `cargo.rs`. BFS-walk: seed = the resolved `(name, version)` tuples of every package whose name is in `direct_prod`. Frontier expansion: each visited package's `dependencies = ["bar 1.0 (registry+...)"]` array; parse each dep string to extract the name, look it up in the lockfile's package list, push to frontier. Returns the closed prod-reachable set of `(name, version)` tuples.
- [X] T009 [P] [US- [ ] T009 [P] [US1]1] Inline unit tests for `compute_cargo_prod_set`: (a) single-level prod chain returns just direct deps; (b) 3-level chain returns full closure; (c) production-wins-over-dev — a crate reachable from both prod and dev edges is in prod set; (d) empty seed returns empty.
- [X] T010 [US1] Modify `parse_lockfile` in `cargo.rs` to accept a `prod_set: &HashSet<(String, String)>` parameter. For each `[[package]]` entry, when emitting the `PackageDbEntry`, set `is_dev: prod_set.contains(&(name, version)).not().then_some(true)`. Lockfile-only path (when no Cargo.toml is found alongside) keeps the existing `is_dev: None` (can't classify without dep-section info).
- [X] T011 [US1] Modify `pub fn read` in `cargo.rs`: drop the `_` from `_include_dev`, threading the real flag through. Steps: (1) discover workspace manifests via T007; (2) for each lockfile, parse the corresponding manifest set via T005, union all sections to get this workspace's direct-prod and direct-dev/build sets; (3) call T008 to compute the prod-reachable closure; (4) pass to T010's modified `parse_lockfile`; (5) when `!include_dev`, drop entries with `is_dev == Some(true)`. Mirrors the milestone 049 pattern at `package_db/mod.rs::apply_go_production_set_filter`.

### Cargo integration tests

- [X] T012 [P] [US- [ ] T012 [P] [US1]1] Add `scan_cargo_dev_dependency_is_tagged_and_droppable` to `mikebom-cli/tests/scan_cargo.rs`. Synthetic workspace with root `Cargo.toml` declaring `[dev-dependencies] criterion = "0.5"`, plus a `Cargo.lock` listing criterion + at least one of its transitive deps. Default scan: assert criterion absent. With `--include-dev`: assert criterion present AND its `properties[]` carry `mikebom:dev-dependency = "true"`.
- [X] T013 [P] [US- [ ] T013 [P] [US1]1] Add `scan_cargo_build_dependency_is_treated_as_dev` to `tests/scan_cargo.rs`. Synthetic crate with `[build-dependencies] cc = "1"` only (no normal/dev section for cc). Default scan: cc absent. With `--include-dev`: cc tagged.
- [X] T014 [P] [US- [ ] T014 [P] [US1]1] Add `scan_cargo_production_wins_over_dev` to `tests/scan_cargo.rs`. Synthetic project where crate `foo` appears in BOTH `[dependencies]` and `[dev-dependencies]`. Either-mode scan: foo present, NOT tagged (production wins per FR-003).
- [X] T015 [P] [US- [ ] T015 [P] [US1]1] Add `scan_cargo_workspace_member_dev_dep_is_tagged` to `tests/scan_cargo.rs`. Synthetic 2-crate workspace where only the workspace member crate has a dev-dep. Default scan: dev-dep absent. With `--include-dev`: tagged.

### Verify + commit

- [X] T016 [US1] `cargo +stable test -p mikebom --test scan_cargo` — all 4 new + existing 8 tests pass.
- [X] T017 [US1] `cargo +stable test -p mikebom --bin mikebom -- package_db::cargo` — new unit tests for `parse_cargo_toml` + `compute_cargo_prod_set` pass.
- [X] T018 [US1] Real-world smoke: `cargo +stable build -p mikebom && ~/Projects/mikebom/target/debug/mikebom sbom scan --path /Users/mlieberman/Projects/mikebom --output /tmp/mb051-cargo-default.cdx.json`. Default scan: `jq '[.components[] | select(.purl | startswith("pkg:cargo"))] | length'` returns ≥ 5 fewer cargo components than alpha.9 baseline (per SC-001). Repeat with `--include-dev`: count back to alpha.9 level AND ≥ 5 components carry `mikebom:dev-dependency = "true"` (per SC-002).
- [X] T019 [US1] Audit goldens: if `mikebom-cli/tests/fixtures/cargo/`'s fixture has dev-deps, run `MIKEBOM_UPDATE_*_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression cargo_byte_identity --test spdx_regression cargo_byte_identity --test spdx3_regression cargo_byte_identity` and inspect the diff. Otherwise no regen needed.
- [X] T020 [US1] `cargo +stable test -p mikebom --test holistic_parity` — 11/11 ok (existing C6 wiring picks up new cargo `is_dev` population automatically).
- [X] T021 [US1] Commit: `feat(051/us1): cargo dev/build dep tagging via Cargo.toml [dev-dependencies] + [build-dependencies]`.

---

## Phase 4: Commit `feat(051/us2)` — Gem development/test group tagging

**Goal**: Gems classified as development/test by Gemfile.lock,
Gemfile, OR `*.gemspec` (union semantics per FR-006) get tagged
`is_dev = Some(true)`. Drop on `--include-dev=off`.

**Independent test**: SC-003 (≥3 fewer gem components on a typical
Ruby project default scan), SC-004 (alpha.9 count back with
`--include-dev` AND tagged components).

### Gem reader extensions

- [X] T022 [US2] Extend `GemfileLockDocument` in `mikebom-cli/src/scan_fs/package_db/gem.rs` with `groups: HashMap<String, Vec<String>>` (gem name → group names). Extend `parse_gemfile_lock` (lines 82-200ish) to read optional `group:` continuation lines under `DEPENDENCIES` entries (newer Bundler emits them; older locks omit). When absent, the gem maps to no groups (default = production).
- [X] T023 [US2] Add `parse_gemfile(path: &Path) -> HashMap<String, Vec<String>>` to `gem.rs`. Line-oriented scanner per R2. Track `group :name [, :name2] do` block context; emit each `gem "name"` inside the block into all listed groups. Handle inline `gem "name", group: :foo` and `gem "name", groups: [:foo, :bar]`. Best-effort — warn-and-skip lines outside the canonical idioms (interpolation, `eval_gemfile`, conditional loading). Default group (no enclosing block, no inline keyword) = empty group list = production.
- [X] T024 [US2] Add `parse_gemspec(path: &Path) -> HashMap<String, Vec<String>>` to `gem.rs`. Line-oriented scanner. Match `s.add_dependency "..."` and `s.add_runtime_dependency "..."` (emit gem name with empty group list = production); match `s.add_development_dependency "..."` (emit gem name with `["development"]`). Other DSL forms warn-and-skip per R2.
- [X] T025 [P] [US- [ ] T025 [P] [US2]1] Inline unit tests in `gem.rs::tests` for: (a) lockfile group annotations parsed correctly when present, gracefully ignored when absent; (b) `parse_gemfile` extracts `group :test do; gem "rspec"; end` and `gem "pry", group: :development`; (c) `parse_gemspec` extracts `add_development_dependency` calls; (d) all three return empty maps for nonexistent / unparseable input.
- [X] T026 [US2] Add `compute_gem_prod_set(direct_prod: &HashSet<String>, lock: &GemfileLockDocument) -> HashSet<String>` to `gem.rs`. BFS through `lock.specs`'s indent-6 transitive edges starting from `direct_prod` gem names. Returns prod-reachable gem name set. Inline test: 3-level chain produces full closure.
- [X] T027 [US2] Modify `pub fn read` in `gem.rs`: drop the `_` from `_include_dev`. Per FR-006 (production-wins union): for each Gemfile.lock found, locate co-located Gemfile + `*.gemspec` files; parse all three; compute the merged grouping map (a gem listed as prod in ANY source counts as prod); seed `direct_prod` from gems with empty / no-group classification; compute prod-reachable closure via T026; tag entries NOT in prod set with `is_dev = Some(true)`; drop tagged entries when `!include_dev`.

### Gem integration tests

- [X] T028 [P] [US- [ ] T028 [P] [US2]1] Add `scan_gem_lockfile_group_annotation_is_tagged` to `mikebom-cli/tests/scan_gem.rs`. Synthetic Ruby project with newer-Bundler-style Gemfile.lock carrying `group: test` on rspec. Default: rspec absent. `--include-dev`: tagged.
- [X] T029 [P] [US- [ ] T029 [P] [US2]1] Add `scan_gem_gemfile_groups_are_tagged` to `tests/scan_gem.rs`. Synthetic project with older Gemfile.lock (no group annotations) + Gemfile declaring `group :development do; gem "pry"; end`. Default: pry absent. `--include-dev`: tagged.
- [X] T030 [P] [US- [ ] T030 [P] [US2]1] Add `scan_gem_gemspec_dev_deps_are_tagged` to `tests/scan_gem.rs`. Synthetic library project with `*.gemspec` calling `add_development_dependency "rspec"` and Gemfile.lock listing rspec. Default: rspec absent. `--include-dev`: tagged.
- [X] T031 [P] [US- [ ] T031 [P] [US2]1] Add `scan_gem_production_wins_over_dev` to `tests/scan_gem.rs`. Project where Gemfile says `gem "json", group: :test` AND gemspec says `add_dependency "json"`. Either-mode scan: json present, NOT tagged (FR-006 union semantic).

### Verify + commit

- [X] T032 [US2] `cargo +stable test -p mikebom --test scan_gem` — all 4 new + existing 3 tests pass.
- [X] T033 [US2] `cargo +stable test -p mikebom --bin mikebom -- package_db::gem` — new unit tests pass.
- [X] T034 [US2] Audit `mikebom-cli/tests/fixtures/gem/`'s fixture for dev-group entries; if present, regen the gem golden across all 3 formats. Otherwise no regen.
- [X] T035 [US2] `cargo +stable test -p mikebom --test holistic_parity` — 11/11 ok.
- [X] T036 [US2] Commit: `feat(051/us2): gem development/test group tagging via Gemfile.lock + Gemfile + gemspec union`.

---

## Phase 5: Commit `feat(051/us3)` — Maven regression guard + chore scaffolding

**Goal**: Add an explicit integration test asserting maven's
existing test-scope tagging behavior (zero code change). Bundle
CHANGELOG entry + speckit scaffolding into the same commit per
the 4-file pattern.

**Independent test**: SC-005 (existing maven tests pass unchanged
+ new test asserts `mikebom:dev-dependency = true` appears on
test-scope deps when `--include-dev`).

### Maven regression test

- [X] T037 [US3] Add `scan_maven_test_scope_is_tagged_with_include_dev` to `mikebom-cli/tests/scan_maven.rs`. Synthetic Maven project with a pom.xml declaring `<dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4.13.2</version><scope>test</scope></dependency>` plus a runtime-scope dep. Default: junit absent. `--include-dev`: junit present AND `properties[]` carry `mikebom:dev-dependency = "true"`. Regression guard against future refactors of `maven.rs:1786-1823`.

### CHANGELOG + scaffolding

- [ ] T038 [US3] Edit `CHANGELOG.md` `[Unreleased]` → `### Changed`: name the cargo + gem dev-dep tagging, the gem 3-source union semantics (lock + Gemfile + gemspec, prod wins), the maven regression-guard test addition, and call out: no behavior change for npm/Poetry/Pipfile/Maven, no new flag/annotation/catalog-row.
- [ ] T039 [US3] Stage `specs/051-polyglot-dev-tagging/` (spec.md, plan.md, tasks.md, checklists/requirements.md) and `CLAUDE.md` (auto-updated by `update-agent-context.sh` during /speckit.plan).
- [X] T040 [US3] `cargo +stable test -p mikebom --test scan_maven` — all existing maven tests + new test pass.

### Verify + commit

- [ ] T041 [US3] `./scripts/pre-pr.sh` clean.
- [ ] T042 [US3] Commit: `feat(051/us3): maven test-scope dev-dep regression test + chore scaffolding`.

---

## Phase 6: PR

- [ ] T043 Verify final `./scripts/pre-pr.sh` clean from a fresh shell.
- [ ] T044 Push branch: `git push -u origin 051-polyglot-dev-tagging`.
- [ ] T045 Open PR titled `feat(051): polyglot dev/test tagging — cargo + gem dev-dep tagging via mikebom:dev-dependency`. Body covers: 3-commit summary, the alpha.9 audit gap (cargo + gem `_include_dev` underscore-prefixed unused), gem 3-source union design, audit-grounded numbers (mikebom workspace ≥5 cargo dev-deps; typical Ruby project ≥3 gem dev-deps), 10-SC verification commands, link to the milestone-049 precedent.
- [ ] T046 Verify SC-010: 3 CI lanes (linux x86_64, linux ebpf, macos-latest) green on the PR.

---

## Dependencies

- **Phase 1 (Setup)** blocks all subsequent phases.
- **Phase 2 (Foundational)** is a no-op; no blocking work.
- **Phase 3 (US1 cargo)** independent of US2 / US3 — can be implemented in any order or in parallel by different agents.
- **Phase 4 (US2 gem)** independent of US1 / US3.
- **Phase 5 (US3 maven)** independent of US1 / US2 — but the CHANGELOG entry should reference all three so the commit naturally lands last in the PR sequence.
- **Phase 6 (PR)** blocked on Phases 3 + 4 + 5 all complete.

## Parallel execution opportunities

Within Phase 3:

- T006 (cargo unit tests for `parse_cargo_toml`) ‖ T009 (cargo unit tests for `compute_cargo_prod_set`) ‖ T012-T015 (cargo integration tests) — all touch test files only and have no dependencies on incomplete tasks.

Within Phase 4:

- T025 (gem unit tests) ‖ T028-T031 (gem integration tests) — all parallel.

Cross-phase:

- US1 and US2 are fully independent. Two agents could work T004-T021 (cargo) and T022-T036 (gem) simultaneously without conflict.

## Implementation strategy

**MVP scope**: T004-T011 (cargo reader extensions). Once that lands locally, the user gets the milestone's biggest impact (cargo is the most-touched ecosystem on this codebase). T012-T021 add the test coverage and verification.

**Incremental delivery**: Each user story phase produces a runnable mikebom that improves cargo / gem / maven separately. The commits are structured so any subset can ship as a partial milestone if scope shrinks.

**Format validation**: All 46 tasks follow the strict checklist format — checkbox `- [ ]` (or `- [X]` for completed setup), Task ID `T###`, optional `[P]` for parallelizable, `[US#]` for user-story phases (omitted in setup/polish), description with explicit file paths.
