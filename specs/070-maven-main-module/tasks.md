---
description: "Task list for milestone 070 — maven source-tree main-module component (closes #104)"
---

# Tasks: maven source-tree main-module component for top-level pom.xml roots + multi-module reactor builds

**Input**: Design documents from `/specs/070-maven-main-module/`
**Prerequisites**: spec.md ✅, plan.md ✅, data-model.md ✅, contracts/maven-main-module-component.md ✅, quickstart.md ✅

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [X] T001 Confirm working tree clean and on branch `070-maven-main-module`.

## Phase 2: Foundational

- [X] T002 [P] Update `docs/reference/sbom-format-mapping.md` C40 row to extend ecosystem-coverage matrix to ALL 6: Go ✅, cargo ✅, npm ✅, pip ✅, gem ✅, maven ✅. Note that #104 is fully closed by this milestone.
- [X] T003 Extend `PomXmlDocument` struct in `mikebom-cli/src/scan_fs/package_db/maven.rs:530` with `pub modules: Vec<String>` field. Update `parse_pom_xml` event-loop (around `maven.rs:570`) to capture `<modules>/<module>` element values into the new field. Test by adding a unit test that parses a multi-module reactor POM and asserts `doc.modules == ["module-a", "module-b"]`.

## Phase 3: User Story 1 — Maven project SBOMs identify the project itself (P1) 🎯 MVP

### Implementation

- [X] T004 [US1] Implement `find_top_level_poms(rootfs: &Path) -> Vec<PathBuf>` in `mikebom-cli/src/scan_fs/package_db/maven.rs`. Walks rootfs alphabetically (cross-host determinism); returns every `pom.xml` at a project root level (NOT inside `target/`, `.m2/`, `node_modules/`, plus standard skip set). Reuse existing `should_skip_descent`-style helper.
- [X] T005 [US1] Implement `MavenInheritanceContext` struct + `build_from_poms(&[PathBuf]) -> Self` in `mikebom-cli/src/scan_fs/package_db/maven.rs`. Pre-parses every discovered POM via `parse_pom_xml` and stores them in a `HashMap<(groupId, artifactId, version), PomXmlDocument>` keyed by `self_coord`. Used by `build_maven_main_module_entry` for parent-POM lookup during inheritance resolution. Add unit test: 3 POMs (parent + 2 children) → context map has 3 entries; child POM lookup by `parent_coord` returns the parent.
- [X] T006 [US1] Implement `resolve_pom_property_value(raw: &str, self_doc: &PomXmlDocument, parent_doc: Option<&PomXmlDocument>) -> ResolvedValue` in `mikebom-cli/src/scan_fs/package_db/maven.rs`. Returns `Literal` (no `${...}` markers), `Resolved` (substitution succeeded), or `Unresolved` (verbatim placeholder + caller logs warn). Resolution order per FR-012: `${project.groupId}` / `${project.artifactId}` / `${project.version}` → `self_doc.self_coord`; `${parent.groupId}` / `${parent.version}` → `self_doc.parent_coord`; `${revision}` and custom keys → `self_doc.properties` first, then `parent_doc.properties` if available. Add unit tests for all 6 patterns + custom key + unresolved case.
- [X] T007 [US1] Implement `build_maven_main_module_entry(pom_path: &Path, doc: &PomXmlDocument, ctx: &MavenInheritanceContext) -> Option<PackageDbEntry>` in `mikebom-cli/src/scan_fs/package_db/maven.rs`. GAV resolution: prefer `doc.self_coord`; fall back to (`doc.parent_coord.0`, `doc.self_artifact_id`, `doc.parent_coord.2`) when groupId/version missing; resolve `${...}` properties via T006's helper. Returns `None` when GAV is fully unresolvable (FR-001 step 5). Constructs the entry per data-model.md: PURL via existing `build_maven_purl`; `parent_purl: None`; `sbom_tier: Some("source")`; C40 annotation; depends from `doc.dependencies` resolved-version GAVs.
- [X] T008 [US1] Implement `dedup_maven_main_modules_by_purl(&mut Vec<PackageDbEntry>) -> Vec<MavenDroppedDuplicate>` in `mikebom-cli/src/scan_fs/package_db/maven.rs`. Mirrors cargo (064 T010) / npm (066) / pip (068) / gem (069) C40-tag-driven dedup.
- [X] T009 [US1] Wire main-module emission into `maven::read()` (or `read_with_claims()` if appropriate) in `mikebom-cli/src/scan_fs/package_db/maven.rs`. Phase A (after the existing dep-emission loops): walk `find_top_level_poms(rootfs)`; build `MavenInheritanceContext` from the results; for each POM, parse → `build_maven_main_module_entry`. Augment-existing-or-emit-new pattern. Call `dedup_maven_main_modules_by_purl` + emit consolidated `tracing::warn!` for collisions and unresolved-property warns. Add `tracing::info!` reporting `main_modules_emitted` count.

