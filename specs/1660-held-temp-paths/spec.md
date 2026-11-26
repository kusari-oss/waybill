# Feature Specification: Keep test output paths reserved (#1727)

**Feature Branch**: `1660-held-temp-paths`
**Created**: 2026-11-26
**Status**: Draft
**Input**: #1719, fourth slice: #1727. Refs #1719, #1727.

## Context

Since spec 1658, tests run under nextest: one process per test, started concurrently.

Some tests take an output path from a temporary file and drop the file in the same statement:

```rust
let out = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
```

The file is deleted and only its name is kept, so nothing reserves the path.

**Verified from the code:**
- `tempfile` 3.27.0 generates names from a `fastrand::Rng` (`util.rs:45-66`).
- `fastrand` 2.4.1 seeds it by hashing `Instant::now()` and the current thread ID (`global_rng.rs:203-213`).
- `tempfile` retries a name only when a file with that name already exists (`AlreadyExists`). A deleted file doesn't, so the retry can't fire.

**Inferred, not verified:** in separate processes the test thread's ID is likely the same. Two processes started within one clock tick could then be handed the same path, and one test would read the other's output.

**Observed once:** `scan_pip::scan_pip_dynamic_version_uses_placeholder` read `pkg:pypi/editable-pkg@1.0.0`, another test's fixture, on macOS CI (PR #1726, run 45741984929, attempt 2). It didn't reproduce locally: 0 failures in 40 parallel runs of `scan_pip`.

#1727 first listed 21 sites in 20 files, from a grep with 3 lines of context. Research R1 replaced that list. A statement-level scan of all of `waybill-cli` finds 23 dropped sites in 22 test files, and none in production code. #1727's list was mostly held sites and missed `scan_pip.rs`; #1727 carries the correction.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Find out whether names really collide across processes (Priority: P1)

A maintainer gets evidence about the mechanism before any change: whether concurrent processes that each create and drop a temporary file are ever handed the same name on this host.

**Why this priority**: the cause is inferred from source code, and it was observed once. A fix built on an unconfirmed mechanism may fix nothing; a measurement says what the fix is worth.

**Independent Test**: a stress harness starts many concurrent processes. Each creates and drops a temporary file the same way the tests do, and records the name. The harness counts duplicate names.

**Acceptance Scenarios**:

1. **Given** the harness, **When** it runs at least 3 times, with process counts at and above nextest's local parallelism, **Then** the duplicate count is reported for each run.
2. **Given** a control harness that deliberately seeds every process identically, **When** it runs, **Then** it reports duplicates. That proves the counting works.
3. **Given** a duplicate count of 0 on this host, **When** the results are written up, **Then** they say plainly that the collision was not reproduced, and the fix is justified on the code facts alone.

---

### User Story 2 - Every test output path stays reserved for the whole scan (Priority: P1)

A contributor's parallel test run can't hand two tests the same output file.

**Why this priority**: it removes a source of failures that look random and are hard to diagnose.

**Independent Test**: every candidate site is reviewed and recorded. After the fix, no site drops the temporary guard before the scan output has been read.

**Acceptance Scenarios**:

1. **Given** the 23 sites found by the statement-level scan (research R1), and any found under other spellings, **When** each is reviewed, **Then** it is recorded as either *dropped* (fixed) or *held* (no change), with the reason.
2. **Given** a fixed site, **When** its test runs, **Then** its assertions are unchanged and it passes.
3. **Given** the full suite, **When** it runs under nextest at least 3 times locally, and once on every CI lane, **Then** every run passes.

---

### User Story 3 - The pattern can't come back (Priority: P2)

A future test that drops its temporary guard in the statement that takes the path is rejected before merge.

**Why this priority**: the pattern spread to 20 files without anyone noticing. A rule in prose won't hold; a check will.

**Independent Test**: the check fails on the old shape (a positive control) and passes on each fixed shape (a negative control). It runs locally and in CI through the same script.

**Acceptance Scenarios**:

1. **Given** a test file containing the old shape, **When** the check runs, **Then** it fails and names the file and line.
2. **Given** the fixed shapes, **When** the check runs, **Then** it passes.
3. **Given** the pre-PR gate and CI, **When** either runs, **Then** the same check runs.

---

### Edge Cases

- **Other spellings of the same defect**, which the review searches for:
  - `NamedTempFile::new_in(…)`;
  - `Builder::new()…tempfile()`;
  - `tempdir()` / `TempDir::new()` followed by `.path()` on a temporary that is dropped;
  - `into_temp_path()` followed by a drop.
- **A site whose path is used only to choose a name**, with the file created by the scan itself and read before any other process could plausibly race it: it is still fixed. The rule is "reserved for the whole scan", with no case-by-case risk judgement.
- **Production code with the same shape**, if any, is out of scope here. It is recorded and filed, not fixed.
- **Windows** (non-blocking lane): a held `NamedTempFile` keeps an open handle, and the scan may need to write to that path. Where that matters, the fix uses a held `TempDir` with a file inside it.

## Requirements *(mandatory)*

### Functional Requirements

**Before changing anything**

- **FR-001**: Before any code change, a stress harness MUST measure duplicate temporary-file names across concurrent processes, using the tests' create-and-drop shape. It MUST run at least 3 times, and MUST be accompanied by a control harness that is forced to collide.
- **FR-002**: The result MUST be reported, including a count of 0, with the host and the process counts.

**The fix**

- **FR-003**: Every candidate site MUST be reviewed by hand and recorded as dropped or held, with the reason. The review MUST also cover the other spellings listed under Edge Cases.
- **FR-004**: Every dropped site MUST be changed so that the temporary guard lives until the scan output has been read. Writing the output inside a temporary directory the test already holds is preferred.
- **FR-005**: No assertion's meaning may change.

**Enforcement**

- **FR-006**: A check MUST reject the dropped shape in test code. It MUST be run by `scripts/pre-pr.sh` and by CI from the same script, and it MUST come with a positive and a negative control.

**Verification and constraints**

- **FR-007**: After the fix, the full suite MUST pass under nextest 3 times locally, and on every CI lane once.
- **FR-008**: Zero new Cargo dependencies, no corpus golden changes, one gate, one PR.

### Key Entities

- **Candidate site**: a file, a line, a shape (which spelling), a verdict (dropped or held), and a reason.
- **Stress run**: a host, the process count, the number of names drawn, and the number of duplicates.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of candidate sites have a recorded verdict, and 0 dropped sites remain.
- **SC-002**: The check fails on the old shape and passes on every fixed shape. Both controls are recorded.
- **SC-003**: The stress-harness duplicate counts are reported for every run, including when they are 0, alongside a control run that shows duplicates.
- **SC-004**: The full suite passes 3 of 3 times locally under nextest, and on every CI lane once.

## Assumptions

- The defect is in test code only. Production code with the same shape, if any is found, is filed separately.
- **Out of scope:** #1713, #1714, and CI compile and cache time.
