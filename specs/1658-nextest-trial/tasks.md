# Tasks: Run test binaries in parallel (nextest trial)

**Input**: `specs/1658-nextest-trial/` (spec, plan, research, data-model, contracts/gate.md, quickstart).

**Tests**: the spec requires them for the gate only (FR-005, contract C3).

**Measurement rules** (CLAUDE.md):
- Every timing is min/median/max over 3 runs on an idle machine.
- Counts come from whole logs, never piped through `head`.
- Mutations are reverted with `cp`, checked with `cmp`, then rebuilt.

## Phase 1: Setup

- [x] T001 Create `specs/1658-nextest-trial/measurements/` holding `runs.tsv` (header from data-model.md), `ci.tsv`, and `isolation.md` with a header. Record `cargo nextest --version` and the toolchain each runner's build reports, which must be 1.98.1 (R4), in `measurements/env.md`.

## Phase 2: Foundational (blocks all stories)

- [x] T002 Write `specs/1658-nextest-trial/measurements/count.py` (stdlib, run with `python3 -I -B`). It reads one log and prints passed, failed and ignored totals per package, for either runner's output format, plus doctest totals from `cargo test --doc` logs. It also prints each binary's time: the `finished in` value for cargo-test logs, and the sum of per-test durations for nextest logs (used by T007). It's a positive control on a known log. The `nxprobe` crate from research.md has known counts of 2 passed, 1 skipped and 1 doctest, and the script must reproduce them.

## Phase 3: User Story 1, measure before adopting (P1)

**Goal**: evidence that nextest is faster and runs the same tests.

**Independent test**: `runs.tsv` has 3 rows per runner. Counts match, or `isolation.md` explains each difference.

- [x] T003 [US1] Baseline: run `cargo test --workspace --no-fail-fast` 3 times on the branch tree, which matches `main` for code, after one warm build. Save each log to `measurements/cargo-test-<n>.log`, and record wall time, per-binary summed time and counts in `runs.tsv`. Run `cargo test --workspace --doc` once and record the doctest count.
- [x] T016a [US1] Before `pre-pr.sh` changes: copy it to `measurements/pre-pr.before.sh`, then time it 3 times through `gate.sh` with the old `--expect`. Record the times in `runs.tsv` (the SC-003 before half).
- [x] T004 [US1] Run `cargo nextest run --workspace --no-fail-fast --color never` 3 times on the same tree. Save the logs to `measurements/nextest-<n>.log` and the rows to `runs.tsv`. Summed time comes from nextest's per-test durations.
- [x] T005 [US1] Compare counts with `count.py` across T003 and T004 (FR-011). For every difference, and every test that failed under only one runner in any run, reproduce it with the quickstart commands. Then write an entry in `measurements/isolation.md`: test, runner, command, cause.
- [x] T006 [US1] Resolve each T005 entry. Fix the cause in the test file where possible. Otherwise add a named test group in `.config/nextest.toml`, with a comment pointing to the `isolation.md` entry. Re-run T004's command once and confirm the entry is resolved (SC-005).
- [x] T007 [P] [US1] Lazy-state check (FR-007). List the 16 test files that use `OnceLock`, `LazyLock` or `lazy_static` under `waybill-cli/tests` in `measurements/lazy-state.md`. Give each its binary's summed time under both runners, taken from the T003 and T004 logs, and flag any whose nextest sum exceeds the `cargo test` sum by more than the larger of the two runners' max−min spread.
- [x] T008 [US1] Adoption decision. In `measurements/decision.md`, compare median wall times (SC-001). If nextest is not faster, stop here and report. Otherwise continue.

**Checkpoint**: US1 proves equal counts and a faster median, or the trial stops.

## Phase 4: User Story 2, a gate that still asserts a positive signal (P1)

**Goal**: the C1 to C3 contract is in force, and its tests have teeth.

**Independent test**: the `release_gates` tests pass, and each fails when its expectation is removed.

- [x] T009 [US2] Amend `.specify/memory/constitution.md` from 4.0.1 to 4.1.0, with a Sync Impact Report at the top as previous amendments have.
  - **Pre-PR table:** step 2 becomes `cargo nextest run --workspace --no-fail-fast`, passing when the C2 summary line appears.
  - **New step 3:** `cargo test --workspace --doc`, passing when every `test result: ok.` line shows 0 failed.
  - **Also update** the Build & Test Commands table, and the `--no-fail-fast` rationale sentence.
