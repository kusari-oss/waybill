---
description: "Task list for milestone 100 — Windows-host build + run support"
---

# Tasks: Windows-host build + run support

**Input**: Design documents from `/Users/mlieberman/Projects/mikebom/specs/100-windows-host-build/`
**Prerequisites**: plan.md, spec.md (with Clarifications), research.md, data-model.md, contracts/windows-host-contracts.md, quickstart.md

**Tests**: Included. 3 new unit tests for the path-normalization helper + iterative POSIX-only test gating during Windows CI bring-up.

**Organization**: Three user stories converge on a small set of files. US1 (P1, MVP) delivers Windows-host build correctness via the path-normalization helper + chokepoint wiring — this is the headline behavior. US2 (P2) is verification-only — the milestone-004/096/098 binary scanner already works cross-platform; the Windows CI lane validates this empirically. US3 (P2) adds the CI + release-pipeline Windows lanes themselves.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files OR different test functions)
- **[Story]**: User story this task belongs to (US1–US3)
- File paths are workspace-relative.

## Path Conventions

Production code under `mikebom-cli/src/scan_fs/` (new helper + chokepoint) + `mikebom-cli/src/generate/{cyclonedx,spdx}/` (3 defensive emission sites). Infrastructure under `.github/workflows/` (new CI + release jobs). Docs in `README.md`. Zero changes outside these paths per FR-006.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment + audit POSIX-gate posture before touching production code.

- [X] T001 Confirm working branch is `100-windows-host-build`. Run `git status` + `git log -1 --oneline`; verify branch was created by `/speckit-specify` and main is at post-PR-#207 (milestone-099 merge) or later.
- [X] T002 Confirm baseline pre-PR gate passes on macOS/Linux dev host. Run `./scripts/pre-pr.sh` once on the unchanged tree; expect `>>> all pre-PR checks passed.` Isolates any post-edit failure as introduced by milestone 100. Also confirm `cargo tree --target x86_64-pc-windows-msvc -p mikebom` resolves with no missing target-specific deps.
- [X] T003 Audit existing `#[cfg(unix)]` posture per research §1. Run:
    ```bash
    grep -rn '#\[cfg(unix)\]\|use std::os::unix' mikebom-cli/src/ | wc -l
    grep -rl '#\[cfg(unix)\]' mikebom-cli/src/ | wc -l
    grep -nE '^    runs-on:|^  lint-and-test-|^  build-' .github/workflows/ci.yml .github/workflows/release.yml
    ```
    Expected: ~94 `cfg(unix)` gate markers (line-level count — multiple markers per file) spread across ~10 files (file-level count). The two counts differ because most affected files contain multiple gates (the `claimed_inodes` plumbing repeats across each function signature + body site). Both numbers are correct; reviewers comparing spec.md "10 files" to research.md "94 gates" should know this. CI has `lint-and-test`, `lint-and-test-macos`, `lint-and-test-ebpf` jobs; release has `build-linux-x86_64`, `build-linux-aarch64`, `build-macos-aarch64`. The Windows lane + Windows build will be the new additions.

    **Note on fallback correctness** (analyze R1): research §1 records the audit conclusion that all 94 gates correctly isolate POSIX-specific code paths from the Windows-build perspective. T003's grep is a *count* check, not a *correctness* check; the runtime correctness is verified by T013 (CI bring-up). If T013 surfaces a Windows-host code-path failure from a missed gate, T018's iterative-fix loop addresses it inline. This is the intended safety net.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement the `normalize_sbom_path` helper. Blocks US1 (chokepoint + emission-site wiring depends on it). US2 + US3 are file-level independent and don't depend on T004.

- [X] T004 Create `mikebom-cli/src/scan_fs/sbom_path.rs` with `pub fn normalize_sbom_path(&Path) -> String` + `pub fn normalize_sbom_path_str(&str) -> String` + 3 unit tests per `data-model.md §sbom_path.rs`. Forward-slash normalization on Windows; no-op (`String::to_string()`) on Unix. Add `pub mod sbom_path;` to `mikebom-cli/src/scan_fs/mod.rs` module-decl block. Verify:
    ```bash
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast scan_fs::sbom_path::tests 2>&1 | grep "test result:"
    # Expected: ok. 3 passed.
    ```

**Checkpoint**: After T004, the helper exists with passing tests. US1 can wire it; US2/US3 don't depend on it.

---

## Phase 3: User Story 1 — Windows scan emits valid CDX (Priority: P1) 🎯 MVP

**Goal**: A Windows-host mikebom scan emits SBOM JSON with forward-slash-normalized path strings, byte-identical to the equivalent Linux/macOS scan (modulo workspace-root prefix). Cross-host SBOM portability preserved.

