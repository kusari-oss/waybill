# Tasks: Monorepo workspace-member visibility (m176)

**Input**: Design documents from `/specs/176-workspace-visibility/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/, quickstart.md

**Tests**: Integration tests ARE included per spec SC-001…SC-008 (each Success Criterion demands executable assertions). Unit tests for the `derive_workspace_root` helper are included per data-model.md §Entity 1 test-cases list.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Single-crate emission-time change contained to `mikebom-cli/`. Paths shown are absolute from repo root.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Bootstrap the branch and confirm zero-dep posture per plan.md Technical Context.

- [X] T001 Verify branch `176-workspace-visibility` is checked out and clean; verify `git log --oneline main..HEAD` shows only the Phase 1 speckit artifacts (spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md, updated CLAUDE.md). **Verified**: branch = `176-workspace-visibility`; no commits ahead of main yet (speckit artifacts are untracked, to be included in the end-of-phase commit); working tree carries only expected files (modified CLAUDE.md from `/speckit-plan`, untracked specs/176-workspace-visibility/, plus untracked specs/175-design-tier-visibility/ which is the intentionally-paused m175 spec).
- [X] T002 Verify plan.md §Technical Context zero-Cargo-dep claim by running `cargo tree -p mikebom --depth 1` and confirming no output changes are needed for m176. **Note**: workspace crate under `mikebom-cli/` is named just `mikebom`, so the correct invocation is `-p mikebom` (not `-p mikebom-cli`). **Verified**: `serde_json`, `tracing`, `anyhow` are all already direct deps (m176 needs only these plus `std::path` / `std::collections::BTreeSet`); zero new Cargo dependencies required.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The `derive_workspace_root` helper is used by ALL three user stories (US1 per-component emission, US2 advisory-log workspace enumeration, US3 doc-scope aggregate). It MUST land + pass unit tests before any story phase begins.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Create `mikebom-cli/src/scan_fs/workspace_root.rs` with `pub(crate) fn derive_workspace_root(source_file_path: &str, scan_root_abs: &std::path::Path) -> Option<String>` matching the data-model.md §Entity 1 contract (handle relative root-relative paths, `path+file://` URI shape, empty-string → None, root-level manifest → `Some(".")`, forward-slash normalization on Windows). The inline `#[cfg(test)] mod tests { ... }` block MUST carry `#[cfg_attr(test, allow(clippy::unwrap_used))]` per Constitution Principle IV + the existing convention throughout `mikebom-cli/src/trace/` — otherwise the T030 pre-PR clippy `-D warnings` gate will fail
- [X] T004 Wire the new module into `mikebom-cli/src/scan_fs/mod.rs` — add `pub(crate) mod workspace_root;` alongside the existing `pub(crate) mod` declarations
- [X] T005 Add 6 unit tests inline in `mikebom-cli/src/scan_fs/workspace_root.rs` covering the data-model.md §Entity 1 test-cases list: `derive_root_level_manifest_returns_dot`, `derive_subdir_manifest_returns_dir_path`, `derive_pip_uri_main_module_returns_relative`, `derive_pip_uri_outside_scan_root_returns_none`, `derive_empty_string_returns_none`, `derive_backslash_windows_normalized`. Guard the `mod tests` item with `#[cfg_attr(test, allow(clippy::unwrap_used))]` if any test uses `.unwrap()` (per T003 note)
- [X] T006 Run `cargo +stable test -p mikebom --bin mikebom scan_fs::workspace_root` and confirm all 6 unit tests pass. **Note**: `scan_fs` is declared in `main.rs`, not `lib.rs`, so `--bin mikebom` (not `--lib`) is the correct target. Result: `ok. 6 passed; 0 failed`.

**Checkpoint**: `derive_workspace_root` helper ships + is unit-tested. All user stories can now start in parallel.

---

## Phase 3: User Story 1 — CVE triage via per-component `mikebom:workspace-member` (Priority: P1) 🎯 MVP

**Goal**: Every workspace-attributable component gains a `mikebom:workspace-member` annotation whose value is a JSON-encoded sorted-deduplicated array of workspace root-relative paths. Consumers can filter `.components[]` by workspace via jq. File-tier components explicitly do NOT gain the annotation (per FR-002 / Q1).

**Independent Test**: `mikebom sbom scan --path <2-workspace-fixture> --format cyclonedx-json` → jq filter on `mikebom:workspace-member` returns workspace-scoped components only. Verified by T014.

### Implementation for User Story 1

