---

description: "Task list for m200 — Cargo Workspace-Root [package] Runtime Classification"
---

# Tasks: Cargo Workspace-Root [package] Runtime Classification

**Input**: Design documents from `/specs/200-cargo-workspace-root-scope/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md (no `contracts/` — internal classifier fix, no wire-format contract change)

**Tests**: Included — every FR requires an executable regression assertion (m194-m199 precedent). FR-006 explicitly mandates a new fixture + integration test.

**Organization**: Two P1 stories — US1 (workspace-root Runtime) and US2 (non-root regression guard). US2 has no implementation work of its own; it's a validation that US1's change didn't over-reach. US2 tasks live in the polish phase since they gate on US1 completion.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file / no dependency on incomplete task
- **[Story]**: US1 (workspace-root fix) — US2 is a validation phase, no story label needed

## Path Conventions

- Rust workspace: `mikebom-cli/src/`, `mikebom-cli/tests/`, `mikebom-cli/tests/fixtures/`
- Absolute paths in every task per plan.md structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Baseline pins + reconnaissance re-verification. No code changes.

- [X] T001 Verify pre-m200 baseline pre-PR is green by running `./scripts/pre-pr.sh` on branch `200-cargo-workspace-root-scope` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m200-prepr-baseline.txt` for SC-006 delta measurement later.
- [X] T002 [P] Golden-drift baseline: run `find /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/ -name '*.json' | xargs grep -lE 'pkg:cargo/[^"]*"[[:space:]]*,?[[:space:]]*"scope"[[:space:]]*:[[:space:]]*"excluded"'` and record the hit list to `/tmp/m200-golden-audit.txt`. Expected: 0 hits per research.md R3. Non-empty hit list means the fix has larger regen scope than R3 predicted — investigate before starting Phase 3.
- [X] T003 [P] Fixture-shape reconnaissance: `grep -rl '^\[package\]' /Users/mlieberman/Projects/mikebom/mikebom-cli/tests/fixtures/*/*/Cargo.toml` — enumerate every cargo fixture with a root [package] block. Cross-check against `grep -rl '^\[workspace\]'` to identify any fixture with BOTH `[package]` + `[workspace]` at root (the m200 pattern). If any found, note in `/tmp/m200-existing-pattern-fixtures.txt` — those fixtures may need golden refresh even without new fixture creation.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None. This is a single-file classifier bug fix with no shared infrastructure to prep. Phase kept as a checkpoint header for consistency with the template — it's a no-op.

**Checkpoint**: Foundation ready (nothing to build). US1 implementation can start immediately after Phase 1.

---

## Phase 3: User Story 1 — Workspace-Root [package] Runtime Classification (Priority: P1) 🎯 MVP

**Goal**: Fix `cargo.rs::parse_cargo_toml` to seed `CargoTomlSections.prod_deps` with the workspace-root `[package].name` value, so the BFS prod-set closure includes workspace-root entries and they classify as Runtime instead of falling through to Development. Closes #585.

**Independent Test**: New fixture at `tests/fixtures/cargo/root_package_lifecycle/` with a workspace-root `[package]` + one member sub-crate. Scan → assert the root's PURL has CDX `scope: null` and NO `mikebom:lifecycle-scope: "development"` annotation. Separate assertion: no-override root-election picks the root, not the member.

### Tests for User Story 1

- [X] T004 [P] [US1] Create fixture directory `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` with:
  - `Cargo.toml` — workspace root declaring `[package] name = "app" version = "0.1.0" edition = "2021"` + `[workspace] members = ["helper"]` + `[dependencies] helper = { path = "helper" }`
  - `src/main.rs` — one-line stub `fn main() {}`
  - `helper/Cargo.toml` — `[package] name = "helper" version = "0.1.0" edition = "2021"`
  - `helper/src/lib.rs` — one-line stub `pub fn stub() {}`
  - `Cargo.lock` — resolved lockfile containing both `[[package]] name = "app"` and `[[package]] name = "helper"` entries with the correct `dependencies = ["helper 0.1.0"]` from app → helper.
  Generate the Cargo.lock via `cargo generate-lockfile --manifest-path <fixture>/Cargo.toml` before committing (documents-vs-authored: pin the generated Cargo.lock as the fixture, not a hand-written stub, so the reader sees a real lockfile shape).
- [X] T005 [P] [US1] Create integration test file `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs` with the `scan_path` helper pattern from `scan_npm.rs:378-411` (shells out to `env!("CARGO_BIN_EXE_mikebom")` with `--offline --path <fixture> --format cyclonedx-json --output <tempfile> --no-deep-hash`).
- [X] T006 [US1] Add test `scan_cargo_workspace_root_is_runtime_m200` to `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs`: scan the T004 fixture, assert (a) exactly one component with PURL starting `pkg:cargo/app@` (or `pkg:cargo/app@0.1.0` explicitly), (b) that component has `scope == null` (JSON null, not the string `"null"`), (c) that component's properties[] does NOT contain `mikebom:lifecycle-scope: "development"` (either the annotation is absent OR its value is `"runtime"`).
- [X] T007 [US1] Add test `scan_cargo_workspace_root_wins_root_election_m200` to the same file: scan the T004 fixture with NO operator overrides (no `--root-name`), assert `metadata.component.name == "app"` (not `"helper"`) AND `metadata.component.purl` starts with `"pkg:cargo/app@"`.

