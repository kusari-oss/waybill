---

description: "Task list for m775 — single-flight Go preflight + go.work directive tolerance"
---

# Tasks: Single-Flight Go Preflight + go.work Directive Tolerance (m775)

**Input**: Design documents from `/specs/775-preflight-single-flight/`
**Prerequisites**: plan.md (Phase 0/1 complete), spec.md (post-clarify, 2 clarifications), research.md (R1–R7), data-model.md, contracts/ (2 files), quickstart.md

**Tests**: Required by design. FR-015 mandates an automated assertion on the preflight counter; Contracts 1/2/3 of `preflight-single-flight.md` each specify a concurrency test; Contracts 2/3/4 of `gowork-directive-vocabulary.md` specify parser tests. Per research R4, coordination tests use an injectable work function so they need no Go toolchain and run in standard CI (Constitution Principle VII).

**Organization**: Two independent user stories. US1 (P1) carries the milestone's measured value; US2 (P2) is separable and can ship or split without rework.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: Required in Phase 3 (US1) and Phase 4 (US2) only
- Every task cites an exact file path

## Path Conventions

Single Cargo workspace at repo root. Three production files touched: `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs` (US1 primary, US2 lenient parser), `waybill-cli/src/scan_fs/package_db/golang/gowork.rs` (US2 primary), `waybill-cli/src/scan_fs/package_db/mod.rs` (US1 counter plumbing + log field). No new files under `src/`; new tests land in in-file `#[cfg(test)]` modules per research R4.

---

## Phase 1: Setup

**Purpose**: Capture the pre-milestone baseline that SC-001, SC-003, and SC-004 are measured against.

