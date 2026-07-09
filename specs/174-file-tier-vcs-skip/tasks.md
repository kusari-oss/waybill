---

description: "Tasks for milestone 174 — exclude `.git`, `.hg`, `.svn` VCS metadata directories + `.git` submodule pointer files from the m133 file-tier walker"
---

# Tasks: File-tier walker VCS metadata exclusion

**Input**: Design documents from `/specs/174-file-tier-vcs-skip/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/walker-exclusion.md, quickstart.md

**Tests**: 5 new unit tests inline in `walker.rs::tests` covering FR-001, FR-002, FR-006 + 1 new integration test at `mikebom-cli/tests/file_tier_vcs_skip.rs` covering end-to-end SBOM emission.

**Organization**: 3 user stories from spec.md (US1 P1 clean SBOM from git-cloned repos + US2 P1 first-party scripts + US3 P2 `--exclude-path` compat) + setup + foundational + polish. Small feature — ~15 tasks total. All LLM-executable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm branch state + verify current pre-fix bug exists (reproduces the langflow audit).

- [X] T001 Confirm current branch is `174-file-tier-vcs-skip` via `git rev-parse --abbrev-ref HEAD`. If not, `git checkout 174-file-tier-vcs-skip`.

- [X] T002 Verify pre-174 bug reproduces on ANY git-cloned repo. Invoke `MIKEBOM_FIXED_TIMESTAMP="2026-01-01T00:00:00Z" cargo +stable run --release -p mikebom -- sbom scan --path . --format cyclonedx-json --output cyclonedx-json=/tmp/pre-174-repro.cdx.json --no-deep-hash` from the mikebom repo root (or substitute `.` with any other git-cloned repo path — the bug reproduces on any working tree with a populated `.git/hooks/`). Then `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(startswith(".git/"))] | length' /tmp/pre-174-repro.cdx.json` MUST return a positive integer (≥14 for typical git-cloned repos since git installs 14 hook samples by default). Confirms the fix targets a real, currently-reproducible bug.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Add the shared `VCS_METADATA_NAMES` const + `is_vcs_metadata_name` helper used by BOTH US1 (directory-descend gate) and US2 (file-form gate). BLOCKS Phase 3 + 4.

- [X] T003 Add `const VCS_METADATA_NAMES: &[&str] = &[".git", ".hg", ".svn"];` at module scope near the top of `mikebom-cli/src/scan_fs/file_tier/walker.rs` (immediately after existing imports; before the `WalkerConfig` struct). Include doc comment per data-model.md Entity 1 covering: FR-001 / FR-006 closed-set semantics, exact base-name match, case-sensitive per Assumptions #3, and that adding a fourth name requires a follow-up milestone. Also add `fn is_vcs_metadata_name(candidate: &std::path::Path) -> bool` (data-model.md Entity 2): matches candidate's base name via `candidate.file_name().and_then(|s| s.to_str())` against `VCS_METADATA_NAMES` iterator; on match emits `tracing::debug!(candidate = %candidate.display(), "file-tier walker: skipping VCS metadata")` and returns `true`; else returns `false`. Pure function, no I/O.

**Checkpoint**: T003 builds clean via `cargo +stable build -p mikebom --bin mikebom`. Phase 3 (US1) + Phase 4 (US2) can start.

---

## Phase 3: User Story 1 — Clean SBOM from git-cloned repository (Priority: P1) 🎯 MVP

**Goal**: `mikebom sbom scan --path <git-cloned-repo>` produces an SBOM with ZERO components whose `mikebom:source-files` annotation contains a path starting with `.git/`. Delivered via the `should_skip` closure change at `walker.rs:94`.

**Independent Test**: quickstart.md Path A — scan the mikebom repo itself post-fix; assert `jq '...startswith(".git/")...' | length == 0`.

- [X] T004 [US1] Replace `should_skip: &|_candidate, _root| false` at `mikebom-cli/src/scan_fs/file_tier/walker.rs:94` with `should_skip: &|candidate, _root| is_vcs_metadata_name(candidate)`. Preserves the closure's type contract per `walk.rs:127`; the change is a single-line body edit. This kills directory-descent into `.git/`, `.hg/`, `.svn/` at any depth (FR-001 + FR-008).

