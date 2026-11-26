# Tasks: Debug info in test builds

**Input**: `specs/1661-test-debuginfo/` (spec, plan, research, data-model, quickstart).

**Rules (CLAUDE.md):**
- Never pipe a check through `head`.
- Timings are min/median/max over 3 runs on an idle machine.
- Use bash for word-splitting.
- No `+toolchain`.
- Don't run `cargo fmt`.

## Phase 1: Setup

- [ ] T001 Create `specs/1661-test-debuginfo/measurements/` with `local.tsv` (header: setting, rebuild, deps_entries, seconds) and `ci.tsv` (header: run_id, lane, setting, test_build_s, tests_step_s, backtrace_test, file_line).

## Phase 2: Foundational

- [ ] T002 Settle research R1 by experiment, not assumption, and write the results in `measurements/local.md`:
  - **(a)** Add `[profile.dev] split-debuginfo = "packed"` under a `[target.'cfg(target_os = "macos")']` table in a scratch `.cargo/config.toml`, and record whether cargo rejects or ignores it. Expected: profiles can't be scoped by target.
  - **(b)** Confirm the rustflags route reaches rustc. With `[target.'cfg(target_os = "macos")'] rustflags = ["-C", "split-debuginfo=packed"]` in place, run `cargo build -p waybill-common -v` and check that the `rustc` command lines contain `-C split-debuginfo=packed`.
  - **(c)** Confirm it does *not* apply to other targets: run `rustup target add x86_64-unknown-linux-gnu` (this adds the standard-library component only; the pinned toolchain is unchanged), then `cargo check -p waybill-common -v --target x86_64-unknown-linux-gnu`, and check the flag is absent from every `rustc` line. Record the command and the count of lines carrying the flag, which must be 0.
  - Remove the scratch file afterwards.

## Phase 3: User Story 1, local rebuilds stop slowing down (P1)

**Goal**: on macOS, rebuilds no longer accumulate object files, and full debug info is kept.

**Independent test**: 3 rebuilds in a fresh target directory hold a flat entry count with no upward time trend, and the backtrace test passes.

- [ ] T003 [P] [US1] Create `waybill-cli/tests/backtrace_resolves_source.rs` per research R3. It holds one test, `a_panic_backtrace_names_this_file_and_line`:
  - **Child mode:** if the env var `WAYBILL_BACKTRACE_PROBE_CHILD=1` is set, panic with the message `backtrace probe`.
  - **Parent mode:** otherwise, run `std::env::current_exe()` with the arguments `["--exact", "a_panic_backtrace_names_this_file_and_line", "--nocapture"]`, the env `WAYBILL_BACKTRACE_PROBE_CHILD=1` and `RUST_BACKTRACE=1`. Assert the child exited non-zero, and that its stderr matches the regex `backtrace_resolves_source\.rs:\d+`, using `regex`, which is already a dependency. The failure message includes the stderr.
  - **Clippy:** follow the crate's `unwrap_used` convention with `#![allow(clippy::unwrap_used)]`, as the other integration tests do.
  - **Platforms:** must work on all three. Use no Unix-only APIs.
