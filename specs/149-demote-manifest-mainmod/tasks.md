---
description: "Task list for milestone 149 — preserve manifest-derived main-module as demoted library entry when --root-name overrides it (closes issue #151)"
---

# Tasks: milestone 149 — preserve manifest-derived main-module as demoted library entry

**Input**: Design documents from `/specs/149-demote-manifest-mainmod/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Spec mandates tests via SC-002 + SC-003 + SC-004 + SC-005. Test tasks are included inline alongside implementation tasks (per the project's existing Rust convention of in-file `#[cfg(test)] mod tests` for unit tests + `mikebom-cli/tests/*.rs` for integration tests).

**Organization**: US1 (P1, MVP) ships the entire feature: helper + CLI flag + demote branch + unit tests + parity-catalog row + docs. US2 (P2) layers cross-ecosystem integration test coverage on top — proves the helper's ecosystem-agnostic design works for Cargo + npm + Go fixtures. The refactor that consolidates the existing duplicated drop logic from three emitter sites into one shared helper is part of US1; it's load-bearing for the preserve branch and net-LOC-neutral on the emitter side.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Maps to user story from spec.md (US1, US2)
- Paths absolute under repo root `/Users/mlieberman/Projects/mikebom/`

## Path Conventions

Touched files: `mikebom-cli/src/generate/root_selector.rs` (new helper + unit tests), `mikebom-cli/src/cli/scan_cmd.rs` + `mikebom-cli/src/generate/mod.rs` (CLI flag + plumbing), three emitter files (`cyclonedx/builder.rs`, `spdx/document.rs`, `spdx/v3_document.rs` — refactor to call helper), four parity-extractor files (`parity/extractors/mod.rs` + `cdx.rs` + `spdx2.rs` + `spdx3.rs` — new C-row for `mikebom:demoted-from-main-module`), two docs files (`docs/reference/sbom-format-mapping.md` + `identifiers.md`), one new integration test file (`mikebom-cli/tests/demote_manifest_mainmod_md149.rs`).

---

## Phase 1: Setup

**Purpose**: Verify baseline before any code change.

- [ ] T001 Confirm baseline pre-PR gate is green on branch `149-demote-manifest-mainmod`. Run `./scripts/pre-pr.sh` from repo root. Expected: clippy `--workspace --all-targets -- -D warnings` clean; `cargo test --workspace` passes except for the pre-existing local-environment `sbomqs_parity::sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` failure documented in milestone-144 T001. If anything ELSE fails, halt and investigate before proceeding.

---

## Phase 2: Foundational

**Purpose**: None required for this milestone. The new helper sits inside the existing `root_selector.rs` module; no shared type plumbing or cross-cutting infrastructure needs to land first. Zero new Cargo dependencies per data-model.md.

(No tasks in this phase. Proceed directly to Phase 3.)

---

## Phase 3: User Story 1 - Operator preserves manifest identity via opt-in flag (Priority: P1) 🎯 MVP

**Goal**: Implement the full feature surface: `apply_main_module_drop_or_demote` shared helper + `--preserve-manifest-main-module` CLI flag + demote-branch behavior + unit tests + new parity-catalog row + Constitution V docs row. After this phase, the feature works end-to-end for any single-main-module scan with operator override + preserve flag set.

**Independent Test**: Construct a Cargo fixture with `[package].name = "foo-internal"`, `version = "0.5.1"`. Scan with `--root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module --format cyclonedx-json`. Assert: (a) `metadata.component.name == "widget-svc"`; (b) `components[]` contains an entry with `purl == "pkg:cargo/foo-internal@0.5.1"`, `type == "library"`; (c) that entry's `properties[]` contains `{name: "mikebom:demoted-from-main-module", value: "true"}`.

### Implementation for User Story 1