- [X] T005 [US1] Add 3 unit tests in the existing `walker.rs::tests` block (`#[cfg(test)] mod tests` at ~line 285):
  - `walker_skips_dot_git_directory`: construct `tempfile::tempdir()`; create `<root>/.git/hooks/pre-commit.sample` with arbitrary bytes; call `walk_file_tier(root, &default_cfg)`; assert returned `Vec<FileTierEntry>` is empty AND `WalkerStats.emitted == 0`.
  - `walker_skips_dot_hg_directory`: same shape with `<root>/.hg/store/data/foo.i`.
  - `walker_skips_dot_svn_directory`: same shape with `<root>/.svn/pristine/xx/xxxx.svn-base`.
  Each test uses `tempfile::tempdir()` for isolation (Constitution VII).

- [X] T006 [US1] Add integration test file `mikebom-cli/tests/file_tier_vcs_skip.rs` (~50 lines):
  - Use `mod common; use common::bin;` pattern from m172/m173 tests.
  - `t006_us1_git_hook_samples_not_emitted`: synthesize a 4-file repo in a tempdir — `<tmp>/.git/hooks/pre-commit.sample` + `<tmp>/.git/HEAD` + `<tmp>/dev.start.sh` + `<tmp>/.gitignore`. Invoke `Command::new(bin())` with `--path <tmp>` + `--output <sbom>` + `--no-deep-hash`. Parse emitted CDX SBOM.
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(startswith(".git/"))] | length' == 0`.
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(. == "dev.start.sh")] | length' == 1` (US2 preview — first-party script surfaces).
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(. == ".gitignore")] | length' == 1` (FR-006 — `.gitignore` is not VCS metadata, surfaces normally).

**Checkpoint**: US1 delivered — no `.git/hooks/*.sample` components in emitted SBOMs; first-party scripts and `.gitignore` still surface.

---

## Phase 4: User Story 2 — First-party scripts still surface + similar-name protection (Priority: P1)

**Goal**: file-form `.git` (git-submodule pointer file) is also excluded, but similar-name directories/files (`.github`, `.githooks`, `.gitignore`, `.gitattributes`, `.gitmodules`) are preserved intact.

**Independent Test**: unit test with `.github/`, `.githooks/`, `.gitignore` at tempdir root — all 3 emitted; unit test with `<tmp>/.git` FILE containing `gitdir: ...` — zero entries.

- [X] T007 [US2] Add file-form check inside the visit callback at `mikebom-cli/src/scan_fs/file_tier/walker.rs:98` — insert BEFORE `symlink_metadata` at line 102 per contracts/walker-exclusion.md ordering constraint. Exact code:
  ```rust
  // Milestone 174 FR-002: skip file-form VCS metadata (git submodule
  // pointer file). MUST run BEFORE symlink_metadata so a symlink
  // named `.git` is also skipped.
  if is_vcs_metadata_name(abs_path) {
      return;
  }
  ```
  Preserves stats counters as-is (no new counter category per FR-005 — the excluded files disappear from stats accounting entirely).

- [X] T008 [US2] Add 2 unit tests in `walker.rs::tests` block:
  - `walker_skips_dot_git_submodule_file`: tempdir with `<root>/submodule/.git` as a FILE (not directory) containing `gitdir: ../.git/modules/submodule\n`; call `walk_file_tier(root, &default_cfg)`; assert zero entries. Verifies FR-002.
  - `walker_preserves_similar_names`: tempdir with `<root>/.github/workflows/ci.yml` + `<root>/.githooks/pre-commit` + `<root>/.gitignore` + `<root>/.gitattributes` + `<root>/.gitmodules`; call `walk_file_tier`; assert each of the 5 files IS present in the returned entries (matched via their content SHA or by walking `entry.paths`). Verifies FR-006.

- [X] T008b [US2] Add FR-009 log-level unit test `walker_vcs_skip_does_not_emit_info_log` in `walker.rs::tests`. Structural guarantee (the code uses `tracing::debug!` which cannot produce INFO-level output at the macro level) is already in place; this test is a belt-and-braces regression gate against a future change that accidentally upgrades the log level. Structure: install a `tracing_subscriber::fmt::Subscriber` with `EnvFilter::new("info")` capturing to an in-memory `Vec<u8>` buffer via `.with_writer(...)`; scan a tempdir containing `<root>/.git/HEAD` (or similar VCS-metadata subtree); after scan, assert the captured buffer does NOT contain the substring `"skipping VCS metadata"`. Dev-dep note: `tracing-subscriber` is already a workspace dep for the mikebom-cli crate; no new Cargo additions.

