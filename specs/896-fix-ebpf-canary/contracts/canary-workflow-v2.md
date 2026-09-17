# Contract: eBPF Canary Workflow (v2)

**File**: `.github/workflows/ebpf-canary.yml`
**Type**: GitHub Actions workflow (scheduled + `workflow_dispatch`)
**Supersedes**: `specs/234-fix-ebpf-linker-regression/contracts/canary-workflow.md`
**Feature**: `896-fix-ebpf-canary` (#685)

This contract states what the canary must do. Deltas from the m234 contract are
marked **[NEW]** or **[CHANGED]**; everything else is preserved verbatim,
including the schedule, the watched component, and the pin mechanism.

---

## C-1. Triggers

Unchanged: daily `cron: '0 6 * * *'`, plus `workflow_dispatch` with `version`
and `dry_run` inputs.

**[NEW]** A third dispatch input that deliberately omits the toolchain
component, for exercising the canary-fault path on demand (R9 item 3). Default
off. It MUST have no effect on the scheduled path.

---

## C-2. Build environment

**[CHANGED]** The canary MUST prepare the same environment as the project's
working eBPF lane at `.github/workflows/ci.yml`. At minimum, and in this order:

1. nightly toolchain **with the `rust-src` component**
2. stable toolchain (the default under which `cargo run -p xtask` executes)
3. `rustup default stable`
4. removal of `rustup-init` from `$HOME/.cargo/bin`, after any cache restore

Ordering is load-bearing: `dtolnay/rust-toolchain` makes whichever toolchain it
installed *last* the default, and `cargo run -p xtask -- ebpf` must run under
stable while the nested `cargo +nightly build` runs under nightly.

Rationale is in research R1/R3. The canary installs stable only today
(`ebpf-canary.yml:70-73`), which is the entire live defect.

---

## C-3. Environment divergence assertion **[NEW]**

The canary MUST fail if its declared toolchain components diverge from
`ci.yml`'s eBPF lane. The check runs **before** the build, so a divergence is
reported as a canary fault rather than surfacing later as a build failure of
ambiguous origin.

Satisfies FR-010 / SC-002. Text-level comparison is sufficient and is the
project's existing posture for this class of gate (m115/m117 walker audit).

---

## C-4. Builds **[CHANGED]**

| | Step | Runs when |
|---|---|---|
| B1 | build eBPF object against the **resolved target** (`latest` on schedule) | always |
| B2 | build eBPF object against the **pinned** version, as control | only when B1 failed |
| B3 | build userspace with `--features ebpf-tracing` | only when B1 succeeded |

- B2 obtains the pinned version by invoking the `install-bpf-linker` composite
  with an empty `version` input, so it reads `.github/env/bpf-linker.env`. The
  composite is safe to invoke twice on the binary path (research R5).
- B1 and B2 MUST NOT share a `CARGO_TARGET_DIR`. Reusing objects linked by a
  different linker version yields a result belonging to neither build.
- B1 and B2 MUST each record their step `outcome` as a job output, and MUST NOT
  abort the job on failure — the job's verdict is computed from both.

---

## C-5. Artifact verification **[NEW]**

After a build step reports success, the canary MUST verify that
`waybill-ebpf/target/<target-dir>/bpfel-unknown-none/release/waybill-ebpf`
exists and is non-empty. Absence is a failure regardless of exit status.

The path MUST be removed before the build, so a file left by an earlier step
cannot satisfy the check (spec edge case; research R4).

Verification is **presence only**. Loading or verifying the object is out of
scope and belongs to the existing `ebpf-tracing` test lane.

Satisfies FR-002a / SC-008.

---

## C-6. Attribution **[NEW]**

The attributed cause is a pure function of build outcomes:

| B1 | B2 | artifact after B1 | cause |
|---|---|---|---|
| success | not run | present | `none` |
| success | not run | **absent** | `canary` |
| failure | success | — | `component` |
| failure | failure | — | `canary` |
| did not run | not run | — | `canary` |

It MUST NOT consult which step failed, nor match against error text (FR-003b).
An attribution of `component` is only reachable with a passing control.

FR-004's veto — no upstream attribution unless the failure occurred at the step
exercising the component — is satisfied **structurally** rather than by a
separate check: `component` requires B1 to have failed, and B1 *is* that step.
FR-003b forbids step position as the determining basis, not as a consequence.

---

## C-7. Reports **[CHANGED]**

Two titles, deduped independently within the shared label set
`canary,ebpf,regression`:

| cause | title | next steps |
|---|---|---|
| `component` | `[canary] bpf-linker eBPF build regression` — **frozen**, #685 lives under it | reproduce, then file at `aya-rs/bpf-linker` |
| `canary` | a distinct canary-fault title | fix the canary; MUST NOT name an upstream project |

Every report of either kind MUST contain:

- the version(s) tested, and for a `component` report both the failing version
  and the passing control version;
- the name of the failing step;
- an excerpt of the error text.

A `canary`-kind report MUST additionally state that the watched component went
**untested** this run (FR-004c). When the canary cannot build, nothing was
learned about the component either way, and a report silent on that reads as
though the component is healthy — the spec edge case "the report must not hide
the second behind the first".

A generic "reproduce locally" instruction does not satisfy this. The current
report contains none of the three, which is why 35 reports carried the same
non-information (SC-005).

Dedupe mechanism is unchanged: list open issues by label, exact-match the
title, comment if found, create if not.

---

## C-8. Streak and escalation **[CHANGED]**

- Elapsed days = `now - created_at` of the open report **of that kind**
  (research R7). No new state is stored.
- The measure MUST NOT depend on any human action having been taken (FR-007a).
- When elapsed days ≥ 30 (`docs/development/ebpf-toolchain.md:97`), the report
  MUST state the elapsed duration prominently and declare the documented
  fallback due (FR-008).
- A report is attributable to exactly one cause; a change of cause opens a
  second report rather than extending the first.

---

## C-9. Recovery **[CHANGED]**

- A run with cause `none` closes open reports of **both** kinds — a green run
  proves both that the canary works and that the target version builds.
- A run with cause K MUST NOT close an open report of the other kind. Today's
  `report-success` closes the single matching title; with two titles it must
  not close an outstanding upstream regression that nothing has fixed.

---

## C-10. Preserved from m234

- Schedule, concurrency group `ebpf-canary`, 15-minute timeout,
  `permissions: issues: write`, SHA-pinned actions.
- The `install-bpf-linker` composite is the only install path.
- The "latest installed the same version as the pin" warning.
- `dry_run` suppresses all issue writes.
- The watched component and the pin file are unchanged.

---

## Verification

| Contract | How verified |
|---|---|
| C-2, C-4 B1 | `workflow_dispatch` with `version` = the pinned version passes (SC-001) |
| C-3 | mutate the canary's components list; the run fails at the check |
| C-5 | delete the artifact between build and check; the run reports red |
| C-6 row 4 | dispatch with the deliberate-break input; expect a `canary` report |
| C-7 | inspect the produced issue for versions, step, error text |
| C-8 | evaluate the elapsed-days rule against #685's `created_at` (2026-08-13) — must escalate |

Dispatch **one workflow at a time** and wait for each to conclude; the
repository's canary workflows share a concurrency posture.
