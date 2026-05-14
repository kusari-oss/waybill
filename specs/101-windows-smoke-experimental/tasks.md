---
description: "Task list for milestone 101 — Windows smoke test + experimental docs callout"
---

# Tasks: Windows smoke test + experimental docs callout

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/101-windows-smoke-experimental/`
**Prerequisites**: plan.md, spec.md (with Clarifications), research.md, data-model.md, contracts/smoke-test-contracts.md, quickstart.md

**Tests**: Yes — the entire feature IS a test. The new `scan_windows_smoke.rs` is the deliverable.

**Organization**: 3 user stories converge on 4 files. US1 (P1) creates the smoke test itself — the headline behavior. US2 (P1) updates docs to mark Windows as experimental. US3 (P2) splits the CI lane's test step so the smoke test becomes a blocking gate. US1 + US2 are file-level independent; US3 depends on US1's test existing (the CI step `cargo test --test scan_windows_smoke` requires the test file).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files OR independent of incomplete tasks)
- **[Story]**: User story this task belongs to (US1–US3)
- File paths are workspace-relative.

## Path Conventions

Test code under `mikebom-cli/tests/` (1 new file). CI infra under `.github/workflows/`. Docs under `README.md` + `docs/user-guide/`. Zero changes outside these paths per FR-009 + FR-010.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + baseline pre-PR gate is clean before touching anything.

- [X] T001 Confirm working branch is `101-windows-smoke-experimental`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit-specify` and main is at post-PR-#209 (milestone-100 merge) or later. Confirm `git diff --name-only main` shows the spec dir as untracked but no other changes.
- [X] T002 Confirm baseline pre-PR gate passes on macOS/Linux dev host. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 101.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None required. The 3 user stories share no foundational code; US3 depends on US1's test file existing but that's captured as a story-level dependency, not a foundational task.

(No tasks in this phase.)

---

## Phase 3: User Story 1 — Windows-host smoke test (Priority: P1) 🎯 MVP

**Goal**: A new integration test at `mikebom-cli/tests/scan_windows_smoke.rs` runs `mikebom.exe sbom scan` against two fixtures (cargo + polyglot) on Windows hosts, asserts exit 0 + well-formed CDX 1.6 + per-ecosystem PURL coverage + zero backslashes in path-shaped fields + 60-second timeout + on-failure diagnostics. `#[cfg(windows)]`-gated so it's a no-op on Linux/macOS.

**Independent Test**: On any host, `cargo +stable test --test scan_windows_smoke` compiles and reports `ok. 0 passed; 0 failed` on non-Windows (the `#[cfg(windows)]` file gate makes the binary empty). On a Windows runner, it reports `ok. 2 passed`. Verified via the Windows CI lane's smoke step on this PR.

### Implementation for User Story 1

- [X] T003 [US1] Create `mikebom-cli/tests/scan_windows_smoke.rs` per `data-model.md §scan_windows_smoke.rs` — the full ~150-line file. File-level `#![cfg(windows)]` + `#![allow(clippy::unwrap_used)]`. Two `#[test]` fns (`smoke_cargo_fixture`, `smoke_polyglot_monorepo`). Three helpers (`run_scan_with_timeout`, `walk_for_backslash_in_path_fields`, `diagnose_and_panic`). One `PATH_FIELD_NAMES` const = `["mikebom:source-files", "mikebom:source-path", "location"]`. Two `const`s: `SCAN_TIMEOUT_SECS = 60`, `TIMEOUT_DETECTION_THRESHOLD_SECS = 58`. No new Cargo deps (per FR-009 / research §1+§2): uses `std::process::Command`, `std::thread::spawn` + `std::thread::sleep`, `std::time::{Duration, Instant}`, `tempfile` (already in dev-deps), `serde_json::Value`, `env!("CARGO_BIN_EXE_mikebom")`, `env!("MIKEBOM_FIXTURES_DIR")`.

