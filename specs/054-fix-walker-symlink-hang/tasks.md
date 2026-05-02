---
description: "Task list for milestone 054 — Fix filesystem-walker symlink-loop hang + realistic-project regression suite"
---

# Tasks: Fix filesystem-walker symlink-loop hang + realistic-project regression suite

**Input**: Design documents from `/specs/054-fix-walker-symlink-hang/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/walker-protection.md ✅, quickstart.md ✅

**Tests**: Test tasks ARE included. Per the spec's FR-009 (synthesized minimal symlink-loop fixture as a unit test under each affected walker) and the contract's "Test parity" cross-walker invariant. Plus the realistic-project CI job IS the headline regression-prevention deliverable from US2.

**Organization**: Tasks are grouped by user story (US1 P1 — hang fix; US2 P2 — realistic-project CI; US3 P3 — audit + harden cross-cutting). US3's deliverables are largely accomplished BY US1's per-walker patches; the audit-comment polish step lives in Phase 5.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no incomplete-task deps)
- **[Story]**: User story label (US1 / US2 / US3) for story phases only

## Path Conventions

Single Cargo workspace: `mikebom-cli/src/` and `mikebom-cli/tests/`. New CI workflow goes to `.github/workflows/`. New shared test fixtures go to `tests/fixtures/`. Paths are repo-relative when clear from context.

---

## Phase 1: Setup

**Purpose**: Branch already created (`054-fix-walker-symlink-hang`); spec/plan/research/data-model/contracts/quickstart already authored. Confirm starting state before changes begin.

- [X] T001 Confirm working tree is clean and on branch `054-fix-walker-symlink-hang` (run `git status --short && git branch --show-current` and verify spec-phase artifacts only).
- [X] T002 Verify pre-054 hang reproduces locally per `quickstart.md` step 1-2 against `/tmp/knative-func-054`. Confirms the regression baseline before the fix lands.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None — milestone 054's tests inline a 2-line symlink-loop fixture (`tempfile::tempdir() + std::os::unix::fs::symlink`) directly in each `#[cfg(test)] mod tests` rather than abstracting into a shared helper. The 2-line fixture is simple enough that DRY isn't justified vs. the cross-target-import friction of sharing. Phase 2 is intentionally empty; tasks proceed directly to Phase 3.

(Per the `/speckit.analyze` M3 finding: T003 was originally a shared `make_minimal_symlink_loop` helper; analysis showed sharing across in-crate unit tests vs. integration tests adds `#[path = "..."]` complexity disproportionate to a 2-line fixture. Inlined per-walker is cleaner.)

---

## Phase 3: User Story 1 — Fix the hang on real-world projects (Priority: P1) 🎯 MVP

**Goal**: `mikebom sbom scan --path <project>` completes in bounded wall-clock time on any input regardless of symlink topology. Closes the user's reported knative/func hang. Ships independently as MVP.

**Independent Test**: SC-001 — clone knative/func at `knative-v1.22.0`, run the scan, verify completion within 60s with exit 0. Per `quickstart.md` 3-step recipe.

### Critical hang fixes (zero protection today)

- [X] T004 [US1] **Fix `mikebom-cli/src/scan_fs/package_db/rpm_file.rs::walk_dir`**: introduce `const MAX_WALK_DEPTH: usize = 16;` at module top; add `(depth: usize, visited: &mut HashSet<PathBuf>)` params per the contract's "After" patch shape; canonicalize-key visited-set check BEFORE recursion; `tracing::debug!(path = %dir.display(), "walker: cycle/visited skip")` on dedup hit; depth bound emits `tracing::debug!(depth, path = ..., "walker: max-depth reached")` on hitting the ceiling. Update `discover_rpm_files` (the entry point) to create the empty `HashSet` and pass it down.
- [X] T005 [P] [US1] **Fix `mikebom-cli/src/scan_fs/binary/discover.rs::walk_dir`**: same patch shape as T004 — add `MAX_WALK_DEPTH` const + `(depth, visited)` params + canonicalize-keyed visited-set + tracing breadcrumbs. Update the entry point (`discover_binaries` or equivalent — verify by reading the file) to create the empty `HashSet`.
- [X] T006 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/package_db/rpm_file.rs::tests`. Inline the fixture (no shared helper per analyze M3 finding): `let tmp = tempfile::tempdir().unwrap(); let loop_dir = tmp.path().join("loop"); std::fs::create_dir_all(&loop_dir).unwrap(); std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();` — then call `walk_dir(tmp.path(), 0, &mut HashSet::new(), &mut Vec::new())` (or equivalent post-T004 signature) and assert the call returns within 5 seconds (don't add a `Duration` timeout assertion — if the loop-protection works, the function returns immediately; if it doesn't, the test framework's own timeout will catch it). Guard test module with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per Constitution Principle IV.
- [X] T007 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/binary/discover.rs::tests` (same inline-fixture structure as T006).

