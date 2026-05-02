---
description: "Task list for milestone 053 — Go source-tree direct dependency edges via synthetic main-module component"
---

# Tasks: Go source-tree direct dependency edges via synthetic main-module component

**Input**: Design documents from `/specs/053-go-main-module-edges/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/main-module-component.md ✅, quickstart.md ✅

**Tests**: Test tasks ARE included. The Constitution's pre-PR verification gate (clippy `--all-targets` + `cargo test --workspace` zero failures) makes test tasks load-bearing for shipping. Per US1 AS#1–4 / US2 AS#1–4 / US3 AS#1–2 in spec.md, each acceptance scenario maps to a concrete integration test.

**Organization**: Tasks are grouped by user story (US1 P1, US2 P2, US3 P3) so each story can be implemented and verified independently. Within each story, the order is: data-model → reader-side emission → generator-side rendering → integration tests → goldens.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1 / US2 / US3)
- File paths are absolute when ambiguous, repo-relative when clear from context

## Path Conventions

Single Cargo workspace: `mikebom-cli/src/`, `mikebom-cli/tests/`, `tests/fixtures/` at repository root. The plan documents the exact files this milestone touches.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Branch already created (`053-go-main-module-edges`); spec/plan/research/data-model/contracts/quickstart already authored. Setup is minimal — verify the working tree is clean before changes start.

- [X] T001 Confirm working tree is clean and on branch `053-go-main-module-edges` (run `git status --short && git branch --show-current` and verify empty status + correct branch)
- [X] T002 Verify pre-053 baseline reproduction at `/tmp/mikebom-053-verify/argo-workflows` per `specs/053-go-main-module-edges/quickstart.md` step 1–3 — should currently emit 1 relationship (this run is the regression baseline)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Type-system additions and helper functions that all three user stories depend on. No user-story work can begin until this phase is complete.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Add `SpdxPrimaryPackagePurpose` enum to `mikebom-cli/src/generate/spdx/packages.rs` with all 12 SPDX 2.3 §7.24 variants (`APPLICATION`, `FRAMEWORK`, `LIBRARY`, `CONTAINER`, `OPERATING-SYSTEM`, `DEVICE`, `FIRMWARE`, `SOURCE`, `ARCHIVE`, `FILE`, `INSTALL`, `OTHER`), per `data-model.md`. Add `Option<SpdxPrimaryPackagePurpose>` field to `SpdxPackage` with `#[serde(rename = "primaryPackagePurpose", skip_serializing_if = "Option::is_none")]` so existing packages stay byte-identical.
- [X] T004 [P] Implement `resolve_workspace_version(project_root: &Path) -> String` private function in `mikebom-cli/src/scan_fs/package_db/golang.rs` per FR-001's 3-step ladder: (1) `git describe --tags --exact-match HEAD`, (2) `git describe --tags --always`, (3) literal `v0.0.0-unknown`. Honor a 2-second timeout via `std::process::Command` + `wait_timeout` (or hand-rolled timeout-thread). Falls through to step 3 on: missing `git` binary, missing `.git` dir, non-zero exit, timeout.
- [X] T005 [P] Add unit tests for `resolve_workspace_version` to `mikebom-cli/src/scan_fs/package_db/golang.rs::tests`: (a) tarball-style dir (no `.git`) returns `v0.0.0-unknown`, (b) `git init` + commit + tag returns the exact tag, (c) `git init` + commit (no tag) returns the synthetic `0-g<sha>` form via `--always`, (d) timeout fallback (mock unreachable git via `PATH` manipulation or skip on CI). Guard test module with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per Constitution Principle IV.
- [X] T006 [P] Update `docs/reference/sbom-format-mapping.md` C40 row: annotate that "primary signal is native fields per milestone 053; `mikebom:component-role` is supplementary." Add a short paragraph in the row's Notes column explaining that for `main-module` specifically, CDX uses `metadata.component.type: "application"`, SPDX 2.3 uses `primaryPackagePurpose: APPLICATION` + `documentDescribes`, and SPDX 3 uses native `softwarePurpose` (when 3.0.1 schema permits) — the C40 annotation is supplementary signal layered on top.

