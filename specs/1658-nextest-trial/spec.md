# Feature Specification: Run test binaries in parallel (nextest trial)

**Feature Branch**: `1658-nextest-trial`
**Created**: 2026-11-25
**Status**: Draft
**Input**: #1719, second slice: #1718. Trial cargo-nextest locally in `scripts/pre-pr.sh` and on the CI `Tests` step. Refs #1719, #1718.

## Context

`cargo test --workspace` runs test binaries one at a time. Tests inside a binary run in parallel, but the binaries themselves run serially.

**Measured so far:**

- **#1718, locally on 18 cores:** a script running binaries 18 at a time took 267 s of wall time against 2,430 s summed. 11,585 tests passed, with no cross-binary collisions in one run.
- **After #1722:** the longest single binary is `sbom_edit_filter`, at about 37 s. It was 151 s before.
- **CI baseline after #1722** (run 45695514065, `Tests` step): macOS 1,135 s, Linux 683 s, Windows 1,146 s. Windows read 935 s on `main`'s run 45613291165, and that gap is unexplained.

**Inference, not measured:** CI runners have 3–4 cores, so the CI gain should be well below the local 9×.

**Already in the tree:**

- **CI test invocations:** `.github/workflows/ci.yml` runs `cargo test --workspace --no-fail-fast` on Linux (line 375), macOS (line 503) and Windows (line 621, `continue-on-error`). The eBPF lane runs it at line 731.
- **Shared state in tests:** 16 test files use `OnceLock`, `LazyLock` or `lazy_static`. 35 files across tests and source call `set_var` or use `EnvGuard`.
- **Not installed:** nextest is not on this machine.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Measure before adopting (Priority: P1)

A maintainer wants evidence about whether nextest is faster and safe on this suite before the gate depends on it.

**Why this priority**: Nextest's model is one process per test, not one per binary, and the 267 s figure measured the per-binary model. Setup shared within a binary would repeat once per test. Tests that rely on sharing a process (env vars, cwd, `OnceLock` caches) could break or slow down. Adopting nextest without measuring would replace a known cost with an unknown one.

**Independent Test**: Run the full suite under both runners on the same machine and compare wall time, summed time, pass counts and the failures that only nextest shows.

**Acceptance Scenarios**:

1. **Given** an idle local machine and one build, **When** the suite runs 3 times under each runner, **Then** min/median/max wall time and summed test time are recorded for each, along with each run's passed, failed and skipped counts.
2. **Given** the nextest runs, **When** a test fails under nextest but passes under `cargo test`, **Then** it is listed with a cause verified by reproduction. The cause is either fixed, or the test is placed in a serial group with the reason recorded beside it.
3. **Given** the test files that keep shared state in a lazy static, **When** each is timed under both runners, **Then** any binary whose nextest summed time exceeds its `cargo test` summed time by more than its own variance is listed with the cause.

---

### User Story 2 - A gate that still asserts a positive signal (Priority: P1)

A contributor runs `./scripts/gate.sh --expect … prepr -- ./scripts/pre-pr.sh` and gets a verdict that cannot pass without the tests having run and passed.

**Why this priority**: The gate's contract (CLAUDE.md, `release_gates.rs`) is that it asserts a positive signal, never the absence of a failure. Changing the runner changes the summary lines that signal is read from.

**Independent Test**: Mutation tests in `release_gates.rs` run the gate against a failing test and against a build that does not compile, and both must fail.

**Acceptance Scenarios**:

1. **Given** a passing suite, **When** `pre-pr.sh` runs, **Then** it prints `>>> all pre-PR checks passed.` only after the runner's own all-passed summary, and the gate's `--expect` matches it.
2. **Given** one failing test, **When** `pre-pr.sh` runs, **Then** every other test still runs, as `--no-fail-fast` provides today, and the gate fails.
3. **Given** a build that does not compile, **When** `pre-pr.sh` runs, **Then** the gate fails.
4. **Given** nextest is not installed, **When** `pre-pr.sh` runs, **Then** it exits non-zero with a message naming the install command. It must not fall back to `cargo test`.
5. **Given** doctests, which nextest does not run, **When** `pre-pr.sh` and CI run, **Then** doctests run in their own step, and their count before and after is equal.

---

### User Story 3 - Faster CI and local gate (Priority: P2)

A contributor gets a CI verdict and a local pre-PR verdict sooner.

**Why this priority**: This is the point of the work, but it only counts if Stories 1 and 2 hold.