### Hardening passes (depth-limited only today — add visited-set on top)

- [X] T008 [P] [US1] **Harden `mikebom-cli/src/scan_fs/package_db/cargo.rs::walk_for_cargo_lockfiles`**: add `visited: &mut HashSet<PathBuf>` param; canonicalize-keyed insert-or-skip BEFORE recursion. Update `discover_cargo_lockfiles` (or whatever the entry point is — confirm by reading the file) to create the empty `HashSet`. Existing `MAX_PROJECT_ROOT_DEPTH = 6` const stays. **Per FR-003: add an inline justification comment** above the const naming the structural reason the tighter bound holds (e.g., `// Cargo workspaces are shallow by convention: a top-level Cargo.toml + per-member subdir + per-target subdir typically max-nests at 3-4 levels; 6 covers any realistic layout. Per milestone-054 FR-003.`).
- [X] T009 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/package_db/cargo.rs::tests` (inline-fixture structure per T006).
- [X] T010 [P] [US1] **Harden `mikebom-cli/src/scan_fs/package_db/gem.rs::walk_for_gemfile_locks`** + `walk_for_gemspecs` (same patch — both functions in the same file). Each gets a `visited: &mut HashSet<PathBuf>` param + the canonicalize-keyed check. Existing depth-limit stays. (Use a SHARED `HashSet` across both invocations within a single scan to avoid double-walking the tree.) **Per FR-003: add an inline justification comment** above the existing depth const (e.g., `// Ruby gem projects are typically a flat or shallow Gemfile + Gemfile.lock at root + lib/ + spec/; 6 covers any realistic layout. Per milestone-054 FR-003.`).
- [X] T011 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/package_db/gem.rs::tests` (inline-fixture structure per T006; one test per the two walkers, each constructing its own minimal fixture inline).
- [X] T012 [P] [US1] **Harden `mikebom-cli/src/scan_fs/package_db/go_binary.rs::walk_for_binaries`**: add `visited` param + canonicalize-keyed dedup. Existing `MAX_BINARY_WALK_DEPTH = 10` const stays. **Per FR-003: add an inline justification comment** above the const (e.g., `// Go binaries land under bin/, /usr/local/bin/, ~/.local/bin/, or container /app/-style paths; 10 levels covers nested-vendor + Bazel-output-like trees. Per milestone-054 FR-003.`).
- [X] T013 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/package_db/go_binary.rs::tests` (inline-fixture structure per T006).
- [X] T014 [P] [US1] **Harden `mikebom-cli/src/scan_fs/package_db/maven.rs::walk_for_maven`**: add `visited` param + canonicalize-keyed dedup. Existing `MAX_PROJECT_ROOT_DEPTH = 6` const stays. **Per FR-003: add an inline justification comment** above the const (e.g., `// Maven multi-module projects nest pom.xml + src/main/{java,resources}/ + nested-modules; 6 covers any realistic layout including spark-style module hierarchies. Per milestone-054 FR-003.`).
- [X] T015 [P] [US1] Add unit test `walks_symlink_loop_without_hanging` to `mikebom-cli/src/scan_fs/package_db/maven.rs::tests` (inline-fixture structure per T006).