- [x] T010 [US2] Update the "Pre-PR verification" section of `CLAUDE.md` to match T009. That covers the numbered list, the `gate.sh` invocation with both `--expect` arguments from C2, and the positive-signal paragraph, which should name the nextest summary line.
- [x] T011 [US2] Edit `scripts/pre-pr.sh` per contract C1:
  - **Missing-nextest check:** exits 1 with the install hint.
  - **Test step:** becomes `cargo nextest run --workspace --no-fail-fast --color never ${feature_args[*]:-}`.
  - **New doctest step:** `cargo test --workspace --doc ${feature_args[*]:-}`.
  - **Keep** `set -euo pipefail`, with no `+toolchain`.
- [x] T012 [US2] Create `.config/nextest.toml` (C5): default profile, `retries = 0`, `fail-fast = false`, plus any groups from T006.
- [x] T013 [US2] Add contract C3's six cases to `waybill-cli/tests/release_gates.rs`: pass, failing test, non-compiling, zero tests, colour forced, nextest absent. All go under the file's existing `#![cfg(unix)]`. The nextest-absent case sets `PATH` to a temporary directory containing only links to `cargo`, `rustc` and `bash`, and asserts that the message names cargo-nextest. Each builds a scratch crate in a `tempfile` directory, runs `scripts/gate.sh` with the C2 summary `--expect` around `cargo nextest run --no-fail-fast`, and asserts `GATE_RESULT`. Use the `#[cfg_attr(test, allow(clippy::unwrap_used))]` convention where needed. If nextest is missing, fail with a message naming it; don't skip.
- [x] T014 [US2] Teeth check. For each T013 case, remove that case's `--expect` (or the assertion it guards) and run the test, which must fail. Revert with `cp`, check with `cmp`, and record the results in `measurements/gate-mutations.md`.
- [x] T015 [US2] Edit `.github/workflows/ci.yml` (C4).
  - **Linux, macOS and Windows:** add the install step `taiki-e/install-action@893d6ee8d714b6c7dfe2bcd94e5759e9a7958a7e # v2.90.10` with `tool: cargo-nextest@0.9.152`. Change `Tests` to the C1 nextest command, including `--color never`, and add a `Doctests` step. Windows keeps `continue-on-error: true` on both steps.
  - **The eBPF lane:** gets only the install step.
  - **Afterwards:** grep that the four `cargo test --workspace --no-fail-fast` call sites now read as intended, and that no `uses:` is pinned by tag.

**Checkpoint**: the gate contract is enforced locally and on CI.

## Phase 5: User Story 3, faster CI and local gate (P2)

**Goal**: reported before and after timings.

**Independent test**: `ci.tsv` and `runs.tsv` have the required rows.

- [x] T016 [US3] Time the new `pre-pr.sh` 3 times (the before runs are T016a), through `gate.sh` with C2. Record the times in `runs.tsv` (SC-003).
- [x] T017 [US3] CI before: take the `Tests` step time per lane from run 45695514065, and dispatch or reuse one more green `main` run (2 per lane). Record both in `ci.tsv`.
- [ ] T018 [US3] CI after: once the draft PR is open (this is FR-006's CI trial), record the `Tests` and `Doctests` step times from its first run, plus one re-run (2 per lane), in `ci.tsv` (SC-002). Enumerate every job's conclusion.
- [ ] T019 [US3] #1716 check. From the T004 and CI-after logs, find the slowest test per lane. If it's one of #1716's four self-scan tests, apply #1716's fix and re-measure. Otherwise note in `decision.md` that #1716 stays filed.

## Phase 6: Polish

- [x] T020 Run the gate: `./scripts/gate.sh` with both C2 expectations, `prepr -- ./scripts/pre-pr.sh`. Check `GATE_RESULT=pass`, the nextest summary line and the doctest lines.
- [ ] T021 Write `measurements/after.md`: local and CI timings, count parity, isolation findings, gate mutations, and the #1716 outcome. Update `CLAUDE.md` Recent Changes with spec 1658.
- [ ] T022 Open the PR (Refs #1719, #1718) at the first green gate. Then tick #1718 in #1719's checklist once it merges.

## Dependencies

- **Order:** T001 → T002 → US1 (T003 → T004 → T005 → T006 → T008; T007 runs alongside T005) → US2 → US3 → Polish.
- **US2 hangs on T008:** if T008 says no, US2 and US3 are skipped and the trial is reported as negative.
- **Within US2:** T009 comes before T010 and T011 (the constitution first). T013 comes before T014. T015 can be done alongside T012 and T013.
- **T016a runs in US1, before T011** changes `pre-pr.sh`.

## Parallel opportunities

- T007 alongside T005 and T006.
- T012, T013 and T015 touch different files and can run together after T011.

## Implementation strategy

US1 is the MVP and the go/no-go point. US2 is required for any adoption. US3 reports the result. One gate, one PR.
