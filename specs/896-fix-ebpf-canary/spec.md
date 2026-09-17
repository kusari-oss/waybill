# Feature Specification: A trustworthy eBPF canary signal

**Feature Branch**: `896-fix-ebpf-canary`
**Created**: 2026-09-17
**Status**: Draft
**Input**: User description: "685"

Addresses [#685](https://github.com/kusari-oss/waybill/issues/685).

## Context

The eBPF canary exists to answer one question every night: *can we move off
the pinned toolchain version yet?* It builds the kernel-side artifact against
the newest available toolchain component and, when that fails, opens an issue
telling a maintainer an upstream regression is blocking the bump.

It has failed **35 consecutive nights**, since 2026-08-13, and every failure
has been reported as an upstream regression in the component it watches. The
issue it maintains instructs the reader to reproduce locally and *"file
upstream at aya-rs/bpf-linker"*.

The failure is not upstream. It is the canary's own build environment:

```
error: ".../nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.lock"
       does not exist, unable to build with the standard library
```

That is a missing toolchain component. The equivalent lane in the main CI
workflow installs it and builds successfully — verified on a run where that
job actually executed rather than being skipped. The canary's own setup omits
it.

So the signal has been inverted for over a month: a green upstream reported
as red, with a next-step pointing at a project that has nothing to fix. The
one question the canary exists to answer has been unanswerable that whole
time, and nobody could tell, because a failing canary looks the same whether
the fault is upstream's or its own.

Two things are therefore wrong, and the second is the one that will recur:
the canary cannot currently build, and a canary that cannot build reports
that as someone else's regression.

## Clarifications

### Session 2026-09-17

- Q: What should the canary do when it fails for its own reasons rather than the watched component's? → A: Open a **separate** report under its own title, deduped independently of the upstream-regression report. The issue title is what a maintainer scans, and "bpf-linker eBPF build regression" sitting on a self-inflicted failure is precisely what hid this for 35 nights; a distinct title makes the distinction visible in the issue list rather than only in a comment body.
- Q: How should the canary decide whether a failure belongs to it or to the watched component? → A: Run the known-good **pinned** version as a control alongside the newest one. Pinned fails too → the canary is broken; pinned passes and newest fails → the component regressed. Chosen over a step-position rule because a positional rule mis-attributes the live failure — it occurs at the build step, which position alone would call upstream's, while the cause is the canary's own missing component.
- Q: What starts the escalation clock? → A: Days elapsed since the streak's first failure. Chosen over m234's rule, whose window is gated on upstream *responsiveness* — `ebpf-canary.yml:166` reads "if unresponsive within the 30-day fallback window", which presupposes upstream was contacted. That gating is the loophole this issue fell through — nobody filed upstream on #685, so by that reading the clock never started and 35 days passed with escalation technically not yet due. A clock that begins automatically cannot be stalled by inaction.
- Q: Should a green run be verified, or is a successful exit status enough? → A: Verify the artifact exists before reporting green. A build that produces nothing is a failure regardless of exit status. The canary's green authorises a version bump, so a false green would be acted on and nothing downstream would catch it — the weaker half of the same mistake this feature exists to fix. Deeper well-formedness checking is out of scope; that shades into testing the loader rather than the build.


## User Scenarios & Testing *(mandatory)*

### User Story 1 - The canary answers the question it exists to ask (Priority: P1)

A maintainer wants to know whether the newest upstream component still builds
the project's kernel-side artifact. The canary runs and gives a truthful
answer: green means it built, red means it did not.

**Why this priority**: Until the canary can build at all, every other
improvement decorates a signal that carries no information. This is also the
only story that restores the feature's original purpose.

**Independent Test**: Run the canary against the *pinned* component version,
which is known to build. It must pass. Today it fails, because the failure
has nothing to do with which component version is being tested.

**Acceptance Scenarios**:

1. **Given** the canary runs against the component version the project
   currently pins, **When** the build executes, **Then** it succeeds — because
   that version is known to build in the main workflow. This run is also the
   control that attribution depends on (FR-003a), so its correctness is
   load-bearing twice over.
2. **Given** the canary runs against the newest available component, **When**
   the build succeeds AND its artifact is present, **Then** the run is green
   and any open regression report is closed.
3. **Given** a build that exits successfully but produces no artifact,
   **When** the canary evaluates it, **Then** the run is red — a green here
   would authorise a bump on the strength of nothing having been built.
4. **Given** the canary's build environment is prepared, **When** it is
   compared against the main workflow's equivalent lane, **Then** it requires
   the same components — a divergence between them is the defect this story
   fixes.

---

### User Story 2 - A failure says whose fault it is (Priority: P2)

A maintainer opens a canary failure report. It tells them whether the thing
being watched regressed, or whether the canary itself could not run. They act
on the right one.

**Why this priority**: This is what turned a one-line environment fix into 35
wasted nights. The canary could not distinguish "upstream broke" from "I
broke", so it asserted the first and sent readers to the wrong repository.
Fixing only Story 1 leaves the next self-inflicted failure equally
misleading.

**Independent Test**: Break the canary's own environment deliberately and
confirm the resulting report does not claim an upstream regression, and does
not instruct the reader to file upstream.

**Acceptance Scenarios**:

1. **Given** the canary fails before reaching the step that exercises the
   watched component, **When** it reports, **Then** it opens a report under
   its own title identifying the failure as the canary's, and does not
   attribute it upstream.
2. **Given** the control build against the pinned version succeeds and the
   build against the newest version fails, **When** it reports, **Then** the
   report attributes the failure to that component and names both versions.
3. **Given** both the control and the newest-version build fail, **When** it
   reports, **Then** the failure is attributed to the canary regardless of
   which step failed.
4. **Given** a report of either kind, **When** a maintainer reads its
   next-steps, **Then** those steps direct them at the party that can
   actually act.

---

### User Story 3 - A long-running failure escalates (Priority: P3)

A failure that has persisted for weeks is visibly different from one that
started last night, and the difference prompts a decision rather than another
identical comment.

**Why this priority**: The existing report carries a 30-day escalation
window. That window elapsed at day 30 and nothing happened; the run count
reached 35 with no change in how the failure presented. Valuable, but only
once the signal underneath it is trustworthy — escalating a
mis-attributed failure faster would have made things worse, not better.

**Independent Test**: Simulate a failure streak that crosses the escalation
threshold and confirm the report changes in a way a maintainer would notice.

**Acceptance Scenarios**:

1. **Given** a failure streak shorter than the escalation window, **When** the
   canary reports, **Then** the report accumulates as it does today.
2. **Given** a streak that crosses the window, **When** the canary reports,
   **Then** the report makes the elapsed duration prominent and states that
   the documented fallback is now due.
3. **Given** a streak that crosses the window and nobody has filed upstream,
   **When** the canary reports, **Then** escalation is still due — the clock
   does not wait on that action.

---

### Edge Cases

- The canary's own environment fails *and* the watched component has genuinely
  regressed — the report must not hide the second behind the first.
- The watched component is unavailable or its version cannot be resolved, so
  nothing was actually tested.
- The newest component and the pinned component are the same version, so the
  canary is testing what the project already ships.
- The canary recovers after a long streak — the report must close rather than
  linger, and the streak count must not survive into a future unrelated
  failure.
- Two failures of different kinds on consecutive nights: the report must not
  read as one continuous streak of the same cause, and the elapsed clock must
  restart with the new cause.
- A streak that spans a period when the canary did not run at all — elapsed
  days and the number of reports will disagree, and the elapsed measure is
  the one that governs escalation.
- The build succeeds but produces no artifact — a failure the exit status
  does not signal, and the one path by which this canary could report a false
  green.
- The artifact exists but is left over from an earlier run rather than
  produced by this one.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The canary MUST prepare its build environment with everything
  the project's own working build requires, so that its result reflects the
  component under test rather than its own setup.
- **FR-002**: The canary MUST be verifiable against a version known to build.
  A run against the currently pinned version MUST pass.
- **FR-002a**: The canary MUST confirm the build produced its artifact before
  reporting success. A build that exits zero having produced nothing MUST be
  treated as a failure, since the canary's green is what authorises a version
  bump.
- **FR-003**: The canary MUST distinguish a failure of its own execution from
  a failure of the component it watches, and MUST say which occurred.
- **FR-003a**: The distinction MUST be determined by also building against the
  version the project currently pins, which is known to build. A failure of
  that control build means the canary is at fault; a control that succeeds
  while the newest version fails means the component regressed.
- **FR-003b**: Attribution MUST NOT rest on which step failed or on matching
  error text. Both mis-attribute the failure this feature exists to fix,
  which occurs at the step that exercises the component but is caused by the
  canary's own environment.
- **FR-004**: A failure report MUST NOT attribute a failure upstream unless
  the failure occurred at the step that exercises the watched component.
- **FR-004a**: A failure of the canary's own execution MUST be reported under
  a title distinct from the upstream-regression report, and deduplicated
  independently of it, so the two are distinguishable without opening either.
- **FR-004c**: A report attributed to the canary MUST state that the watched
  component went untested this run. When the canary cannot build, nothing was
  learned about the component either way, and a report that is silent on that
  reads as though the component is fine.
- **FR-004b**: The two report kinds MUST be able to be open simultaneously —
  a broken canary does not clear an outstanding upstream regression, and an
  outstanding upstream regression does not suppress notice that the canary
  has stopped working.
- **FR-005**: A failure report MUST include the evidence a reader needs to act
  — at minimum the version tested, the step that failed, and the error text —
  rather than a generic instruction to reproduce.
- **FR-006**: Next-step guidance in a report MUST match the attributed cause,
  so a self-inflicted failure never directs a reader to an upstream project.
- **FR-007**: The canary MUST report how long the current failure streak has
  run, measured in days elapsed since the streak's first failure, so a
  persistent failure is distinguishable from a new one.
- **FR-007a**: The elapsed measure MUST NOT depend on any human action having
  been taken. A clock gated on upstream responsiveness never starts when
  nobody files, which is how this failure went 35 days without escalation
  becoming due.
- **FR-008**: When a streak crosses the documented escalation window, the
  report MUST say so explicitly rather than continuing unchanged.
- **FR-009**: A recovery MUST close the open report, and MUST NOT leave streak
  state that could be attributed to a later, unrelated failure.
- **FR-010**: The canary's build environment MUST be checkable against the
  project's main build environment, so a divergence between them is detectable
  rather than discovered a month later.

### Key Entities

- **Canary run**: one scheduled attempt to build the kernel-side artifact
  against a chosen component version. Produces an outcome, an attributed
  cause, and evidence.
- **Attributed cause**: whether a failure belongs to the watched component or
  to the canary's own execution. The distinction the current design lacks
  entirely, and the reason a month was lost.
- **Failure streak**: consecutive failing runs of the same attributed cause.
  Its length is what makes a stale failure visible; a change of cause starts a
  new streak rather than extending the old one.
- **Escalation window**: the documented period after which a persistent
  failure warrants a decision rather than another report. Measured in days
  from the streak's first failure, independently of whether anyone has acted
  on it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A canary run against the currently pinned component version
  passes. Today it fails, and has for 35 consecutive nights.
- **SC-002**: The canary's build environment and the main workflow's
  equivalent require the same components, verified by a check that fails when
  they diverge rather than by inspection.
- **SC-003**: A deliberately broken canary environment produces a report
  under a canary-fault title, distinct from the upstream-regression title,
  containing no instruction to file upstream. A maintainer scanning the issue
  list can tell the two apart without opening either.
- **SC-004**: A genuine failure of the watched component — control build
  passes, newest-version build fails — produces a report naming that component
  and both versions.
- **SC-004a**: A failure affecting both builds is attributed to the canary,
  never upstream, whichever step it occurred at, and the report says the
  component went untested rather than implying it is healthy.
- **SC-005**: Every failure report contains the failing step and the error
  text, so a reader can judge the cause without re-running anything. Today the
  report contains neither, which is why 35 reports conveyed the same
  non-information.
- **SC-006**: A report for a streak past the escalation window is
  distinguishable from a first-night report without counting comments, and
  states the elapsed days explicitly.
- **SC-006a**: A streak reaching the window escalates whether or not anyone
  has acted on it. Replaying #685's history — 35 days, no upstream filing —
  produces an escalated report rather than a 35th identical one.
- **SC-007**: After a recovery, the open report is closed and a subsequent
  unrelated failure starts a streak of one rather than continuing the old
  count.
- **SC-008**: A build that exits zero without producing its artifact is
  reported red. The canary emits no green that is not backed by an artifact
  it can point at.

## Assumptions

- The immediate build failure is the missing toolchain component identified in
  Context. This is treated as established: the error names it, and the main
  workflow's lane installs it and builds successfully. Implementation confirms
  it rather than re-deriving it.
- Whether the newest component version *also* has a genuine problem is
  unknown and cannot be known until the canary builds at all. Discovering a
  real upstream regression after this feature lands is an expected outcome,
  not a failure of it.
- The canary's schedule, the component it watches, and the pinned-version
  mechanism are unchanged. This feature is about the trustworthiness of the
  signal, not its scope.
- Building twice per run — control plus newest — is an accepted cost. This is
  a nightly job with no latency requirement, and the alternative attribution
  mechanisms are the ones that produced the failure being fixed.
- The pinned version is assumed to build. If a control run ever fails while
  the newest succeeds, that is a signal about the pin rather than about
  either build, and the report should say so rather than silently inverting.
- The deduplicating-issue behaviour is kept: one open report per distinct
  failure kind, appended to rather than re-opened. It is the streak counter,
  and it works. This feature adds a second title to that mechanism rather
  than changing it.
- Reproducing the kernel-side build requires Linux. Verification of the build
  itself happens in the project's CI environment rather than on a maintainer's
  machine.
- Artifact verification is presence, not well-formedness. Checking that the
  object loads or verifies would test the loader rather than the build, and
  belongs to the `ebpf-tracing` test lane that already exists.
- No change to the shipped binary, to what the project builds, or to the
  pinned version is in scope. A version bump, if the canary later earns one,
  is separate work.