### Already-protected walkers (verify only)

- [X] T016 [P] [US1] **Verify `mikebom-cli/src/scan_fs/package_db/golang.rs::walk_for_go_roots`** already has the canonicalize-keyed visited-set per `golang.rs:1162-1167`. Add an inline comment block above the visited-set creation naming the protection mechanism per the contract's Audit Rubric (so the post-054 grep audit recognizes this walker as protected). No code changes; comment only.
- [X] T017 [P] [US1] **Verify `mikebom-cli/src/scan_fs/package_db/project_roots.rs::walk_for_project_roots`** already has visited-set + depth-limit per `project_roots.rs:51-69`. Add an inline comment naming the protection mechanism. No code changes; comment only.

### Integration test (the actual user-facing repro)

- [X] T018 [US1] Add integration test `scan_handles_knative_func_style_symlink_loops_without_hanging` to `mikebom-cli/tests/scan_walker_loops.rs` (NEW FILE). Synthesizes a knative/func-style fixture: `tmpdir/proj/pkg/oci/testdata/test-links/{linkToRoot, b/linkToRoot, b/linkToRootsParent, b/c/linkToParent}` symlinks. Runs `mikebom sbom scan --path <fixture> --offline --no-deep-hash` via the binary subprocess pattern from `tests/scan_go.rs`. Asserts scan completes within 30 seconds (CI-friendly bound; actual fixture is microscopic compared to knative/func) with exit 0. Maps directly to US1 AS#1 + AS#2.

**Checkpoint** (US1 complete): SC-001 (knative/func ≤60s), SC-002 (minimal loop ≤5s), SC-006 (pre-054 hang explicitly verified-as-fixed) all green. Issue #102 closed at the walker-protection layer. The remaining work in US2/US3 layers regression prevention + audit polish on top.

---

## Phase 4: User Story 2 — Realistic-project CI regression suite (Priority: P2)

**Goal**: A new CI job clones knative/func (and 1+ others) at fixed git tags per CI run, scans them, schema-validates the output, and asserts component floors. Catches the next "fairly basic issue" before merge.

**Independent Test**: SC-004 — the new CI job runs ≤ 5 min linux / ≤ 10 min macos on knative/func; produces SBOMs that validate against SPDX 2.3 / 3 / CDX schemas; emits ≥ 200 `pkg:golang` components on the knative/func fixture per SC-007.

**Depends on US1**: the new CI job exercises the post-US1 hang fix. Without US1 the job would itself hang.

### CI workflow file

- [X] T019 [US2] Create `.github/workflows/realistic-projects.yml` per `contracts/walker-protection.md` "Realistic-project CI contract" section. Triggers: `pull_request`, `push: branches: [main]`, `workflow_dispatch`. Matrix: `{project: [knative-func], platform: [ubuntu-latest, macos-latest]}`. Each job: (1) `actions/checkout@v6.0.2` (mikebom repo, persist-credentials false per existing `ci.yml` pattern); (2) install Rust stable; (3) `actions/cache@v4` keyed by `<project>:<tag>`; (4) `git clone --depth 1 --branch <tag> <upstream-url>` if cache miss; (5) `cargo build -p mikebom`; (6) run scan with `--offline --no-deep-hash` with isolated `HOME` + empty `GOMODCACHE`; (7) parse output via `jq`, assert `pkg:golang` component count ≥ 200; (8) report scan duration in the job summary. Pin third-party Actions to full-commit-SHA per existing `ci.yml` security convention. Per-platform timeout: 5min linux / 10min macos.
- [X] T020 [US2] Pin third-party Actions in the new workflow to the same SHAs `ci.yml` uses today: `actions/checkout` SHA `de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2`, `dtolnay/rust-toolchain` SHA `29eef336d9b2848a0b548edc03f92a220660cdb8 # stable (2026-04-25)`, `Swatinem/rust-cache` SHA `e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1`. Verify via `git grep -n "actions/checkout@" .github/workflows/ci.yml` for the canonical SHAs at PR-time.