- [ ] T002 [US1] Define `DropOrDemoteResult` struct in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/root_selector.rs` per contracts/demote-helper.md. Public-to-crate (`pub(crate)`). Fields: `effective_components: Vec<ResolvedComponent>` and `redirected_main_module_purls: Vec<String>`. Include doc-comments explaining the relationship-re-anchoring contract per US1 clarification Option A (the redirected PURLs Vec is populated EVEN when entries are kept/demoted, so milestone-084 re-anchoring logic fires regardless).

- [ ] T003 [US1] Implement `apply_main_module_drop_or_demote()` in the same file per contracts/demote-helper.md algorithm sketch. Function signature: `pub(crate) fn apply_main_module_drop_or_demote(components: &[ResolvedComponent], root_override: &RootComponentOverride, preserve_main_module: bool) -> DropOrDemoteResult`. Initial implementation lands ONLY Paths 1 (passthrough — override inactive) AND 2 (drop — override active + preserve OFF), achieving parity with the existing per-emitter drop logic. The Path 3 demote branch is added in T008 after the CLI flag plumbing lands; landing them separately means the refactor (T004-T006) lands first as a no-op and can be reviewed in isolation. Helper signature includes the `preserve_main_module: bool` parameter from the start so the refactor doesn't need to be touched again at T008 — the parameter is just unused for the first commit.

- [ ] T004 [P] [US1] Refactor `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/cyclonedx/builder.rs` lines 325-347 to call the new helper. Replace the manual filter loop:

  ```rust
  // Pre-149:
  let mut dropped_main_module_purls: Vec<String> = Vec::new();
  let filtered_components_owned: Option<Vec<ResolvedComponent>> = if override_active {
      let mut keep: Vec<ResolvedComponent> = Vec::with_capacity(components.len());
      for c in components.iter() {
          let is_main_module = c.extra_annotations.get("mikebom:component-role")...;
          if is_main_module {
              tracing::info!(...);
              dropped_main_module_purls.push(c.purl.as_str().to_string());
          } else {
              keep.push(c.clone());
          }
      }
      Some(keep)
  } else {
      None
  };
  ```

  with:

  ```rust
  // Post-149:
  let drop_result = crate::generate::root_selector::apply_main_module_drop_or_demote(
      components,
      &artifacts.root_override,
      /* preserve_main_module = */ false,  // T007 plumbs the real value
  );
  let filtered_components_owned: Option<Vec<ResolvedComponent>> = if override_active {
      Some(drop_result.effective_components)
  } else {
      None
  };
  let dropped_main_module_purls = drop_result.redirected_main_module_purls;
  ```

  Keep the existing INFO log message inline at the call site for now (or move it inside the helper — doc-comment the choice). Note: until T007 lands the CLI flag, the helper is called with `preserve_main_module = false` hardcoded; behavior is byte-identical to pre-149.

- [ ] T005 [P] [US1] Refactor `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/document.rs` lines 262-282 to call the same helper. Parallel replacement of the equivalent drop loop. Same `preserve_main_module = false` hardcoded until T007.

- [ ] T006 [P] [US1] Refactor `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/generate/spdx/v3_document.rs` lines 57-75 to call the same helper. Parallel replacement.

  **Checkpoint after T002-T006**: at this point, the three emitter sites all call the shared helper, but behavior is byte-identical to pre-149 (preserve=false hardcoded). Run `cargo +stable test --workspace` to confirm zero regression — every existing CDX / SPDX / SPDX3 golden + integration test still passes. This is the load-bearing refactor checkpoint; merging here would already ship a clean drop-logic consolidation.

- [ ] T007 [US1] Add the new `--preserve-manifest-main-module` CLI flag at `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/cli/scan_cmd.rs` via clap `Args`-derive. Field type `bool` with `default_value = "false"` and help-text per spec FR-001:

  ```rust
  /// Preserve the manifest-derived main-module as a `library`-typed
  /// entry in components[] when --root-name (or --root-version /
  /// --root-purl) override is active. Without this flag, the
  /// manifest identity is dropped per milestone 077's clean-
  /// replacement semantic. Demoted entries carry a
  /// `mikebom:demoted-from-main-module = "true"` annotation per
  /// Constitution V parity-bridging. No-op when no root-override
  /// flag is set (issue #151 / milestone 149).
  #[arg(long, default_value = "false")]
  pub preserve_manifest_main_module: bool,
  ```

  Then plumb the value through `ScanRequest` → `ScanArtifacts` (the existing struct passed to the three emitters) so the helper sees the operator-set value at each emitter call site. Pattern: same as milestone 134's `--check-divergence` flag and milestone 119's `--supplement` flag — add the boolean field to both `ScanRequest` and `ScanArtifacts` in `mikebom-cli/src/generate/mod.rs`; the existing `ScanRequest → ScanArtifacts` conversion site forwards the value. Then at the three emitter sites from T004-T006, replace the hardcoded `false` with `artifacts.preserve_manifest_main_module`.

- [ ] T008 [US1] Add the Path 3 demote branch to the helper in `root_selector.rs`. Per contracts/demote-helper.md algorithm sketch + research §B:

  ```rust
  // Inside the single-pass walk that already exists from T003:
  for c in components {
      if is_main_module(c) {
          redirected.push(c.purl.as_str().to_string());
          if effective_preserve {
              // Demote in place — keep entry, transform annotations.
              let mut demoted = c.clone();
              demoted.extra_annotations.remove("mikebom:component-role");
              demoted.extra_annotations.insert(
                  "mikebom:demoted-from-main-module".to_string(),
                  serde_json::Value::String("true".to_string()),
              );
              effective.push(demoted);
          }
          // else: drop (don't push). PURL still in redirected for re-anchoring.
      } else {
          effective.push(c.clone());
      }
  }
  ```

  Also add the multi-main-module guard from Edge Case 4 / FR-013:

  ```rust
  let main_module_count = components.iter().filter(|c| is_main_module(c)).count();
  let effective_preserve = preserve_main_module && main_module_count == 1;
  if preserve_main_module && main_module_count > 1 {
      tracing::info!(
          count = main_module_count,
          "--preserve-manifest-main-module skipped: multi-main-module scan ({} modules detected)",
          main_module_count
      );
  }
  ```

  And the Edge Case 1 no-op log when override is inactive (handled at the CLI parser level OR via an INFO log inside the helper's passthrough branch — implementer's choice; document inline):

  ```rust
  if !override_active && preserve_main_module {
      tracing::info!("--preserve-manifest-main-module has no effect without --root-name override");
  }
  ```

- [ ] T009 [US1] Add unit test `apply_drop_or_demote_no_override_is_passthrough_md149` in `root_selector.rs#mod tests`. Tests Path 1 (passthrough; SC-003 regression guard). Pattern:

  ```rust
  #[test]
  fn apply_drop_or_demote_no_override_is_passthrough_md149() {
      let components = vec![
          make_main_module("pkg:cargo/foo-internal@0.5.1"),
          make_library("pkg:cargo/dep-a@1.0.0"),
      ];
      let no_override = no_override();  // existing test helper at root_selector.rs:545
      let result = apply_main_module_drop_or_demote(&components, &no_override, false);
      assert_eq!(result.effective_components.len(), 2);
      assert!(result.redirected_main_module_purls.is_empty());
      // Main-module entry preserved verbatim, including its role tag.
      let preserved_main = result.effective_components.iter()
          .find(|c| c.purl.as_str() == "pkg:cargo/foo-internal@0.5.1")
          .expect("main-module entry preserved");
      assert_eq!(
          preserved_main.extra_annotations.get("mikebom:component-role").and_then(|v| v.as_str()),
          Some("main-module"),
          "passthrough MUST preserve the main-module role tag"
      );
  }
  ```

  Requires `make_main_module(purl_str: &str) -> ResolvedComponent` helper (define locally in the test module). Covers FR-007 + SC-003.

