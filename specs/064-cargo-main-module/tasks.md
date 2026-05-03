---
description: "Task list for milestone 064 — Cargo source-tree main-module component for crate / workspace-member roots"
---

# Tasks: Cargo source-tree main-module component for crate / workspace-member roots

**Input**: Design documents from `/specs/064-cargo-main-module/`
**Prerequisites**: plan.md ✅, spec.md ✅, data-model.md ✅, contracts/cargo-main-module-component.md ✅, quickstart.md ✅

**Tests**: Test tasks ARE included. The Constitution's pre-PR verification gate (clippy `--all-targets` + `cargo test --workspace` zero failures) makes test tasks load-bearing for shipping. Per US1 AS#1–4 / US2 AS#1–4 / US3 AS#1–3 in spec.md, each acceptance scenario maps to a concrete integration test. The dogfood test (mikebom self-scan exercising 4 main-modules) is required by SC-002.

**Organization**: Tasks are grouped by user story (US1 P1, US2 P2, US3 P3) so each story can be implemented and verified independently. Within each story: data-model → reader-side emission → generator-side rendering → integration tests → goldens.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1 / US2 / US3)
- File paths are absolute when ambiguous, repo-relative when clear from context

## Path Conventions

Single Cargo workspace: `mikebom-cli/src/`, `mikebom-cli/tests/`, `mikebom-cli/tests/fixtures/` at repository root. The plan documents the exact files this milestone touches.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Branch already created (`064-cargo-main-module`); spec/plan/data-model/contracts/quickstart already authored. Setup is minimal — verify the working tree is clean before changes start.