### Implementation for User Story 1

- [X] T008 [US1] Modify `mikebom-cli/src/scan_fs/package_db/cargo.rs::parse_cargo_toml` (line 721+): after the existing three `collect_section_keys` calls (dependencies / dev-dependencies / build-dependencies) and after `collect_optional_dep_keys`, add the FR-001 root-`[package].name` extract per research R1:
  ```rust
  // Milestone 200 (FR-001, closes #585): seed the prod-set BFS with the
  // workspace-root [package].name so it classifies as Runtime rather than
  // falling through to the Development branch at cargo.rs:1106.
  if let Some(root_name) = parsed
      .get("package")
      .and_then(|v| v.as_table())
      .and_then(|t| t.get("name"))
      .and_then(|v| v.as_str())
  {
      out.prod_deps.insert(root_name.to_string());
  }
  ```
  This is the entire code change for US1. ~10 LOC including the doc-comment.
- [X] T009 [US1] Add unit test `parse_cargo_toml_seeds_root_package_name_into_prod_deps` in `mikebom-cli/src/scan_fs/package_db/cargo.rs::tests` (alongside the existing `parse_cargo_toml_extracts_three_sections` test at line 2019+): assert that parsing a synthetic Cargo.toml with `[package] name = "myapp"` + `[dependencies] foo = "1.0"` produces `out.prod_deps.contains("myapp") == true` AND `out.prod_deps.contains("foo") == true`.
- [X] T010 [US1] Add unit test `parse_cargo_toml_virtual_workspace_omits_root_package_seed` in the same tests module: assert that parsing `[workspace]\nmembers = ["a", "b"]\n` (virtual workspace, no `[package]` block) produces `out.prod_deps.is_empty() == true` — the fix MUST NOT synthetic-seed anything when no `[package]` exists (FR-004 guardrail).
- [X] T010a [P] [US1] Add unit test `parse_cargo_toml_isolates_root_package_across_independent_workspaces` in the same tests module (FR-005 dedicated coverage): parse two synthetic Cargo.toml strings representing distinct workspace roots — `[package] name = "app-a"` and `[package] name = "app-b"` — via two separate `parse_cargo_toml` invocations. Assert (a) the first result's `prod_deps` contains `"app-a"` but NOT `"app-b"`, (b) the second result's `prod_deps` contains `"app-b"` but NOT `"app-a"`. This isolates the per-manifest boundary independent of any whole-scan fixture — closes the CG1 coverage gap identified in /speckit-analyze (FR-005: multi-workspace cross-seeding forbidden).

**Checkpoint**: US1 fully functional. Vaultwarden reproducer + new fixture both show correct Runtime classification. US2 validation can now start.

---

## Phase 4: User Story 2 — Non-Root Classification Regression Guard (Priority: P1)

**Goal**: Verify that no non-root cargo entry gets reclassified by the T008 seed change. Existing cargo integration tests must pass byte-identically. Existing goldens must show no drift beyond the intended narrow workspace-root scope change.

**Independent Test**: `cargo test -p mikebom --test transitive_parity_cargo` + `--test optional_dep_classification` + any other cargo integration test in `mikebom-cli/tests/` all pass green post-fix, byte-identical to pre-fix output.

### Validation Tasks for User Story 2

- [X] T011 [P] [US2] Run `cargo test -p mikebom --test transitive_parity_cargo --no-fail-fast 2>&1 | tail` post-T008. Expected: `ok. N passed; 0 failed`. Any failure indicates FR-003 violation (m083 audit fixture is a virtual workspace, so this SHOULD be a no-op verify).
- [X] T012 [P] [US2] Run `cargo test -p mikebom --test optional_dep_classification --no-fail-fast 2>&1 | tail`. Expected: `ok. N passed; 0 failed`. FR-003 verification: `criterion` and other dev-deps should retain their existing `Development` classification.
- [X] T013 [P] [US2] Grep post-fix: `git diff --stat mikebom-cli/tests/fixtures/`. Expected: only the new `cargo/root_package_lifecycle/` fixture directory in the stat output. If ANY existing golden JSON drifts, investigate immediately — that would be an unexpected FR-003 violation. Documented outcome recorded in PR body.
- [X] T014 [US2] Run every cargo-related integration test explicitly: `for t in transitive_parity_cargo optional_dep_classification produces_binaries_cargo scan_cargo cargo_workspace_root_lifecycle_m200; do cargo test -p mikebom --test $t 2>&1 | tail -3; done` (skip missing tests silently — some may not exist). Enumerate any binary that reports `passed; [1-9]` (non-zero failures) per feedback_prepr_gate_bails_on_first_failure memory.

**Checkpoint**: Both US1 (new-behavior guarantee) and US2 (no-regression guarantee) validated. Ready for polish.

---

## Phase 5: Cross-Cutting Golden Drift Re-Verification

