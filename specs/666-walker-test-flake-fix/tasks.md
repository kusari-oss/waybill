# Tasks: Fix walk_registry test flake

**Feature Branch**: `666-walker-test-flake-fix`
**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Contract**: [contracts/test-visit-sink.md](./contracts/test-visit-sink.md)

## Phase 1: Setup

- [X] T001 Verify baseline `cargo +stable test -p waybill --lib -- scan_fs::walk_registry` passes at the pre-fix count on the freshly-checked-out `666-walker-test-flake-fix` branch. Record the count in this task's completion note. Establishes the SC-002 no-regression anchor. **Verified 2026-08-27**: task's cited command uses `--lib` which yields 0 results because `scan_fs::` code lives in the BIN target (`--lib` scope has only `pub mod parity;` per m013 minimal library). Correct command: `cargo +stable test -p waybill --bin waybill -- scan_fs::walk_registry` → **27 passed / 0 failed / 0 ignored / 3432 filtered out**. All three target tests present + green: `walker_survives_symlink_loop`, `walker_respects_exclusion_set`, `walker_skips_default_noise_dirs`. SC-002 anchor established at 27. (Note: T014's full-walker-suite regression guard also refers to the walker tests — same fix should apply there.)
- [X] T002 Run the FLAKE REPRODUCTION 100-iteration harness against the pre-fix code to confirm the flake is reproducible locally on macOS-arm64. Command: `for i in $(seq 1 100); do cargo +stable test -p waybill --lib -- scan_fs::walk_registry::walker::tests::walker_survives_symlink_loop scan_fs::walk_registry::walker::tests::walker_respects_exclusion_set scan_fs::walk_registry::walker::tests::walker_skips_default_noise_dirs --test-threads=8 --nocapture 2>&1 | grep -q "test result: ok. 3 passed" || { echo "FAIL at iter $i"; break; }; done`. Expect ≥1 intermittent failure per SC-001 pre-fix rate assumption. If the flake does NOT reproduce in 100 iterations, expand to 500 iterations before assuming the assumption is wrong (per spec assumption #3 fallback). **Attempted 2026-08-27**: same `--lib` → `--bin waybill` correction from T001 applied. Ran 100 iterations `--test-threads=8` → **0 failures**. Ran fallback 500 iterations → **0 failures**. Local macOS-arm64 does NOT reproduce the CI flake at this parallelism. Root-cause analysis: the observed CI failure was likely macos-latest-arm64-CI-scheduler-specific (higher core count, different preemption characteristics vs my dev laptop) OR a rarer race than the 1-in-20 estimate suggested. **Operator decision (2026-08-27)**: proceed with the fix based on theoretical correctness (per-test-owned `Arc<Mutex<Vec<String>>>` is provably race-free by construction — no code path can leak entries between tests). Verification methodology falls back to (a) code-review argument that shared state is eliminated, (b) CI observation post-merge — if the flake reappears, the fix wasn't right. SC-001's local 100-iter target becomes an ADVISORY (achievement not verifiable pre-fix locally) but the theoretical correctness anchor remains.

## Phase 2: Foundational

**None.** This fix is unit-test-only. There are no blocking prerequisites shared across user stories (there is only one user story). Skip directly to Phase 3.

## Phase 3: US1 — Walker unit tests never spuriously fail (Priority: P1) 🎯 MVP

**Goal**: replace the shared `static SEMANTICS_LOG: Mutex<Vec<String>>` with per-test `VisitSink` instances threaded through the walker's m664 contract C4 state slot. Three tests refactored; one file touched.

**Independent Test**: post-fix 100-iteration harness (T012) shows `3 passed / 0 failed` on every iteration on macOS-arm64 AND Linux-x86_64.

### Implementation

- [X] T003 [US1] Add `type VisitSink = std::sync::Arc<std::sync::Mutex<Vec<String>>>` type alias inside `#[cfg(test)] mod tests` in `waybill-cli/src/scan_fs/walk_registry/walker.rs`, immediately after the existing test-mod `use` block. Per data-model.md §"VisitSink". **Done 2026-08-27**: inserted at `walker.rs:404-414` (after the use-block ending at line 402, before the T013 panic-isolation-test comment header at line 416). Type uses fully-qualified `std::sync::Arc<std::sync::Mutex<Vec<String>>>` — self-contained; no dependency on T010 imports at this stage. Rustdoc comment cross-references contracts/test-visit-sink.md (C1-C6). `cargo +stable check -p waybill` → clean.
- [X] T004 [US1] Add `fn push_visit_to_sink(path: &Path, ctx: &SharedWalkerContext<'_>, reader_id: ReaderId)` helper in the same test mod. Body: `let Some(sink) = ctx.state::<Mutex<Vec<String>>>(reader_id) else { return }; sink.lock().unwrap().push(path.file_name().unwrap().to_string_lossy().into_owned());`. Per data-model.md §"record_visit_* callback family". **Done 2026-08-27**: `push_visit_to_sink` helper inserted at `walker.rs:416-434` immediately after the `VisitSink` alias. `path: &std::path::Path` uses fully-qualified path (avoids depending on any `Path` import). Rustdoc includes the "silent no-op" semantics (state absent → early return without disrupting dispatch loop) per contract C1 cross-reference. `cargo +stable check -p waybill` → clean (3.34s incremental).
- [X] T005 [US1] Add the three per-test callback wrappers in the same test mod, immediately after `push_visit_to_sink`:
  - `fn record_visit_loop(p: &Path, ctx: &SharedWalkerContext<'_>) { push_visit_to_sink(p, ctx, ReaderId::new("visitor-loop")); }`
  - `fn record_visit_exclude(p: &Path, ctx: &SharedWalkerContext<'_>) { push_visit_to_sink(p, ctx, ReaderId::new("visitor-exclude")); }`
  - `fn record_visit_noise(p: &Path, ctx: &SharedWalkerContext<'_>) { push_visit_to_sink(p, ctx, ReaderId::new("visitor-noise")); }`
  **Done 2026-08-27**: three wrappers inserted at `walker.rs:436-452` immediately after `push_visit_to_sink`. Each takes `path: &std::path::Path` (fully-qualified, no import dependency) and `ctx: &SharedWalkerContext<'_>`. Rustdoc block above the wrappers explains WHY three near-identical fns instead of one shared callback (the `FileCallback` bare-fn-pointer typedef precludes captures; reader_id must be compile-time-baked). Cross-references contract C3. `cargo +stable check -p waybill` → clean (2.88s incremental).
- [X] T006 [US1] Refactor `walker_survives_symlink_loop` at `waybill-cli/src/scan_fs/walk_registry/walker.rs:486`:
  - Remove the `SEMANTICS_LOG.lock().unwrap().clear();` line at the fn body's top.
  - Add `let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));` immediately after the fn's opening brace.
  - In the `ReaderRegistration { ... }` literal, change `state: None,` → `state: Some(sink.clone()),` (Rust's `CoerceUnsized` handles the `Arc<Mutex<Vec<String>>>` → `Arc<dyn Any + Send + Sync>` cast at the field-assignment site — no `as ...` needed).
  - Change `on_file: Some(record_visit)` → `on_file: Some(record_visit_loop)`.
  - Change the post-run assertion source from `let log = SEMANTICS_LOG.lock().unwrap();` → `let log = sink.lock().unwrap();`. Assertion bodies stay unchanged.
  **Done 2026-08-27**: all 4 changes applied at `walker.rs:534-570`. Used fully-qualified `std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))` for the sink construction — self-contained; T010 imports still deferred (working fine without local `Arc`/`Mutex` at this scope). Test compiles + runs green: `cargo test --bin waybill -- scan_fs::walk_registry::walker::tests::walker_survives_symlink_loop` → 1 passed / 0 failed / 0.01s. CoerceUnsized works exactly as research R1 predicted.
- [X] T007 [US1] Refactor `walker_respects_exclusion_set` at `waybill-cli/src/scan_fs/walk_registry/walker.rs:524`: same 4-part transformation as T006 with slug `exclude` (sink construction, `state: Some(sink.clone())`, `on_file: Some(record_visit_exclude)`, `SEMANTICS_LOG.lock()` → `sink.lock()`). **Done 2026-08-27**: applied at `walker.rs:570-611`. Test compiles + passes: 1 passed / 0 failed / 0.01s.
- [X] T008 [US1] Refactor `walker_skips_default_noise_dirs` at `waybill-cli/src/scan_fs/walk_registry/walker.rs:567`: same transformation with slug `noise` (`state: Some(sink.clone())`, `on_file: Some(record_visit_noise)`, `SEMANTICS_LOG.lock()` → `sink.lock()`). **Done 2026-08-27**: applied at `walker.rs:613-660`. Test compiles + passes: 1 passed / 0 failed / 0.01s.
- [X] T009 [US1] Remove the now-dead `static SEMANTICS_LOG: Mutex<Vec<String>>` line at `waybill-cli/src/scan_fs/walk_registry/walker.rs:477` and the `fn record_visit(path, _ctx)` at line 479. Post-removal, `grep -c "SEMANTICS_LOG" waybill-cli/src/scan_fs/walk_registry/walker.rs` must return 0. **Done 2026-08-27**: both the `static SEMANTICS_LOG` (was line 523 post-T003-T005 insertions) and the shared `fn record_visit` (was lines 525-530) removed. The T014 section-header comment now includes a two-paragraph explanation of the per-test-sink pattern so a maintainer reading the section header understands WHY the tests below construct their own sinks. `grep -c "SEMANTICS_LOG\|fn record_visit\b"` → **0**. `cargo +stable check -p waybill` → clean (6.62s incremental).
- [X] T010 [US1] Verify + add imports needed by the new code inside `#[cfg(test)] mod tests` in `waybill-cli/src/scan_fs/walk_registry/walker.rs`. Check current import state with `grep -E '^\s*use.*(Arc|Mutex|ReaderId)' waybill-cli/src/scan_fs/walk_registry/walker.rs`. REQUIRED imports:
  - `std::sync::{Arc, Mutex}` — for `VisitSink = Arc<Mutex<Vec<String>>>` construction. **Verify presence; add `use std::sync::{Arc, Mutex};` at the top of the test mod if absent.** (Pre-fix state has `Mutex` via `use std::sync::Mutex;` for the static; `Arc` is new to the test mod.)
  - `ReaderId` — already in scope via the pre-fix test mod's parent `use` chain at the walker.rs module head. **Verify with the grep above; add ONLY if missing.**
  Do NOT reorder existing imports. Do NOT introduce a `use super::*;` wildcard. **Done 2026-08-27**: (a) `Arc` was NOT imported at test-mod scope pre-T010 (my T003-T008 code used fully-qualified `std::sync::Arc::new(...)` and worked). Extended the existing test-mod import `use std::sync::Mutex;` → `use std::sync::{Arc, Mutex};` (line 396). (b) `ReaderId` already in scope via the test mod's `use crate::scan_fs::walk_registry::{...ReaderId...}` block (line 400) — verified with the grep. Post-import cleanup: simplified 3 verbose sink constructions `std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))` → `Arc::new(Mutex::new(Vec::new()))` at `walker.rs:533,570,613` AND the `VisitSink` type alias `std::sync::Arc<std::sync::Mutex<Vec<String>>>` → `Arc<Mutex<Vec<String>>>`. All 9 walker tests still pass: `cargo test --bin waybill -- scan_fs::walk_registry::walker::tests::` → 9 passed / 0 failed / 0.01s.

### Verification

- [X] T011 [US1] Local per-file compile check: `cargo +stable check -p waybill --lib 2>&1 | tail -5` must show `Finished` with no errors. Establishes the fix at least compiles before the parallelism harness runs. **Done 2026-08-27**: (a) `cargo +stable check -p waybill --lib` → `Finished 'dev' profile in 0.44s`. Note: `--lib` scope is minimal (parity module only); the walker.rs code lives in the BIN target. (b) Ran extended `cargo +stable check -p waybill --bin waybill --all-targets` → `Finished in 1m 30s` (cold build; includes all integration tests). Both scopes compile clean; no errors. Ready for parallelism harnesses.
- [X] T012 [US1] Local 100-iteration parallelism harness (SC-001 anchor):
  ```bash
  for i in $(seq 1 100); do
    cargo +stable test -p waybill --lib -- \
        scan_fs::walk_registry::walker::tests::walker_survives_symlink_loop \
        scan_fs::walk_registry::walker::tests::walker_respects_exclusion_set \
        scan_fs::walk_registry::walker::tests::walker_skips_default_noise_dirs \
        --test-threads=8 --nocapture 2>&1 | grep -q "test result: ok. 3 passed" || { echo "FAIL at iter $i"; exit 1; }
  done
  echo "PASS: 100 iterations"
  ```
  MUST show `PASS: 100 iterations`. Record the wall-time in this task's completion note for the record. **Done 2026-08-27**: `--lib` → `--bin waybill` correction from T001/T002 applied. Ran 100 iterations `--test-threads=8` → **0 failures in 25s wall-time** (warm compile cache, ~0.25s per iter). SC-001 anchor cleared. Caveat: pre-fix baseline (T002) also passed 100/100 + 500/500 locally, so this run confirms the fix doesn't INTRODUCE new failures but doesn't independently prove flake elimination. CI on macos-latest is the authoritative post-fix verification.
- [X] T013 [US1] Local 100-iteration `--test-threads=1` deterministic-serial check (SC-004 anchor): rerun T012's loop with `--test-threads=1` and confirm same 100/0 result. Verifies the fix doesn't regress the deterministic-serial path (which the pre-fix code trivially passes). **Done 2026-08-27**: 100 iterations `--test-threads=1` → **0 failures in 19s wall-time**. Fix has no `--test-threads=1` regression; SC-004's dual-parallelism-mode guarantee satisfied.
- [X] T014 [US1] Full walker-test-suite parallel run (regression guard on the sibling `empty_registry_produces_empty_output` test): `for i in $(seq 1 50); do cargo +stable test -p waybill --lib -- scan_fs::walk_registry::walker::tests:: --test-threads=8 2>&1 | grep -q "test result: ok" || { echo "FAIL at iter $i"; exit 1; }; done`. MUST show 50/50 pass. **Done 2026-08-27**: `--lib` → `--bin waybill` correction applied. 50 iterations against the FULL walker suite (all 9 tests × `--test-threads=8`) → **0 failures in 10s wall-time**. Sibling tests (`empty_registry_produces_empty_output`, `panicking_reader_does_not_abort_walker`, `descend_into_absent_preserves_default_behavior`, `descend_into_allows_requesting_reader`, `descend_into_scopes_out_non_requesting_readers`, `sibling_lookup_end_to_end`) unaffected by the m666 changes.

**Checkpoint**: MVP complete. The three tests survive parallel scheduling with 100-iteration confidence; the sibling `empty_registry_produces_empty_output` is unaffected.

## Phase 4: Polish

- [X] T015 Verify FR-005 + FR-006 + SC-005 constraints via two guards: (a) `git diff main..HEAD -- waybill-cli/Cargo.toml Cargo.lock waybill-cli/src/scan_fs/walk_registry/mod.rs waybill-cli/src/scan_fs/walk_registry/walk_context.rs waybill-cli/src/scan_fs/walk_registry/dispatch.rs` MUST produce zero lines (Cargo deps + walker's public API surface unchanged). (b) `grep -cE '(^mod testing|::testing::)' waybill-cli/src/scan_fs/walk_registry/walker.rs` MUST return 0 (SC-005 single-file discoverability — no helper module or cross-file `::testing::` reference introduced). Both guards must pass. **Done 2026-08-27**: (a) `git diff main..HEAD` on Cargo.toml + Cargo.lock + walk_registry/{mod,walk_context,dispatch}.rs → **0 lines**. Zero new deps, zero API surface change. (b) `grep -cE '(^mod testing|::testing::)' walker.rs` → **0**. No helper module, no cross-file `::testing::` reference. FR-005 + FR-006 + SC-005 all satisfied.
- [X] T016 Verify FR-007 no-runtime-change guard: `git diff main..HEAD -- waybill-cli/src/scan_fs/package_db/ waybill-cli/src/generate/` MUST produce zero lines. No production emission code touched. **Done 2026-08-27**: `git diff main..HEAD -- waybill-cli/src/scan_fs/package_db/ waybill-cli/src/generate/` → **0 lines**. FR-007 byte-identical-runtime constraint satisfied.
- [X] T017 Verify SC-003 zero-golden-churn: `git diff main..HEAD -- waybill-cli/tests/fixtures/golden/` MUST produce zero lines. **Done 2026-08-27**: `git diff main..HEAD -- waybill-cli/tests/fixtures/golden/` → **0 lines**. Test-only fix — no emission change, no golden churn. SC-003 satisfied.
- [X] T018 Verify contract C3 reader_id uniqueness (regression guard for future tests): `grep -oE 'ReaderId::new\("visitor-[^"]+"\)' waybill-cli/src/scan_fs/walk_registry/walker.rs | sort | uniq -c | awk '$1 != 2 { exit 1 }'`. Should exit 0. Semantics: each unique reader_id MUST appear exactly 2 times in the file (once in a `ReaderRegistration` literal, once in a `record_visit_*` wrapper fn). With 3 tests × (1 registration + 1 wrapper) = 6 total occurrences of `ReaderId::new("visitor-...")` grouped into 3 lines by `sort | uniq -c`, each line should have count 2. Failure modes the check catches: count 1 = orphaned wrapper or missing wrapper; count ≥3 = two tests sharing a reader_id. **Done 2026-08-27**: grep + sort + uniq -c → `2 ReaderId::new("visitor-exclude")` / `2 ReaderId::new("visitor-loop")` / `2 ReaderId::new("visitor-noise")`. awk `$1 != 2 { exit 1 }` → exit 0. **Contract C3 verified**: all 3 reader_ids appear exactly 2 times, no orphaned wrappers, no cross-test sharing.
- [X] T019 `cargo +stable clippy -p waybill --all-targets 2>&1 | grep -E "^error|^warning: "` — MUST be empty (no new lints). The `.unwrap()` calls in the test module are guarded by the crate root's `#[cfg_attr(test, allow(clippy::unwrap_used))]`. **Done 2026-08-27**: `cargo +stable clippy -p waybill --all-targets` → grep output is only the pre-existing `proc-macro-error2 v2.0.1` future-incompat notice (identical on main; unrelated to m666). Zero new lints from the fix. Clippy clean under `-D warnings`.
- [X] T020 Run full pre-PR gate: `./scripts/pre-pr.sh` MUST exit 0 (`>>> all pre-PR checks passed`). This is the CI-equivalent gate (clippy `--all-targets -D warnings` + `cargo test --workspace`). Follow-up: aggregate the workspace-test passed count via `cargo +stable test --workspace 2>&1 | grep "^test result: ok" | awk ...` and verify the count equals T001's baseline + 0 (zero net test additions — this fix modifies 3 existing tests without adding or removing any). Per SC-002. **Done 2026-08-27**: `./scripts/pre-pr.sh` → `>>> all pre-PR checks passed` (exit 0). Follow-up `cargo +stable test --workspace` aggregate → **5192 passed / 0 failed / 14 ignored** — matches the m665 pre-fix baseline exactly (zero net test additions/removals; the fix refactors 3 tests in place). SC-002 no-regression anchor cleared. FR-003 byte-identity preserved.
- [X] T021 Verify walker-audit gate stays green (memory `feedback_walker_audit_local_check`): `bash --noprofile --norc -c 'STRIP="s/^\([^:]*\):[0-9]*:/\1:/"; EXPECTED=$(grep -v "^#" waybill-cli/src/scan_fs/walk.audit-allowlist.txt | grep -v "^$" | sed "$STRIP" | LC_ALL=C sort -u); LIVE=$(LC_ALL=C grep -rEn --include="*.rs" "fn walk[_(]" waybill-cli/src/scan_fs/ | while IFS=: read -r path line content; do prev=$((line - 1)); if [ "$prev" -ge 1 ]; then prev_line=$(LC_ALL=C sed -n "${prev}p" "$path" 2>/dev/null); case "$prev_line" in *"// walker-audit:"*) continue;; esac; fi; printf "%s:%s:%s\n" "$path" "$line" "$content"; done | sed "$STRIP" | LC_ALL=C sort -u); diff <(echo "$EXPECTED") <(echo "$LIVE") && echo PASS || echo FAIL'. MUST show `PASS`. The fix does not introduce or remove any `fn walk_*` functions. **Done 2026-08-27**: ran under `bash --noprofile --norc -c` per memory `feedback_walker_audit_local_check` → **PASS**. Zero drift from main; the fix touches only test-mod code (removes `fn record_visit` which doesn't match the `fn walk_*` pattern; adds `push_visit_to_sink` + `record_visit_*` which also don't match).
- [X] T022 Update auto-memory: append a short reference-memory entry at `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/reference_walker_test_visit_sink_pattern.md` documenting the per-test `VisitSink` pattern (link to `specs/666-walker-test-flake-fix/quickstart.md` for the 5-step recipe, and link to `contracts/test-visit-sink.md` for the C1-C6 contracts). Cross-link from `MEMORY.md` between the existing `feedback_walker_audit_local_check` line and the `reference_no_binary_scan_flag` line. **Done 2026-08-27**: created `reference_walker_test_visit_sink_pattern.md` covering: root cause (shared static log race under cargo parallelism), full pattern (VisitSink alias + push_visit_to_sink helper + per-test wrappers), key constraints (unique reader_id, bare `fn` pointer, CoerceUnsized cast, single-file discoverability), 4 anti-patterns, 3 cross-links ([[m664 registry]], [[EnvGuard]], [[walker-audit gate]]). MEMORY.md index entry appended immediately after the `feedback_walker_audit_local_check` line at line 18.
- [X] T023 Commit + open PR against main. PR title: `fix(m666): eliminate walk_registry test flake via per-test VisitSink pattern`. PR body MUST include `Closes #720` to trigger auto-close per SC-006. Body should link to `specs/666-walker-test-flake-fix/spec.md`, `contracts/test-visit-sink.md`, and the 100-iteration T012 verification note. **Done 2026-08-27**: commit `5350656` (10 files changed, 928 insertions, 24 deletions). Pushed to `origin/666-walker-test-flake-fix`. PR opened at https://github.com/kusari-oss/waybill/pull/722 with title matching the exact task spec. Body includes `Closes #720` on first line (SC-006 anchor), full design-choices summary, verification checklist (all 8 items ticked), verification caveat about local-non-reproduction, links to spec.md + contracts/test-visit-sink.md + quickstart.md.

**Checkpoint**: PR-ready. Full pre-PR gate green, walker-audit clean, memory entry filed, no golden or production-code diff.

## Dependencies

**Sequential chain** (single user story, so linear):

```text
T001 (baseline test count)
  ↓
T002 (reproduce flake pre-fix)
  ↓
T003 → T004 → T005    (new entities + helper + wrappers — sequential, same file)
  ↓
T006 → T007 → T008    (refactor 3 tests — sequential, same file)
  ↓
T009 → T010           (cleanup + imports — sequential, same file)
  ↓
T011 (compile check)
  ↓
T012 (100-iter harness — SC-001)
  ↓
T013 (--test-threads=1 harness — SC-004)
  ↓
T014 (full walker suite regression guard)
  ↓
T015 → T016 → T017 → T018 → T019 (constraint verification — can run in parallel [P] where noted; all reads no writes)
  ↓
T020 (pre-PR gate)
  ↓
T021 (walker-audit gate)
  ↓
T022 (memory entry — [P] with T023; different files)
T023 (commit + open PR)
```

**No cross-story dependencies** — this feature has one user story (US1) so the only phase-crossing dependency is Setup → US1 → Polish.

## Parallel Execution Opportunities

Within a phase, tasks marked `[P]` can run concurrently. Given this is a single-file fix, most tasks touch `walker.rs` and must be sequential. Genuine parallelism opportunities:

**Polish phase (T015-T019)** — all are read-only verification tasks against the working-tree diff:
```
T015 [P] (git diff API surface)
T016 [P] (git diff production code)
T017 [P] (git diff goldens)
T018 [P] (grep reader_id uniqueness)
T019 [P] (clippy)
```

**Polish phase (T022 + T023)** — memory entry (in `~/.claude/`) and PR opening (in the git repo) touch different filesystems:
```
T022 [P] (memory entry)
T023 [P] (PR open)  — sequential-recommended: land T022 before T023 so the PR body can reference the memory entry
```

## Implementation Strategy

**MVP scope**: US1 (the only user story). Deliverable = green 100-iteration harness on macOS-arm64.

**Delivery cadence**: single-PR fix. No incremental releases; no follow-up milestones. The scope is exactly the 3 tests refactored.

**Rollback plan**: if the fix regresses any pre-existing test, `git revert` the single commit. No downstream artifacts affected (no goldens, no production code, no memory dependencies).

**Estimated wall-time**:
- T001-T002 (baseline + flake repro): ~10 min (T002 dominates — 100 iterations × ~1s per compile-cached iter × ~10 fails to observe)
- T003-T010 (fix implementation): ~30 min (edit + compile-check loop)
- T011-T014 (verification harnesses): ~15 min (parallelism, 100 + 100 + 50 iter each × ~1s)
- T015-T021 (constraint checks + pre-PR + walker-audit): ~10 min (T020 dominates — pre-PR is ~5 min on this branch)
- T022-T023 (memory + PR): ~5 min

**Total**: ~70 min end-to-end for a single-maintainer session.

## Format Validation

All tasks conform to the required checklist format:

- ✅ Checkbox: every task starts with `- [ ]`
- ✅ Task ID: sequential `T001`-`T023`
- ✅ `[P]` marker: applied only to genuinely parallelizable tasks
- ✅ `[US1]` label: applied to every task in Phase 3 (Implementation + Verification)
- ✅ File paths: every implementation task names the exact file it touches
- ✅ Description: every task starts with a clear action verb + concrete scope

Total tasks: **23** (2 Setup + 0 Foundational + 12 US1 + 9 Polish).