**Independent Test**: scan a directory containing a `Cargo.toml` on macOS (current dev host). Inspect emitted CDX. Confirm: (a) no behavioral change vs pre-T004 (Unix branch is no-op `String::to_string()`); (b) existing goldens regression tests still pass with zero diff.

### Implementation for User Story 1

- [X] T005 [US1] Wire the chokepoint in `mikebom-cli/src/scan_fs/mod.rs` at the `ResolvedComponent` builder around line 542 (the package-db-derived ResolvedComponent path). Change `source_file_paths: vec![entry.source_path.clone()]` to `source_file_paths: vec![crate::scan_fs::sbom_path::normalize_sbom_path_str(&entry.source_path)]`. Apply the same normalization to the `source_path` field on `ResolvedComponent` itself if it's emitted to JSON (verify by grepping the cdx_builder + spdx_builder for `source_path` reads).
- [X] T006 [US1] Wire the second chokepoint in `mikebom-cli/src/scan_fs/mod.rs` at the alternate `ResolvedComponent` builder around line 167 (the non-package-db path — file-hash readers, etc.). Same `normalize_sbom_path_str(...)` wrapping pattern. Per `data-model.md §scan_fs/mod.rs chokepoint`.
- [X] T007 [P] [US1] Wire defensive normalization at `mikebom-cli/src/generate/cyclonedx/evidence.rs:~84`. Change `"location": o.location` to `"location": crate::scan_fs::sbom_path::normalize_sbom_path_str(&o.location)` per `data-model.md §CDX / SPDX 2.3 / SPDX 3 emission sites`.
- [X] T008 [P] [US1] Wire defensive normalization at `mikebom-cli/src/generate/spdx/annotations.rs:~244-260` (the D2 evidence.occurrences block). Wrap path-emitting expressions with `normalize_sbom_path_str(...)`.
- [X] T009 [P] [US1] Wire defensive normalization at `mikebom-cli/src/generate/spdx/v3_annotations.rs:~257-272` (same shape for SPDX 3).

### Verification for User Story 1

- [X] T010 [US1] Verify Contract 3 + Contract 4 from `contracts/windows-host-contracts.md`. Run:
    ```bash
    # Unit tests for the helper:
    cargo +stable test -p mikebom --bin mikebom \
        --no-fail-fast scan_fs::sbom_path::tests 2>&1 | grep "test result:"
    # Expected: ok. 3 passed.

    # Goldens regression on macOS — expect zero diff. Forward-slash
    # was already the format on Unix; normalize_sbom_path_str's Unix
    # branch is `String::to_string()` (bytewise identical output).
    cargo +stable test -p mikebom --test cdx_regression --test spdx_regression --test spdx3_regression 2>&1 | grep "test result:"
    # Expected: ok. 9 passed.
    ```
    If ANY golden regenerates on the macOS host, the chokepoint wiring is doing something other than no-op — investigate before proceeding.

**Checkpoint**: US1 complete on the macOS dev host. Forward-slash normalization is in place; Unix behavior is bytewise unchanged. Windows-host behavior is verified later when the Windows CI lane runs (T013 + T015).

---

## Phase 4: User Story 2 — Cross-format binary scanning on Windows (Priority: P2)

**Goal**: A Windows-host mikebom scan correctly identifies ELF, Mach-O, and PE binaries with the same fidelity as Linux/macOS hosts. No new code — the milestone-004 / -096 / -098 binary scanner is already host-platform-agnostic via the `object` crate.

**Independent Test**: the Windows CI lane (T013) runs `cargo test --workspace` which exercises the existing `tests/scan_binary.rs` integration tests + the `tests/binary_id_enrich.rs` mikebom-self spurious-match regression. Any host-platform-specific assumption surfaces as a test failure during Windows CI bring-up.

### Verification for User Story 2

