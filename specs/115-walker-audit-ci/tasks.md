# Tasks: Walker-Audit CI Gate

**Input**: Design documents from `/specs/115-walker-audit-ci/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ci-step.md ✓, quickstart.md ✓

**Tests**: Per spec Assumption "A small negative test that proves the gate fails when a new unauthorized walker is introduced is the principal validation mechanism. A full unit-test suite for the gate's internal logic is unnecessary — the gate is a single shell pipeline." This task list ships ONE negative-test task as part of US1, no broader test scaffolding.

**Organization**: Tasks are grouped by user story. US1 (P1) is the MVP — the gate itself. US2 (P2) is the contributor-facing documentation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2)
- Include exact file paths in descriptions

## Path Conventions

Single-project layout (mikebom workspace at repo root). Affected paths:
- `.github/workflows/ci.yml` (the new CI step)
- `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` (the new allow-list)
- `CONTRIBUTING.md` (the new section)
- `docs/design-notes.md` (the cross-link)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish the baseline file every subsequent task depends on.

- [X] T001 Generate the baseline allow-list by running `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | LC_ALL=C sort -u > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` from the repo root, then `git add mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`. Verify the file has ~28 non-empty lines (the exact count is whatever the post-114 / post-113-polish tree produces at PR-open time) and ends with a single trailing LF. Per data-model.md § "Entity: Allow-list Entry" invariants.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: None. The allow-list file from T001 IS the only foundational artifact, and the CI step (US1) is the next thing to build. No other blocking work exists.

**Checkpoint**: T001 complete → user-story work can begin.

---

## Phase 3: User Story 1 — Maintainer gets the audit verdict automatically (Priority: P1) 🎯 MVP

**Goal**: Wire the CI step into `.github/workflows/ci.yml`'s existing `Lint + test (linux-x86_64)` job so every PR and every push to main runs the audit comparison and either passes silently or fails with the FR-004 message contract.

**Independent Test**: From the negative-test runbook in quickstart.md § "Negative test — Verifying the gate works": open a throwaway branch that adds `fn walk_synthetic_negative(...)` in a new source file under `mikebom-cli/src/scan_fs/`. Push, open a PR. Expected: the `Walker-audit allow-list check` step fails red, the diff hunks identify the synthetic file + line, and the trailing pointer references `CONTRIBUTING.md § Walker-audit CI gate`. Close the PR + delete the branch.

### Implementation for User Story 1

- [X] T002 [US1] Add the `Walker-audit allow-list check` step to `.github/workflows/ci.yml` inside the `Lint + test (linux-x86_64)` job, between the `actions/checkout@v4` step and the existing clippy step. Implement the pipeline per contracts/ci-step.md: `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | LC_ALL=C sort -u > /tmp/live.txt`, then strip blank lines + comments from the allow-list into `/tmp/expected.txt`, then `diff -u /tmp/expected.txt /tmp/live.txt`. On non-zero exit, print the FR-004 failure-message payload (headline starting with `[FAIL]`, the diff hunks, the trailing 2-line pointer) and exit non-zero. On zero exit, print the success line `Walker-audit allow-list check: OK (<N> entries; <M> ms)`. Inline the shell as a single `run:` block so it stays auditable.

- [X] T003 [US1] In the same step from T002, add the missing-or-empty-allow-list precheck: if `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` does not exist OR has zero non-blank-non-comment lines after the filter, print the FR-010 "missing or empty" failure message from contracts/ci-step.md § "Fail path — missing allow-list" and exit non-zero. This precheck runs BEFORE the diff comparison so a missing file produces a maintainer-actionable message rather than a generic diff failure.

- [X] T004 [US1] (pre-merge: local synthetic-walker simulation confirmed gate fails red; live negative-test PR deferred to post-merge per quickstart.md) Run the negative test from quickstart.md § "Negative test — Verifying the gate works" against a throwaway branch off the feature branch. Open a draft PR, observe the `Walker-audit allow-list check` step fails red with the FR-004 headline + diff hunks identifying the synthetic walker. Capture the failing CI log URL in the PR description. Close the PR + delete the branch after verification. Do NOT merge the synthetic walker.

**Checkpoint**: After T002–T004, the gate is live. Every subsequent PR on this branch and every push to main exercises it. US2 work can begin in parallel after T002 lands (T005/T006 only need the step to exist for the docs to reference it accurately).

---

## Phase 4: User Story 2 — Contributor knows the workflow before they start (Priority: P2)

**Goal**: Add a `## Walker-audit CI gate` section to `CONTRIBUTING.md` and a one-line cross-link from `docs/design-notes.md` § "Filesystem walking pattern (milestone 114)" so a first-time contributor finds the workflow via the standard contribution-guide entry point.

**Independent Test**: From spec § US2 Independent Test: a contributor unfamiliar with milestone 114 reads only `CONTRIBUTING.md`. After reading the new section, they can correctly answer (a) where `safe_walk` lives, (b) when to add a new exception vs. migrate to the helper, (c) the exact files they need to edit to make the CI gate pass when adding an exception. Verified by maintainer cold-read of the section without consulting other docs.

### Implementation for User Story 2

- [X] T005 [P] [US2] Add a new `## Walker-audit CI gate` section to `CONTRIBUTING.md` between `## Pre-PR gate (MANDATORY)` (line 48) and `### Performance benchmarks (opt-in)` (line 78). Section contents per research.md § Decision 5: (a) the audit pattern verbatim — `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/` — copy-pasteable; (b) the allow-list file path `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`; (c) the two-step new-exception workflow — append the new grep-output line (sorted) + add a one-sentence reason in `walk.rs`'s comment block; (d) the failure-message contract — diff hunks identify the offending line(s), trailing pointer routes to this section. Reference quickstart.md § "Five-minute walkthrough — Scenario B: legitimate new exception" as the runbook.