### Schema validation step

- [X] T021 [US2] Add a schema-validation step to the new workflow that runs the resulting SBOMs through the existing `tests/spdx3_schema_validation.rs`-style validator (or a `cargo run`-able CLI invocation that exits non-zero on schema failure). The validation step MUST fail the CI job clearly with the offending project's name + SPDX validator output on schema deviation. May reuse existing `cargo +stable test --test spdx3_schema_validation -- <case>` test invocation against the freshly-emitted SBOM, OR a dedicated CLI subcommand if cleaner.

### Failure-mode safeguards

- [X] T022 [US2] Verify the new workflow does NOT silently degrade on clone failure. Specifically: if `git clone` exits non-zero, the CI step MUST exit non-zero (no `|| true`, no `continue-on-error: true`). Per FR-007's "MUST NOT silently degrade to skipping a fixture on clone failure."
- [X] T023 [P] [US2] Manually trigger the new workflow via `gh workflow run realistic-projects.yml` against the milestone-054 PR branch (once T019-T022 land). Verify both linux + macos jobs complete green AND the scan duration is reported in the job summary. Captures Phase 4 acceptance evidence for the PR description.

---

## Phase 5: User Story 3 — Audit + harden every walker (Priority: P3)

**Goal**: The post-054 codebase is auditable via `grep -rn "fn walk" mikebom-cli/src/scan_fs/` — every match either has the visited-set + depth-limit pattern OR an inline `// SAFETY:` comment justifying the deviation. Prevents a third instance of this bug class.

**Independent Test**: SC-003 — the grep audit at PR-review time finds zero unannotated, unprotected walkers.

**Depends on US1**: US3 documents the result of US1's per-walker patches.

### Audit-completeness verification

- [X] T024 [US3] Run `grep -rn "fn walk" mikebom-cli/src/scan_fs/` and verify every match has either (a) a `visited: &mut HashSet<PathBuf>` parameter (or a local creation thereof) AND a `MAX_*_DEPTH` reference, OR (b) an inline `// SAFETY:` comment naming the protection mechanism. If T004-T017 landed cleanly, this should pass on first inspection. If any walker is unannotated, add the SAFETY comment OR fail-fast with a follow-up patch in this task.
- [X] T025 [US3] Update `docs/design-notes.md` with a new section: "Filesystem walking pattern (milestone 054)." Describes the per-walker visited-set + depth-limit contract, points at `contracts/walker-protection.md` for the exact patch shape, points at follow-up issue #108 for the eventual single-helper migration. ~30 lines.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: CHANGELOG, pre-PR gate, live verification, PR open.