- [ ] T010 [P] [US1] Add unit test `apply_drop_or_demote_override_no_preserve_drops_main_module_md149` in same `mod tests`. Tests Path 2 (drop; SC-002 regression guard). Asserts: main-module entry is REMOVED from `effective_components`; its PURL IS in `redirected_main_module_purls`.

- [ ] T011 [P] [US1] Add unit test `apply_drop_or_demote_override_with_preserve_demotes_main_module_md149` in same `mod tests`. Tests Path 3 (demote; FR-001 + FR-004 + US1 clarification Option A). Asserts: (a) main-module entry IS in `effective_components`; (b) the entry's `extra_annotations["mikebom:component-role"]` is REMOVED; (c) the entry's `extra_annotations["mikebom:demoted-from-main-module"] == Value::String("true")`; (d) PURL IS in `redirected_main_module_purls` (US1 Option A — re-anchoring still fires).

- [ ] T012 [P] [US1] Add unit test `apply_drop_or_demote_demote_preserves_other_fields_md149` in same `mod tests`. Tests FR-005: every field of `ResolvedComponent` other than `extra_annotations` content is preserved verbatim across the demote transformation. Construct a main-module with rich non-default values on `lifecycle_scope`, `sbom_tier`, `evidence.confidence`, `licenses`, `hashes`, `supplier`. Assert each preserved field-by-field after the demote.

