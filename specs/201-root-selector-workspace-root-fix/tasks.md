---

description: "Task list for m201 — Root-Selector Workspace-Root Disambiguation"
---

# Tasks: Root-Selector Workspace-Root Disambiguation

**Input**: Design documents from `/specs/201-root-selector-workspace-root-fix/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md (no `contracts/` — internal classifier + internal-annotation-consumer fix)

**Tests**: Included — every FR requires an executable regression assertion (m194-m200 precedent).

**Organization**: Two P1 stories — US1 (correct root election via new positive-identifier signal) and US2 (regression guard). US2 has no implementation work of its own; it's a validation that US1's change didn't over-reach. US2 tasks live in the polish phase since they gate on US1 completion.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file / no dependency on incomplete task
- **[Story]**: US1 (workspace-root disambiguation) — US2 is a validation phase, no story label needed

## Path Conventions

- Rust workspace: `mikebom-cli/src/`, `mikebom-cli/tests/`, `mikebom-cli/tests/fixtures/`
- Absolute paths in every task per plan.md structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline pins + reconnaissance re-verification. No code changes.

- [X] T001 Verify pre-m201 baseline pre-PR is green by running `./scripts/pre-pr.sh` on branch `201-root-selector-workspace-root-fix` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m201-prepr-baseline.txt` for SC-006 delta measurement later.
- [X] T002 [P] Golden-drift baseline: run `git diff --stat main -- mikebom-cli/tests/fixtures/` (expected: only branch-local fixture changes if any) and record to `/tmp/m201-golden-baseline.txt`. Expected: zero pre-existing fixture drift on branch HEAD before implementation. Post-implementation this baseline gets re-compared for regression scope.
- [X] T003 [P] Confirm m200 landed on main: `git log --oneline main -- mikebom-cli/src/scan_fs/package_db/cargo.rs | head -5` MUST show the m200 commit (`impl(200): fix cargo workspace-root [package]...`). If not, halt — the m201 fix builds on m200's `is_workspace_root` stamping infrastructure.
- [X] T004 [P] Verify vaultwarden reproducer baseline: scan `/tmp/test-vaultwarden` with the pre-m201 binary (built at branch HEAD, pre-implementation) and confirm current behavior — `metadata.component.purl == "pkg:cargo/macros@0.1.0"`, heuristic reports `"ecosystem-priority"`. Record to `/tmp/m201-vaultwarden-baseline.json` for post-fix SC-001/SC-002/SC-003 delta comparison.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None. This is a positive-identifier signal addition with no shared infrastructure to prep. Phase kept as a checkpoint header for consistency with the template — it's a no-op.

**Checkpoint**: Foundation ready (nothing to build). US1 implementation can start immediately after Phase 1.

---

## Phase 3: User Story 1 — Workspace-Root Disambiguation via `mikebom:is-cargo-workspace-toplevel` (Priority: P1) 🎯 MVP

**Goal**: Add a new internal-only annotation stamped at cargo m064 emission time on the workspace-toplevel crate; wire it as a positive-identifier short-circuit at the `is_workspace_root` stamping consumer; filter it from emitted SBOMs. Closes #587.

**Independent Test**: Extended m200 fixture at `tests/fixtures/cargo/root_package_lifecycle/` gains a nested `sub/package.json` npm project (3 main-module candidates). Scan → assert `metadata.component.purl` starts with `pkg:cargo/app@` AND scan log heuristic is `"repo-root"`.

### Tests for User Story 1

- [X] T005 [P] [US1] Extend the m200 fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` by adding a nested npm project. Create exactly two new files: `sub/package.json` with content `{"name": "sub", "version": "1.0.0"}` (newline-terminated) AND `sub/index.js` with content `// m201 stub — nested npm project for the multi-ecosystem root-election reproducer (see specs/201-root-selector-workspace-root-fix/spec.md FR-005).` (newline-terminated). This introduces a 3rd main-module candidate (`pkg:npm/sub`) alongside the existing cargo-root `app` and cargo-member `helper`, reproducing the multi-ecosystem shape from #587.
- [X] T006 [US1] Add integration test `scan_cargo_workspace_root_wins_multi_ecosystem_m201` to `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs`: scan the T005-extended fixture, assert (a) `metadata.component.name == "app"` (not `"helper"` or `"sub"`), (b) `metadata.component.purl` starts with `"pkg:cargo/app@"`, (c) neither `app` nor `helper` appears in `components[]` with a duplicate PURL (metadata.component IS the sole `app` emission per m127 behavior).
- [X] T007 [US1] Verify the emitted SBOM does NOT contain the new internal annotation: assert `jq '..|.name? // empty' <sbom> | grep -c "mikebom:is-cargo-workspace-toplevel"` returns 0. Enforce FR-007 (new annotation is internal-emission-only, filtered by extended `is_internal_emission_key`). Add as an assertion within the T006 test or as a separate `scan_cargo_new_internal_annotation_is_filtered_from_output_m201` test.