**Checkpoint**: Foundation ready — `SpdxPackage.primary_package_purpose` is wired (default-None, byte-identical-to-pre-053 for non-main-module packages), version-resolution helper exists with tests, mapping doc updated. User story implementation can now begin.

---

## Phase 3: User Story 1 — Fresh-clone Go scan emits direct dependency edges (Priority: P1) 🎯 MVP

**Goal**: Closes issue #102. A `mikebom sbom scan --path <fresh-go-clone>` against a Go repo with empty GOMODCACHE emits `DEPENDS_ON` edges for every direct require in `go.mod`. Pre-053 emits 1 relationship; post-053 emits ≥ N where N is the require count.

**Independent Test**: SC-001 — clone argo-workflows v3.3.9, scan offline with empty `GOMODCACHE`, verify `relationships[]` contains ≥ 14 `DEPENDS_ON` edges (one per direct require). Test invocation captured in `tests/scan_go.rs::scan_go_emits_main_module_direct_edges_with_empty_cache`.

### Implementation for US1

- [X] T007 [US1] Implement `build_main_module_entry(doc: &GoModDocument, source_path: &str, version: &str) -> Option<PackageDbEntry>` in `mikebom-cli/src/scan_fs/package_db/golang.rs`. Returns `None` when `doc.module_path` is `None` (malformed `go.mod`). Constructs the entry per `data-model.md`'s field-by-field spec: `purl = pkg:golang/<module-path>@<version>`, `name = <module-path>`, `version = <version>`, `parent_purl = None`, `sbom_tier = Some("source")`, `extra_annotations = vec![mikebom:component-role: "main-module"]`, `depends = doc.requires.iter().filter_map(|r| apply_replace_and_exclude(&r.path, ..., &doc.replaces, &doc.excludes).map(|(p,_)| p)).collect()`, `licenses = vec![]`, all other fields `None`/`vec![]` per the data-model table.
- [X] T008 [US1] Wire `build_main_module_entry` into the second pass of `golang::read()` (`mikebom-cli/src/scan_fs/package_db/golang.rs:675+` second-pass loop). After the `build_entries_from_go_module` call for each parsed root, call `resolve_workspace_version(project_root)` and `build_main_module_entry(doc, &source_path, &version)`, and push the result onto `out` (deduped by `seen_purls`). Order: AFTER the existing transitive-entries push so the main-module appears at a deterministic position; goldens will lock the exact ordering.
- [X] T009 [US1] Update the existing `golang::read()` `tracing::info!` "no Go binary found alongside go.mod" log message in the same file to also report the new `main_module_emitted: bool` field (so operators can see at a glance whether the new path fired).
- [X] T010 [US1] Add unit tests for `build_main_module_entry` to `mikebom-cli/src/scan_fs/package_db/golang.rs::tests`: (a) `module example.com/x` with three `require` lines produces an entry whose `depends` is exactly those three paths; (b) `module example.com/x` with one direct + one `// indirect` require produces an entry with both in `depends` (deliberate Trivy-divergence per spec Edge Cases); (c) `module example.com/x` + a `replace example.com/y => ./local` produces a `depends` entry with the replaced path; (d) `module example.com/x` + an `exclude example.com/z v1.0.0` produces `depends` without z; (e) `parent_purl` is `None`; (f) `extra_annotations` contains exactly the C40 entry; (g) zero-require go.mod produces an entry with empty `depends`.

### Tests for US1 (integration-level, validates SC-001 + acceptance scenarios)