- [X] T008c [US2] Add FR-007 bare-repo unit test `walker_bare_repo_completes_successfully` in `walker.rs::tests`. Synthesize a bare-repo shape at the tempdir top level: `<root>/HEAD` (containing `ref: refs/heads/main\n`), `<root>/refs/heads/main` (containing a fake SHA), `<root>/config` (containing `[core]\n\tbare = true\n`), `<root>/objects/.gitkeep` (empty). Call `walk_file_tier(root, &default_cfg)`. Assert: the call returns without panicking (structural verify that no assertion or unwrap fires). Do NOT assert on component count — per FR-007 (post-remediation) the tool MAY emit file-tier components for text files at the bare repo's own root because the m174 exclusion is scoped to descendants NAMED `.git`/`.hg`/`.svn`, not to bare-repo internal-layout detection. This test documents the deliberate scope limit; a follow-up milestone MAY add bare-repo detection if operator demand surfaces.

**Checkpoint**: US2 delivered — file-form `.git` skipped; all similar-name files preserved; bare-repo scans complete successfully (component-count behavior deliberately unconstrained per FR-007 post-remediation).

---

## Phase 5: User Story 3 — `--exclude-path` still composes (Priority: P2)

**Goal**: operator's `--exclude-path` flag continues to work independently of the built-in VCS exclusion; both compose without conflict.

**Independent Test**: integration test with `--exclude-path 'ignored/**'` + a `.git/` subtree + an `ignored/` subtree; both are excluded, first-party content survives.

- [X] T009 [US3] Add integration test `t009_us3_exclude_path_composes` in `mikebom-cli/tests/file_tier_vcs_skip.rs`:
  - Synthesize a 4-file repo: `<tmp>/.git/hooks/pre-commit.sample` + `<tmp>/ignored/junk.sh` + `<tmp>/dev.start.sh` + `<tmp>/ci-test.sh`.
  - Invoke `Command::new(bin())` with `--path <tmp>` + `--exclude-path 'ignored/**'` + `--output <sbom>` + `--no-deep-hash`.
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(startswith(".git/"))] | length' == 0` (built-in VCS exclusion).
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(startswith("ignored/"))] | length' == 0` (operator's `--exclude-path`).
  - Assert: `jq '[.components[]?.properties[]? | select(.name == "mikebom:source-files") | .value | fromjson | .[] | select(. == "dev.start.sh" or . == "ci-test.sh")] | length' == 2` (first-party content preserved).

- [X] T010 [US3] Add integration test `t010_us3_redundant_exclude_path_git_is_noop`:
  - Same fixture as T006 (`<tmp>/.git/hooks/pre-commit.sample` + `<tmp>/dev.start.sh` + `<tmp>/.gitignore`).
  - Invoke scan with a redundant `--exclude-path '.git/**'` flag (the pre-174 operator workaround).
  - Assert: emitted SBOM is byte-identical (modulo timestamp / serial number) to the T006 output with no `--exclude-path` flag. Verified via `jq -S 'del(.metadata.timestamp, .serialNumber)'` diff. Confirms SC-006 — operators removing their pre-174 workaround see no behavioral change.

**Checkpoint**: US3 delivered — `--exclude-path` composes cleanly with the built-in VCS exclusion; redundant `.git/**` pattern is a harmless no-op.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Walker-audit CI-gate + pre-PR gate + diff scope check.

- [X] T011 [P] Run walker-audit CI-gate locally per memory `feedback_walker_audit_local_check`. m174 modifies `walker.rs` which is inside `scan_fs/` — the audit's grep `fn walk[_(]` will match `walk_file_tier` unchanged (already allowlisted). Expected: PASS (allowlist byte-identical to live grep). Command per m171 T033 precedent — the CI-step-1 script from `.github/workflows/ci.yml`.