- [ ] T013 [P] [US1] Add unit test `apply_drop_or_demote_multi_main_module_with_preserve_is_noop_md149` in same `mod tests`. Tests Edge Case 4 + FR-013: when 2+ main-modules are present AND preserve is set, the helper falls through to the drop path (no demote fires). Asserts: `effective_components` has 0 main-module entries (both dropped); `redirected_main_module_purls` has BOTH PURLs (consistent with the drop path).

- [ ] T014 [P] [US1] Add unit test `apply_drop_or_demote_demoted_entry_has_no_outbound_relationships_in_helper_input_md149` in same `mod tests`. Tests US1 clarification Option A: the helper itself doesn't manipulate relationships (relationships live outside the components Vec), but the test documents the invariant that the demoted entry's PURL goes into `redirected_main_module_purls` so the downstream milestone-084 re-anchoring code path fires. Assert: `redirected.contains(&main_module_purl)` even when the entry is kept/demoted. (This is a duplicate assertion vs T011 but worth its own test for the regression-guard intent.)

- [ ] T015 [US1] Add new parity-catalog row for `mikebom:demoted-from-main-module` in `/Users/mlieberman/Projects/mikebom/mikebom-cli/src/parity/extractors/`. First grep `grep -n "row_id:" mikebom-cli/src/parity/extractors/mod.rs | tail -3` to find the next available C-number (likely C102 — milestone 148 added C101). Add the row in `mod.rs` per the milestone-147/148 pattern:

  ```rust
  ParityExtractor {
      row_id: "C102",  // verify next number; bump if collision
      label: "mikebom:demoted-from-main-module",
      cdx: c102_cdx,
      spdx23: c102_spdx23,
      spdx3: c102_spdx3,
      directional: Directionality::SymmetricEqual,
      order_sensitive: false,
  },
  ```

  Then add three sibling extractor functions `c102_cdx` / `c102_spdx23` / `c102_spdx3` in `cdx.rs` / `spdx2.rs` / `spdx3.rs` using the existing `cdx_anno!(c102_cdx, "mikebom:demoted-from-main-module", component)` / `spdx23_anno!(...)` / `spdx3_anno!(...)` macro patterns (search for nearby `_anno!` invocations to match the exact macro syntax). Add the import lines to `mod.rs`'s `use cdx::{...}` / `use spdx2::{...}` / `use spdx3::{...}` blocks. Covers SC-005.

