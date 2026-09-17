# Tasks: A trustworthy eBPF canary signal

**Input**: Design documents from `/specs/896-fix-ebpf-canary/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/canary-workflow-v2.md, quickstart.md
**Issue**: [#685](https://github.com/kusari-oss/waybill/issues/685)

**Tests**: This feature has no unit-testable surface — it is one GitHub Actions
workflow. "Test" tasks here are **dispatch observations on a real runner**, per
research R9 and quickstart.md. They are not optional: without them nothing about
this feature has been verified, since the canary has produced zero green runs in
its lifetime and there is no baseline to regress against.

**Organization**: grouped by user story. US1 is a hard precondition for US2 —
the control build has no known-good half until the canary can build at all.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependencies)
- Almost everything lands in `.github/workflows/ebpf-canary.yml`, so **[P] is
  rare by construction**. Tasks touching that one file are sequential. Marking
  them parallel would be a lie about a merge conflict.

## Path Conventions

Repository root. Primary file: `.github/workflows/ebpf-canary.yml`.
Reference-only (do not modify): `.github/workflows/ci.yml`,
`.github/workflows/release.yml`, `.github/actions/install-bpf-linker/action.yml`,
`xtask/src/main.rs`, `waybill-ebpf/rust-toolchain.toml`.

---

## Phase 1: Setup (evidence baseline)

**Purpose**: capture the pre-change state so every later check has teeth. Per the
project's "keep the probe" rule, this is committed next to the spec.

- [X] T001 Create `specs/896-fix-ebpf-canary/measurements/` and write `run-history.md` recording the full canary run history: `gh run list --workflow=ebpf-canary.yml --limit 60 --json conclusion,createdAt,databaseId`. Must show 36 runs, 0 successes, 2026-08-13 → 2026-09-17.
- [X] T002 [P] Save the current failing log excerpt to `specs/896-fix-ebpf-canary/measurements/baseline-failure.log` from run `35189734662` (`gh run view 35189734662 --log-failed`), trimmed to the `rustlib/src/rust/library/Cargo.lock does not exist` error and the `rustup component add rust-src` line the compiler prints.
- [X] T003 [P] Save the *earlier* failure cause to `specs/896-fix-ebpf-canary/measurements/baseline-failure-llvm.log` from run `31674387553` (2026-08-13) showing `could not find llvm-config`, establishing that this one streak contains two distinct canary-fault causes (research R1).
- [X] T004 Write `specs/896-fix-ebpf-canary/measurements/README.md` explaining what each capture proves and naming commit `e1024ab5` (PR #686, `install-method: binary` default) as the transition between the two causes.

**Checkpoint**: the defect is documented from observation, not from inference.

---

## Phase 2: Foundational (blocking prerequisites)

**Purpose**: structural changes to the canary job that both US1 and US2 build on.
No behaviour change on its own.

- [X] T005 In `.github/workflows/ebpf-canary.yml`, add a `workflow_dispatch` input `break_env` (boolean, default `false`, description naming it a test-only switch that omits the `rust-src` component) per contract C-1. It must have no effect on the `schedule` path.
- [X] T006 In `.github/workflows/ebpf-canary.yml`, add a step `Resolve pinned version` that reads `BPF_LINKER_VERSION` from `.github/env/bpf-linker.env` and exposes it as step output `pinned`, then add `pinned_version` to the `canary` job's `outputs:` block. Needed by US2's control build and by FR-005's requirement to name both versions.
- [X] T007 In `.github/workflows/ebpf-canary.yml`, extend the `canary` job `outputs:` block with the remaining fields from `data-model.md` § *Canary run*: `latest_build_outcome`, `control_build_outcome`, `failing_step`, `error_excerpt`, `artifact_present`, `attributed_cause`. Wire them to placeholder step outputs for now; later tasks populate them.

**Checkpoint**: the job can carry the data a report needs. Nothing observable has changed yet.

---

## Phase 3: User Story 1 — the canary answers the question it exists to ask (P1)

**Goal**: the canary can build, and a green run is backed by an artifact it can point at.

**Independent test**: dispatch against the currently pinned bpf-linker version. It
must pass. Today it fails, and the failure has nothing to do with which version
is under test.

### Environment parity (FR-001, contract C-2)

- [X] T008 [US1] In `.github/workflows/ebpf-canary.yml`, add an `Install eBPF build deps` step before the toolchain steps, mirroring `ci.yml:628-642` — `nick-fields/retry` (same pinned SHA) wrapping `sudo apt-get update && sudo apt-get install -y clang llvm libelf-dev pkg-config libssl-dev`.
- [X] T009 [US1] In `.github/workflows/ebpf-canary.yml`, add `Install nightly Rust with rust-src` **before** the existing stable install, using the same SHA-pinned `dtolnay/rust-toolchain` nightly ref as `ci.yml:644-647` with `components: rust-src`. Ordering is load-bearing: the action makes whichever toolchain it installed last the default.
- [X] T010 [US1] In `.github/workflows/ebpf-canary.yml`, gate the `components:` value on the `break_env` input from T005 so the deliberate-break path omits `rust-src` — this is the only way SC-003 is testable without waiting for an accident. Implement it as **one step with an expression-valued `components:`** (`${{ inputs.break_env == 'true' && '' || 'rust-src' }}`), NOT two conditional steps: T016 extracts that field by text and a duplicated step would give it two candidates and no disambiguation rule.
- [X] T011 [US1] In `.github/workflows/ebpf-canary.yml`, add `Make stable the default toolchain` (`rustup default stable`) after the stable install, matching `ci.yml:648-652`. `cargo run -p xtask` must run under stable; the nested `cargo +nightly build` selects nightly itself.
- [X] T012 [US1] In `.github/workflows/ebpf-canary.yml`, add `Swatinem/rust-cache` (same pinned SHA as `ci.yml`) after the toolchain installs.
- [X] T013 [US1] In `.github/workflows/ebpf-canary.yml`, add the `Remove rustup-init` step verbatim from `ci.yml:665-687`, placed **after** the cache restore (the cache's `$CARGO_HOME/bin` restore re-creates `rustup-init`).

### Artifact verification (FR-002a, contract C-5)

- [X] T014 [US1] In `.github/workflows/ebpf-canary.yml`, add a step before the eBPF build that removes any pre-existing object at `waybill-ebpf/target/**/bpfel-unknown-none/release/waybill-ebpf`, so a leftover cannot satisfy the later check (spec edge case; research R4 chose deletion over mtime comparison deliberately — an mtime check is a race).
- [X] T015 [US1] In `.github/workflows/ebpf-canary.yml`, add a step after the eBPF build that asserts the object exists and is non-empty, sets the `artifact_present` output, and fails the run when absent regardless of the build's exit status.

### Divergence check (FR-010, contract C-3)

- [X] T016 [US1] In `.github/workflows/ebpf-canary.yml`, add an `Assert toolchain parity with ci.yml` step that runs **before** the build, extracts the `components:` value under each file's nightly `dtolnay/rust-toolchain` step from both `ci.yml` and this workflow, and fails on mismatch. POSIX `grep`/`sed`/`diff` only — same posture as the m115/m117 walker-audit gate. Each file must yield exactly **one** `components:` line under its nightly toolchain step (guaranteed for this file by T010's single-step shape); assert that count and fail loudly if it is not 1, so the check cannot silently compare the wrong line. It must skip its own assertion when `break_env` is true, or T010's deliberate break would fail here instead of at the build.

### Verification (SC-001, SC-002, SC-008)

- [ ] T017 [US1] Dispatch `gh workflow run ebpf-canary.yml -f version=<pinned> -f dry_run=true` and confirm `success`. This is SC-001 and the gate for the whole feature — the canary's first green run ever. Wait for it to conclude before dispatching anything else (shared concurrency posture).
- [ ] T018 [US1] Teeth-check T016: temporarily change the `components:` value in `.github/workflows/ebpf-canary.yml`, dispatch, confirm the run fails **at the parity step and before the build**, then revert. A parity check that has never failed is not known to work.
- [ ] T019 [US1] Teeth-check T015: temporarily make the build step in `.github/workflows/ebpf-canary.yml` delete its own artifact on success, dispatch, confirm the run is reported red with the artifact named. Revert. Without this, SC-008 is asserted rather than observed.
- [ ] T020 [US1] Record T017–T019 outcomes (run IDs, conclusions) in `specs/896-fix-ebpf-canary/measurements/us1-verification.md`.

**Checkpoint**: the canary builds. Everything downstream now has a known-good half to compare against.

---

## Phase 4: User Story 2 — a failure says whose fault it is (P2)

**Goal**: a failure report names the party that can act on it.

**Independent test**: break the canary's own environment deliberately and confirm
the report does not claim an upstream regression and does not tell the reader to
file upstream.

**Depends on**: Phase 3 complete. The control build is only meaningful once the
pinned version is known to build under this job.

### Capture (FR-005, contract C-4)

- [ ] T021 [US2] In `.github/workflows/ebpf-canary.yml`, give the eBPF build step an `id`, set `continue-on-error: true`, and route its combined output through `tee` to a log file (with `set -o pipefail` so the exit status survives the pipe). Record the step's `outcome` into `latest_build_outcome`.
- [ ] T022 [US2] In `.github/workflows/ebpf-canary.yml`, add a step that, on failure, writes the tail of that log (cap ~2000 bytes) into the `error_excerpt` output using a `GITHUB_OUTPUT` heredoc delimiter, and records the failing step's name into `failing_step`.
- [X] T023 [US2] In `.github/workflows/ebpf-canary.yml`, make the userspace `cargo build --features ebpf-tracing` step conditional on the eBPF build having succeeded (contract C-4 B3), so a failed kernel-side build does not produce a second, confusing failure.

### Control build (FR-003a, contract C-4 B2)

- [ ] T024 [US2] In `.github/workflows/ebpf-canary.yml`, add a second `./.github/actions/install-bpf-linker` invocation, conditional on the latest build having failed, with an **empty** `version` input so it reads `.github/env/bpf-linker.env`. Research R5 established the binary path is safe to invoke twice — the idempotency short-circuit only fires when target ≠ `latest` and the installed version already matches.
- [ ] T025 [US2] In `.github/workflows/ebpf-canary.yml`, add the control eBPF build step under a **distinct `CARGO_TARGET_DIR`**, conditional on the latest build having failed, `continue-on-error: true`, recording `control_build_outcome`. A shared target dir lets the second build reuse objects linked by the first linker version and report a result belonging to neither.
- [ ] T026 [US2] In `.github/workflows/ebpf-canary.yml`, apply the same pre-delete + presence assertion from T014/T015 to the control build's own artifact path.

### Attribution (FR-003, FR-003b, contract C-6)

- [ ] T027 [US2] In `.github/workflows/ebpf-canary.yml`, add a step computing `attributed_cause` strictly from the table in `data-model.md` § *Attributed cause* — `latest_build_outcome`, `control_build_outcome`, `artifact_present`, and nothing else. It MUST NOT branch on step name or grep the error text (FR-003b); both mis-classify the live failure. Add a YAML comment recording that FR-004's veto (no upstream attribution unless the failure was at the component step) is satisfied structurally: `component` is only reachable when the latest build failed, and that build **is** that step. FR-003b forbids step position as the determining basis, not as a consequence — a reviewer reading the two side by side will otherwise take them as contradictory.
- [ ] T028 [US2] In `.github/workflows/ebpf-canary.yml`, make the `canary` job's final verdict derive from `attributed_cause` rather than from the individual `continue-on-error` steps, so a run whose builds both "succeeded as steps" but produced no artifact still concludes red.

### Reports (FR-004, FR-004a, FR-004b, FR-006, contract C-7)

- [ ] T029 [US2] In `.github/workflows/ebpf-canary.yml` `report-failure` job, replace the single hardcoded `title` with a selection on `attributed_cause`: the frozen `[canary] bpf-linker eBPF build regression` for `component`, and a distinct canary-fault title for `canary`. Do not alter the component title — #685 lives under it and external references must keep resolving.
- [ ] T030 [US2] In `.github/workflows/ebpf-canary.yml` `report-failure` job, rewrite the issue and comment bodies to carry the FR-005 evidence: the version(s) tested (both versions when the cause is `component`), `failing_step`, and `error_excerpt`. Route all of them through `env:` as the existing code does, never string-interpolated into the JS source.
- [ ] T030a [US2] In `.github/workflows/ebpf-canary.yml` `report-failure` job, add to every `canary`-kind report an explicit line stating the watched component went **untested** this run (FR-004c). When the canary cannot build, nothing was learned about the component either way; a report silent on that reads as though the component is healthy, which is the spec edge case "the report must not hide the second behind the first".
- [ ] T031 [US2] In `.github/workflows/ebpf-canary.yml` `report-failure` job, branch the next-steps text on cause: `component` keeps "reproduce, then file at aya-rs/bpf-linker"; `canary` gives canary-repair steps and MUST NOT name an upstream project (FR-006). The current unconditional "file upstream" line is what sent 35 reports to the wrong repository.
- [ ] T032 [US2] In `.github/workflows/ebpf-canary.yml` `report-failure` job, confirm the dedupe stays exact-title-match within the `canary,ebpf,regression` label set. Two distinct titles then coexist with no further work, satisfying FR-004b.
- [ ] T033 [US2] In `.github/workflows/ebpf-canary.yml` `report-success` job, fix the close logic to close **per kind**. A green run (`attributed_cause = none`) closes both titles; a red run of one kind must never close the other. Today it closes the single matching title — with two titles that would close an outstanding upstream regression nothing has fixed.

### Verification (SC-003, SC-004a, SC-005)

- [ ] T034 [US2] Dispatch with `break_env=true` and `dry_run=false`; confirm a new issue appears under the canary-fault title *alongside* (not replacing) #685, and that `gh issue view <n> --json body --jq .body | grep -ci 'bpf-linker/issues'` returns `0`. This is SC-003 and SC-004a in one observation.
- [ ] T035 [US2] Inspect the same issue for the FR-005 triple — version(s), failing step, error text (SC-005). Today's reports contain none of the three.
- [ ] T035a [US2] In the same issue from T034, confirm the FR-004c "component untested this run" line is present (SC-004a's second clause). Its absence is the failure mode where a broken canary silently implies upstream is fine.
- [ ] T036 [US2] Confirm #685 is still open under its original title and that its history is intact after T034.
- [ ] T037 [US2] Close the test-created canary-fault issue and record T034–T036 (run IDs, issue numbers, body excerpts) in `specs/896-fix-ebpf-canary/measurements/us2-verification.md`.

**Checkpoint**: a failure of either kind is distinguishable from the issue list without opening either.

---

## Phase 5: User Story 3 — a long-running failure escalates (P3)

**Goal**: a streak that crosses the documented window says so, and the clock cannot be stalled by inaction.

**Independent test**: evaluate the rule against #685's real `created_at` — 35 days against a 30-day window — and confirm it escalates.

**Depends on**: Phase 4 (the report bodies it modifies, and the per-kind titles the clock is scoped to).

- [ ] T038 [US3] In `.github/workflows/ebpf-canary.yml` `report-failure` job, compute elapsed days as `now - created_at` of the matched open issue **of that kind**, whole days. No new state: research R7 verified the issue timestamp tracks the streak start to within one run's duration (#685 created 52s after its first failing run).
- [ ] T039 [US3] In `.github/workflows/ebpf-canary.yml` `report-failure` job, include the elapsed-days figure in every failure comment so a persistent failure is distinguishable from a new one without counting comments (FR-007; the spec's edge case rules comment-counting out — a streak spanning a gap in runs gives a count that disagrees with elapsed days).
- [ ] T040 [US3] In `.github/workflows/ebpf-canary.yml` `report-failure` job, when elapsed days ≥ 30 add a prominent escalation block stating the elapsed duration and that the documented fallback is now due (FR-008). It MUST NOT be conditioned on any upstream issue having been filed (FR-007a).
- [ ] T041 [US3] Replace the responsiveness-gated framing with "days since the streak's first failure" in **both** places it actually appears — verified by grep, since the phrase this task originally cited existed in no file:
  (a) `.github/workflows/ebpf-canary.yml:166`, the report line reading `3. Track upstream response; if unresponsive within the 30-day fallback window (spec.md FR-011), execute downstream mitigation.` — this is the operative text and T040 already rewrites the surrounding body;
  (b) `docs/development/ebpf-toolchain.md:94-104`, where step 4's window follows step 2's "File upstream", making the clock start implicitly on a human action. State the new start condition explicitly there.
- [ ] T042 [US3] Verify the rule against the live case: `gh issue view 685 --json createdAt` (2026-08-13T06:36:09Z) against the 30-day window, and confirm the next report for that streak escalates rather than repeating unchanged (SC-006, SC-006a). Record in `specs/896-fix-ebpf-canary/measurements/us3-verification.md`.

- [ ] T042a [US3] Verify the reset half of FR-009/SC-007, which research R8 argues but nothing observes: after a green run has closed the reports, dispatch `break_env=true` again and confirm the fresh report's elapsed days reads 0 rather than inheriting the previous streak's age.

**Checkpoint**: replaying #685's own history through the new rule escalates.

---

## Phase 6: Polish & cross-cutting

- [ ] T043 [P] In `docs/development/ebpf-toolchain.md`, document the two report titles, the control-build attribution rule, and the artifact check, so a maintainer reading a canary issue knows which kind they are looking at.
- [ ] T044 [P] In `CLAUDE.md` § *eBPF toolchain pin (m234)*, update the canary sentence: it now runs a pinned control build and opens one of two titles depending on attributed cause.
- [ ] T045 [P] Add a superseded note at the top of `specs/234-fix-ebpf-linker-regression/contracts/canary-workflow.md` pointing at `specs/896-fix-ebpf-canary/contracts/canary-workflow-v2.md`.
- [ ] T045a Run the mandatory pre-PR gate before opening the PR: `./scripts/pre-pr.sh` (`cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace`). The constitution's wording is unconditional — *"Before opening or updating ANY pull request"* — and this feature changes no Rust, so it should pass trivially. Running it is the point; assuming it would pass is what the rule exists to prevent.
- [ ] T046 Run the full quickstart end to end (`specs/896-fix-ebpf-canary/quickstart.md` §§ 1-7) against the merged workflow and record every outcome. Section 4 (a genuine component regression) cannot be manufactured — note it as unobserved rather than claiming it passed.
- [ ] T047 Observe the first scheduled run after merge. Green → let `report-success` close #685 and confirm it did. Red → confirm the cause is attributed correctly and the report carries evidence; a real upstream regression surfacing here is an expected outcome per the spec's Assumptions, not a failure of this feature.
- [ ] T048 [P] File the two follow-ups recorded in `research.md`: (a) drop `+nightly` from `xtask::build_ebpf` so `waybill-ebpf/rust-toolchain.toml` governs every build site; (b) factor the eBPF build environment into one composite action consumed by all three lanes, making the research R3 divergence table structurally impossible.

---

## Dependencies

```
Phase 1 (T001-T004)  evidence baseline
        │