- [X] T011 [US1] Create new fixture `tests/fixtures/go/argo-style-no-cache/` containing: a tarball-style trimmed go.mod (NO `.git` dir) with `module github.com/argoproj/argo-workflows`, `go 1.17`, and ~14 direct require lines mirroring argo's actual go.mod (a representative subset is fine — the count and names lock SC-001's "≥14 DEPENDS_ON" assertion). Also include a stub `LICENSE` file (Apache-2.0 text) so US2 can later assert no-license-detection-still-doesn't-error. Add a README explaining the fixture is for milestone 053 SC-001 verification.
- [X] T012 [US1] Add integration test `scan_go_emits_main_module_direct_edges_with_empty_cache` to `mikebom-cli/tests/scan_go.rs`: shells out to mikebom binary with `--path /tmp/argo-style-no-cache --offline --format spdx-2.3-json --output ... --no-deep-hash` (with isolated `HOME` + empty `GOMODCACHE` envs to mirror the issue-#102 reproduction). Asserts `relationships[]` contains ≥ 14 `DEPENDS_ON` entries whose `spdxElementId` resolves to the main-module's SPDXID. Maps directly to US1 AS#1 + AS#4.
- [ ] T013 [P] [US1] Add integration test `scan_go_emits_main_module_with_populated_cache_no_regression` to `mikebom-cli/tests/scan_go.rs`: scans the same fixture with a synthetic `GOMODCACHE` containing one of the direct requires' `.mod` file (mocked so the cache lookup hits for one require, misses for the rest). Asserts the direct edge to that require is present (cache hit doesn't suppress the direct edge per FR-007) AND the existing transitive edge from cache is also present. Maps to US1 AS#2.
- [X] T014 [P] [US1] Add integration test `scan_go_zero_requires_emits_main_module_no_edges` to `mikebom-cli/tests/scan_go.rs`: tiny fixture with `module example.com/empty\ngo 1.17\n` (no requires). Asserts the main-module component IS emitted but has zero outgoing `DEPENDS_ON` edges. Maps to US1 AS#3.

### Generator-side wiring for US1 (CDX edges)

- [X] T015 [US1] In `mikebom-cli/src/generate/cyclonedx/builder.rs`, modify the components-builder pass to **exclude** the main-module entry from the top-level `components[]` array. Detection: `extra_annotations` contains `mikebom:component-role: "main-module"`. The exclusion preserves the existing components-builder behavior for everything else; only the main-module entry is filtered out. Edges from the main-module continue to emit via `dependencies[]` because the existing edge-emission loop reads from the components+relationships pair as input.
- [X] T016 [US1] In `mikebom-cli/src/generate/cyclonedx/metadata.rs::build_metadata`, when the components vec contains exactly one entry tagged `mikebom:component-role: "main-module"`, replace the synthetic-`pkg:generic/...` `metadata.component` construction with one derived from that entry: `name = entry.name`, `version = entry.version`, `purl = entry.purl.as_str()`, `cpe = synthesize from name+version per existing `cpe_sanitize`, `properties = entry.extra_annotations + sbom-tier + any other component-level mikebom:* properties`. The `bom-ref` MUST be the entry's PURL string (so existing `dependencies[].ref` lookups still resolve).

### Cross-cutting Go-reader interactions for US1 (FR-009 + FR-010)

- [X] T035 [US1] **FR-009 — Go binary BuildInfo dedup**: Implement the source-tree-vs-binary main-module dedup in `mikebom-cli/src/scan_fs/package_db/go_binary.rs`. After `golang::read()` and `go_binary::read()` both run for the same scan, walk the components vec: when both a source-tree main-module entry (extra_annotations contains `mikebom:component-role: main-module` AND source_path ends with `go.mod`) AND a binary-derived main-module entry (same module path, source_path comes from the binary file) exist, suppress the binary-derived entry and merge its metadata onto the source-tree entry per the FR-009 precedence table: `version` overrides only when source-tree is the literal `v0.0.0-unknown` placeholder; `hashes`, `depends`: source-tree always wins; all other fields fill source-tree's None/empty slots. Add a unit test asserting the merge outcome. Execute after T008 (which establishes the source-tree main-module emission this dedup is gating on).
- [X] T036 [P] [US1] **FR-010 — main-module excluded from `mikebom:not-linked` annotation**: Audit the not-linked classifier path in `mikebom-cli/src/scan_fs/package_db/mod.rs` (the function that applies the `mikebom:not-linked` annotation per milestone 050). Add an explicit guard: components whose `extra_annotations` contains `mikebom:component-role: main-module` MUST be skipped — they are by definition the linker root, never a non-linked dep. Add a unit test asserting that a main-module entry passed to the classifier does NOT receive the `mikebom:not-linked` annotation, even when no Go binary is present. Execute after T007 (which establishes the main-module entry shape this guard reads).

**Checkpoint** (US1 complete): SC-001 passes — argo-style fixture scan produces ≥ 14 `DEPENDS_ON` edges for SPDX 2.3 output AND CDX `metadata.component` is the Go main-module (not the `pkg:generic/...` placeholder). Issue #102 closed at the SPDX + CDX layers. FR-009 + FR-010 wired (no double-emission, no spurious not-linked tag on the project itself). SPDX 3 work moves to US2 next.

---

## Phase 4: User Story 2 — Main-module component is identifiable and excludable (Priority: P2)

**Goal**: Downstream tooling (sbomqs, vuln scanners, etc.) can distinguish the main-module from regular dependencies via standards-native fields (CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`) AND the supplementary `mikebom:component-role: main-module` annotation. sbomqs licensing-coverage doesn't regress.