**Independent Test**: Compare the `Tests` step time per lane, and the local `pre-pr.sh` wall time, before and after.

**Acceptance Scenarios**:

1. **Given** at least 2 CI runs per lane before and after, **When** the `Tests` step times are compared, **Then** each lane's figures are reported with no pre-committed target.
2. **Given** the same idle machine, **When** `pre-pr.sh` runs 3 times before and 3 after, **Then** min/median/max are reported.

---

### Edge Cases

- **A test that reads its own executable's name, or argv:** nextest invokes tests differently. Find any such test through the comparison in Story 1.
- **Doctests:** see Story 2, scenario 5.
- **Build-script environment** (`WAYBILL_FIXTURES_DIR`): nextest sets it as cargo does. #1718's ad-hoc harness did not, which caused its only failure.
- **Ignored tests:** they must stay ignored, with the same count, under both runners.
- **Wall-time floor:** if one of the four self-scan tests in #1716 sets the nextest wall-time floor on a lane, fold #1716's fix in. Otherwise leave it filed.
- **Flaky tests:** a test that fails under nextest in 1 of 3 runs is a finding to resolve, not to retry away. No nextest retries are configured.

## Requirements *(mandatory)*

### Functional Requirements

**Runner and gate**

- **FR-001**: The CI `Tests` step on Linux and macOS MUST run the workspace's tests under nextest, with no-fail-fast behaviour.
- **FR-002**: `scripts/pre-pr.sh` MUST run tests under nextest. Clippy is unchanged. Neither command gets a `+toolchain` argument.
- **FR-003**: Doctests MUST run as a separate step locally and on each CI lane that runs nextest. The doctest count must be equal before and after.
- **FR-004**: `pre-pr.sh` MUST fail with an install hint when nextest is missing. A silent fallback is not allowed.
- **FR-005**: The gate's positive-signal contract MUST hold under the new runner. `release_gates.rs` MUST gain mutation tests showing that a failing test and a non-compiling build each fail the gate.

**Measurement and test isolation**

- **FR-006**: Before adoption, the suite MUST be run at least 3 times locally and once per CI lane under nextest. Every test that fails only under nextest MUST be recorded with a reproduced cause, and either fixed or serialised through a named test group in `.config/nextest.toml`, with the reason recorded beside it.
- **FR-007**: Per-binary times under both runners MUST be recorded for the files with shared lazy state, and any binary that regresses MUST be listed.

**Tool, scope and reporting**

- **FR-008**: Nextest MUST be installed on CI through an action pinned to a commit SHA, with the version pinned. No new Cargo dependency is allowed.
- **FR-009**: Windows (non-blocking) MUST adopt the same invocation as Linux and macOS. It stays `continue-on-error`.
  - **Decision:** a lane-specific command would bring back the divergence the gate exists to remove. The lane is non-blocking, so a Windows-only failure does not block a merge, and it is reported like any other finding.
- **FR-010**: The eBPF lane, the nightly workflow and the public-corpus workflow are out of scope, unless the change on that lane is the same one-line runner swap and its result is verified.
- **FR-011**: The test counts MUST be equal under both runners: passed plus ignored per package, and the doctest count. Any difference MUST be explained test by test.

### Key Entities

- **Runner comparison record**: one per run. It holds the runner, the machine, wall time, summed time, the passed/failed/ignored counts, and the failures seen only under one runner.
- **Serial group**: a named nextest group. It lists its member tests, the reason for each, and the reproduced failure the group resolves.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The local suite's wall time under nextest is reported as min/median/max over 3 runs against `cargo test` on the same machine. Nextest is adopted only if its median is lower.
- **SC-002**: The CI `Tests` step time per lane is reported over at least 2 runs before and 2 after, with no pre-committed target.
- **SC-003**: The local `pre-pr.sh` wall time is reported as min/median/max over 3 runs, before and after.
- **SC-004**: There are 0 unexplained count differences between the runners: passed, ignored and doctest totals.
- **SC-005**: Every serialised test has a recorded reason and a reproduced failure, and none is serialised on suspicion alone.
- **SC-006**: The new gate mutation tests fail when their check is removed, and pass otherwise.

## Assumptions

- Nextest's per-test process model sets the same build-script environment that `cargo test` does. This is checked in Story 1, not assumed in a requirement.
- The hosted runner images can install a pinned nextest binary through the SHA-pinned action.
- **Out of scope:** #1713 (redundant tests) and #1714 (consolidating binaries). #1716 is in scope only under its edge-case condition above.
- One gate, one PR. Zero new Cargo dependencies.