Phase 2 (T005-T007)  outputs + dispatch input
        │
Phase 3 US1 (T008-T020)  ◀── MVP. Delivers SC-001, SC-002, SC-008.
        │                     Without this nothing else is observable.
Phase 4 US2 (T021-T037)  ◀── needs a building canary for the control half
        │
Phase 5 US3 (T038-T042)  ◀── modifies the report bodies US2 establishes
        │
Phase 6 (T043-T048)
```

**Story independence**: US1 is genuinely standalone and shippable — it is the
whole of SC-001/SC-002/SC-008. US2 and US3 are *not* independent of US1, and the
plan says so rather than pretending otherwise: an attribution rule whose control
build cannot run, and an escalation clock on a mis-attributed streak, would both
make things worse than the status quo. US3 depends on US2 only for the report
bodies it extends.

## Parallel opportunities

Deliberately few. `.github/workflows/ebpf-canary.yml` is a single file, so
T005–T040 are sequential — marking them `[P]` would invite merge conflicts for
no gain.

Genuinely parallel:

- **T002, T003** — separate capture files.
- **T041, T043, T044, T045, T048** — `docs/development/ebpf-toolchain.md`,
  `CLAUDE.md`, the m234 contract, and a GitHub issue. T041 and T043 both touch
  the same doc, so run them together as one edit or sequence them.

## Implementation strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1).** That is ~20 tasks and produces the
canary's first green run in its 36-run history. It is independently valuable and
independently mergeable: a canary that builds but still mis-attributes is
strictly better than one that cannot build at all, because at least its green
means something.

Ship US2 next — it is what prevented a one-line environment fix from being found
for 35 nights, and the next self-inflicted failure is equally misleading without
it. US3 last, as the spec's own priority reasoning argues: escalating a
mis-attributed failure faster would have made things worse.

## Verification standard

Every `Verification` task is a dispatch against a real runner, not a local
simulation. Two rules carried from the project's practice:

1. **Dispatch one at a time and wait.** The repository's canary workflows share
   a concurrency posture.
2. **A check that has never failed is not known to work.** T018 and T019 exist
   because the parity check and the artifact check would otherwise pass
   vacuously — the same trap that let three earlier m895 tests pass against the
   defect they were written to catch.