- [X] T011 [US2] Verify Contract 5 from `contracts/windows-host-contracts.md` after the Windows CI lane (T013) lands. **Preferred path**: extend `tests/scan_binary.rs::find_system_binary()` to include Windows-side PE binaries — this gives SC-002 ("PE/ELF/Mach-O all identifiable on Windows") explicit CI coverage rather than implicit-by-graceful-skip:
    ```rust
    // tests/scan_binary.rs:
    fn find_system_binary() -> Option<PathBuf> {
        for candidate in [
            "/bin/ls", "/usr/bin/ls",                  // Linux/macOS
            r"C:\Windows\System32\cmd.exe",            // Windows PE coverage (milestone 100)
            r"C:\Windows\System32\notepad.exe",        // backup
        ] {
            let p = PathBuf::from(candidate);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }
    ```
    AND audit the test bodies (e.g., `scan_system_binary_emits_file_level_and_linkage` around `scan_binary.rs:71`) — they assert `mikebom:binary-class` is one of `elf | macho | pe`, which already accepts PE. The linkage-kind assertion accepts `dynamic | static | mixed` which also accepts PE. The existing tests SHOULD generalize without per-format branches; verify by running them on `windows-latest` via T013.
    **Fallback path** (acceptable only if extension produces flaky tests — e.g., `cmd.exe` triggers an assertion that doesn't hold for PE): leave `find_system_binary()` unchanged; the test gracefully skips on Windows. Document the skip reason explicitly. Choosing the fallback means SC-002 is verified only by the milestone-096/098 binary-scanner unit tests on Windows (which DO run via `cargo test --workspace`), not by an end-to-end PE-scan integration test — acceptable but less defensive.

**Checkpoint**: US2 complete. Either the test extends to cover Windows PE binaries OR gracefully skips. Either way, the existing binary scanner works unchanged on Windows for any user-supplied binary input.

---

## Phase 5: User Story 3 — CI + release pipeline Windows lanes (Priority: P2)

**Goal**: `.github/workflows/ci.yml` gains `lint-and-test-windows`; `.github/workflows/release.yml` gains `build-windows-x86_64`; the next alpha tag push produces a `mikebom-*-x86_64-pc-windows-msvc.zip` artifact alongside the existing 3 tarballs.

**Independent Test**: open a PR with the YAML changes; verify the new Windows CI job appears in the PR's checks list and runs to completion. Push a test tag (e.g., `v0.1.0-alpha.99-windows-test`); verify `release.yml` produces the Windows zip asset.

### Implementation for User Story 3

- [X] T012 [US3] Add the `lint-and-test-windows` job to `.github/workflows/ci.yml` per `research.md §3` + `data-model.md §.github/workflows/ci.yml`. Insert after the existing `lint-and-test-macos` job (around line 245). Job runs `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace` on `windows-latest`. Cache cargo + fixture-repo using the same `actions/cache` + `Swatinem/rust-cache` action SHA-pins as the macOS lane.
- [ ] T013 [US3] Verify Contract 6 — the new CI lane runs successfully. After T012 is committed + pushed to the PR branch:
    ```bash
    # After pushing the branch:
    gh pr checks --watch $(gh pr view --json number --jq '.number')
    # Expected: the lint-and-test-windows job appears in the checks list
    # and reports `success` after run completion.
    ```
    If the lane fails on first run, triage failures into 3 categories per `quickstart.md §Recipe 4`: (a) POSIX-only test needing `#[cfg(unix)]` gate → add gate inline, (b) test asserting non-normalized path output → update assertion to expect forward-slash, (c) mikebom bug exposed on Windows → fix code. Iterate until the lane passes. **Scope-control heuristic**: if the iteration count exceeds ~5 distinct categories of failures (i.e., suggests deeper Windows-host issues than spec anticipated), stop and either (i) descope by adding `#[cfg(unix)]` to the affected test functions OR (ii) open a follow-up ticket for the Windows-specific work AND descope to skip-on-Windows-for-now. Don't blow out the milestone scope chasing every Windows-specific test fix in a single PR.
- [X] T014 [US3] Add the `build-windows-x86_64` job to `.github/workflows/release.yml` per `research.md §4` + `data-model.md §.github/workflows/release.yml`. Insert after the existing `build-macos-aarch64` job (around line 250). Uses `shell: pwsh` for PowerShell steps; produces `.zip` via `Compress-Archive`.
- [X] T015 [US3] Update the `release` aggregation job's `needs:` array in `.github/workflows/release.yml` (around line 250). Add `build-windows-x86_64` to the existing list `[build-linux-x86_64, build-linux-aarch64, build-macos-aarch64]`.
- [ ] T016 [US3] Verify Contract 7 — the new Windows release artifact builds + uploads cleanly. The next alpha tag push (post-milestone-100 merge) exercises this; for in-PR verification, manually trigger `release.yml` via `gh workflow run release.yml --ref 100-windows-host-build` if the workflow supports `workflow_dispatch`, OR rely on visual diff against the existing macOS job + actionlint validation. **Manual smoke-test (post-merge, closes SC-001 end-to-end gap per analyze C2)**: on a Windows host (or via a colleague), download `mikebom-v<version>-x86_64-pc-windows-msvc.zip` from the published pre-release, extract `mikebom.exe`, and run `mikebom.exe sbom scan --path <a Rust project dir>` — confirm a valid CDX SBOM emits with `pkg:cargo/...` components. This is the human-loop verification that complements the in-CI tests; document the smoke-test result in the post-merge release notes.

**Checkpoint**: US3 complete. CI + release pipeline have Windows lanes; the next alpha tag push will produce a Windows artifact.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: README documentation; POSIX-only test gating discovered during Windows CI bring-up; diff-scope audit; pre-PR gate.

- [X] T017 [P] Update `README.md` with the Windows install + usage section per `data-model.md §README.md` + FR-011. ~30 lines: download instructions, PowerShell usage example, note about Linux-specific features not applying.
- [ ] T018 (Discovery + fix) POSIX-only test gating. As Windows CI lane (T013) surfaces test failures, audit each:
    - **POSIX-specific behavior the test asserts** (e.g., `std::os::unix::fs::symlink` calls, dev/inode access, file-mode checks): add `#[cfg(unix)]` to the test function.
    - **Test asserts non-normalized path output**: update the assertion to expect forward-slash output (the milestone-100 normalization is in effect).
    - **mikebom bug exposed on Windows**: **scope-control choice** — if the bug is small + isolated, fix the underlying code inline; if the bug is large (>~30 min of work OR requires architectural changes), defer to a follow-up ticket AND gate the test `#[cfg(unix)]` for now so milestone 100 ships. This is the implementer's discretion; document the deferred bug in the PR description so it's visible.
    
    Known pre-suspected candidates (audit explicitly): `tests/filesystem_walker_*.rs` (milestone-054 symlink-loop tests — may have ungated `std::os::unix::fs::symlink` calls). Iterate until T013 passes.
- [X] T019 Verify Contract 8 — diff scope guardrails. Run:
    ```bash
    # No new Cargo deps (FR-005 / SC-007):
    git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' | wc -l
    # Expected: 0

    # Production code outside scan_fs/ + generate/ + workflows + docs:
    git diff --name-only main | grep -E '^mikebom-cli/src/' \
      | grep -vE '^mikebom-cli/src/scan_fs/' \
      | grep -vE '^mikebom-cli/src/generate/' \
      | wc -l
    # Expected: 0 (test-tree edits don't count here — they're under tests/)

    # Golden regen scope (SC-008 = no schema changes):
    git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
    # Expected: empty (forward-slash was already the format on Linux/macOS;
    # no goldens regenerate)

    # Diff scope allowlist:
    git diff --name-only main | sort
    # Expected only:
    #   .github/workflows/ci.yml
    #   .github/workflows/release.yml
    #   CLAUDE.md                                                  (auto-updated)
    #   README.md
    #   mikebom-cli/src/generate/cyclonedx/evidence.rs
    #   mikebom-cli/src/generate/spdx/annotations.rs
    #   mikebom-cli/src/generate/spdx/v3_annotations.rs
    #   mikebom-cli/src/scan_fs/mod.rs
    #   mikebom-cli/src/scan_fs/sbom_path.rs                       (NEW)
    #   specs/100-windows-host-build/...
    #   (optional) tests/filesystem_walker_*.rs                    (if gates added per T018)
    #   (optional) tests/scan_binary.rs                            (if PE binary support added per T011)
    ```
- [X] T020 Run the mandatory pre-PR gate on macOS/Linux per Contract 9. Run `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh`. Expect: `>>> all pre-PR checks passed.` with zero clippy warnings and zero test failures across the workspace. The 3 new unit tests in `scan_fs::sbom_path::tests` pass; the existing goldens regression continues to pass with zero diff (Unix branch of normalization is no-op).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies. Start immediately.
- **Foundational (Phase 2)**: T004 implements the helper. Blocks US1.
- **US1 (Phase 3, P1, MVP)**: Depends on T004. Wires 2 chokepoint sites + 3 defensive emission sites. T010 verifies macOS-host behavior is unchanged.
- **US2 (Phase 4, P2)**: Verification-only. Depends on US3's CI lane being in place (T013) to exercise binary scanning on Windows.
- **US3 (Phase 5, P2)**: Independent at file level. Touches only `.github/workflows/*` YAML.
- **Polish (Phase 6)**: T017 (README) is parallel-safe with everything. T018 (POSIX test gating) is iterative during T013 bring-up. T019 (diff audit) + T020 (pre-PR gate) gate the merge.

### User Story Dependencies

- **US1 (P1)**: depends on T004 (foundational helper).
- **US2 (P2)**: depends on US3 — the Windows CI lane is the verification mechanism. Functionally a side-effect of US3.
- **US3 (P2)**: independent at file level. Can be implemented + verified before US1 even if T004/T005-T009 haven't landed (the CI lane will just fail until US1 lands due to Windows-incompatible code paths surfacing). Recommended order: US1 first (validate path normalization on macOS), then US3 (turn on Windows CI), then iterate.

### Within Each User Story

- US1: T005 + T006 are sequential (same file). T007 + T008 + T009 are parallel-safe (different files). T010 verifies after T005-T009.
- US2: T011 alone, runs after T013.
- US3: T012 → T013 (CI lane verification depends on the lane existing). T014 + T015 are parallel-safe (different YAML edits in same file, but different sections). T016 verifies after T014 + T015.

### Parallel Opportunities

- T007 / T008 / T009 — 3 parallel-safe defensive-normalization wiring tasks (different files).
- T012 / T014 — parallel-safe (different YAML files: ci.yml vs release.yml).
- T017 — README update, parallel-safe with everything else.

---

## Parallel Example: Phase 3 wire-up

```bash
# After T004 + T005 + T006 land (sequential — same file), the 3
# defensive emission sites can be wired in any order:
Task: "Wire CDX evidence.rs normalization (T007)"
Task: "Wire SPDX 2.3 annotations.rs normalization (T008)"
Task: "Wire SPDX 3 v3_annotations.rs normalization (T009)"
```

After T005-T009 land, T010 runs goldens regression to confirm zero behavioral change on macOS.

---

## Implementation Strategy

### MVP First (US1 only)

US1 alone delivers the headline behavior: mikebom emits forward-slash-normalized SBOM JSON. MVP path:

1. Phase 1: Setup (T001-T003, ~5 min)
2. Phase 2: Foundational (T004, ~15 min — helper + tests)
3. Phase 3: US1 (T005-T010, ~30 min — 5 wire sites + verify on macOS)
4. Phase 6 partial: T020 (pre-PR gate)
5. **STOP and VALIDATE**: confirm macOS dev host behavior is bytewise unchanged. The Windows-side behavior is implicit (Unix-only branch in normalize_sbom_path is no-op).

US2 + US3 layer on after MVP-validation. The full milestone delivers all three stories in a single PR.

### Incremental Delivery (recommended)

Single PR shipping all three stories — the CI lane verification (T013) is the iterative discovery phase that may take 1-3 retries to get clean. Total estimated time: ~3-5 hours single-developer (mostly waiting on CI feedback during T013).

### Single-Developer Strategy

1. T001-T003 (setup, ~5 min)
2. T004 (foundational helper, ~15 min)
3. T005-T010 (US1, ~30 min — chokepoints + emission sites + verify on macOS)
4. T012 + T014 + T015 (CI + release YAML, ~30 min)
5. T013 (CI bring-up + iterate, **30-90 min** — typically 1-3 retries for POSIX-test gating per T018)
6. T011 (US2 binary-scan verification — either extend `find_system_binary()` for Windows or graceful-skip, ~15 min)
7. T016 (release artifact verification, post-merge)
8. T017 + T019 + T020 (README + diff audit + pre-PR gate, ~15 min)

Total: ~3 hours focused time + CI iteration overhead.

---

## Notes

- [P] markers = different files OR different test functions with no shared edit-dependency.
- [Story] label maps task to user story for traceability.
- The path-normalization architecture (single chokepoint + 3 defensive emission sites) means a future maintainer adding a new SBOM-emission code path that bypasses the chokepoint MAY still emit unnormalized paths — the defensive sites only cover the 3 known emission spots. Future code-review should remind contributors to call `normalize_sbom_path_str(...)` on any new path-string emission.
- The 88 `to_string_lossy()` call sites identified during planning are NOT all rewritten — they continue to populate internal `PackageDbEntry.source_path` with native-OS strings (used for logging, file opens, error messages). Only the SBOM-bound path strings (≤4 emission sites) are normalized.
- Windows CI lane uses `windows-latest` runners (Windows Server 2022 as of 2026-05). Microsoft periodically updates the latest tag; if a future Windows Server version breaks the build, pin to a specific version via `runs-on: windows-2022`.
- Pre-PR gate (T020) runs on macOS/Linux dev hosts. The Windows pre-PR experience (running `cargo test` via PowerShell or Git Bash) is documented in CLAUDE.md but not gated by a Windows-side `pre-pr.ps1` (deferred per spec out-of-scope).
- Commit boundary suggestion: single commit per phase (5-6 commits total) OR squash to a single PR-level commit at merge time. The CI bring-up iteration (T013) may produce intermediate commits to fix test gates; squash those at merge.