- [ ] T016 [US1] Update `/Users/mlieberman/Projects/mikebom/docs/reference/sbom-format-mapping.md` to add a new row for `mikebom:demoted-from-main-module` per Constitution Principle V's documentation requirement (spec FR-011). Use the existing row format (see milestone-147's C101 row as the template). Cite the milestone (`milestone 149 / issue #151`), the per-format wire shape (CDX `components[].properties[]`, SPDX 2.3 + SPDX 3 envelope annotations), and the explicit standards-native-alternatives-rejected justification table from the spec's Constitution V audit section (6 rejected alternatives: CDX `component.type`, CDX `component.scope`, SPDX 2.3 `primaryPackagePurpose`, SPDX 2.3 typed-relationship enum, SPDX 3 `software_softwarePurpose`, SPDX 3 `LifecycleScopedRelationship.scope`). Covers FR-011.

**Checkpoint**: After Phase 3, US1 is fully functional. `cargo +stable test --workspace` MUST pass. Manual smoke per quickstart §Scenario 1: scan a Cargo fixture with `--root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module` and confirm `pkg:cargo/foo-internal@0.5.1` appears as a `library`-typed entry in `components[]` with the new annotation.

---

## Phase 4: User Story 2 - Cross-ecosystem coverage (Cargo + npm + Go integration test) (Priority: P2)

**Goal**: Prove the helper's ecosystem-agnostic design works across the three flagship manifest-driven ecosystems (Cargo + npm + Go per SC-004 representative trio).

**Independent Test**: A single new integration test file at `mikebom-cli/tests/demote_manifest_mainmod_md149.rs` containing three tests (one per ecosystem) that run `mikebom sbom scan` against existing per-ecosystem fixtures with the new flag set and assert the full US1 contract (root has operator identity, demoted entry exists with manifest PURL + library type + annotation).

### Implementation for User Story 2

- [ ] T017 [US2] Create `/Users/mlieberman/Projects/mikebom/mikebom-cli/tests/demote_manifest_mainmod_md149.rs` with a Cargo-ecosystem test `demote_cargo_main_module_emits_byte_identical_annotation_across_formats_md149`. Uses the existing `tests/fixtures/golden/cyclonedx/cargo.cdx.json` driver fixture (`cargo/lockfile-v3` per `tests/common/mod.rs:74`). Runs `mikebom sbom scan` three times (CDX, SPDX 2.3, SPDX 3) with `--root-name widget-svc --root-version 1.2.3 --preserve-manifest-main-module`. For each format: parse output, locate the demoted entry by manifest PURL, extract the `mikebom:demoted-from-main-module` annotation value, assert all three are `"true"` and bytewise-identical (SC-005 cross-format invariance proof). Pattern modeled on milestone-147's `peer_edge_targets_annotation_present_md147` style — extract-and-assert across three formats.

- [ ] T018 [P] [US2] Add test `demote_npm_main_module_emits_byte_identical_annotation_across_formats_md149` in the same file. Same shape; uses `npm/node-modules-walk` fixture (existing per `tests/common/mod.rs:79`). Demoted entry's PURL prefix MUST be `pkg:npm/`.

- [ ] T019 [P] [US2] Add test `demote_go_main_module_emits_byte_identical_annotation_across_formats_md149` in the same file. Same shape; uses `go/simple-module` fixture (existing per `tests/common/mod.rs:78`). Demoted entry's PURL prefix MUST be `pkg:golang/`.

**Checkpoint**: After Phase 4, the cross-ecosystem coverage from research §C is empirically validated. `cargo +stable test --test demote_manifest_mainmod_md149` shows 3 tests pass green; the new C102 parity-catalog row's `Directionality::SymmetricEqual` invariant gets exercised by the existing `cross_format_byte_identity` + `holistic_parity` tests automatically.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Documentation update, golden audit + refresh, pre-PR gate, commit chain.