**Independent Test**: SC-003 — sbomqs licensing-coverage score on the new fixture stays within ±1pp of the pre-053 baseline. SC-008 — CDX has main-module in `metadata.component` (not in `components[]`); SPDX 2.3 has `primaryPackagePurpose: "APPLICATION"` on the package targeted by `documentDescribes`.

**Depends on US1**: US2's wiring for the SPDX `primaryPackagePurpose` field requires the main-module entry from US1's `build_main_module_entry`. The CDX `metadata.component` placement is also US1's deliverable; US2 layers the supplementary tag verification on top.

### Implementation for US2

- [X] T017 [US2] In `mikebom-cli/src/generate/spdx/packages.rs::build_packages`, detect the main-module entry (by `extra_annotations` containing `mikebom:component-role: "main-module"`) and set `primary_package_purpose: Some(SpdxPrimaryPackagePurpose::Application)` on the resulting `SpdxPackage`. Other packages stay `None` (byte-identical pre-053 for non-main-module entries — verified by goldens).
- [X] T018 [US2] In `mikebom-cli/src/generate/spdx/v3_relationships.rs`, add a follow-up implementation note (TODO comment with task reference + spec citation) that SPDX 3.0.1's native `softwarePurpose` field SHOULD be set on the main-module element when present in the targeted v3 schema. Cross-check the field's actual existence in 3.0.1 by reading `tests/spdx3_schema_validation.rs`'s loaded schema; if the field is in 3.0.1, wire it; if it's only in 3.x rc, leave the TODO and rely on the C40 annotation only. Do NOT block this task on a live cross-check — landing the comment is sufficient if the field is missing or ambiguous.
- [X] T019 [P] [US2] Verify the existing C40 wiring continues to emit `mikebom:component-role: main-module` on the main-module via the parity-extractor framework. Add a positive assertion in `mikebom-cli/tests/holistic_parity.rs`'s C40 path for the new Go fixture: assert that `parity_golang` reports the main-module's SPDXID/component-id has the C40 annotation/property. This test MUST run as part of the existing 9-ecosystem `holistic_parity` set; it is layered on top, not a new test fn.

### Tests for US2 (integration-level, validates SC-003 + SC-008 + acceptance scenarios)

