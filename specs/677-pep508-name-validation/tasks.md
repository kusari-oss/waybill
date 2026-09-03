---

description: "Task list for feature 677: Reject phantom pip components with malformed names (PEP 508 validation)"
---

# Tasks: Reject phantom pip components with malformed names (PEP 508 validation)

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/677-pep508-name-validation/`
**Prerequisites**: `plan.md` (required), `spec.md` (required), `research.md`, `data-model.md`, `contracts/name-validation-module.md`, `quickstart.md`

**Tests**: 14 unit tests inside `name_validation.rs` land under Phase 2 (Foundational) per `contracts/name-validation-module.md`'s testing contract. 1 integration test lands under Phase 3 (US1). Test tasks are called out inline where they belong.

**Organization**: Phase 2 delivers the reusable helper module. Phase 3 (US1 MVP) wires it to the pip reader + adds fixture + integration test. Phases 4-5 (US2/US3) are verification-only — US1 tasks already deliver the extension pattern and transparency contract; US2/US3 phases confirm coverage.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Every task includes exact file paths from the repo root at `/Users/mlieberman/Projects/mikebom/`.

## Path Conventions

Single-project layout — all changes under `waybill-cli/`. New module at `waybill-cli/src/scan_fs/package_db/name_validation.rs`. Filter integration in `waybill-cli/src/scan_fs/package_db/pip/mod.rs`. Fixture + integration test under `waybill-cli/tests/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify the workspace tree is in a clean state ready for the fix.

- [X] T001 Verify current branch is `677-pep508-name-validation` and working tree is clean via `git status --short` (empty output). Confirm `Cargo.toml` workspace version reads `0.6.0` — this feature branches from post-v0.6.0 `main`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Create the `name_validation` module that ALL user stories depend on. Ships with 14 unit tests per the contract.

**⚠️ CRITICAL**: US1, US2, US3 all depend on this phase completing. The module is the "reusable helper" mandated by FR-004.

- [X] T002 Create new file `waybill-cli/src/scan_fs/package_db/name_validation.rs` with a doc comment header referencing `specs/677-pep508-name-validation/contracts/name-validation-module.md`. Empty skeleton with three `pub(crate)` items marked `todo!()`: (a) `enum NameValidationError { Empty, Malformed { reason: String } }`, (b) `fn is_pep508_name(name: &str) -> bool`, (c) `fn validate_pep508_name(name: &str) -> Result<(), NameValidationError>`.
- [X] T003 Add `impl std::fmt::Display for NameValidationError` in `waybill-cli/src/scan_fs/package_db/name_validation.rs`. Empty → `"name is empty or whitespace-only"`; Malformed → `format!("name malformed: {reason}")`.
- [X] T004 Implement `is_pep508_name` in `waybill-cli/src/scan_fs/package_db/name_validation.rs` per `data-model.md` §Entity 2. Use `std::sync::OnceLock<regex::Regex>` to compile-once the regex `^[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?$`. Return `re.is_match(name)`.
- [X] T005 Implement `validate_pep508_name` in `waybill-cli/src/scan_fs/package_db/name_validation.rs` per `data-model.md` §Entity 3 + `contracts/name-validation-module.md` "Semantic contract" section. Order-of-checks: (1) trim empty → `Err(Empty)`; (2) first char non-alphanumeric → `Err(Malformed { reason: "must start with alphanumeric character".to_string() })`; (3) last char non-alphanumeric → `Err(Malformed { reason: "must end with alphanumeric character".to_string() })`; (4) `is_pep508_name` returns false → `Err(Malformed { reason: "contains invalid character(s); allowed: A-Z a-z 0-9 . - _".to_string() })`; (5) otherwise `Ok(())`.
- [X] T006 Add unit tests in `waybill-cli/src/scan_fs/package_db/name_validation.rs` under `#[cfg(test)] mod tests { ... }` with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. Cover the 14-row testing table from `contracts/name-validation-module.md`: (1) `django` accepts, (2) `Django` accepts, (3) `PyYAML` accepts, (4) `my-pkg` accepts, (5) `my.pkg` accepts, (6) `my_pkg` accepts, (7) `zope.interface` accepts, (8) `""` → `Empty`, (9) `"   "` → `Empty`, (10) `"{{package-name}}"` → `Malformed(must start with alphanumeric)`, (11) `".pkg"` → `Malformed(must start with alphanumeric)`, (12) `"pkg-"` → `Malformed(must end with alphanumeric)`, (13) `"pkg@2"` → `Malformed(contains invalid character(s)...)`, (14) `"pkg name"` → `Malformed(contains invalid character(s)...)`.
- [X] T007 Register the module in `waybill-cli/src/scan_fs/package_db/mod.rs` by adding `pub(crate) mod name_validation;` alongside the other module declarations. Grep-verify existing `pub mod pip;` etc. to match declaration style.
- [X] T008 Compile + run unit tests: `cargo test -p waybill --bin waybill scan_fs::package_db::name_validation`. All 14 tests MUST pass. Fix any compile errors surfaced (unlikely — types are inference-friendly).