- [ ] T020 [P] Update `/Users/mlieberman/Projects/mikebom/docs/reference/identifiers.md` to add a new section documenting `--preserve-manifest-main-module` + its interaction with milestone-077's override flags. Covers SC-007. Per spec Out of Scope §1, the section must explicitly state that the flag is OPT-IN and the milestone-077 clean-replacement default is unchanged. Cross-reference the issue #151 GitHub link.

- [ ] T021 Audit existing byte-identity goldens for `mikebom:component-role` / `mikebom:demoted-from-main-module` drift potential. Run:

  ```bash
  grep -rl 'mikebom:component-role' \
      /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/
  grep -rl 'mikebom:demoted-from-main-module' \
      /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/golden/
  ```

  Per spec SC-002 + SC-003, the EXISTING goldens (default-mode scans without the new flag) MUST be byte-identical pre/post 149. The new flag is opt-in; the existing harness doesn't pass it. So goldens should be unchanged. Document the audit finding (expected: zero drift on existing goldens) for the PR description.

- [ ] T022 Refresh affected goldens via the standard env-var trifecta IF the integration tests from T017-T019 use the existing golden-update harness OR if the new C102 parity-catalog row registration forces parity-test goldens to refresh:

  ```bash
  MIKEBOM_UPDATE_CDX_GOLDENS=1   cargo test --test cdx_regression
  MIKEBOM_UPDATE_SPDX_GOLDENS=1  cargo test --test spdx_regression
  MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test --test spdx3_regression
  ```

  Inspect `git diff --stat -- mikebom-cli/tests/fixtures/golden/`. Per FR-010 + spec SC-002 + SC-003, the existing default-mode goldens MUST NOT drift; any drift is a sign of regression and the PR should NOT proceed until the cause is understood. Reject any unrelated drift. If empty diff, that's the expected outcome — the new behavior fires ONLY when the new flag is set.

- [ ] T023 Run mandatory pre-PR gate per Constitution Development Workflow + memory `feedback_prepr_gate_full_output.md`: `./scripts/pre-pr.sh` from repo root. Both clippy + test steps MUST pass clean (excepting the pre-existing local `sbomqs_parity` env-only failure documented in milestone-144 T001 — CI will validate on a clean runner). If any OTHER test fails, scan the FULL output (do NOT grep on `^test result: FAILED` — known to drop multi-test-suite summaries). Covers SC-006.

- [ ] T024 Commit the milestone-149 changes. Per project convention (matching milestones 134/144/145/146/147/148), use the 4-commit chain:
  - `spec(149): preserve manifest-derived main-module as demoted library entry (closes #151)` — spec.md + checklists/requirements.md
  - `plan(149): apply_main_module_drop_or_demote helper + CLI flag + parity-bridging design` — plan + research + data-model + contracts + quickstart + CLAUDE.md
  - `tasks(149): 24 tasks across 5 phases for preserve-manifest-main-module opt-in` — tasks.md
  - `impl(149): --preserve-manifest-main-module CLI flag + mikebom:demoted-from-main-module annotation` — `mikebom-cli/src/generate/root_selector.rs` + the three refactored emitter files + `cli/scan_cmd.rs` + `generate/mod.rs` + four parity-extractor files + two docs files + new integration test file + any golden refresh from T022

  Do NOT commit until T023 passes clean. Use `git add <specific paths>` (never `-A`). Each commit ends with the standard `Co-Authored-By` trailer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. Verifies baseline.
- **Phase 2 (Foundational)**: EMPTY — no foundational work required.
- **Phase 3 (US1)**: Depends on Phase 1. T002 (struct) + T003 (helper Paths 1+2) lay the foundation; T004-T006 [P] refactor the three emitters to call the helper (the refactor checkpoint after T006 is the load-bearing structural change); T007 wires the CLI flag through ScanArgs → ScanRequest → ScanArtifacts; T008 adds the Path 3 demote branch; T009-T014 add unit tests; T015 adds the parity-catalog row; T016 adds the docs row. Sequential except for the [P]-marked subsets.
- **Phase 4 (US2)**: Depends on Phase 3 — specifically T008 (the demote branch) and T015 (the parity-catalog row). T017 establishes the integration-test file scaffolding; T018 + T019 add the npm + Go cases in parallel.
- **Phase 5 (Polish)**: Depends on US1 + US2 being functionally complete.