- [X] T020 [US2] Add integration test `scan_go_main_module_carries_primary_package_purpose_application` to `mikebom-cli/tests/scan_go.rs`: scans the argo-style fixture, parses the SPDX 2.3 output, asserts the main-module package has `primaryPackagePurpose == "APPLICATION"` AND `documentDescribes` contains its SPDXID. **Also assert FR-005 + FR-006 invariants on the same package**: `licenseDeclared == "NOASSERTION"` AND `licenseConcluded == "NOASSERTION"` (FR-005 — empty licenses, no LICENSE-file detection in milestone 053), AND the package's `mikebom:sbom-tier` annotation has value `"source"` (FR-006). Maps to US2 AS#2 + US3 AS#1 (SPDX side) + FR-005/FR-006 explicit verification (resolves analyze findings M5 + L3).
- [X] T021 [P] [US2] Add integration test `scan_go_main_module_in_cdx_metadata_component` to `mikebom-cli/tests/scan_go.rs`: scans the argo-style fixture, parses the CDX 1.6 output, asserts `metadata.component.type == "application"`, `metadata.component.purl` starts with `pkg:golang/`, and the same PURL does NOT appear in `components[]`. Asserts the supplementary `mikebom:component-role: main-module` property IS present in `metadata.component.properties`. **Also assert FR-005 + FR-006 invariants**: `metadata.component.licenses` is absent or empty array (FR-005), AND `metadata.component.properties` contains a property with name `"mikebom:sbom-tier"` and value `"source"` (FR-006). Maps to US2 AS#1 + FR-005/FR-006 (resolves analyze finding M5 + L3, CDX side).
- [X] T022 [P] [US2] Add integration test `scan_go_main_module_emits_c40_annotation_in_spdx` to `mikebom-cli/tests/scan_go.rs`: scans the fixture, parses the SPDX 2.3 output, asserts the main-module package's `annotations[]` contains a `mikebom-annotation/v1` envelope with `field: "mikebom:component-role"` and `value: "main-module"`. Maps to US2 AS#3 (and the SPDX 3 side per AS#3 if v3 emission is enabled).
- [X] T023 [P] [US2] Add a manual sbomqs-validation note to `specs/053-go-main-module-edges/quickstart.md` (extend step 5): how to run sbomqs against the post-053 SBOM and confirm licensing-coverage doesn't regress. (No automated test — sbomqs is an external tool; this is a reviewer-facing verification per SC-003.)

**Checkpoint** (US2 complete): SC-008 passes — main-module placement is standards-native across CDX + SPDX 2.3, with SPDX 3 cross-check noted. SC-003 verified manually via the quickstart sbomqs note. C40 supplementary annotation continues to emit unchanged.

---

## Phase 5: User Story 3 — Document root points at the main-module component (Priority: P3)

**Goal**: SBOM consumers walking from `documentDescribes` (SPDX) or BOM root (CDX) to dep components in a Go-only scan reach the main module as the first hop, where pre-053 they reached a synthetic `DocumentRoot-*` placeholder with no edges.

**Independent Test**: SC-005 — for a Go-only scan, the SPDX `documentDescribes[]` contains exactly the main-module's SPDXID (not a synthetic root). For a polyglot scan, `documentDescribes[]` contains the main-module + every per-ecosystem placeholder root in deterministic ecosystem-name-sorted order.

**Depends on US1**: US3 leverages the main-module created in US1.

### Implementation for US3