- [X] T006 [P] [US2] Add a one-line cross-link in `docs/design-notes.md` § "Filesystem walking pattern (milestone 114)" pointing at `CONTRIBUTING.md § Walker-audit CI gate` as the canonical contributor-facing workflow. Insert after the existing "Any match outside the union of those two lists is a regression…" paragraph at line 234. Do NOT duplicate the workflow content into design-notes — the design-notes section retains its "why we did this" framing while CONTRIBUTING.md owns "what to do."

**Checkpoint**: After T005 + T006, US2's documentation contract is complete. A new contributor finds the workflow via the standard CONTRIBUTING.md entry point AND a maintainer following design-notes' milestone-114 section gets routed to the same place.

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Pre-PR gate validation + final sanity checks before opening the PR.

- [X] T007 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` from the repo root and verify both clippy (`--workspace --all-targets`) and the full workspace test suite pass clean. Although this feature ships YAML + plain text + Markdown only, the pre-PR script still gates merges; per CLAUDE.md it is MANDATORY before opening any PR.

- [X] T008 Verify the allow-list bootstrap invariants from data-model.md § "Invariants" with the one-liner `LC_ALL=C sort -u mikebom-cli/src/scan_fs/walk.audit-allowlist.txt | diff - mikebom-cli/src/scan_fs/walk.audit-allowlist.txt && tail -c1 mikebom-cli/src/scan_fs/walk.audit-allowlist.txt | od -c | head -1` — expect empty diff output AND a trailing `\n` in the od dump. Confirms sort-stability + final-newline.

- [X] T009 Cold-read the final `CONTRIBUTING.md § Walker-audit CI gate` section against the spec's US2 Independent Test criteria. Without consulting `walk.rs` or `design-notes.md`, verify the section answers: (a) where `safe_walk` lives, (b) when to add a new exception vs migrate, (c) which two files to edit + that both must land in the SAME PR. Adjust wording if any criterion is unmet.

- [X] T010 Open the PR. Title: `feat(ci): walker-audit gate (closes #342)`. Body includes: (1) the feature spec link; (2) the negative-test CI URL from T004 as evidence the gate fires; (3) a `## Test plan` section listing the four spec acceptance scenarios for US1 + the two for US2 as manually-verified-on-this-PR checklist items. Verify CI on the feature PR itself goes green on the `Walker-audit allow-list check` step.

---

## Dependencies & Execution Order

```text
T001 (Setup)
  └─→ T002 (US1: CI step)
        ├─→ T003 (US1: missing-allow-list precheck — same file, sequential)
        │     └─→ T004 (US1: negative test — depends on T002 + T003 being live)
        │           └─→ T007 (Polish: pre-PR gate)
        │                 └─→ T008 (Polish: bootstrap invariants)
        │                       └─→ T010 (Polish: open PR)
        ├─→ T005 [P] (US2: CONTRIBUTING.md — independent file)
        │     └─→ T009 (Polish: cold-read US2 criteria — sequentially needs T005)
        │           └─→ T010 (above)
        └─→ T006 [P] (US2: design-notes.md cross-link — independent file)
              └─→ T010 (above)
```

**Sequential chain**: T001 → T002 → T003 → T004 → (T007 → T008) → T010.
**Parallel branches**: After T002, T005 + T006 may run in parallel (different files; no shared state). T009 sequences after T005.

## Parallel Opportunities

The two US2 documentation edits operate on different files (`CONTRIBUTING.md` vs. `docs/design-notes.md`) and have no shared state:

```text
# After T002 (the CI step exists), these two can land concurrently:
T005 [US2] — edit CONTRIBUTING.md
T006 [US2] — edit docs/design-notes.md
```

No other parallel opportunities exist — T001/T002/T003 share the same files; T004 sequences after the step is live; the polish phase is a linear ratchet.

## Independent Test Criteria

Per the spec's two user stories:

- **US1 (P1) — MVP**: Confirmed by T004's negative-test PR — the `Walker-audit allow-list check` step fails red on a throwaway branch that adds an unauthorized walker. CI log URL captured and linked from the feature PR description.
- **US2 (P2)**: Confirmed by T009's cold-read — a maintainer reads only the new `CONTRIBUTING.md § Walker-audit CI gate` section and can answer the three US2 Independent Test questions (helper location, when to add exception vs migrate, two files to edit).

## Implementation Strategy

**MVP scope**: T001 + T002 + T003 + T004 + T007 + T008 + T010 (the gate itself, validated, with the pre-PR gate clean, in a mergeable PR). US2 (T005 + T006 + T009) is non-blocking polish — without docs, the gate still functions; failure messages still route contributors to `walk.rs`'s comment block (FR-004), which is the second-best landing place.

**Recommended order**: Linear execution top-to-bottom is fine — this feature has ~80 lines of YAML + ~30 lines of text file + ~50 lines of docs total. There's no work-parallelism payoff worth coordinating around. Single contributor, single PR, single commit (optional: separate commits for "setup baseline" / "wire CI step" / "docs" if the reviewer prefers atomic-history).

**Format validation**: All 10 tasks above use the required checklist format — checkbox + sequential ID (T001…T010) + optional [P] marker + [US1]/[US2] label for user-story tasks (Setup + Polish tasks have no story label) + description with exact file path(s).