**Checkpoint**: Phase 2 complete → foundation ready. The `name_validation` module is the reusable helper per FR-004; future readers add sibling `is_<ecosystem>_name` functions here.

---

## Phase 3: User Story 1 — SBOM operator scanning a monorepo with a project template (Priority: P1) 🎯 MVP

**Goal**: Wire PEP 508 validation into the pip reader as a pre-filter over `project_roots`. Add fixture + integration test. Zero phantom components emit from a Cookiecutter-shape `pyproject.toml`; exactly one WARN log names the offending path + name.

**Independent Test**: Scan the new `malformed_name_placeholder` fixture; assert (a) zero `pkg:pypi/*` components, (b) exactly one WARN log line with structured fields, (c) `names_rejected=1` in the reader-complete log.

### Implementation for User Story 1

- [X] T009 [US1] Add `use super::name_validation::{validate_pep508_name, NameValidationError};` at the top of `waybill-cli/src/scan_fs/package_db/pip/mod.rs` alongside existing `use super::...` imports.
- [X] T010 [US1] Add `fn filter_project_roots_by_name(project_roots: &[PathBuf]) -> (Vec<PathBuf>, usize)` in `waybill-cli/src/scan_fs/package_db/pip/mod.rs` per `data-model.md` §Entity 4. Iterate each root: read `pyproject.toml` (silent skip on IO error / TOML-parse-fail — existing convention), extract effective name using SAME extraction logic as `build_pip_main_module_entry` at lines 644-651 (`[project].name` first, `[tool.poetry].name` fallback, otherwise skip validation and retain the root). On name present, call `validate_pep508_name(name)`. On `Err`, emit `tracing::warn!(manifest = %manifest_path.display(), name = %name, reason = %err, "pip: pyproject.toml [project].name failed PEP 508 validation; skipping whole manifest")` and increment counter. On `Ok`, retain the root. Return `(retained: Vec<PathBuf>, rejected_count: usize)`.
- [X] T011 [US1] Wire the filter pass into `read()` at `waybill-cli/src/scan_fs/package_db/pip/mod.rs`. Insert immediately BEFORE the `pyproject_declared_deps` loop (currently around line 343, look for `for project_root in &project_roots { let manifest_entries = pyproject_declared_deps(project_root);`). Add: `let (project_roots, names_rejected) = filter_project_roots_by_name(&project_roots);`. This rebinds `project_roots` so both downstream loops (line ~344 for `pyproject_declared_deps` and line ~388 for `build_pip_main_module_entry`) iterate the filtered set with zero further modification.
- [X] T012 [US1] Extend the reader-complete `tracing::info!` log at `waybill-cli/src/scan_fs/package_db/pip/mod.rs` (locate via grep `pip reader complete`). Add `names_rejected,` as a new structured field alongside `main_modules_emitted, poetry_skips, ...`. Byte-identity guarantee preserved for pre-fix passing fixtures because `names_rejected=0` in those cases and the field-addition is additive to the log line.
- [X] T013 [US1] Compile-check the crate: `cargo build -p waybill 2>&1 | tail -5`. Must complete without errors or unused-import warnings.
- [X] T014 [US1] Run existing pip tests to verify FR-006 non-regression: `cargo test -p waybill --bin waybill scan_fs::package_db::pip` (existing in-file unit tests) + `cargo test --test scan_python_m670` (integration tests). All must pass unchanged.
- [X] T015 [US1] Create fixture directory + `pyproject.toml` at `waybill-cli/tests/fixtures/pip/malformed_name_placeholder/` per `data-model.md` §Entity 6. Content: `[project]` section with `name = "{{package-name}}"`, `version = "0.0.0"`, and `dependencies = ["waybill-fixture-real-dep-1", "waybill-fixture-real-dep-2"]`. Deliberately include valid dep names to prove whole-manifest reject.
- [X] T016 [US1] Create integration test file `waybill-cli/tests/scan_python_m677.rs` per `data-model.md` §Entity 7. Use the same subprocess-invocation pattern as `waybill-cli/tests/scan_python_m670.rs` (grep for a similar test for structure). Single test `malformed_name_pyproject_emits_zero_components_with_warn`. Steps: (1) scan the fixture with `--offline --format cyclonedx-json --output <tmp>/out.cdx.json` and `RUST_LOG=info`; (2) assert `output.status.success()`; (3) parse `out.cdx.json`; (4) count `pkg:pypi/*` components — assert `== 0`; (5) assert stderr contains EXACTLY one line matching `pyproject.toml \[project\].name failed PEP 508 validation`; (6) assert stderr contains `names_rejected=1` (ANSI-stripped comparison per m672 test convention).
- [X] T017 [US1] Run the new integration test: `cargo test --test scan_python_m677`. Must pass with `1 passed; 0 failed`.