### User Story Dependencies

- **US1 (P1, MVP)**: Standalone after Phase 1. Delivers the entire feature (helper + flag + demote + unit tests + parity row + docs). T002 + T003 + T004-T006 (refactor checkpoint) + T007 + T008 + T009-T014 (6 unit tests) + T015 + T016.
- **US2 (P2)**: Depends on US1 being complete (specifically T008 + T015). Adds three cross-ecosystem integration tests as additional defensive coverage on the same helper.

### Within Each User Story

- T002 → T003 are sequential (struct must be defined before helper uses it).
- T004 / T005 / T006 are sequential-after-T003 BUT parallel-with-each-other (three different files; same call pattern).
- T007 is sequential-after-T004-T006 (the CLI flag must be wired through `ScanArtifacts` so the three emitter call sites have something to pass; the refactor at T004-T006 hardcoded `false`, T007 replaces with the real value).
- T008 is sequential-after-T003 (extends the helper with the new branch) AND should land after T007 (so the demote branch can be exercised end-to-end via the CLI flag).
- T009-T014 are tests and can land in parallel with each other after T008 (all in the same `mod tests` block in `root_selector.rs`).
- T015 is parallel-with-anything-in-US1 after T008 (touches different files: 4 parity-extractor files).
- T016 is parallel-with-anything-in-US1 (touches a docs file).
- T017 / T018 / T019 are tests; T017 establishes the test file, T018 + T019 add cases (parallel with each other once T017's scaffolding lands).

### Parallel Opportunities

- Phase 3: T004 + T005 + T006 [P] (three emitter files; same call pattern).
- Phase 3: T009 + T010 + T011 + T012 + T013 + T014 [P] (six unit tests, same `mod tests` block — landable in one editor pass).
- Phase 3: T015 + T016 [P] (different files — parity catalog vs docs).
- Phase 4: T018 + T019 [P] (two integration tests in the same file after T017's scaffolding).

---

## Parallel Example: Phase 3 (after T003 lands)

```bash
# Three emitter refactors can be done in one editor pass:
Task T004: cyclonedx/builder.rs        (replace lines 325-347 with helper call)
Task T005: spdx/document.rs            (replace lines 262-282 with helper call)
Task T006: spdx/v3_document.rs         (replace lines 57-75 with helper call)

# Refactor checkpoint: cargo test --workspace, confirm zero regression.
# Then T007 (CLI flag wiring), T008 (demote branch).

# After T008, six unit tests can be added in one editor pass:
Task T009: apply_drop_or_demote_no_override_is_passthrough_md149                                 (FR-007 + SC-003)
Task T010: apply_drop_or_demote_override_no_preserve_drops_main_module_md149                     (FR-007 + SC-002)
Task T011: apply_drop_or_demote_override_with_preserve_demotes_main_module_md149                 (FR-001 + FR-004 + US1 Option A)
Task T012: apply_drop_or_demote_demote_preserves_other_fields_md149                              (FR-005)
Task T013: apply_drop_or_demote_multi_main_module_with_preserve_is_noop_md149                    (Edge Case 4 + FR-013)
Task T014: apply_drop_or_demote_demoted_entry_has_no_outbound_relationships_in_helper_input_md149 (US1 Option A regression guard)

# Plus in parallel:
Task T015: parity-catalog row C102 + 3 sibling extractors
Task T016: docs/reference/sbom-format-mapping.md row update
```

## Parallel Example: Phase 4 (after T017's scaffolding lands)

```bash
Task T018: demote_npm_main_module_emits_byte_identical_annotation_across_formats_md149
Task T019: demote_go_main_module_emits_byte_identical_annotation_across_formats_md149
```

---

## Implementation Strategy

### MVP First (US1 only — ships the feature end-to-end)

1. Complete Phase 1: T001 baseline check.
2. Complete Phase 3:
   - T002 (struct) + T003 (helper Paths 1+2)
   - T004 + T005 + T006 (refactor three emitters) — **refactor checkpoint here, validate zero regression**
   - T007 (CLI flag wiring)
   - T008 (demote branch)
   - T009-T014 (six unit tests)
   - T015 (parity-catalog row)
   - T016 (docs row)
3. **STOP and VALIDATE**: `./scripts/pre-pr.sh` clean. Manual smoke per quickstart §Scenario 1: scan a Cargo fixture with `--preserve-manifest-main-module` and observe the demote behavior.
4. This is a shippable PR. The cross-ecosystem coverage from US2 is additive defensive validation; skipping it leaves the feature correct but with a smaller integration-test surface.

### Incremental / Recommended (single-PR delivery)

1. Phase 1 (T001) baseline.
2. Phase 3 (T002-T016) US1 — feature complete.
3. Phase 4 (T017-T019) US2 — cross-ecosystem integration tests.
4. Phase 5 (T020-T024) polish — docs + golden audit + pre-PR + commit.

Total: 24 tasks. Estimated ~40 LOC helper + ~10 LOC CLI flag wiring + ~150 LOC unit tests + ~80 LOC integration test + ~25 LOC removed from each of 3 emitters (consolidated into helper call) + ~15 LOC parity-catalog row + small docs updates.

### Single-developer Note

This milestone is small enough that one developer can work through all phases in one session. The [P] markers exist primarily to signal "no cross-file write conflict" — useful for tooling that automates task execution but not load-bearing for a human implementer. The refactor checkpoint after T006 is the most important review boundary: it lands a structurally-clean drop-logic consolidation BEFORE adding any new behavior, so a reviewer can validate the refactor in isolation.

---

## Notes

- Unit tests live in-file under `#[cfg(test)] mod tests` per the project's existing convention in `root_selector.rs`. Integration tests live in `mikebom-cli/tests/demote_manifest_mainmod_md149.rs` per the project's convention for cross-format byte-equality tests.
- The `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention applies to any test module per Constitution Principle IV (the existing `root_selector.rs#mod tests` already has it; no change needed). The new integration test file MUST add the same guard at its `mod common` boundary.
- Memory `feedback_prepr_gate_full_output.md` is directly relevant: when verifying T023, scan the FULL output rather than greping on `^test result: FAILED`.
- Memory `feedback_dont_dismiss_test_failures.md` is relevant if any new test failures surface during golden refresh: verify reproducibility before calling anything "pre-existing flake".
- The commit-message convention (T024) follows the milestone-134/144/145/146/147/148 precedent: `spec(149):` / `plan(149):` / `tasks(149):` / `impl(149):`.
- Per spec SC-007 (operator-cadence cross-ecosystem verification): document in the PR description that the operator should run the pip / gem / Maven cases manually post-merge to confirm cross-ecosystem coverage per Assumption 8. The harness is NOT a CI gate; the CI integration tests T017-T019 cover Cargo + npm + Go.
- Per spec FR-008 + plan.md Constitution V audit: the new `mikebom:demoted-from-main-module` annotation IS a parity-bridge per Constitution V. The audit table in the spec enumerates 6 rejected native-field alternatives across CDX 1.6 / SPDX 2.3 / SPDX 3. T016 enshrines it in `docs/reference/sbom-format-mapping.md`.
- The refactor at T004-T006 is structurally important — it consolidates ~75 LOC of duplicated drop logic into one ~40 LOC helper. Even without the new preserve branch, this refactor IS a worthwhile cleanup; the milestone-149 feature is just the natural opportunity to land it.