### Tests for US1

- [X] T010 [P] [US1] Create `mikebom-cli/tests/fixtures/maven-multi-module-reactor/`: parent `pom.xml` (groupId=com.example, artifactId=parent, version=1.0.0, packaging=pom, modules=[module-a, module-b]); `module-a/pom.xml` (parent block + own artifactId=module-a); `module-b/pom.xml` (parent block + own artifactId=module-b + `<version>${project.version}</version>` for property substitution coverage). Plus README.md.
- [X] T011 [US1] Add integration tests in `mikebom-cli/tests/scan_maven.rs` (new file or extension): `scan_maven_single_module_emits_main_module` (US1 AS#1 / SC-001); `scan_maven_multi_module_reactor_emits_per_submodule_main_modules` (US1 AS#2 / SC-002 — assert exactly 3 APPLICATION-purpose packages: parent + module-a + module-b, all with correct PURL); `scan_maven_parent_inheritance_resolves_groupid_and_version` (US1 AS#3 — submodule with no own groupId/version inherits from `<parent>`); `scan_maven_property_substitution_resolves_revision` (US1 AS#4 / FR-012 — `${revision}` from `<properties>`); `scan_maven_install_state_paths_skipped` (FR-003 — `target/` excluded).

## Phase 4: Polish

- [X] T012 Regenerate `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/maven.{cdx,spdx,spdx3}.json` if the existing maven golden fixture changes shape. The existing maven fixture probably tests dep-emission via JAR walks or installed-artifact paths, not project-self emission, so likely no regen needed. Run tests pre-regen to confirm.
- [X] T013 CHANGELOG.md `[Unreleased]` entry for milestone 070 — same structure as 064/066/068/069. Reference #104 (now FULLY CLOSED with all 6 ecosystems shipped), #103 (license follow-up), #125 (divergent-PURL follow-up). Note this is the 5th per-ecosystem main-module milestone post-alpha.12 and updates the per-ecosystem coverage matrix to all 6 ✅.
- [X] T014 Update `docs/design-notes.md` per-ecosystem coverage matrix to "all done" + remove the "tracked in #104" line.
- [ ] T015 Run `./scripts/pre-pr.sh`; fix any issues. Update any pre-existing tests that assumed maven-pyject-style projects emit zero main-modules if they now hit the milestone-070 emission path (parallel to milestone-068's `pip::dist_info::tests` updates).
- [ ] T016 Open PR via `gh pr create` with title `feat(070): maven source-tree main-module component (closes maven slice + #104 in full)`.

## Dependencies

```text
T001 → [T002, T003] (parallel; T003 is the parser extension)
       → [T004 → T005 → T006 → T007 → T008 → T009] (US1 implementation chain)
              → T010 (fixture, parallel with helpers)
              → T011 (integration tests, depends on T009 wire-up)
                    → [T012, T013, T014] → T015 → T016
```

## Format validation

All 16 tasks follow the required format. Setup (T001), Foundational (T002-T003), US1 (T004-T011), Polish (T012-T016).

## MVP scope

US1 alone covers SC-001 + SC-002 + FR-002 (multi-module reactor) + FR-012 (property substitution). US2 (consumer signal) and US3 (doc root) inherit from milestones 053+064+066+068+069+#127 with zero implementation work.

## Closing #104

Post-merge of this milestone, **issue #104 is fully closed** — all 6 per-ecosystem main-modules shipped (Go ✅, cargo ✅, npm ✅, pip ✅, gem ✅, maven ✅). T013's CHANGELOG entry should explicitly note this and reference #104 closure.