- [X] T001 Confirm branch `775-preflight-single-flight` is checked out and the tree is clean via `git status --short && git branch --show-current`. — verified: branch `775-preflight-single-flight`, tree clean apart from the CLAUDE.md agent-context update.
- [X] T002 Build a pre-milestone baseline binary from `main` in a worktree: `git worktree add /tmp/waybill-main main && cd /tmp/waybill-main && cargo build -p waybill --release`. This binary is the comparison anchor for SC-001 (wall time), SC-002 (byte-identity), and SC-009 (single-workspace overhead). — done: worktree at `/tmp/waybill-main`, release binary built from `68d0768`.
- [X] T003 [P] Capture the SC-001 baseline: run `time /tmp/waybill-main/target/release/waybill --offline sbom scan --path /tmp/test-kubernetes --no-deep-hash --format cyclonedx-json --output /tmp/base.cdx.json` twice, record the better wall time. Reference measurements on the profiling host were 26.29s / 28.83s. Retain `/tmp/base.cdx.json` for the SC-002 byte-identity comparison. — done: baseline wall 28.32s / 26.64s (best 26.64s); baseline CDX retained for the SC-002 diff.
- [X] T004 [P] Recreate the `go`-binary logging shim used during profiling (a wrapper script on `PATH` that records each invocation's argv and duration), and capture the baseline preflight count by scanning `/tmp/test-kubernetes` with it. Expected baseline: 22 `go list all` invocations totaling ~198s. This is the independent cross-check for SC-003/SC-004 and validates that the FR-015 counter added in T012 counts real spawns rather than requests. — done: shim baseline confirms **22** `go list all` calls totaling **201.24s**, plus 38 `go mod why` (69.93s).

---

## Phase 2: Foundational

**Purpose**: Nothing blocks both stories — US1 (`mod_why.rs` preflight branch + `mod.rs`) and US2 (`gowork.rs` vocabulary + `mod_why.rs` lenient parser) touch disjoint code paths and can proceed in either order or concurrently.

- [X] T005 Confirm the two stories' edit regions do not overlap: US1 touches `analyze_main_module`'s preflight branch (~`waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:676-726`), `SharedPreflightCache` (~`mod_why.rs:299`), and `MainModuleAnalysis` (~`mod_why.rs:380`); US2 touches the lenient `parse_go_work` (~`mod_why.rs:69-110`) and all of `gowork.rs`. Both stories modify `mod_why.rs`, so if worked concurrently, serialize the actual file writes. — verified during planning: US1 edits `analyze_main_module`'s preflight branch, `SharedPreflightCache`, `MainModuleAnalysis`; US2 edits the lenient `parse_go_work` and `gowork.rs`. Disjoint apart from both living in `mod_why.rs`.

**Checkpoint**: No code changes yet. Baselines captured; edit regions confirmed disjoint.

---

## Phase 3: User Story 1 - Single-flight preflight per go.work scope (Priority: P1) 🎯 MVP

**Goal**: Exactly one `go list all` spawn per `go.work` scope per scan, with waiters reusing the claimant's outcome, distinct scopes never serializing against each other, and the spawn count observable on the FR-013 summary line.

**Independent Test**: Scan `/tmp/test-kubernetes` and read the preflight-invocation count from the classifier summary line — must be ≤ 6 (was 22). Cross-check with the T004 shim. Wall time ≤ 21s per SC-001, output byte-identical per SC-002.

### Phase 3a — Data model

- [X] T006 [US1] In `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`, add the `inflight` field to `SharedPreflightCache` (~line 299): a map from scope root directory to a shared, per-scope single-flight cell holding an optional completed outcome. Document that `entries` remains the fast-path authority and `inflight` exists solely to serialize the slow path. Reference: data-model.md § "Modified entities — US1"; research R1. — done: `inflight` map added with the stampede rationale documented.
- [X] T007 [US1] In the same file, add the `preflight_spawned` field to `MainModuleAnalysis` (~line 380). Doc-comment MUST state that it is true only when THIS call performed an actual subprocess spawn — false for cache-fast-path reads and for waiters — and that counting spawns rather than requests is what makes SC-003 meaningful (a request counter reads 39 both before and after the fix). Reference: research R3. — done: `preflight_spawned` added; doc-comment states spawns-not-requests.
- [X] T008 [US1] Ensure every construction site of `MainModuleAnalysis` in `mod_why.rs` sets `preflight_spawned` correctly, including the early-return paths that skip the preflight entirely (budget already exhausted, no `go` toolchain). Those paths set it false — they spawned nothing. — verified: `MainModuleAnalysis` derives `Default`, so both pre-preflight `return analysis` paths report `false` correctly.

### Phase 3b — Coordination logic

- [X] T009 [US1] Rewrite the shared-scope preflight branch in `analyze_main_module` (`waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`, ~lines 681-724), replacing the check-release-work-reacquire pattern. New shape per data-model.md's coordination diagram: (1) acquire the cache mutex briefly — return a cloned completed outcome if `entries` has one, else fetch-or-insert this scope's cell and clone the handle out; (2) release the cache mutex; (3) acquire the cell; (4) if the cell is empty, this call is the claimant — run the preflight, write the outcome into both the cell and `entries`, set `preflight_spawned = true`; if the cell is populated, this call waited — read the memoized outcome, leave `preflight_spawned = false`; (5) release the cell. **The cache mutex MUST NOT be held across the preflight** (FR-004) and the cell MUST be per-scope, never global (FR-003). Delete the stale comment claiming the race is "very rare" with "a bounded 1-extra-preflight-per-scope worst case" — it is what this milestone disproves. — done: preflight branch now calls `preflight_single_flight`; stale "very rare" comment removed.
- [X] T010 [US1] Verify the cell is *fetched-or-inserted under the cache mutex and cloned out*, not constructed locally per caller. A locally-constructed cell compiles and passes a single-threaded test while providing zero mutual exclusion — this is the most likely way to implement T009 wrongly (quickstart § "Troubleshooting", first entry). — verified programmatically: cell is `.entry().or_default().clone()` under the cache mutex, not locally constructed.
- [X] T011 [US1] Confirm mutex acquisition uses `.expect("<descriptive message>")` rather than `.unwrap()` throughout the new code — the `waybill-cli` crate root denies `clippy::unwrap_used`, and `--all-targets` enforces it in test modules too. Poisoning semantics are intentional per research R2: a panicking claimant propagates to that scope's waiters rather than leaving them blocked (NFR-002). — verified: zero `.unwrap()` in the new code; all acquisitions use `.expect()`.

### Phase 3c — Counter plumbing (FR-015)

- [X] T012 [US1] In `waybill-cli/src/scan_fs/package_db/mod.rs`, add the `preflight_invocations` counter field to `GoModWhyOutcome` (~line 1345) and aggregate `analysis.preflight_spawned` into it at both accumulation sites — the parallel-worker path and the serial fallback path (~lines 1244 and 1308, where `workspace_modules` is already aggregated). Mirror the m231 `workspace_modules` pattern exactly. Reference: research R3. — done: `preflight_invocations` field + aggregation at both accumulation sites.
- [X] T013 [US1] In the same file, add the counter as one additional field on the FR-013 summary log line (~lines 2213-2226). Every existing field (`analyzed`, `prod`, `test`, `not_needed`, `unresolved`, `unknown_marked`, `workspace_modules`, `skipped`, `elapsed_ms`) MUST retain its name, meaning, position, and firing conditions — the new field is purely additive (FR-010, Contract 5). — done: additive `preflight_invocations=` field; all nine pre-existing fields unchanged.

### Phase 3d — Tests

- [X] T014 [US1] Introduce the test seam research R4 requires, **by extracting the single-flight coordination block from T009 into a private helper that takes the preflight work as a closure parameter**; `analyze_main_module` calls that helper with the real `run_preflight`. **Do NOT add a parameter to `analyze_main_module` itself** — it is `pub` (`waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:648`), so a signature change would ripple to every call site and widen the milestone's blast radius for no test benefit. Tests call the private helper directly with a work function that records invocations and blocks. Production behavior must be identical. This is what lets T015–T018 assert FR-001/FR-002/FR-003 deterministically in CI with no Go toolchain (Constitution Principle VII). — done per F3: private `preflight_single_flight` helper taking the work as a closure; `analyze_main_module`'s public signature unchanged.
- [X] T015 [US1] Add a test in `mod_why.rs`'s `#[cfg(test)]` module asserting Contract 1: N threads (N ≥ 4) request the preflight for a single scope concurrently; the injected work function is invoked exactly once, and every thread receives a successful outcome. — passing: 8 threads / 1 scope → exactly 1 invocation.
- [X] T016 [US1] [P] Add a test asserting Contract 2 (failure propagation): the injected work function returns a failure outcome; assert it ran exactly once and that all N waiting threads observe that same failure — not a synthesized or differing one. This is the m771 FR-007 skip semantic preserved through the new path. — passing: 6 threads observe the claimant's failure verbatim; 1 invocation.
- [X] T017 [US1] [P] Add a test asserting Contract 3 (cross-scope concurrency): two distinct scopes, with an injected work function that blocks on a barrier requiring both scopes to be in flight simultaneously. The test passes only if both entered concurrently. **A serializing implementation — e.g. holding the cache mutex across the preflight — deadlocks this barrier and fails by timeout.** This is the test that rules out the naive fix, which would satisfy every other contract. Give the barrier wait a bounded timeout so failure reports as an assertion rather than hanging CI. — passing, and **mutation-tested**: the naive "hold cache mutex across spawn" implementation passes 4/5 tests and is caught ONLY by this one, failing as a bounded assertion in 5.01s. Rewritten to use a deadline-bounded rendezvous instead of `Barrier` (which has no timeout and would hang CI).
- [X] T018 [US1] [P] Add a test asserting Contract 5 (counter accuracy): with the injected work function, drive a mix of scopes and loose workspaces and assert the aggregated count equals actual invocations of the work function — verifying the counter counts spawns, not requests. — passing: 3 scopes × 4 requests → 3 spawns; completed-scope request takes the cache fast path.

- [X] T018b [US1] [P] Add a test asserting NFR-002 (panic safety): inject a work function that panics on the first call. Assert that waiters for that scope terminate — failing fast rather than blocking — within a bounded timeout, and that the failure surfaces rather than being silently swallowed into a passing classification (Principle III, Fail Closed). Panic-under-lock is precisely the behavior code review gets wrong, which is why research R2's poisoning semantics need an executable assertion rather than an inspection step. — passing: panicking claimant propagates; a later waiter terminates within 5s rather than blocking (NFR-002).

### Phase 3e — Empirical validation

- [X] T019 [US1] Build release (`cargo build -p waybill --release`) and verify SC-001 per quickstart Step 1: scan `/tmp/test-kubernetes` twice, take the better wall time, assert ≤ 21s against the T003 baseline. Scratch verification measured 18.73s. — done: wall 22.24s / 18.63s (best **18.63s**) vs 26.64s baseline. SC-001 ≤21s ✓
- [X] T020 [US1] Verify SC-003 per quickstart Step 2: run with informational logging and assert the summary line's preflight-invocation count is ≤ 6 (baseline 22). Cross-check against a fresh run under the T004 shim — the shim's `go list all` count MUST equal the reported counter, proving the counter tracks real spawns. — done: counter reports `preflight_invocations=5`; shim independently confirms exactly 5 `go list all`. Counter tracks real spawns ✓ SC-003 ≤6 ✓
- [X] T021 [US1] Verify SC-004 per quickstart Step 3: sum `go list all` wall time from the shim run; assert ≥ 90% reduction from the ~198s baseline. Note for the record: summed `go mod why` wall may *rise* while scan wall falls sharply (scratch: 68.5s → 84.8s with wall down 29%) — summed wall across concurrent processes is not a cost metric and is not a regression signal. — done: `go list all` wall 201.24s → 7.52s = **96.3%** reduction. SC-004 ≥90% ✓ (`go mod why` summed wall rose 69.93s→86.72s as expected; not a cost metric.)
- [X] T022 [US1] Verify SC-002 for US1 alone: mask document-identity fields and diff this scan's CycloneDX output against the T003 baseline. US1 changes no emitted content, so the diff MUST be empty. **Ordering caveat**: this empty-diff expectation holds only if US1 is verified before US2 lands. US1 and US2 are order-independent by design (see Dependencies), so if US2 landed first, the correct expectation is exactly one difference — the `waybill:go-workspace-mode` annotation value — and nothing else. US2's own golden verification is T031. — done: masked CDX diff vs baseline EMPTY; 817/817 components. US1 changes no emitted content ✓
- [X] T023 [US1] Verify SC-005 (determinism) per quickstart Step 6: two independent scans must produce byte-identical masked output. — done: two independent scans byte-identical ✓
- [X] T023b [US1] Verify SC-009 (single-workspace overhead). **First establish a genuinely single-workspace fixture** — do not assume one. The m669 `go-module-medium` fixture is the obvious candidate but contains 36 `go.mod` files under its vendor tree, and m774 hit exactly this trap. Confirm the candidate by scanning it with informational logging and checking the FR-015 summary line reports one workspace / one preflight; if it reports more, construct a minimal single-`go.mod` fixture in a temp dir instead. Only then compare wall time against the T002 baseline binary, asserting ±3%. — done: `go-module-medium` verified as genuinely single-workspace (`workspaces_scanned=1` despite 36 vendored go.mod files — the F5 trap avoided). 15-sample median delta **−0.74%**, within ±3% ✓ (a 5-sample mean first showed −9.84%, which was noise: baseline spread alone is 17.7% of median).

**Checkpoint**: US1 is independently shippable. The milestone's measured value is delivered.

---

## Phase 4: User Story 2 - Tolerate modern go.work directives (Priority: P2)

**Goal**: `go.work` files declaring `godebug` (Go 1.23+) or `toolchain` (Go 1.21+) report as detected with their member count rather than malformed, via a directive vocabulary shared by both parsers.

**Independent Test**: Scan a fixture whose `go.work` contains `godebug default=go1.26` and assert the emitted `waybill:go-workspace-mode` annotation reports detected-with-member-count. Verifiable without US1.

### Phase 4a — Shared vocabulary

- [X] T024 [US2] In `waybill-cli/src/scan_fs/package_db/golang/gowork.rs`, define the shared `go.work` directive vocabulary — `go`, `toolchain`, `godebug`, `use`, `replace` — as a single definition, with a doc-comment stating that it is the one place a future Go directive gets added, and that this single-definition property is what FR-014 requires (Clarifications Q2, research R5). — done: `GO_WORK_DIRECTIVES` + `is_known_go_work_directive` in `gowork.rs`, with the drift history documented.
- [X] T025 [US2] Update the strict parser's top-level directive dispatch (`gowork.rs`, ~lines 162-181) to consult the shared vocabulary before returning `unknown-directive`. `toolchain` and `godebug` are accept-and-ignore: recognized as valid, contributing nothing to `GoWorkDocument`, which gains no fields (research R6). Repeated `godebug` lines are valid — Go permits them — so no duplicate detection applies, unlike `use` paths. — done: strict parser consults the vocabulary before returning `unknown-directive`; `toolchain`/`godebug` accept-and-ignore.
- [X] T026 [US2] Update the lenient member-extractor (`mod_why.rs`, ~lines 88-110) to consult the same shared vocabulary when confirming that a non-`use` line it skips is a directive it legitimately ignores. Replace the illustrative comment listing directives inline with a reference to the shared definition, so the two parsers cannot drift. — done: lenient parser's comment now points at the shared definition. **Deviation**: an initial `debug_assert!` enforcing the vocabulary here was REVERTED — it broke the lenient parser's tolerate-anything contract and failed the existing `m771_parse_go_work_malformed_returns_empty` test. Agreement is enforced by T029's test over VALID inputs instead; asserting on malformed input contradicts the m771 FR-008 fallback path.

### Phase 4b — Tests

- [X] T027 [US2] [P] Add tests in `gowork.rs`'s `#[cfg(test)]` module for Contracts 2 and 3: a `go.work` containing `godebug default=go1.26` parses as detected with the correct member count; likewise one containing `toolchain go1.26.0`; and one containing repeated `godebug` lines is valid rather than malformed. — passing: `godebug` (k8s shape), `toolchain`, and repeated `godebug` lines all parse as detected.
- [X] T028 [US2] [P] Add a test for Contract 4 (anti-over-correction): a `go.work` containing a token outside the vocabulary still yields `unknown-directive`, an unparseable `use` path still yields `invalid-use-path`, and a repeated `use` path still yields `duplicate-use-path`. The existing malformed-input tests must pass unchanged — tolerance is scoped, not permissive. — passing: a token outside the vocabulary still yields `unknown-directive`; `duplicate-use-path` preserved.
- [X] T029 [US2] [P] Add a test for Contract 1 of the vocabulary contract (parser agreement): both parsers agree on validity across a corpus covering every vocabulary directive. The test MUST be written so that editing one parser's directive knowledge in isolation fails it — this is the anti-recurrence guard FR-014 exists for. — passing: both parsers agree across a 6-case corpus; plus `m775_every_vocabulary_directive_parses` guards against listing a keyword without wiring it through.

### Phase 4c — Golden churn

- [X] T030 [US2] Verify SC-008 per quickstart Step 5: scan `/tmp/test-kubernetes` and assert the `waybill:go-workspace-mode` annotation reports detected-with-member-count rather than `malformed: unknown-directive`. Cross-check Contract 7 self-consistency: the same document's `waybill:workspaces-detected` lists all 38 workspaces, so the two annotations now tell a consistent story (pre-milestone they contradicted each other). — done: k8s now emits `waybill:go-workspace-mode = detected: 34 use-modules` (was `malformed: unknown-directive`). Contract 7 self-consistency holds: 34 declared `use` members vs 38 discovered workspaces is coherent, not contradictory.
- [X] T031 [US2] Regenerate the goldens affected by the US2 annotation change and confirm the diff is confined to `waybill:go-workspace-mode`. Per memory `feedback_verify_golden_churn_normalized`, mask content-addressed IDs (`rel-`, `anno-`) and `LC_ALL=C sort` before diffing, or SPDX-3 array reordering will fake semantic hits. **Any component, relationship, or other-annotation change is a defect, not expected churn** (research R7, Contract 6). Per memory `feedback_release_bump_regen_all_golden_tests`, check whether any golden-writing test beyond the three `*_regression` suites is affected. — done, **no regeneration required**. Full masked diff vs baseline is exactly 1 changed line (the annotation value); 817/817 components. Research R7 predicted the k8s corpus golden would churn, but k8s is not among the in-repo corpus goldens, no golden contains `malformed: unknown-directive`, and no fixture `go.work` uses `toolchain`/`godebug`. R7's prediction was wrong in a harmless direction.

**Checkpoint**: US2 complete. Both stories delivered.

---

## Phase 5: Polish & Cross-Cutting Concerns

- [X] T032 Run `./scripts/pre-pr.sh` — the mandatory pre-PR gate (SC-007). Per memory `feedback_prepr_gate_full_output`, read the full per-suite "N passed; 0 failed" lines rather than trusting a failure-grep. Per memory `feedback_prepr_gate_bails_on_first_failure`, if anything fails, re-run with `--no-fail-fast` and enumerate every failing target before concluding. Note the known pre-existing `m203_us2_5_env_var_override_shortens_timeout` helm-timing flake (memory `reference_m203_helm_test_flake`) — re-run it in isolation before treating it as a regression. — **PASSED**: `>>> all pre-PR checks passed.` — 296 suites ok, 0 failed. One real failure was caught and fixed mid-implementation (`m771_parse_go_work_malformed_returns_empty`, broken by a `debug_assert!` I added and then reverted; see T026). No m203 helm flake this run.
- [X] T033 [P] Confirm SC-006 (zero new dependencies): `git diff Cargo.toml Cargo.lock` returns empty. Confirm FR-008 (no new operator surface): `cargo run -p waybill -- sbom scan --help` is unchanged. — done: no `Cargo.toml`/`Cargo.lock` change; `sbom scan --help` byte-identical to baseline.
- [X] T034 [P] Re-run the concurrency tests (T015–T018) at least 20 times to confirm they are not flaky. A coordination fix whose own tests are racy is worse than the stampede it replaces — this is an explicit rollback trigger in quickstart § "Rollback triggers". — done: 20/20 clean runs of the 12 m775 tests. No flake.
- [X] T035 [P] Write memory file `reference_m775_preflight_single_flight.md` and add a one-line pointer to `MEMORY.md`. Record: the stampede mechanism (m771 US2's pool defeating m771 US3's cache), the measured before/after, the shim technique that found it, and the general lesson — a phase-level log that does not reconcile with per-invocation measurement warrants subprocess-level instrumentation. Cross-reference `[[feedback-perf-spec-needs-per-phase-decomposition]]`. — done: `reference_m775_preflight_single_flight.md` written; `MEMORY.md` pointer added.
- [X] T036 Remove the reference scratch branch: `git branch -D scratch-m775-preflight-stampede` (commit `d572e95`). It was verification-only; the milestone re-implements the fix cleanly. — done: `scratch-m775-preflight-stampede` (d572e95) deleted.
- [ ] T037 Commit with a message citing the measured before/after, the root cause, and both user stories. Do not push until the user reviews.
- [ ] T038 [P] Open the PR against `main` citing SC-001 through SC-009 status, the profiling table from spec.md § Motivation, and the note that summed `go mod why` wall rising is expected rather than a regression. Reference issue #793.

---

## Dependencies

Phase 1 → Phase 2 → (Phase 3 ∥ Phase 4) → Phase 5.

**US1 and US2 are independent** and may be implemented in either order or concurrently, subject to T005's note that both touch `mod_why.rs` (serialize the writes).

Within US1: T006–T008 (data model) → T009–T011 (coordination) → T012–T013 (counter) → T014 (test seam, private helper) → T015–T018b (tests, parallel after T014) → T019–T023b (empirical, needs a release build).

Within US2: T024 (vocabulary) → T025–T026 (both parsers consult it, parallel) → T027–T029 (tests, parallel) → T030–T031 (golden churn).

Within Phase 5: T032 blocks T037 (do not commit on a red gate); T037 blocks T038. T033–T035 are parallel.

## Parallel execution examples

**US1 tests, after T014 lands the seam**: T016, T017, T018 are independent assertions (T015 first, as it establishes the harness shape the others reuse).

**US2 parsers, after T024**: T025 and T026 touch different files and can proceed together.

**US2 tests, after T026**: T027, T028, T029 are independent.

**Phase 5**: T033, T034, T035 run in parallel while T032 is the gate for T037.

## Implementation strategy (MVP-first)

**MVP = US1.** It carries the entire measured wall-time value (26.3s → 18.7s) and is a defect fix restoring m771 US3's stated design intent. US2 is a correctness fix on an operator-facing annotation with no wall-time effect; it can ship in this milestone or split into its own without rework.

Suggested order:
1. Phase 1–2 (T001–T005): ~30 min. Baselines and region check.
2. Phase 3a–3c (T006–T013): ~1.5 h. The load-bearing production change.
3. Phase 3d (T014–T018): ~2 h. The test seam is the slow part; the four assertions are quick once it exists.
4. Phase 3e (T019–T023): ~45 min. Empirical validation against the k8s fixture.
5. Phase 4 (T024–T031): ~1.5 h, golden regeneration included.
6. Phase 5 (T032–T038): ~45 min.

Total ≈ 7 hours. Budget an extra hour for T017 — a barrier-based concurrency test is the likeliest task to need iteration to make deterministic.