### Implementation for User Story 1

- [X] T008 [US1] Modify `mikebom-cli/src/scan_fs/package_db/cargo.rs::build_cargo_main_module_entry` (line 363+): after successfully parsing `[package]` (existing line 369), check `parsed.get("workspace").is_some()`. When true, insert into the `extra_annotations` BTreeMap being built at line 374+:
  ```rust
  // Milestone 201 (FR-001, closes #587): stamp the workspace-toplevel
  // positive-identifier annotation. Consumed downstream by
  // scan_fs/mod.rs to distinguish workspace-ROOT crates from
  // workspace-MEMBER crates when both share the workspace Cargo.lock
  // path in evidence.source_file_paths. Internal-emission-only:
  // filtered out of CDX/SPDX output via is_internal_emission_key at
  // root_selector.rs.
  if parsed.get("workspace").is_some() {
      extra_annotations.insert(
          "mikebom:is-cargo-workspace-toplevel".to_string(),
          serde_json::Value::Bool(true),
      );
  }
  ```
- [X] T009 [US1] Modify `mikebom-cli/src/scan_fs/mod.rs` at the `is_workspace_root` stamping site (line 922-942): BEFORE the existing filesystem-based comparison, check for the new annotation on the component. When present + true, short-circuit `is_workspace_root = true`:
  ```rust
  // Milestone 201 (FR-001, closes #587): positive-identifier
  // short-circuit for cargo workspace-toplevel crates. When the cargo
  // reader stamped `mikebom:is-cargo-workspace-toplevel: true` (Cargo.toml
  // has both [package] AND [workspace] blocks), skip the filesystem-based
  // check below (which cannot distinguish workspace-root from workspace-
  // member cargo crates because m064 augmented main-modules share the
  // workspace Cargo.lock path in evidence.source_file_paths).
  let is_cargo_workspace_toplevel = c
      .extra_annotations
      .get("mikebom:is-cargo-workspace-toplevel")
      .and_then(|v| v.as_bool())
      .unwrap_or(false);

  let is_workspace_root = if is_cargo_workspace_toplevel {
      true
  } else {
      // Existing filesystem-based logic (unchanged).
      match (manifest_path, canonical_root.as_ref()) {
          (Some(p), Some(canon_root)) => { /* existing code */ }
          _ => false,
      }
  };
  ```
- [X] T010 [US1] Modify `mikebom-cli/src/generate/root_selector.rs::is_internal_emission_key` (line 437-439): extend the filter to include the new annotation key. Change from single-`==` to `matches!` for readability:
  ```rust
  pub fn is_internal_emission_key(key: &str) -> bool {
      matches!(
          key,
          IS_WORKSPACE_ROOT_KEY | "mikebom:is-cargo-workspace-toplevel"
      )
  }
  ```
