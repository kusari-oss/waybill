# Contract — preflight single-flight coordination (US1)

**Feature**: 775-preflight-single-flight
**Status**: Complete
**Date**: 2026-09-05
**Supersedes**: nothing — this repairs the m771 US3 shared-preflight mechanism to deliver its own stated design intent ("exactly one preflight per scope").

Internal coordination contract for `analyze_main_module`'s preflight branch in `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs`. Reviewers verify these properties at code-review time.

---

## Contract 1 — At most one preflight spawn per scope per scan (FR-001)

**Pre-milestone**: every worker reaching the cache-miss branch spawns. With an 18-worker pool starting simultaneously, 22 spawns were observed on a fixture where 6 was correct.

**Post-milestone**: for any `go.work` scope S, at most one worker per scan performs a `go list all` spawn attributed to S.

**Verification**: unit test — N threads (N ≥ 4) request the preflight for one scope against an injectable work function (research R4); assert the work function was invoked exactly once. Plus end-to-end via Contract 5's counter on a real fixture.

---

## Contract 2 — Waiters reuse the claimant's outcome (FR-002, FR-005)

**Post-milestone**: a worker arriving while scope S's preflight is in flight blocks until it completes, then observes the identical outcome — `Ok` or `Skipped(reason)` — that the claimant produced. It MUST NOT spawn its own subprocess, and MUST NOT synthesize a different outcome.

Failure outcomes propagate exactly as success outcomes do. A scope whose preflight failed skips every member per m771 FR-007, with one spawn total rather than one per member.

**Verification**: unit test — injectable work function returns a failure outcome; assert every one of N waiting threads observes that same failure, and the work function ran once.

---

## Contract 3 — Distinct scopes never serialize against each other (FR-003)

**Post-milestone**: preflights for two different scopes proceed concurrently. The single-flight mechanism is keyed per scope; there is no global preflight lock.

This is the property that rules out the simplest possible fix (holding the shared-cache mutex across the subprocess), which would satisfy Contract 1 while converting a fixed cost into a new serialization bottleneck on multi-scope repositories.

**Verification**: unit test — two scopes, an injectable work function that blocks on a barrier requiring both to be in flight simultaneously; the test completes only if both entered concurrently. A serializing implementation deadlocks the barrier and fails the test by timeout.

---

## Contract 4 — Cache mutex is never held across a subprocess spawn (FR-004)

**Post-milestone**: the shared-cache mutex is acquired only for map read/insert operations. Any wait that spans the subprocess happens on the per-scope cell, not the cache.

Consequence: a worker whose scope already has a completed outcome takes the fast path without blocking, even while an unrelated scope's 11s preflight is in flight.

**Verification**: code review — no subprocess-spawning call appears within a cache-mutex guard's lifetime. Contract 3's concurrency test fails if this is violated.

---

## Contract 5 — Preflight invocation count is observable (FR-015)

**Post-milestone**: the m112 FR-013 summary log line carries one additional field reporting actual preflight subprocess spawns for the scan.

Semantics: the count increments on a real spawn only — not on cache reads, not on waiter wake-ups. A request-counting implementation would report 39 both before and after the fix and would satisfy nothing.

**Wire shape**: every field present pre-milestone (`analyzed`, `prod`, `test`, `not_needed`, `unresolved`, `unknown_marked`, `workspace_modules`, `skipped`, `elapsed_ms`) retains its name, meaning, and firing conditions. The new field is purely additive (FR-010).

**Verification**: automated test asserts the reported count on a multi-workspace fixture equals one per distinct scope plus one per loose workspace (SC-003).

---

## Contract 6 — Loose-workspace fallback unchanged (FR-006)

**Post-milestone**: a workspace governed by no `go.work` scope runs its own preflight exactly once, exactly as it did pre-milestone. The m771 FR-008 fallback path is untouched.

**Verification**: the fixture-level count in Contract 5 includes loose workspaces at one spawn each; a fixture with zero `go.work` files must show a count equal to its workspace count, matching pre-milestone behavior.

---

## Contract 7 — No deadlock, no spin, panic-safe (NFR-001, NFR-002)

**Post-milestone**:
- A waiter's block terminates when the claimant completes, fails, or exhausts the shared budget. No busy-wait.
- The wait is transitively bounded: the preflight subprocess is already bounded by the existing timeout and the shared budget (research R4 deferral note), so no independent waiter timeout is introduced.
- A panic in a claimant propagates to that scope's waiters rather than leaving them blocked forever. Waiters fail fast (research R2). This matches pre-milestone behavior, where a panicking preflight unwound its worker.

**Verification**: Contract 3's barrier test would time out on a deadlock. Panic propagation is verified by code review against research R2's stated poisoning semantics.

---

## Contract 8 — Byte-identity of emitted documents (FR-007, SC-002)

**Post-milestone**: US1 changes no emitted SBOM content whatsoever. Every CycloneDX, SPDX 2.3, and SPDX 3 document is byte-identical modulo document-identity fields.

Scratch verification measured 817/817 components and dependencies identical on the k8s fixture.

**Verification**: the existing `cdx_regression`, `spdx_regression`, `spdx3_regression`, `golang_*`, `scan_go_*`, and m669 corpus golden suites pass unchanged.

---

## Contract 9 — Zero new operator surface, zero new dependencies (FR-008, FR-009)

**Post-milestone**: no new CLI flags, no new environment variables, no `Cargo.toml` or lockfile change. Coordination uses `std::sync` primitives already imported in the touched module.

**Verification**: `git diff` shows no change to `Cargo.toml` or the lockfile; CLI help output is unchanged.