- [ ] T004 [US1] Create `.cargo/config.toml` with only the macOS rustflags table and a comment block covering:
  - why it exists (the `*.rcgu.o` accumulation, with the numbers from #1719);
  - why it's in rustflags and not a profile (T002(a));
  - that `RUSTFLAGS` overrides it;
  - that release builds carry no debug info, so they're unaffected.

  Then check the release build: run `cargo build --release -p waybill -v 2>&1` and check the flag is present but release has `debug = false`. Record the release binary's size before and after; it should be unchanged within noise. Also confirm from the `-v` output that the release `rustc` lines carry no debug info, either `-C debuginfo=0` or no `debuginfo` flag at all. That's the property the spec's edge case claims.
- [ ] T005 [US1] Measure locally, in a fresh `CARGO_TARGET_DIR=target/dbgcheck`: a warm `cargo nextest run --workspace --no-run`, then 3 × (touch `waybill-cli/src/lib.rs`, `waybill-common/src/lib.rs` and `waybill-cli/src/main.rs`; rebuild; record the `ls target/dbgcheck/debug/deps | wc -l` entry count and the seconds) into `local.tsv` as setting `packed-config`.
  - **Expected:** entry counts within 1% of each other (SC-001), and the 3rd time no more than the run-to-run spread above the 1st (SC-002).
  - **Then** run T003's test against this target dir (SC-003), and remove the target dir.
- [ ] T006 [US1] Positive and negative controls for the backtrace test, recorded in `local.md`:
  - with `CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0` in a separate fresh target dir, the test must **fail** (no file:line);
  - with the default, it must pass.

  Remove the target dir afterwards.

**Checkpoint**: SC-001 to SC-003 hold locally.

## Phase 4: User Story 2, CI measurement and decision (P2)

**Goal**: decide CI's debug-info setting from CI measurements.

**Independent test**: `ci.tsv` has 2 runs per setting per lane, with a recorded decision.

- [ ] T007 [US2] Run the gate (`./scripts/gate.sh` with both CLAUDE.md `--expect` patterns, around `prepr -- ./scripts/pre-pr.sh`), commit T003–T006, and open the PR (Refs #1719). This "full" commit is the baseline. Record its CI run and one re-run in `ci.tsv`: for Linux, macOS and Windows, the test-build `Finished` time and `Tests`-step seconds (use `specs/1658-nextest-trial/measurements/ci_steps.py`), and the backtrace test's pass or fail.
- [ ] T008 [US2] Push a measurement commit adding `CARGO_PROFILE_DEV_DEBUG: line-tables-only` and `CARGO_PROFILE_TEST_DEBUG: line-tables-only` to the top-level `env:` of `.github/workflows/ci.yml`. Record its run and one re-run. The backtrace test's result and captured stderr go in `ci.md`.
- [ ] T009 [US2] Push a measurement commit setting both to `0`. Record its run and one re-run. The backtrace test is expected to fail here, and that failure is the measurement. Record it rather than fixing it. Also record **every** failing test in these runs, with its cause. Self-scan and binary-ID tests read waybill's own binary and might depend on debug info. A failure other than the backtrace test counts against this setting in T010.
- [ ] T010 [US2] Decide per research R4. Windows is recorded but doesn't decide, because the lane is non-blocking (#210). Adopt a setting only if its 2 test-build times are below both of full's on Linux and macOS *and* the backtrace test passes. Revert the measurement commits so the workflow has either no new `env` line or only the adopted one. If one is adopted, confirm in the final run that the eBPF lane passes too. Write the decision and the numbers in `ci.md`.

## Phase 5: User Story 3, guidance (P3)

- [ ] T011 [US3] In `CLAUDE.md`, under "Pre-PR verification", add a short note:
  - what `.cargo/config.toml` does and why (with the numbers);
  - that `RUSTFLAGS` overrides it;
  - that a directory which grew before this change needs one `cargo clean`.
- [ ] T012 [US3] Update the memory notes `keep-target-dir-clean.md` and `project_prepr_slow_investigation.md`, and their `MEMORY.md` index lines, under `/Users/mlieberman/.claude/projects/-Users-mlieberman-Projects-mikebom/memory/`. Name the cause (unpacked debug info leaving `*.rcgu.o` files, about 31k per rebuild) and the fix (`.cargo/config.toml`, spec 1661), and keep `cargo clean` only for the one-time cleanup.

## Phase 6: Ship

- [ ] T013 Write `measurements/after.md`, run the gate on the final head, push, and enumerate every CI job's conclusion on that head. After merge: comment on #1719 with the result and tick nothing, since #1719 has no checklist item for this. Then switch to `main`, pull, delete the branch, and run `cargo clean` (the one-time cleanup).

## Dependencies

- **Order:** T001 → T002 → US1 (T003 ∥ T004 → T005 → T006) → US2 (T007 → T008 → T009 → T010) → US3 (T011, T012) → T013.
- **Timing:** T008 and T009 each wait for their CI runs.

## Implementation strategy

US1 is the guaranteed win, and it's local. US2 adopts a CI change only if CI proves it. One gate, one PR.
