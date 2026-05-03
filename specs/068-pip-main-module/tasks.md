---
description: "Task list for milestone 068 — pip source-tree main-module component (PEP 621)"
---

# Tasks: pip source-tree main-module component for PEP 621 pyproject.toml roots

**Input**: Design documents from `/specs/068-pip-main-module/`
**Prerequisites**: spec.md ✅, plan.md ✅, data-model.md ✅, contracts/pip-main-module-component.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [ ] T001 Confirm working tree clean and on branch `068-pip-main-module`.

## Phase 2: Foundational

- [ ] T002 [P] Update `docs/reference/sbom-format-mapping.md` C40 row to extend ecosystem-coverage matrix: Go ✅, cargo ✅, npm ✅, pip ✅; maven/gem still in #104.

## Phase 3: User Story 1 — pip project SBOMs identify the project itself (P1) 🎯 MVP

### Implementation

- [ ] T003 [US1] Implement `build_pip_main_module_entry(project_root: &Path) -> Option<PackageDbEntry>` in `mikebom-cli/src/scan_fs/package_db/pip/mod.rs`. Reads `pyproject.toml` via existing `toml::from_str`. Returns `None` when `[project]` table is absent (Poetry-only or non-Python `pyproject.toml`). Returns `None` when `[project].name` is absent. Otherwise: build PURL via existing `build_pypi_purl_str` (which calls `normalize_pypi_name_for_purl` for PEP 503); resolve version per FR-001 (literal `[project].version` → use it; missing OR in `[project].dynamic` → `0.0.0-unknown` placeholder + `tracing::warn!` if version simply missing without dynamic flag); set `parent_purl: None`, `sbom_tier: Some("source")`, C40 annotation `mikebom:component-role: "main-module"`; populate `depends` from `[project.dependencies]` + `[project.optional-dependencies].*` keys (parse PEP 508 requirement strings to extract just the package name).
- [ ] T004 [US1] Implement `dedup_pip_main_modules_by_purl(entries: &mut Vec<PackageDbEntry>) -> Vec<DroppedDuplicate>` in `mikebom-cli/src/scan_fs/package_db/pip/mod.rs`. Mirrors cargo (064 T010) / npm (066 T005) C40-tag-driven dedup. Returns `DroppedDuplicate` records for caller-side `tracing::warn!`.
- [ ] T005 [US1] Wire into `pip::read()` in `mikebom-cli/src/scan_fs/package_db/pip/mod.rs`. Phase A (after the existing per-project-root tier loop): walk `candidate_python_project_roots(rootfs)` again, call `build_pip_main_module_entry` per root. Augment-existing-entry pattern: when a same-PURL entry already exists (lockfile-derived OR Tier-1 venv-derived per FR-011), layer C40 + `parent_purl: None` on top while preserving the existing entry's `sbom_tier`/`evidence_kind`/`hashes` (FR-011: venv evidence wins). When no same-PURL entry exists, emit net-new. Call `dedup_pip_main_modules_by_purl` + emit consolidated `tracing::warn!` for collisions. Add `tracing::info!` reporting `main_modules_emitted` count.
- [ ] T006 [US1] Add `tracing::info!` for the FR-002 Poetry-only-skip case in `build_pip_main_module_entry` (or in `read()`'s Phase A loop). Message: `"pip: skipping main-module emission for [tool.poetry]-only pyproject.toml — Poetry schema deferred per #104"`. Include the manifest path. Operator-facing visibility for the deliberate scope decision.
- [ ] T007 [US1] Add unit tests in `pip::tests` mod (or sibling test module): (a) `[project] name + version` → entry with PURL `pkg:pypi/<normalized-name>@<version>`; (b) `[project] name = "Some_Package.Name"` → PURL `pkg:pypi/some-package-name@<version>` (PEP 503 norm); (c) `[project]` with `dynamic = ["version"]` → entry with `0.0.0-unknown` placeholder; (d) `[tool.poetry]` only (no `[project]`) → returns `None`; (e) both `[project]` AND `[tool.poetry]` → emit from `[project]` (FR-003); (f) `[project]` with `name` but no `version` and not in `dynamic` → entry with placeholder; (g) `dedup_pip_main_modules_by_purl`: no collision → empty Vec; two same-PURL → first kept, one DroppedDuplicate; predicate is C40-tag-driven (regular pip components untouched).

### Tests for US1

- [ ] T008 [P] [US1] Create `mikebom-cli/tests/fixtures/pip-pyproject-pep621/`: minimal PEP 621 `pyproject.toml` (`name = "my_pkg"`, `version = "1.0.0"`, no `[tool.poetry]`); README.md.
- [ ] T009 [P] [US1] Create `mikebom-cli/tests/fixtures/pip-pyproject-poetry-only/`: `pyproject.toml` with `[tool.poetry]` only (no `[project]`); README.md noting this fixture exercises FR-002 skip.
- [ ] T010 [US1] Add integration tests in `mikebom-cli/tests/scan_pip.rs` (new file or extension of existing): `scan_pip_pep621_emits_main_module_in_metadata_component` (US1 AS#1 / SC-001), `scan_pip_pep503_name_normalization_in_purl` (US1 AS#2 / SC-002), `scan_pip_dynamic_version_uses_placeholder` (US1 AS#3), `scan_pip_poetry_only_skips_main_module` (US1 AS#4 / FR-002), and `scan_pip_editable_install_merges_venv_evidence` (FR-011 — synthesize a tempdir with `pyproject.toml` (`name`, `version`) plus a fake venv `.dist-info` directory whose METADATA declares the matching `(name, version)`; assert the resulting main-module carries `mikebom:sbom-tier: deployed` (from venv) AND `mikebom:component-role: main-module` (from Phase A) — verifies the augment-existing-entry logic correctly preserves venv evidence on the main-module identity).

## Phase 4: Polish

- [ ] T011 Regenerate `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/pip.{cdx,spdx,spdx3}.json` plus polyglot fixtures bundling pip. Apply cross-host playbook.
- [ ] T012 Add new fixture goldens for `pip-pyproject-pep621/` if elevated to a golden-locked fixture (optional — depends on whether the integration tests exercise byte-identity).
- [ ] T013 CHANGELOG.md `[Unreleased]` entry for milestone 068 — same structure as 064/066. Reference #104, #103, #125. Note this is the third per-ecosystem main-module milestone post-alpha.12 and updates the per-ecosystem coverage matrix to Go ✅, cargo ✅, npm ✅, pip ✅; maven/gem still pending.
- [ ] T014 Update `docs/design-notes.md` per-ecosystem coverage matrix.
- [ ] T015 Run `./scripts/pre-pr.sh`; fix any issues.
- [ ] T016 Open PR via `gh pr create` with title `feat(068): pip source-tree main-module component (closes pip slice of #104)`.

## Dependencies

```text
T001 → T002 → [T003 → T004 → T005 → T006 → T007] (US1 implementation chain)
                    → [T008, T009] (parallel fixtures)
                    → T010 (integration tests, depends on T005 wire-up)
                          → [T011, T012, T013, T014] → T015 → T016
```

## Format validation

All 16 tasks follow the required format. Setup (T001), Foundational (T002), US1 (T003-T010), Polish (T011-T016).

## MVP scope

US1 alone covers SC-001 + SC-002 + FR-002 (the dominant value of this milestone). US2 (consumer signal) and US3 (doc root) inherit from milestones 053+064+066+#127 with zero additional work — verifying them is implicit in the goldens regen at T011.