- [X] T011 [P] [US1] Add unit test `build_cargo_main_module_entry_stamps_workspace_toplevel_annotation_m201` in `mikebom-cli/src/scan_fs/package_db/cargo.rs::tests` (alongside the existing m200-era tests at line 2500+): construct a synthetic Cargo.toml with BOTH `[package] name = "app"` AND `[workspace] members = ["helper"]`, call `build_cargo_main_module_entry`, assert the returned entry's `extra_annotations` contains `mikebom:is-cargo-workspace-toplevel: true`.
- [X] T012 [P] [US1] Add unit test `build_cargo_main_module_entry_omits_workspace_toplevel_for_member_crate_m201` in the same tests module: construct a synthetic Cargo.toml with `[package] name = "helper"` ONLY (no `[workspace]` block), call `build_cargo_main_module_entry`, assert the returned entry's `extra_annotations` does NOT contain `mikebom:is-cargo-workspace-toplevel`.
- [X] T013 [P] [US1] Add unit test `build_cargo_main_module_entry_omits_workspace_toplevel_for_single_crate_m201` in the same tests module: construct a synthetic Cargo.toml with `[package] name = "single"` and no `[workspace]` (a standalone single-crate project), call `build_cargo_main_module_entry`, assert the annotation is ABSENT. The existing filesystem-based logic at `scan_fs/mod.rs` correctly handles single-crate correctness via the fallback branch — this test documents that m201 doesn't over-stamp.
- [X] T014 [P] [US1] Add unit test `is_internal_emission_key_filters_workspace_toplevel_annotation_m201` in `mikebom-cli/src/generate/root_selector.rs::tests` (alongside the existing tests at line 640+): call `is_internal_emission_key("mikebom:is-cargo-workspace-toplevel")` and assert `true`. Complement: `is_internal_emission_key("mikebom:some-other-annotation")` returns `false` (guardrail that the filter didn't over-broaden).
- [X] T014a [P] [US1] Add unit test `is_workspace_root_falls_back_to_filesystem_check_for_non_cargo_m201` in `mikebom-cli/src/scan_fs/mod.rs::tests` (or the closest existing tests module — search for the existing `is_workspace_root` stamping tests): construct a synthetic `ResolvedComponent` with `mikebom:component-role: "main-module"`, an npm PURL like `pkg:npm/foo@1.0.0`, `evidence.source_file_paths = ["package.json"]` (rootfs-relative), and NO `mikebom:is-cargo-workspace-toplevel` annotation. Set `canonical_root` to the manifest's parent directory. Invoke the stamping helper. Assert the component ends up with `mikebom:is-workspace-root == true` — the existing filesystem-based fallback path fires unchanged (FR-003 explicit coverage; closes CG1 identified in /speckit-analyze).

**Checkpoint**: US1 fully functional. Vaultwarden reproducer + extended m200 fixture both show correct root election. US2 validation can now start.

---

## Phase 4: User Story 2 — Non-Vaultwarden-Shape Regression Guard (Priority: P1)

**Goal**: Verify that no non-vaultwarden-shape scan gets a different root-election outcome post-m201. Existing cargo integration tests must pass byte-identically. Existing goldens' `metadata.component.purl` values MUST hold.

**Independent Test**: Every existing cargo integration test (`transitive_parity_cargo`, `optional_dep_classification`, `produces_binaries_cargo`, `scan_cargo`, `cargo_workspace_root_lifecycle_m200`) passes without modification.

### Validation Tasks for User Story 2

- [X] T015 [P] [US2] Run `cargo test --manifest-path mikebom-cli/Cargo.toml --test transitive_parity_cargo --no-fail-fast 2>&1 | tail` post-T008/T009/T010. Expected: `ok. N passed; 0 failed`. FR-004 verification: existing cargo m083 audit fixture (clap workspace) root election unchanged.
- [X] T016 [P] [US2] Run `cargo test --manifest-path mikebom-cli/Cargo.toml --test optional_dep_classification --no-fail-fast 2>&1 | tail`. Expected: `ok. N passed; 0 failed`. FR-004 verification: single-crate optional-dep fixture unchanged.
- [X] T017 [P] [US2] Run `cargo test --manifest-path mikebom-cli/Cargo.toml --test produces_binaries_cargo --no-fail-fast 2>&1 | tail`. Expected: `ok. N passed; 0 failed`. FR-004 verification: virtual workspace + single-crate + library-only fixtures unchanged.
- [X] T018 [P] [US2] Run `cargo test --manifest-path mikebom-cli/Cargo.toml --test scan_cargo --no-fail-fast 2>&1 | tail`. Expected: `ok. N passed; 0 failed`. FR-004 verification: general cargo integration tests unchanged.
- [X] T019 [P] [US2] Run `cargo test --manifest-path mikebom-cli/Cargo.toml --test cargo_workspace_root_lifecycle_m200 --no-fail-fast 2>&1 | tail`. Expected: `ok. N passed; 0 failed` (existing m200 tests still pass; new T006 test also passes). FR-004 + m201 US1 verification combined.
- [X] T020 [US2] Grep post-fix: `git diff --stat mikebom-cli/tests/fixtures/`. Expected: only the new `sub/package.json` (+ optionally `sub/index.js`) in the extended `root_package_lifecycle/` fixture. If ANY existing golden JSON drifts, investigate immediately — that would be an unexpected FR-004 violation. Documented outcome recorded in PR body.

**Checkpoint**: Both US1 (new-behavior guarantee) and US2 (no-regression guarantee) validated. Ready for polish.

---

## Phase 5: Cross-Cutting Golden Drift Re-Verification

**Purpose**: Follow the m199/m200 empirical-verification lesson. Research R4 predicted 0 golden drifts, but that's an unverified claim until implement time. Explicitly re-audit post-implementation.

- [X] T021 [P] Re-run T002 audit post-implementation: `git diff --stat mikebom-cli/tests/fixtures/` — the ONLY changes should be the new `sub/package.json` (+ optional `sub/index.js`) files. Any existing golden JSON in the diff means unexpected drift.
- [X] T022 If T021 reveals drift on `mikebom-cli/tests/fixtures/public_corpus/rust-ripgrep/{cdx,spdx-2.3,spdx-3}.json` OR any other public-corpus golden, plan a follow-up regen PR via `gh workflow run public-corpus.yml --field branch=main --field regen_goldens=true` after m201 merges (m196/m199/m200 pattern). If drift is on any NON-public-corpus golden, HALT — that would be a genuine FR-004 regression requiring code investigation.

---

## Phase 6: Polish & Verification

- [X] T023 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 5 seconds per SC-006. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per feedback_prepr_gate_bails_on_first_failure memory).
- [X] T024 [P] Manually execute quickstart.md Reproducer 1 (vaultwarden reproducer) end-to-end against `/tmp/test-vaultwarden`. Confirm (a) `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` (SC-001), (b) no `pkg:cargo/vaultwarden` in `components[]` (SC-002 implicit — component moved to metadata.component), (c) scan-log heuristic reports `"repo-root"` with confidence 0.90 (SC-002 explicit).
- [X] T025 [P] Verify SC-003 explicitly: for the same vaultwarden scan, jq the losers list from the scan-log (or from a separate mikebom scan run with `--log-format json` if available). Assert `losers` contains `"pkg:cargo/macros@0.1.0"` AND `"pkg:npm/scenarios@1.0.0"`.
- [X] T026 Draft PR body with `Closes #587` per SC-007. Include: (a) 1-paragraph summary of the disambiguation mechanism (new internal-only annotation), (b) research R4 empirical-verification outcome (T021 result), (c) code-diff LOC + files touched (~120 LOC, 3 source + 1 fixture + 1 test), (d) test coverage summary (1 new integration test + 4 new unit tests), (e) vaultwarden reproducer before/after scan-log output (heuristic transition `"ecosystem-priority"` → `"repo-root"`).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. T001 sequential; T002 + T003 + T004 parallel.
- **Phase 2 (Foundational)**: No-op — no tasks.
- **Phase 3 (US1)**: Depends on Phase 1. Fixture extension (T005) [P]; integration test T006 depends on T005; T007 depends on T006 (same test file, may be one batched Edit). Code edits T008 → T009 → T010 sequential (each layers on the previous or on a shared assumption). Unit tests T011-T014 depend on T008/T010 but are independent of each other → parallel.
- **Phase 4 (US2)**: Depends on Phase 3 T008/T009/T010 completion.
- **Phase 5 (Golden Drift Re-Verification)**: Depends on Phase 4 completion.
- **Phase 6 (Polish)**: Depends on Phase 5 completion.