### Verification for User Story 1

- [X] T004 [US1] Verify Contract 1 (smoke test compiles + runs cleanly on macOS as empty) + Contract 9 (diff scope). Run:
    ```bash
    cargo +stable test --test scan_windows_smoke 2>&1 | grep "test result:"
    # Expected on macOS: ok. 0 passed; 0 failed; 0 ignored (empty test binary).
    cargo +stable clippy -p mikebom --all-targets -- -D warnings 2>&1 | tail -3
    # Expected: zero warnings.
    git diff --name-only main | grep -E '^mikebom-cli/' | head -5
    # Expected: only `mikebom-cli/tests/scan_windows_smoke.rs` (NEW).
    ```

**Checkpoint**: US1 complete on the macOS dev host. The smoke test file exists, compiles to empty on non-Windows, and trips zero clippy warnings. Windows-host runtime behavior is verified later via the Windows CI lane (T009 in US3 — after the CI step is split to run it).

---

## Phase 4: User Story 2 — Docs experimental callout (Priority: P1)

**Goal**: README.md + `docs/user-guide/installation.md` get a prominent "🧪 Experimental" callout in their Windows sections (consistent wording, links to #210, lists the known gap categories), and the README's platform-support table's Windows row cell-text changes from `✅ supported (milestone 100)` to `🧪 experimental (milestone 100, #210)`.

**Independent Test**: render README.md in GitHub's Markdown preview; the experimental callout appears within the first paragraph of the Windows install section. Diff the callout content between README.md and installation.md — must be identical. Run the verification recipes in `contracts/smoke-test-contracts.md §Contract 8`.

### Implementation for User Story 2

- [X] T005 [P] [US2] Update the platform-support table's Windows row in `README.md` (current line ~272 post-milestone-100). Change `| Windows x86_64    | ✅ supported (milestone 100)         | ❌                          |` to `| Windows x86_64    | 🧪 experimental (milestone 100, [#210](https://github.com/kusari-sandbox/mikebom/issues/210)) | ❌ |`. Per `data-model.md §README.md Change 1` + FR-007.
- [X] T006 [P] [US2] Insert the canonical experimental callout blockquote AT THE TOP of the "Windows install" subsection in `README.md` (above the existing download instructions). The callout wording is byte-fixed per `data-model.md §README.md Change 2` / research §6. Must include: `🧪 **Experimental.**`, the gap-categories list (dpkg/rpm/apk, HOME-env, OCI cache, path-resolver matcher, Python stdlib collapse), `[#210](...)`, and "Do not rely on the Windows binary for production SBOM workflows." Per FR-005.
- [X] T007 [P] [US2] Update `docs/user-guide/installation.md` (verified at plan-time: file exists, has a platform-support table at line ~5 and no dedicated Windows-install subsection yet). Two concrete edits:
    1. **Platform-support table** (around line 7): replace `Windows (WSL2)` with `Windows (native — 🧪 experimental, [#210](https://github.com/kusari-sandbox/mikebom/issues/210); WSL2 for tracing)` in the `Needs` column of the Scanning row, OR equivalent wording that names the native-Windows experimental status and links #210.
    2. **Add a new `### Windows install (experimental)` subsection** inserted between `## Pre-built binaries (recommended)` and `## Build from source`. Body MUST contain the canonical experimental callout blockquote (byte-identical to README's per Contract 8) AND a brief reference back to README's "Windows install" section for the canonical download instructions: `For the latest Windows x86_64 binary, follow the [Windows install instructions in the README](../../README.md#windows-install).`
    Per FR-006.

### Verification for User Story 2

- [X] T008 [US2] Verify Contract 8 from `contracts/smoke-test-contracts.md`. Run:
    ```bash
    grep -n '🧪 \*\*Experimental' README.md docs/user-guide/installation.md
    # Expected: ≥1 match in each file.
    grep -n '#210\|issues/210' README.md docs/user-guide/installation.md
    # Expected: ≥1 match in each file (each line should mention #210 or the URL).
    grep '| Windows x86_64' README.md
    # Expected: contains '🧪 experimental' (no '✅ supported').
    diff <(sed -n '/🧪 \*\*Experimental/,/until #210 closes/p' README.md) \
         <(sed -n '/🧪 \*\*Experimental/,/until #210 closes/p' docs/user-guide/installation.md)
    # Expected: empty diff (callout text identical).
    ```

**Checkpoint**: US2 complete. Both docs have the experimental callout; the platform-support table's Windows row reads experimental.

---

## Phase 5: User Story 3 — CI lane test step split (Priority: P2)

**Goal**: `.github/workflows/ci.yml`'s `lint-and-test-windows` job splits its single `Tests (non-blocking ...)` step into TWO steps: a new `Smoke test (blocking — milestone 101)` step without `continue-on-error` (runs the new smoke test as a merge gate), and the existing broader `Tests (non-blocking, see issue #210)` step retained with `continue-on-error: true` (per-test backlog stays visible but non-blocking). Smoke step runs FIRST so it benefits from cargo-cache warmth from the clippy step.

**Independent Test**: visual diff of `ci.yml` against the milestone-100 baseline shows the smoke step inserted before the workspace test step. After the PR pushes, the Windows CI lane runs both steps; the smoke step gates the merge.

**Story dependency**: US3 requires US1's test file (`scan_windows_smoke.rs`) to exist. If US3 lands without US1, `cargo test --test scan_windows_smoke` would fail with "no test target named scan_windows_smoke." Don't merge US3 in isolation.

### Implementation for User Story 3

- [X] T009 [US3] Update `.github/workflows/ci.yml` per `data-model.md §ci.yml`. In the `lint-and-test-windows` job (around line 258 post-milestone-100), find the existing single `- name: Tests (non-blocking, see issue #210)` step (around line 289) and replace it with TWO steps:
    - `- name: Smoke test (blocking — milestone 101)` running `cargo +stable test --test scan_windows_smoke` (NO `continue-on-error:` attribute).
    - `- name: Tests (non-blocking, see issue #210)` running `cargo +stable test --workspace` (KEEP `continue-on-error: true`).
    Smoke step MUST appear BEFORE the workspace step (sequenced for cargo-cache warmth per research §5). Update the comment block above the workspace step to reference the new smoke step as the blocking gate. Per FR-008.

### Verification for User Story 3

- [X] T010 [US3] Verify Contract 7 from `contracts/smoke-test-contracts.md`. Run:
    ```bash
    grep -nE 'Smoke test \(blocking|Tests \(non-blocking' .github/workflows/ci.yml | head -4
    # Expected: 2 matches in order: smoke first, then workspace.
    awk '/Smoke test \(blocking/,/^      - name:/' .github/workflows/ci.yml | grep -c 'continue-on-error'
    # Expected: 0 (smoke step is blocking).
    awk '/Tests \(non-blocking/,/^  [a-z]/' .github/workflows/ci.yml | grep -c 'continue-on-error: true'
    # Expected: 1.
    ```
    The empirical verification — that a smoke-test-breaking regression blocks the merge on the Windows lane — happens on this PR's own CI run + the post-merge first PR that touches `mikebom-cli/src/`.

**Checkpoint**: US3 complete. The Windows CI lane has two test steps; the smoke step is the new blocking gate.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final diff-scope audit + pre-PR gate run + PR open.

- [X] T011 Verify Contract 9 (diff scope) end-to-end. Run:
    ```bash
    # Allowlisted file set:
    git diff --name-only origin/main | sort
    # Expected:
    #   .github/workflows/ci.yml
    #   CLAUDE.md                                              (auto-updated by /plan)
    #   README.md
    #   docs/user-guide/installation.md
    #   mikebom-cli/tests/scan_windows_smoke.rs                (NEW)
    #   specs/101-windows-smoke-experimental/...

    git diff --name-only origin/main | grep -E '^Cargo\.(lock|toml)$|/Cargo\.(lock|toml)$' | wc -l
    # Expected: 0

    git diff --name-only origin/main | grep -E '^mikebom-cli/src/' | wc -l
    # Expected: 0

    git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
    # Expected: empty (no goldens regenerated).
    ```
- [X] T012 Run the mandatory pre-PR gate on macOS/Linux per Contract 10. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` with zero clippy warnings and zero test failures across the workspace. The new `scan_windows_smoke.rs` compiles to empty on non-Windows (its `#![cfg(windows)]` file-top gate); the docs + CI changes don't affect any Unix code path.
- [ ] T013 Open the PR. Title: `feat(101): Windows smoke test + experimental docs callout`. Body should mention the smoke test exercises cargo + pypi + npm (FR-002), 60-second per-scan timeout (FR-011), inline diagnostics + `actual.cdx.json` on failure (FR-012), CI smoke step is the new blocking gate (FR-008), docs callouts link to #210 (FR-005/FR-006). Mark as ready-for-review (not draft).

---

## Dependencies

```text
T001 (branch check) → T002 (baseline pre-PR)
T002 → T003 [US1] (create smoke test) → T004 [US1] (verify clean)
T002 → T005 [US2] [P], T006 [US2] [P], T007 [US2] [P] → T008 [US2] (verify callouts)
T004 [US1] → T009 [US3] (CI split — needs the test file to reference) → T010 [US3] (verify ci.yml)
T008 [US2] + T010 [US3] → T011 (diff-scope audit) → T012 (pre-PR gate) → T013 (open PR)
```

US1 (T003–T004) and US2 (T005–T008) can run in parallel — they touch different files (`tests/` vs `README.md` + `docs/`). US3 (T009–T010) gates on US1 being complete.

## Parallel Execution Opportunities

- T005 + T006 + T007 all touch different files (table cell in README, install subsection in README, callout in installation.md). Can be implemented in parallel.
- US1's T003 and US2's T005–T007 are file-level independent; can be parallel as well.
- T009 depends on T003 (the CI step references the test file name).

## Implementation Strategy

**MVP scope**: US1 + US2 are both P1. Either is independently shippable, but landing them together makes the most sense (the smoke test gains value once the docs reflect Windows-experimental status — both are about honest signal). US3 is a quality improvement on US1's enforcement; ship together to avoid a follow-up PR.

**Suggested execution order**: T001 → T002 → (parallel: T003 + T005 + T006 + T007) → T004 → T008 → T009 → T010 → T011 → T012 → T013. Total: ~13 tasks, ~45 min of focused work.

**Risk**: T009's CI step won't actually exercise the smoke test until the PR's first Windows lane run on `windows-latest`. If the smoke test has a bug (e.g., wrong fixture path, wrong env var name), it'll fail on the first PR push. Treat the first Windows-lane run as the empirical verification step.

**Backup plan**: if US3 (T009–T010) introduces unexpected CI complexity (e.g., cargo-cache interaction), ship US1 + US2 alone; the smoke test still runs inside the existing non-blocking workspace test step, surfaces in CI logs, and a follow-up PR can add the blocking-step split without re-litigating the smoke test design.

## Task format validation

All 13 tasks follow the required format `- [ ] TXXX [P?] [USX?] Description with file path`:
- ✅ Checkboxes start every line
- ✅ Sequential task IDs T001 – T013
- ✅ [P] markers only where parallelization is sound
- ✅ [US1/US2/US3] labels on every user-story-phase task
- ✅ Setup (T001, T002), Polish (T011, T012, T013) — NO story label
- ✅ Every task includes a concrete file path (or "branch check" / "pre-PR gate" for the meta tasks)
