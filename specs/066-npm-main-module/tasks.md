---
description: "Task list for milestone 066 — npm source-tree main-module component"
---

# Tasks: npm source-tree main-module component for package.json roots + workspace members

**Input**: Design documents from `/specs/066-npm-main-module/`
**Prerequisites**: spec.md ✅, plan.md ✅, data-model.md ✅, contracts/npm-main-module-component.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [ ] T001 Confirm working tree clean and on branch `066-npm-main-module`.

## Phase 2: Foundational

- [ ] T002 [P] Update `docs/reference/sbom-format-mapping.md` C40 row to extend ecosystem-coverage matrix: Go ✅, cargo ✅, npm ✅; pip/maven/gem still in #104.

## Phase 3: User Story 1 — npm project SBOMs identify the project itself (P1) 🎯 MVP

### Implementation

- [ ] T003 [US1] Implement `build_npm_main_module_entry(manifest_path: &Path) -> Option<PackageDbEntry>` in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. Reads `package.json` JSON; returns `None` if `name` missing OR (`private: true` AND no `version`). Otherwise constructs the entry with PURL via existing `build_npm_purl(name, version)` helper (handles scope encoding); version is the literal `version` string or `"0.0.0-unknown"` placeholder per Q1; `parent_purl: None`; `sbom_tier: Some("source")`; C40 annotation `mikebom:component-role: "main-module"`; populates `depends` from `dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies` keys (post-existing-scope-filter).
- [ ] T004 [US1] Implement `find_package_json_manifests(rootfs: &Path) -> Vec<PathBuf>` in `mikebom-cli/src/scan_fs/package_db/npm/walk.rs`. Walks rootfs alphabetically (deterministic for cross-host goldens); honors existing `should_skip_descent` (`node_modules/`, `target/`, etc.); returns every `package.json` discovered. Independent of lockfile presence.
- [ ] T005 [US1] Implement `dedup_npm_main_modules_by_purl(entries: &mut Vec<PackageDbEntry>) -> Vec<DroppedDuplicate>` in `walk.rs`. Predicate is C40-tag-driven; first-discovered wins; returns dropped-path records for caller-side `tracing::warn!` emission. Mirrors cargo's `dedup_main_modules_by_purl` from milestone 064 T010.
- [ ] T006 [US1] Wire main-module emission into `npm::read()` in `mikebom-cli/src/scan_fs/package_db/npm/mod.rs`. Pass A: build all main-modules via `find_package_json_manifests` + `build_npm_main_module_entry`. Pass B (existing): lockfile-driven dep emission. Phase A runs after Phase B with augment-existing-or-emit-new logic mirroring cargo `read()` from milestone 064 T011. Add `tracing::info!` for `main_modules_emitted` count + `same_purl_duplicates_dropped` count.
- [ ] T007 [US1] Add unit tests for the new helpers in `npm/walk.rs::tests`: (a) literal version basic, (b) scoped name encoding, (c) `private: true` + no version → None, (d) `private: true` + version → emits, (e) name without version → placeholder, (f) name missing → None, (g) workspaces-only manifest (no name/version) → None, (h) dedup with collision drops first-wins, (i) dedup with no collision returns empty Vec.

### Tests for US1

- [ ] T008 [US1] Create `mikebom-cli/tests/fixtures/npm-workspace/`: workspace root with `private: true` + `workspaces: ["packages/*"]`; member `a/package.json` (`name: "a"`, `version: "0.5.0"`); member `b/package.json` (`name: "b"`, `version: "0.5.0"`, `dependencies: { "a": "*" }`); committed `package-lock.json`; minimal `index.js` files. Add a README.md.
- [ ] T009 [P] [US1] Create `mikebom-cli/tests/fixtures/npm-scoped-package/package.json` with `name: "@kusari/foo"` + `version: "1.0.0"` for scope-encoding regression.
- [ ] T010 [US1] Add integration tests in `mikebom-cli/tests/scan_npm.rs` (new file): `scan_npm_single_package_emits_main_module`, `scan_npm_scoped_name_encodes_at_sigil`, `scan_npm_workspace_emits_per_member_main_modules`, `scan_npm_workspace_path_dep_emits_member_to_member_edge`, `scan_npm_workspace_root_with_private_no_version_skipped`.

## Phase 4: Polish

- [ ] T011 Regenerate `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/npm.{cdx,spdx,spdx3}.json` plus polyglot fixtures bundling npm. Apply cross-host playbook.
- [ ] T012 Add new fixture goldens for `npm-workspace/` and `npm-scoped-package/`.
- [ ] T013 CHANGELOG.md `[Unreleased]` entry for milestone 066 — same structure as the 064 entry. Reference #104, #103, #125. Note that the multi-main-module + plural-DESCRIBES infrastructure from #127 carries over at zero marginal cost.
- [ ] T014 Update `docs/design-notes.md` per-ecosystem coverage matrix.
- [ ] T015 Run `./scripts/pre-pr.sh`; fix any issues.
- [ ] T016 Open PR via `gh pr create` with title `feat(066): npm source-tree main-module component (closes npm slice of #104)`.

## Dependencies

```text
T001 → T002 → [T003, T004, T005] (parallel) → T006 → T007 → T008 → [T009, T010] → [T011, T012, T013, T014] → T015 → T016
```

## Format validation

All 16 tasks follow the required format. Setup phase (T001), Foundational (T002), US1 (T003–T010), Polish (T011–T016).