**Checkpoint**: US1 delivered. The P1 bug (issue #768) is empirically closed via the fixture-based test.

---

## Phase 4: User Story 2 — waybill maintainer extending validation to another reader (Priority: P2)

**Goal**: Ensure the reusable helper is genuinely extensible — a future contributor adding npm/maven/etc. validation follows the pattern documented in `contracts/name-validation-module.md`.

**Independent Test**: The `contracts/name-validation-module.md` file's "Extension pattern for future readers" section exists and describes the concrete steps.

**Note**: The reusable helper is ALREADY delivered by Phase 2 (module + `pub(crate)` API). US2 is verification-only in this feature.

- [X] T018 [US2] Verify the `Extension pattern for future readers` section exists at the bottom of `specs/677-pep508-name-validation/contracts/name-validation-module.md` and names the concrete steps (add sibling `is_<ecosystem>_name` + `validate_<ecosystem>_name` functions; wire per-reader `filter_project_roots_by_name`-analog). No code change; this is a documentation cross-check ensuring the promise is on record.

**Checkpoint**: US2 delivered (via Phase 2 substrate + T018 documentation cross-check).

---

## Phase 5: User Story 3 — Transparency for skipped malformed names (Priority: P3)

**Goal**: Ensure every rejected manifest surfaces an operator-visible WARN log AND a structured completion-log count.

**Independent Test**: Scan any fixture with a malformed name and observe (a) one WARN line, (b) `names_rejected=<N>` in the reader-complete log.

**Note**: Both signals are delivered by Phase 3 tasks (T010 for the WARN, T012 for the count). US3 is verification-only.

- [X] T019 [US3] Verify Phase 3's integration test (T016) contains explicit assertions for BOTH the WARN log line AND the `names_rejected=1` structured field. If either assertion is missing, extend T016 rather than adding a duplicate test.

**Checkpoint**: US3 delivered (via T010 + T012 emission + T016 test coverage).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final verification + PR mechanics.

- [ ] T020 Run the full pre-PR gate: `./scripts/pre-pr.sh`. Confirms workspace clippy + all workspace tests pass. Per memory `feedback_prepr_gate_full_output`, verify the final "all pre-PR checks passed" line — do not grep-and-declare-victory on partial output.
- [ ] T021 Commit changes with a message summarizing the fix (name_validation module + pip filter + fixture + integration test). Reference issue #768 explicitly. Include the `Co-Authored-By: Claude Opus 4.7 (1M context)` line.
- [ ] T022 Push the branch and open a PR against `main` on `kusari-oss/waybill`. PR body includes: Summary (bug + fix approach), Before/After empirical numbers from T017, research decision rationale (Option A whole-manifest reject via Session 2026-09-03 Q1), Test plan checklist. Reference `Closes #768`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: T001 — no dependencies. Read-only verification.
- **Phase 2 (Foundational)**: T002-T008. T002 creates the file; T003-T005 fill it in (same file, sequential); T006 adds tests (same file); T007 registers the module (different file); T008 verifies. All later phases depend on Phase 2.
- **Phase 3 (US1)**: T009-T017 depend on Phase 2. Sequential within the phase — most tasks touch `pip/mod.rs` (single file).
- **Phase 4 (US2)**: T018 depends on Phase 3 (verifies the contract doc references the shape that Phase 3 wires).
- **Phase 5 (US3)**: T019 depends on Phase 3 T016 (test file must exist).
- **Phase 6 (Polish)**: T020 depends on all preceding. T021 → T020. T022 → T021.

### Within Phase 2 (Foundational)

- T002 first (creates the file).
- T003, T004, T005 sequential (same file — modify the enum + impl + functions in one flow).
- T006 depends on T003-T005 (needs the impl to test).
- T007 depends on T002 (needs the file to exist).
- T008 depends on T006 + T007.

### Within Phase 3 (US1)

- T009 first (import statement — must exist before the fn that uses it).
- T010 depends on T009.
- T011 depends on T010.
- T012 depends on T011.
- T013 depends on T009-T012 (compile-check the wired code).
- T014 depends on T013 (existing tests must pass after wiring before continuing).
- T015 can run in parallel with T009-T014 (different file — fixture creation).
- T016 depends on T015 (fixture must exist for the test to reference).
- T017 depends on T016.

### Parallel Opportunities

- **Phase 3**: T015 (fixture creation) can run in parallel with the pip/mod.rs edits (T009-T014). Rest is single-file-serial.
- **Phase 4 + Phase 5**: T018 (doc-check) and T019 (test-assertion-check) are both read-only verifications — can run in parallel with each other.

---

## Parallel Example: Phase 3 (US1)

```bash
# Parallel — different files
Task: "Edit waybill-cli/src/scan_fs/package_db/pip/mod.rs (T009-T012)"
Task: "Create fixture at waybill-cli/tests/fixtures/pip/malformed_name_placeholder/pyproject.toml (T015)"

# Sequential — verification chain
cargo build -p waybill                              # T013
cargo test -p waybill --bin waybill scan_fs::package_db::pip  # T014
cargo test --test scan_python_m670                  # T014 continued
# ... then integration test:
cargo test --test scan_python_m677                  # T017
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 (T001) — verify state.
2. Phase 2 (T002-T008) — build the helper module + 14 unit tests.
3. Phase 3 (T009-T017) — wire the filter + fixture + integration test.
4. **STOP and VALIDATE**: T017's integration test satisfies US1's Independent Test.
5. Feature is shippable at this checkpoint. US2 + US3 add verification-only tasks that fold naturally into the same PR.

### Incremental Delivery

The feature is scope-tight enough to land in one PR. Splitting adds review overhead without shipping-velocity gain. Recommended: one PR covering all six phases.

### Parallel Team Strategy

With one contributor (expected staffing), the task ordering above is the natural flow. `[P]` opportunities exist in Phase 3 (fixture vs source edits) but batching them serially is fine.

---

## Notes

- The production fix is ~70 lines of Rust across two files (`name_validation.rs` + `pip/mod.rs`). Scope-tight bug fix. SC-007 (≤ 200 line production diff) satisfied by ~3x margin.
- No new Cargo dependencies (SC-006). `regex`, `tracing`, `toml` all pre-existing workspace deps.
- No changes to `waybill-common` or `waybill-ebpf`. Constitution Principle VI (Three-Crate Architecture) unaffected.
- Follow-up work extending validation to non-pip readers (npm, maven, gem, cargo, ...) is EXPLICITLY OUT OF SCOPE for this feature per Assumptions in spec.md. Each extension is a separate follow-up milestone.
- The `[tool.poetry].name` fallback in the filter (T010) must mirror `build_pip_main_module_entry`'s extraction logic verbatim — otherwise the two functions could rescue different manifest sets, breaking whole-manifest reject.