- [X] T007 [US1] Per-component CDX emission — **implementation approach changed**: rather than editing `generate/cyclonedx/metadata.rs` directly, populated the annotation at scan-time via new helper `scan_fs::tag_components_with_workspace_member` (in `mikebom-cli/src/scan_fs/mod.rs` alongside the m133 `tag_components_with_layer_digest` precedent). The annotation flows into each component's `extra_annotations` bag and is auto-emitted by the existing extra-annotations→CDX-properties wiring at `builder.rs:1245`. Zero new plumbing through BuilderConfig/ScanArtifacts. Called from `cli/scan_cmd.rs` post-`tag_components_with_layer_digest` with the canonicalized `root_path`.
- [X] T008 [US1] Per-Package SPDX 2.3 emission — same approach: extra_annotations flows through `spdx/annotations.rs::annotate_component:371-382` which pushes the annotation via the standing m080 `MikebomAnnotationCommentV1` envelope. Zero SPDX-side changes needed.
- [X] T009 [US1] Per-`software_Package` SPDX 3 emission — same approach: extra_annotations flows through `spdx/v3_annotations.rs:382-393` which pushes the annotation via the standing typed-Annotation graph element mechanism. Zero SPDX 3-side changes needed.
- [X] T010 [US1] In `mikebom-cli/src/parity/extractors/cdx.rs`, added `cdx_anno!(c120_cdx, "mikebom:workspace-member", component);` after `c119_cdx`
- [X] T011 [US1] In `mikebom-cli/src/parity/extractors/spdx2.rs`, added `spdx23_anno!(c120_spdx23, "mikebom:workspace-member", component);` after `c119_spdx23`
- [X] T012 [US1] In `mikebom-cli/src/parity/extractors/spdx3.rs`, added `spdx3_anno!(c120_spdx3, "mikebom:workspace-member", component);` after `c119_spdx3`
- [X] T013 [US1] In `mikebom-cli/src/parity/extractors/mod.rs`, registered the C120 row in EXTRACTORS (`row_id: "C120"`, `label: "mikebom:workspace-member"`, `directional: SymmetricEqual`, `order_sensitive: false`) inserted after C119; imports updated across the cdx/spdx2/spdx3 sub-modules.
- [X] T014 [US1] Created `mikebom-cli/tests/workspace_visibility.rs` with 3-workspace pip fixture (root/pyproject.toml + subproject_a/pyproject.toml + subproject_b/pyproject.toml, each with distinct deps + shared `shared-dep`) and integration test `t007_us1_per_component_workspace_member_annotation` asserting acceptance scenarios 1+2 + SC-006 (distinct-set gate: exactly `[".", "subproject_a", "subproject_b"]`). **Result**: `ok. 1 passed; 0 failed`. Clippy `--all-targets -D warnings` on `-p mikebom`: clean.

**Checkpoint**: US1 fully functional and testable. Consumers can jq-filter emitted components by workspace across CDX / SPDX 2.3 / SPDX 3.

---

## Phase 4: User Story 2 — Advisory log surfaces monorepo shape at scan time (Priority: P1)

**Goal**: When the scan detects N > 1 workspaces AND emits at least one component, one INFO-level advisory log line lands on stderr containing the workspace count, each workspace path, and a stable grep-substring pointing at `docs/reference/monorepos.md`.

**Independent Test**: `mikebom sbom scan --path <3-workspace-fixture> 2> stderr.log` → `grep -cF 'monorepo shape detected: ' stderr.log` = 1. Verified by T016.

### Implementation for User Story 2

- [X] T015 [US2] In `mikebom-cli/src/cli/scan_cmd.rs`, at the emission-tail site (just before the "SBOM written" `tracing::info!` line), added the advisory-log block per data-model.md §Entity 6. Predicate `workspaces_detected.len() > 1 && !components.is_empty()`; body carries stable substring `"monorepo shape detected: "` + count-prefix + comma-separated workspace list + `docs/reference/monorepos.md` cross-reference; INFO level; no offline gating. Workspace set derived from `components[].extra_annotations["mikebom:workspace-member"]` (populated in Phase 3).
- [X] T016 [US2] Extended `mikebom-cli/tests/workspace_visibility.rs` with 5 US2 tests: `t008_us2_advisory_log_fires_once_on_monorepo` (3-workspace fixture, exactly 1 hit, body names all 3 paths); `t009_us2_advisory_log_silent_on_single_workspace` (1-workspace, 0 hits); `t010_us2_advisory_log_silent_on_bare_directory` (no manifests, 0 hits); `t011_us2_advisory_log_fires_under_offline` (2-workspace, 1 hit under `--offline` — FR-006 gate); `t011a_us2_sc005_ten_workspace_advisory_stability` (10-workspace fixture, 1 hit, body carries `"10 workspaces"` count-prefix, names all 10 paths). **Result**: `ok. 6 passed; 0 failed` (5 new + T014). Clippy `-p mikebom --all-targets -D warnings`: clean.

