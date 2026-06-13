# Tasks: Line-stable walker-audit allow-list

**Input**: Design documents from `/specs/117-line-stable-allowlist/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ci-step.md ✓, quickstart.md ✓

**Tests**: Per spec Assumption "A small noise test — a synthetic PR that adds an unrelated helper above an existing walker — is the principal validation mechanism. Combined with the unchanged negative test from milestone 115 (synthetic new walker addition still fails red), the two tests together verify the contract: signal preserved, noise eliminated." Two manual verification tasks below cover this.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1)
- Include exact file paths in descriptions

## Path Conventions

Single-project layout (mikebom workspace at repo root). Affected paths:
- `.github/workflows/ci.yml` (the `Walker-audit allow-list check` step's `run:` block)
- `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` (regenerated in NEW shape)
- `CONTRIBUTING.md` (§ Walker-audit CI gate's regenerate snippet)
- `specs/115-walker-audit-ci/contracts/ci-step.md` (supersession note + cross-link)

No new files in production paths. All changes are localized to four existing files.

---

## Phase 1: Setup

No setup tasks. The sed regex IS the implementation — there's no shared helper to build, no docs row to pre-add, no fixture infrastructure to bootstrap. Go directly to US1.

---

## Phase 2: Foundational

No foundational tasks. Single user story; no shared prerequisites.

---

## Phase 3: User Story 1 — Insertion-above-walker PRs don't false-positive (Priority: P1) 🎯 MVP

**Goal**: The `Walker-audit allow-list check` CI step's `run:` block extends its pipeline with a sed line-strip on both sides. The committed allow-list regenerates in the NEW `<file>:<content>` shape. CONTRIBUTING.md + the milestone-115 spec contract reflect the new pipeline.

**Independent Test**: Two verification scenarios per quickstart.md:
1. **Positive (the case the feature exists to fix)**: synthesize a ~50-line helper at the top of an existing source file under `scan_fs/` whose body has no walker semantics. Without modifying the allow-list, run the gate locally; expect green.
2. **Negative (milestone-115 signal preservation)**: synthesize a file with `fn walk_synthetic_negative_test_DO_NOT_MERGE()` in a non-allow-listed location. Without modifying the allow-list, run the gate locally; expect red with the unchanged milestone-115 failure-message contract.

### Implementation for User Story 1

- [X] T001 [US1] Update `.github/workflows/ci.yml`'s `Walker-audit allow-list check` step `run:` block (lines 68–129) to add the symmetric sed strip per data-model.md § "This feature (NEW) pipeline". Specifically: (a) define a local shell variable `STRIP_LINE_NUMBERS='s/^\([^:]*\):[0-9]*:/\1:/'` near the top of the run-block (after `ALLOWLIST=...`); (b) add `| sed "$STRIP_LINE_NUMBERS"` to the `$EXPECTED` pipeline between the comment/blank-line `grep -v` filters and `LC_ALL=C sort -u`; (c) add `| sed "$STRIP_LINE_NUMBERS"` to the `$LIVE` pipeline between the live `grep -rEn ...` and `LC_ALL=C sort -u`. Preserve every other line of the run-block byte-identically — same headline, same precheck branches, same success-line format. The failure-message `echo "$FAIL_HEADLINE"` + diff hunks + trailing pointer all stay untouched per FR-003.

- [X] T002 [US1] In the same `run:` block, update the `+++ live:` failure-message header line (currently `echo "+++ live: grep -rEn 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sort -u (actual)" >&2`) to document the new pipeline: `echo "+++ live: grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed -e 's/^\([^:]*\):[0-9]*:/\1:/' | sort -u (actual)" >&2`. Opportunistically corrects milestone-115's minor `--include='*.rs'` omission in that header per research.md § Decision 4.

- [X] T003 [US1] Regenerate `mikebom-cli/src/scan_fs/walk.audit-allowlist.txt` in the NEW `<file>:<content>` shape per data-model.md § "Lifecycle". Run from the repo root: `LC_ALL=C grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ | sed 's/^\([^:]*\):[0-9]*:/\1:/' | LC_ALL=C sort -u > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`. Expected entry count: 28 (same as the milestone-115 baseline; none of the extractor helpers introduced in milestone-116 PR-B/PR-C — `extract_pom_plugin_final_name`, `extract_go_package_main_directory_names`, `file_declares_package_main` — match the `fn walk*` audit pattern). All entries in NEW shape (no `:NNN:` middle column), sort-stable, single trailing LF.

- [X] T004 [US1] Local positive-test verification per quickstart.md § "Positive test". Create `/tmp/synthetic-helper.rs` with ten one-line helper functions (`fn synthetic_helper_zero() {}` etc.). Insert at top of `mikebom-cli/src/scan_fs/package_db/maven.rs` via `sed -i.bak '1r /tmp/synthetic-helper.rs' mikebom-cli/src/scan_fs/package_db/maven.rs`. Run the gate's pipeline locally: `bash -c '... pipeline copied from ci.yml ...'`. Expected: exit code 0, success message printed. Restore: `mv mikebom-cli/src/scan_fs/package_db/maven.rs.bak mikebom-cli/src/scan_fs/package_db/maven.rs`. Verifies FR-009 / SC-001.

- [X] T005 [US1] Local negative-test verification per quickstart.md § "Negative test". Create `mikebom-cli/src/scan_fs/synthetic_negative_test.rs` containing `fn walk_synthetic_negative(root: &std::path::Path) -> Vec<std::path::PathBuf> { vec![root.to_path_buf()] }`. Run the gate's pipeline locally. Expected: exit code non-zero, `[FAIL]` headline printed, diff hunks identify the synthetic file's walker (in NEW shape: `+ mikebom-cli/src/scan_fs/synthetic_negative_test.rs:fn walk_synthetic_negative(...)`), both pointers printed. Delete the synthetic file after verifying. Verifies FR-004 / SC-002.

- [X] T006 [US1] Update `CONTRIBUTING.md § Walker-audit CI gate`'s regenerate snippet to use the new pipeline. Specifically the `git diff` should show changes ONLY to the regenerate-command code block (currently `grep -rEn --include='*.rs' 'fn walk[_(]' mikebom-cli/src/scan_fs/ \| LC_ALL=C sort -u > mikebom-cli/src/scan_fs/walk.audit-allowlist.txt`); replace with the form from quickstart.md § "Scenario C: legitimate new exception" (adds `| sed 's/^\([^:]*\):[0-9]*:/\1:/'` step between grep and sort). The section's prose around the snippet stays unchanged. Per FR-010.

- [X] T007 [US1] Update `specs/115-walker-audit-ci/contracts/ci-step.md` to record the supersession by issue #347 / this feature. Add a one-paragraph note at the top of the file (immediately after the front-matter) reading: "**Supersession note** (2026-06-13): The 'Pipeline', 'Outputs', and 'Backwards compatibility' sections below are superseded by [`specs/117-line-stable-allowlist/contracts/ci-step.md`](../../117-line-stable-allowlist/contracts/ci-step.md). The other sections (Step name, Trigger surface, Step ordering, Inputs, Performance, Idempotency, Failure-mode parity, Out-of-band overrides) remain canonical here." No other edits to milestone 115's contract document. Per FR-011.

**Checkpoint**: After T001–T007, the gate's pipeline is line-stable. Run T008 + T009 to verify, then T010–T012 to ship.

---

## Phase 4: Polish

- [X] T008 Verify the regenerated allow-list invariants per data-model.md § "Invariants". One-liner: `LC_ALL=C sort -u mikebom-cli/src/scan_fs/walk.audit-allowlist.txt | diff - mikebom-cli/src/scan_fs/walk.audit-allowlist.txt && tail -c1 mikebom-cli/src/scan_fs/walk.audit-allowlist.txt | od -c | head -1 && grep -v '^$' mikebom-cli/src/scan_fs/walk.audit-allowlist.txt | wc -l`. Expected: empty diff (sort-stable), trailing `\n` (final-newline), entry count matches the live grep output. Confirms invariants 1-5 + the NEW-shape invariant 6.

- [X] T009 Run `MIKEBOM_SKIP_DOCKER_INTEGRATION=1 ./scripts/pre-pr.sh` from the repo root. Although this feature ships YAML + plain text + Markdown only, the pre-PR script gates merges; per CLAUDE.md it is MANDATORY before opening any PR.

- [X] T010 Update `specs/117-line-stable-allowlist/tasks.md` (this file) marking T001–T009 as `[X]` completed.

- [X] T011 Commit per CLAUDE.md commit protocol. Commit title: `fix(ci): walker-audit allow-list ignores line-number drift (closes #347)`. Commit body summarizes: (a) the sed-strip pipeline addition on both sides; (b) the allow-list regenerated to NEW `<file>:<content>` shape; (c) the milestone-115 contract supersession note; (d) the CONTRIBUTING regenerate-command update; (e) explicit FR-004 / SC-002 statement that the gate's catch-rate for real changes (new / removed / renamed / signature-changed / relocated walkers) is unchanged; (f) the two local verifications performed (positive + negative). NO `--no-verify` flag.

- [X] T012 Open PR. Title: `fix(ci): walker-audit allow-list ignores line-number drift (closes #347)`. Body includes: (1) issue #347 link; (2) `## Summary` listing the four changes (ci.yml pipeline, allow-list regen, CONTRIBUTING.md snippet, milestone-115 contract supersession); (3) `## Test plan` listing the seven spec acceptance scenarios (positive PR with helper above walker; new walker; renamed walker; signature-changed walker; relocated walker; deleted walker + allow-list entry; OLD-form-line forgiveness on read) as manually-verified checklist items.

---

## Dependencies & Execution Order

```text
T001 (ci.yml pipeline) ──┐
                         ├─→ T003 (regenerate allow-list)
T002 (failure-message)   ┘            │
                                      ▼
                                 T004 (positive test)
                                      │
                                      ▼
                                 T005 (negative test)
                                      │
                                      ▼
                                 T006 (CONTRIBUTING.md)
                                      │
                                      ▼
                                 T007 (115 supersession)
                                      │
                                      ▼
                                 T008 (invariants)
                                      │
                                      ▼
                                 T009 (pre-PR gate)
                                      │
                                      ▼
                                 T010 (mark tasks)
                                      │
                                      ▼
                                 T011 (commit)
                                      │
                                      ▼
                                 T012 (open PR)
```

T001 and T002 both edit the same file (`ci.yml`) in the same `run:` block — sequential by file-coordination rule. T003 regenerates the allow-list AFTER T001+T002 land in the working tree so the regen runs against the now-NEW pipeline shape. T004 and T005 are local verifications that depend on T001+T002+T003 being applied. T006 and T007 are doc updates that depend on T001+T002+T003 (so the doc snippets reflect what's actually in ci.yml). T008–T012 are polish.

## Parallel Opportunities

None. Single-story feature with tight file-coordination dependencies — every task either touches `ci.yml`, the allow-list, or sequentially-ordered docs. The polish-phase tasks are linear.

## Independent Test Criteria

US1 is verified by T004 (positive — gate stays green on noise-only insertion above existing walker) AND T005 (negative — gate fires red on synthetic new walker). T004 covers FR-009 / SC-001 (false-positive elimination); T005 covers FR-004 / SC-002 (signal preservation). The other three reviewer-attention-worthy change classes (rename, signature-change, relocate) are covered by the regex semantics — same matched-line content for the file produces a comparison hit/miss respectively — and don't require separate local synthesis tasks.

## Implementation Strategy

**Single-PR ship**: T001 → T012 in one PR. The whole feature is one user story, ~20 lines of YAML edit + a regenerated text file + two small doc edits. No PR-split required.

**MVP scope**: T001–T012 IS the MVP. There is no follow-up scope; the feature closes #347 entirely in one shot.

**Format validation**: All 12 tasks above use the required checklist format — checkbox + sequential ID (T001…T012) + [US1] label for user-story tasks (Setup + Foundational + Polish have no story label) + description with exact file path(s).