**Purpose**: Follow the m199 empirical-verification lesson. Research R3 predicted 0 golden drifts, but that's an unverified claim until implement time. Explicitly re-audit post-implementation.

- [X] T015 [P] Re-run the T002 grep post-implementation on `git diff`: `git diff -- mikebom-cli/tests/fixtures/` — the ONLY changes should be the new `cargo/root_package_lifecycle/` fixture files (T004). Any existing golden JSON in the diff means unexpected drift.
- [X] T016 If T015 reveals drift on `mikebom-cli/tests/fixtures/public_corpus/rust-ripgrep/{cdx,spdx-2.3,spdx-3}.json`, plan a follow-up regen PR via `gh workflow run public-corpus.yml --field branch=main --field regen_goldens=true` after m200 merges (m196/m199 pattern). If drift is on any OTHER golden, HALT — that would be a genuine FR-003 regression requiring code investigation.

---

## Phase 6: Polish & Verification

- [X] T017 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 5 seconds per SC-006. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per feedback_prepr_gate_bails_on_first_failure memory).
- [X] T018 [P] Manually execute quickstart.md Reproducer 1 (vaultwarden reproducer) end-to-end against `/tmp/test-vaultwarden`. Confirm `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` and no `pkg:cargo/vaultwarden` in components[] (SC-001 + SC-002).
- [X] T019 [P] Verify SC-003 explicitly: run vaultwarden scan pre-fix (checkout main) and post-fix (checkout branch), diff `[.components[] | select(.scope == "excluded")] | length` — MUST decrease by ≥ 1.
- [X] T020 Draft PR body with `Closes #585` per SC-007. Include: (a) 1-paragraph summary of the classifier bug + fix, (b) research R3 empirical-verification outcome (T015 result), (c) code-diff LOC + files touched (~150 LOC, 3 files: cargo.rs + fixture dir + test file), (d) test coverage summary (2 new integration tests + 2 new unit tests), (e) vaultwarden reproducer before/after jq output.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies. T001 sequential; T002 + T003 parallel.
- **Phase 2 (Foundational)**: No-op — no tasks. Phase kept for template consistency.
- **Phase 3 (US1)**: Depends on Phase 1. Fixture (T004) + test-file scaffold (T005) parallel; T006 + T007 depend on T005 (same test file); T008 depends on T006/T007 (need failing tests first if following TDD); T009 + T010 depend on T008 (unit tests reference the modified `parse_cargo_toml`).
- **Phase 4 (US2)**: Depends on Phase 3 T008 completion (need the fix in place to validate no regression).
- **Phase 5 (Golden Drift Re-Verification)**: Depends on Phase 4 completion (validation tasks done, ready to inspect diff).
- **Phase 6 (Polish)**: Depends on Phase 5 completion.

### Within US1

- Fixture (T004) [P] + integration-test-file scaffold (T005) [P] → integration tests (T006 → T007 must be sequential in same file) → parse_cargo_toml edit (T008) → unit tests (T009 + T010 parallel).

### Within US2

- T011 + T012 + T013 all parallel (independent test binaries + independent grep).
- T014 sequential (enumeration loop over multiple test binaries).

### Parallel Opportunities

- **Phase 1**: T002 + T003 in parallel (independent audits).
- **Phase 3**: T004 + T005 in parallel (different files). T009 + T010 in parallel (different unit tests in same file — can be added as one batched Edit).
- **Phase 4**: T011 + T012 + T013 in parallel (independent test-binary invocations).
- **Phase 6**: T018 + T019 in parallel (independent verification steps).

---

## Parallel Example: Phase 3 Fixture + Test Scaffold

```bash
# Kick off both US1 setup pieces in parallel:
Task: "Create fixture at mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/"
Task: "Create integration test scaffold at mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs"
```

---

## Implementation Strategy

### MVP First (US1 Only)

1. Phase 1 (Setup) → baselines captured.
2. Phase 2 (Foundational) → no-op skip.
3. Phase 3 (US1) → fixture + tests + code edit + unit tests.
4. STOP + VALIDATE: T006/T007 pass, T009/T010 pass, `./scripts/pre-pr.sh` green.
5. Optional stopping point — US1 alone closes #585. US2 is a regression guarantee, not a separate deliverable.

### Full-Bundle Delivery (Preferred)

1. Phases 1 → 3 → 4 → 5 → 6 in order.
2. Single PR closes #585 with US1 + US2 validation in one merge.

---

## Notes

- [P] tasks = different files, no cross-dependency on incomplete task.
- Every FR has ≥1 executable test: FR-001 via T008 code + T009 unit; FR-002 via T006 integration assertion; FR-003 via T011/T012/T014 existing-test regression guard; FR-004 via T010 unit (virtual-workspace guardrail); FR-005 via T010a dedicated unit test (per-manifest boundary isolation) + T003 fixture-shape recon; FR-006 via T004 + T006 + T007; FR-007 via T017 wall-clock verify.
- Empirical R3 claim (0 pre-existing goldens require regen) is re-verified at implement time via T015.
- Zero new Cargo dependencies.
- Total ~150 LOC across 3 files (per plan.md scope estimate).