**Checkpoint**: US2 fully functional. Operators see the monorepo advisory at scan time; CI dashboards can grep-substring-detect monorepo scans.

---

## Phase 5: User Story 3 — Doc-scope `mikebom:workspaces-detected` enumerates workspaces without walking `components[]` (Priority: P2)

**Goal**: A single doc-scope annotation exposes the workspace list; consumers avoid full component traversal. The C121 value MUST equal the union of every C120 value (FR-012 cross-annotation invariant).

**Independent Test**: `jq '.metadata.properties[]? | select(.name == "mikebom:workspaces-detected") | .value | fromjson'` on an emitted CDX SBOM returns the workspace array. Verified by T023.

### Implementation for User Story 3

- [X] T017 [US3] In `mikebom-cli/src/generate/cyclonedx/metadata.rs`, added doc-scope C121 emission block after C119. Value is `serde_json::to_string(&workspaces)` (JSON-encoded array in a string, matching m173 precedent). Gated on non-empty result from the shared helper. Added new module `mikebom-cli/src/generate/workspace_detected.rs` with `compute(&[ResolvedComponent]) -> Vec<String>` + 5 inline unit tests (all pass); wired into `generate/mod.rs`.
- [X] T018 [US3] In `mikebom-cli/src/generate/spdx/annotations.rs`, added doc-scope C121 emission after C119 using the same shared helper. Envelope-wrapped via `push()` → `MikebomAnnotationCommentV1`.
- [X] T019 [US3] In `mikebom-cli/src/generate/spdx/v3_annotations.rs`, added doc-scope C121 emission after C119 using the same shared helper. Typed `Annotation` graph element on the `SpdxDocument` root.
- [X] T020 [US3] Added `cdx_anno!(c121_cdx, "mikebom:workspaces-detected", document);` in `parity/extractors/cdx.rs` after `c120_cdx`.
- [X] T021 [US3] Added `spdx23_anno!(c121_spdx23, "mikebom:workspaces-detected", document);` in `parity/extractors/spdx2.rs`.
- [X] T022 [US3] Added `spdx3_anno!(c121_spdx3, "mikebom:workspaces-detected", document);` in `parity/extractors/spdx3.rs`.
- [X] T023 [US3] Registered C121 in EXTRACTORS in `parity/extractors/mod.rs` (`SymmetricEqual`, non-order-sensitive); imports updated across all 3 sub-modules.
- [X] T024 [US3] Added 3 US3 tests to `mikebom-cli/tests/workspace_visibility.rs`: `t012_us3_doc_scope_workspaces_detected_annotation` (3-workspace fixture → annotation present, sorted 3-element array); `t013_us3_absent_when_zero_workspaces` (bare directory → absent per FR-003); `t014_us3_c121_equals_union_of_c120` (FR-012 cross-annotation invariant). **Result**: `ok. 9 passed; 0 failed` (3 new US3 + 5 US2 + 1 US1). Clippy `-p mikebom --all-targets -D warnings`: clean.

**Checkpoint**: US3 fully functional. Doc-scope aggregate materializes the workspace list; FR-012 invariant enforced.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Docs, golden regeneration under the SC-004 byte-identity gate, pre-PR verification.