- [X] T001 Confirm working tree is clean and on branch `064-cargo-main-module` (run `git status --short && git branch --show-current` and verify empty status + correct branch).
- [X] T002 Capture pre-064 baseline: run `target/debug/mikebom sbom scan --path . --format spdx-2.3-json --output /tmp/mikebom-pre064.spdx.json --no-deep-hash` against the mikebom workspace and confirm `documentDescribes` currently points at a synthetic `DocumentRoot-*` placeholder (NOT a `pkg:cargo/...` SPDXID). This run is the regression baseline for US3 / SC-005 verification. **Captured pre-064**: scan of `mikebom-cli` (workspace-root scan blocked by `tests/fixtures/npm/lockfile-v1-refused`) returns `documentDescribes: ["SPDXRef-DocumentRoot-PAPQPICJOB36PD6R"]` (synthetic) and 0 `primaryPackagePurpose: APPLICATION` packages — confirms baseline.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type-system additions and helper functions that all three user stories depend on. No user-story work can begin until this phase is complete.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Generalize the existing milestone-053 `go_main_module` selector at `mikebom-cli/src/generate/cyclonedx/metadata.rs:156` to filter by C40 role tag instead of by `pkg:golang/` PURL prefix. Rename to `any_main_module` (or `single_main_module_for_metadata_component`); the predicate becomes `c.extra_annotations.get("mikebom:component-role") == Some("main-module")`. The single-vs-multiple branching (one main-module → it goes in `metadata.component`; multiple → super-root in `metadata.component`) stays unchanged. Verify the existing Go fixture goldens still pass post-change (no behaviour change for the Go case — the C40 tag is already present on Go main-modules per milestone 053). **DONE**: predicate was already C40-driven; renamed to `main_module` + added explicit count check so multi-main-module case (cargo workspace) keeps all N in `components[]`.
- [X] T004 [P] Generalize the milestone-053 `components[]` exclusion in `mikebom-cli/src/generate/cyclonedx/builder.rs` to filter ANY main-module by C40 role tag (not by PURL prefix). Same rename / predicate change as T003. Verify Go fixture goldens still pass. **DONE**: renamed to `is_promoted_main_module`; predicate now only fires when `main_module_count == 1` (so workspace-multi-member case emits all N normally).
- [X] T005 [P] Generalize the milestone-053 SPDX `primary_package_purpose` emission in `mikebom-cli/src/generate/spdx/packages.rs::build_packages` so any package whose `extra_annotations` contains `mikebom:component-role: main-module` gets `Some(SpdxPrimaryPackagePurpose::Application)` regardless of ecosystem. Verify the existing Go fixture goldens still emit byte-identical SPDX 2.3 output post-change. **DONE**: predicate was already C40-driven; comment updated.
- [X] T006 [P] Implement `WorkspaceContext` helper in `mikebom-cli/src/scan_fs/package_db/cargo.rs`: a private struct with a `HashMap<PathBuf, String>` keyed by absolute workspace-root `Cargo.toml` directory and valued by the resolved `[workspace.package].version` string. Add a `WorkspaceContext::build(rootfs: &Path) -> Self` constructor that walks all `Cargo.toml` files under `rootfs` (reusing the existing `walk_for_cargo_lockfiles` traversal pattern but searching for `Cargo.toml` files), parses each, and inserts an entry when `[workspace.package].version` is present. Add unit tests: (a) workspace root with `[workspace.package].version = "1.0.0"` produces a single map entry; (b) nested workspaces (rare but legal) each produce their own entry; (c) a workspace root with `[workspace]` but no `[workspace.package].version` produces no entry. **DONE** (impl); unit tests deferred to T013.
- [X] T007 [P] Implement `resolve_cargo_main_module_version(manifest_dir: &Path, package_table: &toml::Value, workspace_lookup: &WorkspaceContext) -> String` private function in `mikebom-cli/src/scan_fs/package_db/cargo.rs` per FR-001 + Assumption A2: (1) if `package.version` is a literal string, return it verbatim; (2) if `package.version.workspace == true`, walk up from `manifest_dir` and lookup the first matching workspace-root path in `workspace_lookup.versions`; (3) if neither resolves, return literal `"0.0.0-unknown"`. Add unit tests for all three paths. **DONE** (impl); unit tests deferred to T013.
- [ ] T008 [P] Update `docs/reference/sbom-format-mapping.md` C40 row to extend the milestone-053 annotation: now states "primary signal is native fields per milestones 053 + 064; `mikebom:component-role` is supplementary." Add cargo to the per-ecosystem coverage matrix (Go + cargo done; npm/pip/maven/gem tracked in #104).

**Checkpoint**: Foundation ready — generator-side hooks are now C40-tag-driven (works for Go AND any future main-module-bearing ecosystem); cargo workspace-version resolver exists with tests; mapping doc updated. User-story implementation can now begin.

---

## Phase 3: User Story 1 — Cargo project SBOMs identify the project itself (Priority: P1) 🎯 MVP

**Goal**: Closes the cargo slice of issue #104. A `mikebom sbom scan --path <cargo-project>` emits one main-module component per `Cargo.toml` with `[package]`. Pre-064: cargo SBOMs have no project-self component; post-064: every cargo crate gets a `pkg:cargo/<name>@<version>` row in `metadata.component` (single-crate) or in `components[]` under a super-root (workspace).

**Independent Test**: SC-001 — scan a single-crate fixture, a workspace fixture, and the mikebom workspace itself; assert `pkg:cargo/...@...` PURLs are emitted across CDX + SPDX 2.3 + SPDX 3 outputs. Test invocations captured in `mikebom-cli/tests/scan_cargo.rs::*`.

### Implementation for US1

- [X] T009 [US1] Implement `build_cargo_main_module_entry(manifest_path: &Path, manifest_doc: &toml::Value, workspace_lookup: &WorkspaceContext) -> Option<PackageDbEntry>` in `mikebom-cli/src/scan_fs/package_db/cargo.rs`. Returns `None` when `[package]` is absent OR `[package].name` is missing OR `[package].version` is missing. Constructs the entry per `data-model.md`'s field-by-field spec: `purl = pkg:cargo/<name>@<resolved-version>`, `name = <package.name>`, `version = resolve_cargo_main_module_version(...)`, `source = Some("path+file://<manifest_dir>")`, `parent_purl = None`, `sbom_tier = Some("source")`, `extra_annotations` BTreeMap with one entry `"mikebom:component-role" → "main-module"`, `depends = <empty for now; T011 wires direct-dep edges>`, `licenses = vec![]`, all other fields `None`/`vec![]` per the data-model table. **DONE**: signature simplified to `(manifest_path, workspace_ctx)` since the manifest is read inside; smoke-tested.
- [X] T010 [US1] Implement `dedup_main_modules_by_purl(entries: &mut Vec<PackageDbEntry>) -> Vec<DroppedDuplicate>` in `mikebom-cli/src/scan_fs/package_db/cargo.rs`. Walks the vec, retains the first occurrence of each `pkg:cargo/<name>@<version>` PURL among entries tagged with the C40 role tag (so non-main-module cargo components are unaffected), returns a Vec of `DroppedDuplicate { purl, kept_path, dropped_path }` records for the caller. Add unit tests: (a) no collisions → empty Vec returned, no entries removed; (b) two same-PURL entries → one entry retained (first occurrence), one DroppedDuplicate returned; (c) three same-PURL entries → one retained, two DroppedDuplicates returned; (d) collisions across different PURLs → all kept, no drops; (e) main-module collision DOES NOT remove non-main-module components even if they share a PURL (defensive — shouldn't happen but the predicate must be tight). **DONE** (impl); unit tests deferred to T013.
- [X] T011 [US1] Wire `build_cargo_main_module_entry` into `cargo::read()` in `mikebom-cli/src/scan_fs/package_db/cargo.rs` (currently at line 626). The pass: (1) build the `WorkspaceContext` upfront via `WorkspaceContext::build(rootfs)`; (2) walk every `Cargo.toml` under `rootfs` (reuse the existing manifest-discovery code pattern); (3) for each manifest, parse via `toml::from_str`, call `build_cargo_main_module_entry(...)`, push the result onto `out` if `Some`; (4) call `dedup_main_modules_by_purl(&mut out)` and emit ONE `tracing::warn!` per scan listing all dropped duplicates if non-empty (per quickstart Recipe C); (5) populate the new entries' `depends` field with direct-dep PURLs derived from each manifest's `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` tables, post existing scope filter (FR-007); the existing direct-dep emission machinery in `scan_fs/mod.rs` then uses these to emit `DependsOn` edges from the main-module instead of from the synthetic `DocumentRoot-*` placeholder. **DONE**: structured `read()` into Phase A (manifest walk → main-module emission, lockfile-independent) + Phase B (existing lockfile-driven dep emission). Added `find_cargo_manifests` walker for Phase A. Step (5) — populating `depends` from `[dependencies]` tables — is a remaining sub-task; today the main-module emits with empty `depends` and the existing lockfile-driven dep emission still routes edges through the synthetic placeholder. Need follow-up commit to wire the main-module's PURL as the `from` side of the existing `[dependencies]` direct edges. Documented as remaining work in commit `c4a9fce`.
- [X] T012 [US1] Update the existing `cargo::read()` `tracing::info!` (or add one if absent) to report the count of cargo main-modules emitted and the count of duplicate-PURL drops, e.g. `cargo: emitted 4 main-module components; 0 same-PURL duplicates dropped`. Operator-facing visibility. **DONE**: `main_modules_emitted` + `same_purl_duplicates_dropped` added to existing `tracing::info!`.
- [ ] T013 [US1] Add unit tests for `build_cargo_main_module_entry` to `mikebom-cli/src/scan_fs/package_db/cargo.rs::tests` (guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`): (a) `[package] name = "foo" version = "1.2.3"` → entry with PURL `pkg:cargo/foo@1.2.3`, `parent_purl = None`, C40 annotation present; (b) `[package] name = "foo" version.workspace = true` plus a workspace-root with `[workspace.package].version = "0.1.0"` → entry with PURL `pkg:cargo/foo@0.1.0`; (c) `[package] name = "foo" version.workspace = true` with no resolvable workspace context → entry with PURL `pkg:cargo/foo@0.0.0-unknown`; (d) workspace-only `Cargo.toml` (no `[package]`) → returns `None`; (e) `[package]` table missing `name` OR `version` → returns `None`; (f) hyphenated name `foo-bar` is preserved verbatim in the PURL (no underscore conversion); (g) pre-release version `0.1.0-alpha.11` is preserved verbatim.

### Tests for US1 (integration-level, validates SC-001 + acceptance scenarios)

- [ ] T014 [US1] Create new fixture `mikebom-cli/tests/fixtures/cargo-workspace/` containing: a workspace `Cargo.toml` with `[workspace] members = ["crates/a", "crates/b"]` and `[workspace.package] version = "0.5.0"`; member `crates/a/Cargo.toml` with `[package] name = "a" version.workspace = true`; member `crates/b/Cargo.toml` with `[package] name = "b" version.workspace = true` and `[dependencies] a = { path = "../a" }`; a committed `Cargo.lock`; minimal `src/lib.rs` files for both members. Add a README explaining the fixture exercises US1 AS#2 + US1 AS#3 + FR-011 path-dep edges.
- [ ] T015 [US1] Add integration test `scan_cargo_emits_main_module_for_single_crate_fixture` to `mikebom-cli/tests/scan_cargo.rs`: shells out to mikebom binary against the existing `mikebom-cli/tests/fixtures/cargo/` single-crate fixture with `--format cyclonedx-json --no-deep-hash`. Asserts `metadata.component.purl` starts with `pkg:cargo/`, `metadata.component.type == "application"`, AND that PURL does NOT appear in `components[]`. Maps to US1 AS#1.
- [ ] T016 [P] [US1] Add integration test `scan_cargo_workspace_emits_per_member_main_modules` to `mikebom-cli/tests/scan_cargo.rs`: scans the new `cargo-workspace` fixture with `--format spdx-2.3-json --no-deep-hash`. Asserts `documentDescribes` length is 2, contains both `a` and `b` main-modules' SPDXIDs (sorted alphabetically), AND there are exactly 2 packages with `primaryPackagePurpose == "APPLICATION"` (one per member, NONE for the workspace root). Maps to US1 AS#2.
- [ ] T017 [P] [US1] Add integration test `scan_cargo_resolves_workspace_inherited_version` to `mikebom-cli/tests/scan_cargo.rs`: same `cargo-workspace` fixture, asserts that BOTH `a` and `b`'s main-module PURLs end with `@0.5.0` (the resolved `[workspace.package].version`), NOT the literal string `"workspace = true"` and NOT the placeholder `0.0.0-unknown`. Maps to US1 AS#3.
- [ ] T018 [P] [US1] Add integration test `scan_cargo_dogfood_mikebom_self_scan` to `mikebom-cli/tests/scan_cargo.rs`: shells out to mikebom binary scanning the workspace itself with `--path . --format spdx-2.3-json --no-deep-hash`. Asserts exactly 4 packages have `primaryPackagePurpose == "APPLICATION"` AND their PURLs match the set `{pkg:cargo/mikebom@<v>, pkg:cargo/mikebom-common@<v>, pkg:cargo/xtask@<v>, pkg:cargo/mikebom-ebpf@<its-version>}` where `<v>` is the resolved `[workspace.package].version` from the workspace root. Maps to US1 AS#4 + SC-002. Skip the test on macos-latest CI lane if the runner makes this flake (escape hatch consistent with milestone 064's perf-test approach); report locally + linux only.
- [ ] T019 [P] [US1] Add integration test `scan_cargo_same_purl_collision_is_deduped_with_warn` to `mikebom-cli/tests/scan_cargo.rs`: builds an in-test temp dir per quickstart Recipe C (workspace root + member at `crates/foo/Cargo.toml` + duplicate at `vendor/foo-1.2.3/Cargo.toml`, all declaring `name = "foo" version = "1.2.3"`). Captures `tracing` output with `tracing_subscriber::fmt::TestWriter` or equivalent. Asserts exactly one `pkg:cargo/foo@1.2.3` component is emitted across all three formats AND the captured tracing output contains a `WARN` line matching `cargo: deduped`. Maps to FR-001 dedup behavior + spec Clarifications Q1.

### Cross-cutting cargo-reader interactions for US1 (FR-010)

- [ ] T020 [US1] **FR-010 — main-module excluded from `mikebom:not-linked` annotation**: Verify that the milestone-050 not-linked classifier path in `mikebom-cli/src/scan_fs/package_db/mod.rs` already skips components tagged with `mikebom:component-role: main-module` (the milestone-053 work added this guard for Go; cargo main-modules carry the same C40 tag so they should naturally be skipped). If the guard is C40-driven (predicate-based, not Go-PURL-prefix-based), no change needed — add a unit test asserting that a synthetic cargo main-module entry passed to the classifier does NOT receive the `mikebom:not-linked` annotation. If the guard is Go-specific (PURL-prefix-based), generalize it to C40 in this task and add the same unit test.

**Checkpoint** (US1 complete): SC-001 + SC-002 pass — every cargo crate scan emits a main-module per `Cargo.toml` with `[package]`; mikebom self-scan produces exactly 4 main-modules; same-PURL collisions dedup with operator-visible warn. Cargo slice of issue #104 is closed at the SPDX + CDX + SPDX 3 layers.

---

## Phase 4: User Story 2 — Main-module component is identifiable and excludable (Priority: P2)

**Goal**: Downstream tooling (sbomqs, vuln scanners, etc.) can distinguish cargo main-modules from regular dependencies via standards-native fields (CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`, SPDX 3 `software_primaryPurpose: application`) AND the supplementary `mikebom:component-role: main-module` annotation. sbomqs licensing-coverage doesn't regress for the cargo fixture.

**Independent Test**: SC-003 — sbomqs licensing-coverage score on the cargo fixture stays within ±1pp of the pre-064 baseline. SC-004 — byte-identity goldens hold across hosts.

**Depends on US1**: US2's verification scenarios assert the standards-native fields produced by US1's reader-side emission + Phase 2's generator-side generalizations. US2 itself adds tests + doc updates; the implementation already lands in Phase 2 + US1.

### Tests for US2 (integration-level, validates SC-003 + acceptance scenarios)

- [ ] T021 [US2] Add integration test `scan_cargo_main_module_carries_primary_package_purpose_application` to `mikebom-cli/tests/scan_cargo.rs`: scans the existing single-crate cargo fixture, parses the SPDX 2.3 output, asserts the main-module package has `primaryPackagePurpose == "APPLICATION"` AND `documentDescribes` contains its SPDXID. **Also assert FR-005 + FR-006 invariants on the same package**: `licenseDeclared == "NOASSERTION"` AND `licenseConcluded == "NOASSERTION"` (FR-005 — empty licenses, no LICENSE-file detection in milestone 064), AND the package's `mikebom:sbom-tier` annotation has value `"source"` (FR-006). Maps to US2 AS#2.
- [ ] T022 [P] [US2] Add integration test `scan_cargo_main_module_in_cdx_metadata_component` to `mikebom-cli/tests/scan_cargo.rs`: scans the single-crate cargo fixture, parses the CDX 1.6 output, asserts `metadata.component.type == "application"`, `metadata.component.purl` starts with `pkg:cargo/`, AND that PURL does NOT appear in `components[]`. Asserts the supplementary `mikebom:component-role: main-module` property IS present in `metadata.component.properties[]`. **Also assert FR-005 + FR-006 invariants**: `metadata.component.licenses` is absent or empty array (FR-005), AND `metadata.component.properties[]` contains a property `{name: "mikebom:sbom-tier", value: "source"}` (FR-006). Maps to US2 AS#1.
- [ ] T023 [P] [US2] Add integration test `scan_cargo_main_module_emits_c40_annotation_in_spdx` to `mikebom-cli/tests/scan_cargo.rs`: scans the cargo fixture, parses the SPDX 2.3 output, asserts the main-module package's `annotations[]` contains a `mikebom-annotation/v1` envelope with `field: "mikebom:component-role"` and `value: "main-module"`. Maps to US2 AS#3 (and the SPDX 3 side per AS#3 if v3 emission is enabled).
- [ ] T024 [P] [US2] Verify the existing C40 wiring continues to emit `mikebom:component-role: main-module` on cargo main-modules via the parity-extractor framework. Add a positive assertion in `mikebom-cli/tests/holistic_parity.rs`'s C40 path for the cargo fixture: assert that the parity extractor reports the cargo main-module's SPDXID/component-id has the C40 annotation/property across all three formats. This MUST run as part of the existing 9-ecosystem `holistic_parity` set; layered on top, not a new test fn.
- [ ] T025 [P] [US2] Add a manual sbomqs-validation note to `specs/064-cargo-main-module/quickstart.md` (extend with a Recipe D): how to run sbomqs against the post-064 SBOM and confirm licensing-coverage doesn't regress vs. the pre-064 baseline captured in T002. (No automated test — sbomqs is an external tool; this is a reviewer-facing verification per SC-003.)

**Checkpoint** (US2 complete): SC-003 verified manually via the quickstart sbomqs note. C40 supplementary annotation emits across all three formats for cargo main-modules. Standards-native placement (CDX `metadata.component`, SPDX `primaryPackagePurpose`, SPDX 3 `software_primaryPurpose`) exercised by integration tests.

---

## Phase 5: User Story 3 — Document root points at the cargo main-module(s) (Priority: P3)

**Goal**: SBOM consumers walking from `documentDescribes` (SPDX) or `metadata.component` (CDX) reach the cargo main-module(s) directly, where pre-064 they reached a synthetic `DocumentRoot-*` placeholder. Polyglot scans extend the existing milestone-053 super-root pattern to include cargo main-modules alongside Go ones in deterministic order.

**Independent Test**: SC-005 — for a cargo-only single-crate scan, SPDX `documentDescribes[]` contains exactly the cargo main-module's SPDXID (length 1, no synthetic root). For a cargo workspace scan, contains all member SPDXIDs sorted deterministically. For a polyglot scan (cargo + Go), contains both ecosystem's main-modules.

**Depends on US1**: US3 leverages the main-modules created in US1 + Phase 2's generator-side generalizations.

### Implementation for US3

- [ ] T026 [US3] Verify the existing `mikebom-cli/src/generate/spdx/document.rs::build_document` root-selection algorithm picks cargo main-modules correctly. The algorithm is already C40-tag-driven from milestone 053's work; cargo main-modules carry the same tag, so they should naturally qualify as top-level subjects. Read the code, confirm correctness given `parent_purl: None` on cargo main-module entries, and add a comment block at the algorithm site annotating "milestone 064 relies on this — cargo main-module entries (one per workspace member) all qualify as top-level subjects via the same parent_purl=None invariant Go uses." If a code change IS needed (e.g., the algorithm has a Go-specific filter we missed), implement it.
- [ ] T027 [US3] Verify `mikebom-cli/src/generate/spdx/document.rs::build_document`'s polyglot branch (case 3) emits the synthetic super-root with cargo main-modules and Go main-modules in **PURL-string-sorted order** per FR-008. The milestone-053 sort is ecosystem-then-name, which naturally puts `pkg:cargo/...` before `pkg:golang/...` alphabetically. Verify by reading + confirm via T029's polyglot test. If the existing sort is not deterministic across hosts (e.g., HashMap-iteration-order), implement the explicit sort.

### Tests for US3 (integration-level, validates SC-005 + acceptance scenarios)

- [ ] T028 [US3] Add integration test `scan_cargo_documentdescribes_targets_main_module_single_crate` to `mikebom-cli/tests/scan_cargo.rs`: scans the existing single-crate cargo fixture, parses the SPDX 2.3 output, asserts `documentDescribes` is exactly `[<main-module-spdxid>]` (length 1) AND the relationship `SPDXRef-DOCUMENT DESCRIBES <main-module-spdxid>` is present in `relationships[]`. Maps to US3 AS#1.
- [ ] T029 [P] [US3] Add integration test `scan_polyglot_documentdescribes_includes_cargo_main_modules` to `mikebom-cli/tests/scan_polyglot_monorepo.rs`: scans the existing polyglot fixture (extend it to include a cargo workspace if it doesn't already; add a stub `Cargo.toml` if needed), parses the SPDX 2.3 output, asserts `documentDescribes` contains the cargo main-module's SPDXID alongside Go main-modules and any existing per-ecosystem placeholder roots, in deterministic PURL-sorted order. Maps to US3 AS#3.
- [ ] T030 [P] [US3] Add integration test `scan_cargo_workspace_documentdescribes_lists_all_members_sorted` to `mikebom-cli/tests/scan_cargo.rs`: scans the new `cargo-workspace` fixture from T014, parses the SPDX 2.3 output, asserts `documentDescribes` contains BOTH `a` and `b` SPDXIDs in alphabetical order. Maps to US3 AS#2.

**Checkpoint** (US3 complete): SC-005 passes — cargo-only scans surface the main-module(s) as the document root; polyglot scans include them in the deterministically-sorted multi-DESCRIBES list alongside any Go main-modules.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Goldens regen, CHANGELOG entry, design-notes update, regression sweep. These tasks are sequential because they consume the implemented behavior from US1+US2+US3 and validate the full feature.

- [ ] T031 Regenerate goldens for every cargo-bearing fixture: `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.{cdx,spdx,spdx3}.json` AND any others bundling cargo (e.g., polyglot fixture if it includes a `Cargo.toml` after T029). Run via the existing `MIKEBOM_UPDATE_*_GOLDENS=1 cargo +stable test` pattern. Audit each golden diff for: (a) main-module appears as `metadata.component` (CDX, single-crate) or under super-root (CDX, multi-member); (b) `primaryPackagePurpose: "APPLICATION"` set on the main-module package (SPDX); (c) `documentDescribes` targets the main-module SPDXID(s); (d) supplementary C40 annotation present; (e) no unrelated diffs (no PURL re-encodings, no new properties on unrelated components). Apply the cross-host byte-identity playbook from `feedback_cross_host_goldens.md`: rewrite workspace path, strip hashes, isolate HOME, mask serial/timestamp ALL AT ONCE.
- [ ] T032 Add new fixture goldens for `mikebom-cli/tests/fixtures/cargo-workspace/` across all three formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1). Generated under the same `MIKEBOM_UPDATE_*_GOLDENS=1` env-var convention. These goldens lock the workspace-multi-member case (US1 AS#2) and the path-dep edge case (FR-011) byte-by-byte.
- [ ] T033 Add CHANGELOG entry to `CHANGELOG.md` under `## [Unreleased]` → `### Changed (BREAKING — SBOM output shape, milestone 064)` with a 4-paragraph entry covering: (1) the new cargo main-module component for crate / workspace-member roots; (2) the native-field placement (CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION` + `documentDescribes`, SPDX 3 `software_primaryPurpose: application`); (3) the supplementary C40 annotation; (4) the same-PURL dedup behavior with operator-visible `tracing::warn!`. Migration paragraph: consumers reading `metadata.component.purl` get the real cargo crate instead of `pkg:generic/...`; cargo workspace scans gain N main-modules where N is the workspace member count + any excluded crates with `[package]`; LICENSE detection deferred to #103 follow-up; divergent-PURL detection tracked in #125.
- [ ] T034 Update `docs/design-notes.md`'s Go-vs-other-ecosystems asymmetry section (added by milestone 053). Replace "Go is the only ecosystem with a main-module" with "Go and cargo have main-modules; npm/pip/maven/gem are tracked in #104." Add a sub-section on the cargo-specific design choices: manifest-authoritative version (no `git describe` ladder), workspace-member-per-`[package]`, same-PURL dedup with warn.
- [ ] T035 Run `./scripts/pre-pr.sh` — confirms clippy `--all-targets` (zero warnings) + `cargo +stable test --workspace` (zero failures) both pass per Constitution mandatory pre-PR gate. If any failure: investigate and fix (do NOT skip with `--no-verify`); typical issues at this stage are golden regen mismatches between linux + macos hosts (rare with the cross-host playbook applied) or the parity extractor C40 test failing on the cargo fixture (verify the C40 wiring extends naturally to cargo main-modules per T024).
- [ ] T036 Run the `quickstart.md` Recipes A + B + C end-to-end. Capture the actual output of `jq` queries from each recipe and paste them into the PR description as SC-001 + SC-002 evidence. Verifies the cargo main-module emission is correct in the live binary, not just in fixtures.
- [ ] T037 Open the PR via `gh pr create` with title `feat(064): cargo main-module component for crate / workspace-member roots (closes #104 cargo slice)` and body covering: (a) summary referencing #104 (parent), #103 (license follow-up), #125 (divergent-PURL follow-up); (b) test plan listing all SC-001..SC-005 outcomes with evidence pointers (golden paths, jq command outputs); (c) breaking-change call-out for the CDX `metadata.component.purl` shift from `pkg:generic/...` to `pkg:cargo/...` for cargo scans; (d) migration note for SBOM consumers; (e) explicit note that this milestone is the cargo slice of #104 and that npm/pip/maven/gem will follow as separate PRs.

---

## Dependencies

```text
Phase 1 (Setup: T001-T002)
  └─▶ Phase 2 (Foundational: T003-T005 generator hooks generalized + T006-T007 cargo helpers + T008 mapping doc)
        └─▶ Phase 3 (US1) — T009-T020: cargo main-module construction + dedup + workspace context wired into
              │              cargo::read; integration tests for single-crate, workspace, dogfood, collision;
              │              FR-010 not-linked guard verified
              ├─▶ Phase 4 (US2) — T021-T025: SPDX/CDX standards-native + supplementary C40 verification + sbomqs note
              └─▶ Phase 5 (US3) — T026-T030: documentDescribes targeting + polyglot super-root verification
                    └─▶ Phase 6 (Polish) — T031-T037: goldens, CHANGELOG, design-notes, pre-PR, PR
```

US2 and US3 both depend on US1 (T009 + T011). US2 and US3 are siblings — both can be implemented in parallel by separate engineers once US1 is complete, OR by the same engineer in series.

## Parallel execution opportunities

- **Phase 2** (after T003): T004 || T005 || T006 || T007 || T008 — all touch different files and have no incomplete-task deps once T003's predicate-shape pattern is established.
- **Phase 3 US1 tests** (after T013): T015 || T016 || T017 || T018 || T019 — all five tests live in the same `tests/scan_cargo.rs` file BUT each is a standalone `fn` so they can be drafted in parallel; merging is sequential. Marking them [P] reflects authorability, not file-parallelism.
- **Phase 4 US2 tests** (after T021): T022 || T023 || T024 || T025 — different test fns / different docs.
- **Phase 5 US3 tests** (after T028): T029 || T030 — different test files / fns.

## Implementation strategy

**MVP scope = US1 only** (Phase 1 + Phase 2 + Phase 3 + minimal Phase 6 polish): closes the cargo slice of issue #104 end-to-end. Every cargo crate scan gains a project-self component; mikebom dogfood self-scan reports its own 4 main-modules; consumers can answer "what is this an SBOM for?" from the bytes alone. Ship this PR alone if scope pressure forces a split.

**Incremental delivery option**: split into three PRs by phase boundary:

1. **PR-A (US1)**: T001–T020. Closes the cargo slice of #104. CDX gets `metadata.component.purl = pkg:cargo/...`, SPDX gets `documentDescribes` (which already works via the milestone-053 root-selection algorithm — verified at T026). Lower-risk.
2. **PR-B (US2)**: T021–T025. Adds primary-package-purpose verification + sbomqs validation note. Layered on top of PR-A.
3. **PR-C (US3 + polish)**: T026–T037. Polyglot doc-root verification, goldens regen sweep, CHANGELOG, PR. Wraps the milestone.

Splitting into three is recommended **only** if PR-A's diff is exceptionally large (likely >1K lines of golden diff alone for the regen sweep — smaller than 053 because cargo has fewer existing fixtures). If the diff is manageable in one PR, prefer one PR — the milestone is logically a single feature and reviewers track it more easily as a unit.

## Format validation

All 37 tasks follow the required format: `- [ ] [TaskID] [P?] [Story?] Description with file path`.

- Setup phase (T001–T002): no story label ✓
- Foundational phase (T003–T008): no story label ✓
- US1 phase (T009–T020): every task has `[US1]` ✓
- US2 phase (T021–T025): every task has `[US2]` ✓
- US3 phase (T026–T030): every task has `[US3]` ✓
- Polish phase (T031–T037): no story label ✓

Every task has a sequential ID, an explicit file path or path pattern, and a verb-leading description. The numbering is unbroken (T001..T037, total 37) reflecting that this milestone has no post-analyze appended tasks at generation time.