- [X] T024 [US3] Verify the existing `mikebom-cli/src/generate/spdx/document.rs::build_document` root-selection algorithm at lines 248-281 picks the main-module via case 1 (single top-level component when Go-only) or case 3 (name-match-on-target_name for polyglot). This is a verification task — read the code, confirm the algorithm picks correctly given `parent_purl: None` on the main-module, and add a comment block at the algorithm site annotating "milestone 053 relies on this — main-module entry has parent_purl=None so it qualifies as top-level." If a code change IS needed (e.g., the algorithm doesn't natively handle the new entry), implement it and update this task to "Implement..." rather than "Verify...".
- [X] T025 [US3] In `mikebom-cli/src/generate/spdx/document.rs::build_document`, ensure the polyglot branch (case 3, multiple top-levels) emits a synthetic super-root that DESCRIBES every per-ecosystem main-module / placeholder root in **ecosystem-name-sorted order** per FR-008. If the existing `synthesize_root` path's emission is already deterministic and ecosystem-sorted, this is a verification task; if it's not (e.g., insertion-order-dependent), implement the sort.

### Tests for US3 (integration-level, validates SC-005 + acceptance scenarios)

- [X] T026 [US3] Add integration test `scan_go_documentdescribes_targets_main_module` to `mikebom-cli/tests/scan_go.rs`: scans the argo-style fixture (Go-only), parses the SPDX 2.3 output, asserts `documentDescribes` is exactly `[<main-module-spdxid>]` (length 1) AND the relationship `SPDXRef-DOCUMENT DESCRIBES <main-module-spdxid>` is present in `relationships[]`. Maps to US3 AS#1.
- [X] T027 [P] [US3] Add integration test `scan_polyglot_documentdescribes_includes_go_main_module` to `mikebom-cli/tests/scan_polyglot_monorepo.rs`: scans the existing polyglot fixture (Go + npm + maven), parses the SPDX 2.3 output, asserts `documentDescribes` contains the Go main-module's SPDXID alongside the existing per-ecosystem placeholder roots, in deterministic ecosystem-name-sorted order. Maps to US3 AS#2.

**Checkpoint** (US3 complete): SC-005 passes — Go-only scans surface the main-module as the document root; polyglot scans include it in the ecosystem-sorted multi-DESCRIBES list.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Goldens regen, CHANGELOG entry, design-notes update, regression sweep. These tasks are sequential because they consume the implemented behavior from US1+US2+US3 and validate the full feature.

- [X] T028 Regenerate goldens for every Go-bearing fixture: `tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/golang.{cdx,spdx,spdx3}.json` AND any others that bundle Go (e.g., polyglot, container-image fixtures bundling go binaries that incidentally have go source). Run `MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression_golang` and equivalents for SPDX 2.3 + SPDX 3. Audit each golden diff for: (a) main-module appears as `metadata.component` (CDX) / `primaryPackagePurpose: APPLICATION` package (SPDX); (b) ≥1 new `DEPENDS_ON` edge per fixture's go.mod direct requires; (c) supplementary C40 annotation present on the main-module entry; (d) no unrelated diffs (e.g., no PURL re-encodings, no new properties on unrelated components). Cross-host byte-identity verified by running on macOS local + linux CI.
- [X] T029 Add new fixture goldens for `tests/fixtures/go/argo-style-no-cache/` across all three formats (CDX 1.6, SPDX 2.3, SPDX 3.0.1). Generated under the same `MIKEBOM_UPDATE_*_GOLDENS=1` env-var convention. These goldens lock SC-001 + SC-007 byte-identity.
- [X] T030 Add CHANGELOG entry to `CHANGELOG.md` under `## [Unreleased]` → `### Changed (BREAKING — SBOM output shape, milestone 053)` with a 4-paragraph entry covering: (1) the new main-module component for Go workspace roots; (2) the native-field placement (CDX `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION` + `documentDescribes`); (3) the supplementary C40 annotation; (4) the version-resolution ladder. Migration paragraph: consumers reading `metadata.component.purl` get the real Go module instead of `pkg:generic/...`; SBOMs gain ≥N edges per Go workspace; LICENSE detection deferred to #103; per-ecosystem main-modules tracked in #104.
- [X] T031 Update `docs/design-notes.md` with a new section: "Go vs other ecosystems: main-module asymmetry." Documents that milestone 053 added a synthetic main-module for Go because go.mod's edge encoding requires one (zero-edges-when-no-cache bug), but other ecosystems' lockfiles encode edges directly against named packages and don't need a synthetic root. Per-ecosystem main-modules are tracked in issue #104 if/when the consumer-value case (project identification, vuln self-lookup) is strong enough.
- [X] T032 Run `./scripts/pre-pr.sh` — confirms clippy `--all-targets` (zero warnings) + `cargo +stable test --workspace` (zero failures) both pass per Constitution mandatory pre-PR gate. If any failure: investigate and fix (do NOT skip with `--no-verify`); typical issues at this stage are golden regen mismatches between linux + macos hosts (rare with the cross-host playbook applied) or new clippy lints triggered by the SPDX enum addition.
- [X] T033 Run the `quickstart.md` 5-step verification recipe end-to-end against a fresh `argo-workflows v3.3.9` clone. Capture the actual output of `jq` queries from steps 3–5 and paste them into the PR description as SC-006 evidence. Verifies the issue-#102 reproduction case is closed in the live binary, not just in fixtures.
- [X] T034 Open the PR via `gh pr create` with title `feat(053): Go main-module component + direct dependency edges (closes #102)` and body covering: (a) summary referencing #102, #103, #104; (b) test plan listing all SC-001..SC-008 outcomes with evidence pointers (golden paths, jq command outputs); (c) breaking-change call-out for the CDX `metadata.component.purl` shift from `pkg:generic/...` to `pkg:golang/...` for Go scans; (d) migration note for SBOM consumers.

---

## Dependencies

```text
Phase 1 (Setup)
  └─▶ Phase 2 (Foundational: T003 enum, T004 version helper, T005 helper tests, T006 mapping doc)
        └─▶ Phase 3 (US1) — T007–T016 + T035 + T036: main-module entry construction +
              │              CDX edge wiring + FR-009 BuildInfo dedup + FR-010 not-linked exclusion
              │              (T035 sequenced after T008; T036 sequenced after T007 [P with T035])
              ├─▶ Phase 4 (US2) — T017–T023: SPDX primaryPackagePurpose + supplementary C40 verification
              └─▶ Phase 5 (US3) — T024–T027: documentDescribes targeting + polyglot super-root
                    └─▶ Phase 6 (Polish) — T028–T034: goldens, CHANGELOG, design-notes, pre-PR, PR
```

US2 and US5 both depend on US1 (T007). US2 and US3 are siblings — both can be implemented in parallel by separate engineers once US1 is complete, OR by the same engineer in series.

## Parallel execution opportunities

Within each phase, the `[P]` markers indicate tasks that touch different files and have no incomplete-task dependencies. Concrete parallelizable groups:

- **Phase 2 setup** (after T003): T004 (golang.rs helper) || T005 (golang.rs unit tests) || T006 (mapping doc). T005 depends on T004's signature being committed; T004 + T006 can fire in parallel from the start.
- **Phase 3 US1 tests** (after T010): T013 || T014 (different test fns, no incomplete deps).
- **Phase 3 US1 cross-cutting** (after T008): T035 (FR-009 dedup) || T036 (FR-010 not-linked guard) — different files, both sequenced after T007/T008.
- **Phase 4 US2 tests** (after T020): T021 || T022 || T023 (different test fns / different docs).
- **Phase 5 US3 tests** (after T026): T027 (different test file).

## Implementation strategy

**MVP scope = US1 only** (Phase 1 + Phase 2 + Phase 3 + minimal Phase 6 polish): closes issue #102 end-to-end. The argo-workflows reproduction case goes from "1 relationship total" to "≥14 DEPENDS_ON edges." Ship this PR alone if scope pressure forces a split.

**Incremental delivery option**: split into three PRs by phase boundary:
1. **PR-A (US1)**: T001–T016. Closes #102. CDX gets `metadata.component`, SPDX gets `documentDescribes` (which already works via the existing root-selection algorithm — verified at T024). Lower-risk.
2. **PR-B (US2)**: T017–T023. Adds `primaryPackagePurpose: APPLICATION` and SPDX-3 native-field cross-check. Layered on top of PR-A.
3. **PR-C (US3 + polish)**: T024–T034. Polyglot doc-root verification, goldens regen sweep, CHANGELOG, PR. Wraps the milestone.

Splitting into three is recommended **only** if PR-A's diff is exceptionally large (likely >2K lines of golden diff alone for the regen sweep). If the diff is manageable in one PR, prefer one PR — the milestone is logically a single feature and reviewers track it more easily as a unit.

## Format validation

All 36 tasks follow the required format: `- [ ] [TaskID] [P?] [Story?] Description with file path`.

- Setup phase (T001–T002): no story label ✓
- Foundational phase (T003–T006): no story label ✓
- US1 phase (T007–T016 + T035 + T036): every task has `[US1]` ✓ (T035/T036 appended post-analyze for FR-009/FR-010 coverage; sequenced after T007/T008 per the in-task "Execute after Tn" notes)
- US2 phase (T017–T023): every task has `[US2]` ✓
- US3 phase (T024–T027): every task has `[US3]` ✓
- Polish phase (T028–T034): no story label ✓

Every task has a (sequential or post-analyze-appended) ID, an explicit file path or path pattern, and a verb-leading description. The append-with-explicit-ordering convention (rather than mass-renumber) preserves the dependency graph references in the Dependencies section while accommodating the C1/C2 coverage gaps surfaced by `/speckit.analyze`.