- [X] T025 [P] Created `docs/reference/monorepos.md` (~230 lines) covering workspace concept, reader precedent (m068/m066/m064/m053/m070), the four jq recipes (enumerate workspaces, filter by workspace, per-workspace CVE scoping, C120↔C121 invariant verification), format-neutral consumption for CDX/SPDX 2.3/SPDX 3, two composition patterns (per-workspace CVE dashboard + per-workspace license inventory), what m176 does NOT restructure, CI integration recipe, cross-references to reading-a-mikebom-sbom.md + sbom-format-mapping.md + component-tiers.md.
- [X] T026 [P] Added `mikebom:workspace-member` + `mikebom:workspaces-detected` subsection to `docs/reference/reading-a-mikebom-sbom.md` §3.1 (Vulnerability scanning) — full wire contract + KEEP-NO-NATIVE audit note + 4 jq recipes (enumerate, filter, per-CVE scoping, FR-012 invariant verification).
- [X] T027 [P] Added C120 + C121 rows to `docs/reference/sbom-format-mapping.md` Section C after C119, both KEEP-NO-NATIVE with rejected alternatives cited per Constitution Principle V (CDX `component.group` semantically different — authoring org, not scan-target boundary; SPDX 2.3 `Package.sourceInfo` free-text; SPDX 3 `Element.namespace` identity-URI scope, etc.).
- [X] T028 Regenerated all 33 golden fixtures (`MIKEBOM_UPDATE_CDX_GOLDENS=1` for cdx_regression, `MIKEBOM_UPDATE_SPDX_GOLDENS=1` for spdx_regression, `MIKEBOM_UPDATE_SPDX3_GOLDENS=1` for spdx3_regression). Diff verification: `git diff --stat` shows 30 files changed, +1356 insertions, 0 deletions. Property-name grep on additions returns ONLY `mikebom:workspace-member` and `mikebom:workspaces-detected` — SC-004 byte-identity gate holds by inspection. Re-run without update env vars: all 33 goldens pass.
- [X] T029 Added `t015_sc004_monorepo_byte_identity_gate_semantic` + `t015b_sc004_single_project_byte_identity_gate_semantic` to `mikebom-cli/tests/workspace_visibility.rs`. Semantic in-code SC-004 gate (belt-and-braces alongside the 33 golden regressions): (a) monorepo 3-workspace fixture — asserts doc-scope contains `mikebom:workspaces-detected` and per-component contains `mikebom:workspace-member`; (b) single-project requirements.txt fixture — asserts both annotations present AND C121 value is exactly `["."]` (FR-013 + SC-008 gate). **Result**: 11 tests pass (US1+US2+US3+SC-004 semantic gates).
- [X] T030 Ran `./scripts/pre-pr.sh` — final line: `>>> all pre-PR checks passed.` Zero failures across the full log. Two intermediate failures caught and fixed during Phase 6: (a) `parity_npm` — npm main-module promoted to `metadata.component` was losing its `mikebom:workspace-member` annotation on the CDX side, breaking the C120 `SymmetricEqual` invariant vs SPDX 2.3/SPDX 3. Fix: propagate the annotation into `metadata.component.properties[]` at `cyclonedx/metadata.rs` matching the existing `mikebom:source-files`/`mikebom:detected-go`/`mikebom:produces-binaries` propagation pattern. (b) `pkg_alias_binding_us1::no_alias_scan_is_byte_identical_to_pre_feature_golden` — additional CDX golden fixture at `tests/fixtures/pkg_alias_binding/image-baz.cdx.json` needed regenerating (missed in T028's 33-fixture batch). Regenerated via the test's built-in `MIKEBOM_UPDATE_CDX_GOLDENS=1` mechanism.
- [X] T031 Walked all quickstart.md verification paths against the vendored `kusari-sandbox/test-langflow` fixture using a release build. **Path A (workspace enumeration)**: `mikebom:workspaces-detected` returned `[".", "docs", "scripts/gp", "src/frontend"]`. Per-workspace filters returned 818 components for `.` (pypi root) and 1018 components for `src/frontend` (npm). Per-CVE scoping demo (`fastapi`): all 4 fastapi PURLs correctly scope to workspace `.`. **Path B (advisory log)**: `grep -cF 'monorepo shape detected: '` = 1; body contains all 4 workspace paths + `docs/reference/monorepos.md` cross-reference. **Path C (FR-012 cross-annotation invariant)**: jq produced `{match: true}`. **Observation** (unrelated to m176 emission correctness): langflow ships 9 `pyproject.toml` files but mikebom's readers surface only 4 as workspace-attributable main-modules (deep-nested pyprojects under `src/lfx`, `src/sdk`, `src/backend/base`, `src/bundles/*` don't produce main-module components — a pip/m068 reader limitation). C120/C121 correctly reflects the workspaces mikebom's readers actually detect. Follow-up milestone candidate: broaden pip main-module detection for deep-nested pyprojects (out of m176 scope per FR-009's "reuse existing reader detection").

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No blockers — starts immediately.
- **Foundational (Phase 2)**: Depends on Setup. **BLOCKS ALL user stories** — the `derive_workspace_root` helper is the substrate every emission task calls.
- **User Story 1 (Phase 3)**: Depends on Phase 2. All emission tasks (T007/T008/T009) can run in parallel across the three format emitters, BUT parity infra (T010–T013) touches the same 4 files US3 will touch — do US1's parity edits before US3's.
- **User Story 2 (Phase 4)**: Depends on Phase 2. Independent of US1/US3 emission code (touches `scan_cmd.rs`, not the emitters). Can run in parallel with US1/US3.
- **User Story 3 (Phase 5)**: Depends on Phase 2. Emission tasks (T017/T018/T019) touch the SAME emitter files US1 modified — must run AFTER Phase 3 (or coordinate as sequential edits within each file). Parity tasks (T020–T023) touch the SAME parity files US1 modified — must run AFTER T010–T013.
- **Polish (Phase 6)**: Docs tasks (T025/T026/T027) can run in parallel and don't block on user-story code. Golden regen (T028) MUST run after all of Phase 3+4+5. SC-004 gate (T029) + pre-PR (T030) + quickstart walk (T031) MUST be last.

