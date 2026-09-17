# Phase 1 Data Model: A trustworthy eBPF canary signal

**Feature**: `896-fix-ebpf-canary` | **Date**: 2026-09-17

There is no persistent store. Every entity below is represented either by a
GitHub Actions job output (lifetime: one run) or by GitHub issue state
(lifetime: the streak). That is deliberate — the streak counter that works
today *is* the issue, and R6/R7 keep it.

---

## Entity: Canary run

One scheduled or dispatched attempt to build the kernel-side artifact.

| Field | Representation | Source | Notes |
|---|---|---|---|
| `target_version` | job output | `steps.resolve_target.outputs.target` | existing; `latest` on schedule |
| `installed_version` | job output | `steps.install.outputs.installed-version` | existing; may be empty if install failed |
| `pinned_version` | job output | `.github/env/bpf-linker.env` | new; needed to name both versions in a report (FR-005) |
| `latest_build_outcome` | job output | build step `outcome` | new; `success` \| `failure` \| `skipped` |
| `control_build_outcome` | job output | control step `outcome` | new; `skipped` when the latest build passed |
| `failing_step` | job output | step name | new; FR-005 |
| `error_excerpt` | job output | captured stderr tail | new; FR-005 |
| `artifact_present` | job output | post-build existence check | new; FR-002a |
| `attributed_cause` | job output | derived, see below | new; the entity the feature adds |

**Lifetime**: the run. Nothing is carried forward except through issue state.

**Validation**:
- `artifact_present` is only meaningful where the corresponding build step
  reported `success`; a build that failed has no artifact claim to make.
- `installed_version` empty is a legal state and MUST render as an explicit
  "install failed" marker rather than an empty string in a report.

---

## Entity: Attributed cause

The distinction the current design lacks entirely.

**Domain**: `component` | `canary` | `none`

**Derivation** — a pure function of two outcomes, and of nothing else.
Per FR-003b it reads neither step position nor error text:

| `latest_build_outcome` | `control_build_outcome` | `artifact_present` | `attributed_cause` |
|---|---|---|---|
| success | (not run) | true | `none` |
| success | (not run) | **false** | `canary` |
| failure | success | — | `component` |
| failure | failure | — | `canary` |
| skipped (run died earlier) | (not run) | — | `canary` |

Row 2 is FR-002a: a zero exit with no artifact is a failure, and it is the
canary's, not the component's — nothing about the watched component explains a
build that claims success and produces nothing.

Row 4 is SC-004a: both failing means canary, *whichever step failed*. It is the
row that re-classifies the live 35-run streak.

**Validation**: `attributed_cause = component` requires
`control_build_outcome = success`. There is no path to an upstream attribution
that does not have a passing control behind it — which is the whole point.

---

## Entity: Failure report

A GitHub issue. Two kinds, distinguished by title (R6).

| Field | `component` kind | `canary` kind |
|---|---|---|
| title | `[canary] bpf-linker eBPF build regression` (unchanged) | distinct, canary-fault title |
| labels | `canary,ebpf,regression` | `canary,ebpf,regression` |
| dedupe key | exact title match within that label set | same mechanism, different title |
| next steps | file upstream at `aya-rs/bpf-linker` | fix the canary; never mentions upstream (FR-006) |
| body must name | both versions, failing step, error excerpt | failing step, error excerpt, the divergence if known |

**Invariants**:
- At most one open issue per kind (existing dedupe).
- Both kinds MAY be open simultaneously (FR-004b) — they are independent
  issues, so this holds without extra logic.
- A run of kind K closes only open issues of kind K. A green run
  (`attributed_cause = none`) closes both, because a green run proves the
  canary works *and* that latest builds.
- The title of the `component` kind is frozen: #685 exists under it and
  external references to it must keep resolving.

---

## Entity: Failure streak

Consecutive failing runs sharing one attributed cause.

| Field | Representation |
|---|---|
| first failure | `created_at` of the open report of that kind |
| elapsed days | `now - created_at`, in whole days |
| length in runs | number of comments (diagnostic only; never governs escalation) |

**State transitions**:

```
no open report ──(failure of kind K)──> open report of kind K, elapsed = 0
open report K ──(failure of kind K)──> same report, comment appended, elapsed grows
open report K ──(failure of kind K')──> report K stays open; report K' opens at elapsed 0
open report K ──(green run)──> report K closed; a later failure starts at elapsed 0
```

The third transition is FR-009/SC-007: a change of cause opens a new report
rather than extending the old one, so the old streak's age is never attributed
to the new cause.

**Validation**: elapsed days is derived at report time from `created_at` and
never stored, so it cannot drift from the issue it describes.

---

## Entity: Escalation window

| Field | Value | Source |
|---|---|---|
| length | 30 days | `docs/development/ebpf-toolchain.md:97,103` |
| measured from | streak's first failure (`created_at`) | FR-007a |
| effect when crossed | report states elapsed days prominently and declares the documented fallback due | FR-008 |

**Validation**: the window MUST NOT be conditioned on any human action. This is
the loophole FR-007a names — m234's window is gated on upstream
*responsiveness* (`ebpf-canary.yml:166`), which never started because nobody
filed upstream, and 35 days passed with escalation technically not yet due.

---

## Entity: Environment divergence check

Not a runtime entity; an assertion that runs inside the canary.

| Field | Representation |
|---|---|
| subject | the nightly-toolchain `components:` declared by the canary |
| reference | the same field in `ci.yml`'s eBPF lane |
| outcome | pass, or run-failure attributed to `canary` |

A divergence is by definition a canary fault, so its failure feeds the same
attribution path as any other pre-build failure.