- [X] T012 [P] Diff working tree against `main` per SC-003. Expected paths changed:
  - `mikebom-cli/src/scan_fs/file_tier/walker.rs` (T003 + T004 + T007 + T005 + T008 tests)
  - `mikebom-cli/tests/file_tier_vcs_skip.rs` (T006 + T009 + T010 — new file)
  - `CLAUDE.md` (auto-updated by /speckit-plan)
  - `specs/174-file-tier-vcs-skip/**` (new)
  Verify SC-003 explicitly: `git diff main --stat -- 'mikebom-cli/tests/fixtures/golden/**'` MUST return empty (no golden delta because no golden currently contains a `.git/` subtree per research §R4).

- [X] T013 Run `./scripts/pre-pr.sh` per test-plan validation. Verify green — `>>> all pre-PR checks passed.` Enumerate any `^---- .+ stdout ----` failure lines before claiming green per memory `feedback_prepr_gate_bails_on_first_failure`. This exercises the 5 new unit tests (T005 + T008) + 3 new integration tests (T006 + T009 + T010) + the 33-golden byte-identity regression suite.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001-T002. No prerequisites.
- **Foundational (Phase 2)**: T003 — depends on Phase 1. **BLOCKS Phase 3, 4, 5**. Both `should_skip` closure (US1) and visit-callback file-form check (US2) need the const + helper.
- **User Story 1 (Phase 3, P1 MVP)**: T004-T006 — depends on Phase 2 T003 complete.
- **User Story 2 (Phase 4, P1)**: T007-T008 — depends on Phase 2 T003 complete. Can run in PARALLEL with Phase 3.
- **User Story 3 (Phase 5, P2)**: T009-T010 — depends on Phase 3 T004 + Phase 4 T007 complete (needs both gates working to test composition).
- **Polish (Phase 6)**: T011-T013 — depends on Phases 3+4+5 complete.

### Within User Story 1

Sequential (all touch `walker.rs`):
1. **T004** (closure change) — sequential prereq
2. **T005** (3 unit tests) — depends on T004; adds tests to the same file
3. **T006** (integration test) — depends on T005 (unit tests establish correctness before end-to-end verification)

### Within User Story 2

Sequential (all touch `walker.rs`):
1. **T007** (visit-callback check) — sequential prereq
2. **T008** (2 unit tests) — depends on T007

### Cross-Story Parallel Windows

- **Phase 3 (US1) and Phase 4 (US2)**: touch the SAME file (`walker.rs`) so must run SEQUENTIALLY, not in parallel. Recommended: do US1 first (MVP), then US2.
- **Phase 5 (US3)**: only touches `tests/file_tier_vcs_skip.rs`; can run in PARALLEL with US2 once US1 T004 lands (US3 tests depend on the closure being in place).
- **Phase 6 polish**: T011 + T012 parallel [P] (different concerns); T013 sequential last.

### Parallel Opportunities Summary

- **Phase 1**: T001 → T002 sequential.
- **Phase 2**: T003 single task, no parallelism.
- **Phase 3 US1**: T004 → T005 → T006 sequential.
- **Phase 4 US2**: T007 → T008 sequential. Can run in parallel with Phase 3 T006 (integration test file is different from walker.rs).
- **Phase 5 US3**: T009 → T010 sequential (both in the same test file — same-file edits are inherently sequential per the [P] semantic of "different files"; the tests are logically independent but merging append-order avoids conflicts).
- **Phase 6**: T011 + T012 parallel [P]; T013 sequential last.

### Independent Test Criteria per User Story

- **US1**: quickstart.md Path A — scan mikebom repo, assert zero `.git/` paths in emitted `mikebom:source-files` annotations.
- **US2**: unit tests demonstrating `.git` file-form skipped + `.github`/`.githooks`/`.gitignore` preserved.
- **US3**: integration test with `--exclude-path` composing cleanly with built-in VCS exclusion + byte-identical output when operator's pre-174 workaround (redundant `--exclude-path '.git/**'`) is left in place.

### MVP Scope

**Suggested MVP**: US1 alone (T001-T006 + T011-T013). Delivers the reported bug fix. US2 (file-form + similar-name protection) and US3 (`--exclude-path` composition) are important robustness stories but the directly-reported bug is solved by US1.

**Recommended**: land all three stories in one PR. Estimated ~200 lines source + tests total; splitting adds process overhead disproportionate to size (matches the m172 + m173 rationale).