### User Story Dependencies

- **US1 (P1) MVP** — independent given Phase 2.
- **US2 (P1)** — independent given Phase 2. Touches `scan_cmd.rs` only; zero file conflict with US1/US3.
- **US3 (P2)** — depends on US1's emission-file + parity-file edits landing first (shared files: `cyclonedx/metadata.rs`, `spdx/annotations.rs`, `spdx/v3_annotations.rs`, `parity/extractors/{mod,cdx,spdx2,spdx3}.rs`).

### Within Each User Story

- Emission across three formats can proceed in parallel (different files).
- Parity infra edits touch shared files — sequential within a story.
- Tests can be written alongside implementation but MUST pass before phase checkpoint.

### Parallel Opportunities

- **Phase 3 emission (T007, T008, T009)** [P] — three different emitter files, no dep between them.
- **Phase 4 (T015)** — runs in parallel with all of Phase 3 (different file: `scan_cmd.rs`).
- **Phase 5 emission (T017, T018, T019)** [P] — three different files, no dep between them (but each depends on the same file's T007/T008/T009 landing first).
- **Phase 6 docs (T025, T026, T027)** [P] — three different files.
- **US1 and US2** can be worked on by different developers in parallel from the Phase 2 checkpoint.

---

## Parallel Example: User Story 1 emission tasks

```bash
# Three format emitters can be edited in parallel — no dep between them:
Task: "Add per-component mikebom:workspace-member emission in mikebom-cli/src/generate/cyclonedx/metadata.rs"
Task: "Add per-Package mikebom:workspace-member envelope emission in mikebom-cli/src/generate/spdx/annotations.rs"
Task: "Add per-software_Package mikebom:workspace-member Annotation graph element in mikebom-cli/src/generate/spdx/v3_annotations.rs"

# Independently, US2's advisory log runs in parallel:
Task: "Add monorepo-shape advisory tracing::info! block in mikebom-cli/src/cli/scan_cmd.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T002).
2. Complete Phase 2: Foundational (T003–T006) — `derive_workspace_root` helper + unit tests.
3. Complete Phase 3: User Story 1 (T007–T014) — per-component C120 emission + parity + integration test.
4. **STOP and VALIDATE**: run the T014 integration test; confirm jq queries slice the SBOM as expected.
5. This is the MVP — consumers can already scope components to workspaces via jq at this point.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. Add User Story 1 → test independently → **MVP demo-able** (per-component workspace tag).
3. Add User Story 2 in parallel → test independently → operators see monorepo advisory at scan time.
4. Add User Story 3 → test independently → doc-scope aggregate for large-SBOM performance.
5. Polish phase (docs + goldens + pre-PR) → PR-ready.

### Parallel Team Strategy

With two developers:

1. Together: complete Setup + Foundational.
2. Once Foundational done:
   - Developer A: US1 (Phase 3) + US3 (Phase 5) — sequential on shared emission files.
   - Developer B: US2 (Phase 4) + Polish (Phase 6) — independent files.
3. Regroup for golden regen (T028) + SC-004 gate (T029) + pre-PR (T030) + quickstart walk (T031).

### Solo Strategy (recommended for m176 given ~90 lines emission + ~200 lines test)

1. Sequential through phases 1→2→3→4→5→6, ~one-sitting delivery.
2. Golden regen batched at T028 avoids the release-bump-PR-slow trap (regen-once, not per-emission-edit).

---

## Notes

- [P] tasks = different files, no dependencies.
- [Story] label maps task to specific user story for traceability.
- Every FR from spec.md maps to at least one task; every SC has a verifying task in Phase 6 or the story's test task.
- Reuse of milestone-172 (C117) + milestone-173 (C118/C119) precedent means the parity/emission scaffolding is a copy-shape-not-invent activity.
- Golden regeneration is the largest single mechanical change (33 fixtures). Batching at T028 avoids repeated regen churn.
- SC-004 byte-identity gate (T029) is the load-bearing correctness assertion — if it fails, an unintended non-C120/C121 delta leaked; investigate before proceeding.
- Pre-PR gate (T030) is MANDATORY per project CLAUDE.md — do not open PR without both clippy + tests clean.