### Within US1

- Fixture (T005) [P] → integration tests (T006 → T007 sequential in same file) → code edits (T008 → T009 → T010) → unit tests (T011 + T012 + T013 + T014 parallel).

### Within US2

- T015-T019 all parallel (independent test binaries).
- T020 sequential after all above (needs post-code-edit filesystem state).

### Parallel Opportunities

- **Phase 1**: T002 + T003 + T004 in parallel (independent audits).
- **Phase 3 fixture + tests**: T005 in parallel with reading existing test file. T011-T014 parallel unit tests.
- **Phase 4**: T015-T019 all parallel (5 independent test-binary invocations).
- **Phase 5**: T021 standalone; T022 conditional.
- **Phase 6**: T024 + T025 parallel (independent verification steps).

---

## Parallel Example: Phase 4 Regression Guard

```bash
# Kick off all 5 US2 regression checks in parallel:
Task: "cargo test transitive_parity_cargo"
Task: "cargo test optional_dep_classification"
Task: "cargo test produces_binaries_cargo"
Task: "cargo test scan_cargo"
Task: "cargo test cargo_workspace_root_lifecycle_m200"
```

---

## Implementation Strategy

### MVP First (US1 Only)

1. Phase 1 (Setup) → baselines captured.
2. Phase 2 (Foundational) → no-op skip.
3. Phase 3 (US1) → fixture + tests + 3 code edits + 4 unit tests.
4. STOP + VALIDATE: T006/T007 pass, T011-T014 pass, quickstart Reproducer 1 shows vaultwarden wins root election.
5. Optional stopping point — US1 alone closes #587. US2 is a regression guarantee, not a separate deliverable.

### Full-Bundle Delivery (Preferred)

1. Phases 1 → 3 → 4 → 5 → 6 in order.
2. Single PR closes #587 with US1 + US2 validation in one merge.

---

## Notes

- [P] tasks = different files, no cross-dependency on incomplete task.
- Every FR has ≥1 executable test: FR-001 via T008 code + T011/T012/T013 unit; FR-002 via T009 code + T006 integration; FR-003 via T014a dedicated unit test (non-cargo fallback path) + T009 code + T015-T019 implicit suite coverage; FR-004 via T015-T020 regression guard; FR-005 via T005 + T006; FR-006 via T023 wall-clock; FR-007 via T007 integration + T014 unit + T010 code.
- Empirical R4 claim (0 pre-existing goldens require regen) is re-verified at implement time via T020 + T021.
- Zero new Cargo dependencies.
- Zero new user-facing `mikebom:*` annotations (T010 filters the new annotation from emission).
- Total ~120 LOC across 3 source files + 1 fixture + 1 test file (per plan.md scope estimate).