- [X] T030 **Perf-snapshot regression check (FR-005 / SC-005 evidence)**: snapshot scan times for the 9 ecosystem fixtures `tests/fixtures/{apk,cargo,deb,gem,golang,maven,npm,pip,rpm}/` BEFORE the walker patches land (run `time ./target/debug/mikebom --offline sbom scan --path <fixture> --no-deep-hash --output /tmp/perf.json` 3x per fixture, take the median). Run AFTER the walker patches land using the identical command set. Assert per-fixture wall-clock variance ≤ 15% (the `canonicalize` syscalls add minor stat overhead; ≤15% is the spec's documented noise budget). Capture the before/after table in the PR description as SC-005 evidence. If any fixture exceeds the budget, investigate; do NOT silently accept regressions. Sequenced after T002 + after every US1 walker patch lands (T004-T015) so the post-patch snapshot reflects the full delta.

- [X] T026 Add CHANGELOG entry to `CHANGELOG.md` under `## [Unreleased]` → `### Fixed` with: title "Filesystem-walker symlink-loop hang on real-world projects (milestone 054)"; 3-paragraph body covering (1) the hang shape + reproducer (knative/func) + closes #102; (2) the per-walker visited-set hardening; (3) the new realistic-project CI job (regression prevention going forward). Migration paragraph: "no SBOM-output change for projects without symlink loops; projects with loops now produce a complete SBOM instead of hanging."
- [X] T027 Run `./scripts/pre-pr.sh` — confirms clippy `--all-targets -D warnings` + `cargo +stable test --workspace` both pass per Constitution mandatory pre-PR gate. If any failure: investigate and fix (do NOT skip with `--no-verify`); typical issues at this stage are unused-import warnings on the new `HashSet` import in the patched walkers.
- [X] T028 Run `quickstart.md` 3-step verification recipe end-to-end against a fresh `knative/func` clone at `knative-v1.22.0`. Capture the actual `jq` output of step 3 + scan duration; paste into the PR description as SC-001 + SC-006 acceptance evidence.
- [X] T029 Open the PR via `gh pr create` with title `fix(054): filesystem-walker symlink-loop hang + realistic-project regression suite (closes #102 again)` and body covering: (a) summary referencing #102, #108; (b) test plan listing SC-001..SC-007 outcomes with evidence pointers; (c) the audit-grep result from T024; (d) call-out that PR #107 (alpha.10 release) is paused pending this milestone per the user's instruction.

---

## Dependencies

```text
Phase 1 (Setup: T001-T002)
  └─▶ Phase 3 (US1: T004-T018 — 7 walker patches + 7 unit tests + 2 verify-only audits + 1 integration test)
        │   (Phase 2 intentionally empty per /speckit.analyze M3 finding —
        │    shared symlink-loop helper dropped in favor of inlined 2-line
        │    fixture per unit test.)
        ├─▶ Phase 4 (US2: T019-T023 — new CI workflow + schema validation + manual trigger)
        └─▶ Phase 5 (US3: T024-T025 — audit grep + design-notes update)
              └─▶ Phase 6 (Polish: T026-T030 — CHANGELOG, pre-PR, live verification, PR open + perf-snapshot regression check)
```

US2 and US3 both depend on US1 (the per-walker patches). They are siblings — both can be done in parallel by separate engineers once US1 is complete, OR by the same engineer in series.

## Parallel execution opportunities

- **Phase 3 critical fixes** (no Phase 2 dependency post-M3): T004 || T005 || T006 || T007 || T008 || T009 || T010 || T011 || T012 || T013 || T014 || T015 || T016 || T017 — 14 tasks marked [P], all touching different files. T018 (integration test) sequenced after the walker patches it exercises.
- **Phase 4** (after T019): T020 + T021 + T022 are sequential edits to the same workflow file (T019); T023 (manual trigger) parallel after T019-T022 land.
- **Phase 5**: T024 + T025 touch different concerns; can run in parallel.

## Implementation strategy

**MVP scope = US1 only** (T001-T018 + minimal T026-T029 polish): closes the user's reported hang end-to-end. Ship as a single PR if scope pressure forces a split — US2 + US3 can land in a follow-up. Given the user's stated urgency ("before cutting a new release"), MVP-only is a defensible choice if T019-T025 prove unexpectedly large.

**Recommended scope: US1 + US2** (T001-T023 + T026-T029): ships the fix AND adds the CI gate that prevents the next instance. US3 (audit polish) is a 30-min addition (T024-T025) — likely lands in the same PR without trouble.

**Out-of-scope (deferred)**:
- Issue #108: full migration to a shared `safe_walk` helper. Per Q1 clarification.

## Format validation

All 29 tasks follow the required format: `- [ ] [TaskID] [P?] [Story?] Description with file path`. (T003 dropped via M3; T030 appended via C1; net 29 tasks.)

- Setup phase (T001–T002): no story label ✓
- Foundational phase (intentionally empty post-M3): N/A — no T003 ✓
- US1 phase (T004–T018): every task has `[US1]` ✓
- US2 phase (T019–T023): every task has `[US2]` ✓
- US3 phase (T024–T025): every task has `[US3]` ✓
- Polish phase (T026–T030): no story label ✓ (T030 = perf-snapshot regression check, appended post-analyze for FR-005/SC-005 coverage)

Every task has a sequential ID, an explicit file path or path pattern, and a verb-leading description.
