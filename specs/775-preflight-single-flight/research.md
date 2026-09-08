# Phase 0 Research — m775 single-flight preflight + go.work directive tolerance

**Feature**: 775-preflight-single-flight
**Status**: Complete
**Date**: 2026-09-05

Zero `NEEDS CLARIFICATION` remain after `/speckit.clarify` (Q1 → observability counter; Q2 → shared directive vocabulary). Research below records the decisions the plan phase locks in for `/speckit.tasks`.

---

## R1 — Single-flight primitive: per-scope cell holding a `Mutex` across the subprocess

**Decision**: Store, per scope, an `Arc<Mutex<Option<PreflightOutcome>>>` cell in the shared cache. A worker acquires the *cache* mutex only long enough to (a) read a completed outcome, or (b) clone-or-create the scope's cell; it then releases the cache mutex and acquires the *cell* mutex across the subprocess. The first holder finds `None`, runs the preflight, writes the outcome into both the cell and the cache. Later holders find `Some(..)` and return it without spawning.

**Rationale**:
- Satisfies FR-001/FR-002 (exactly one spawn per scope; waiters reuse) and FR-003 (distinct scopes hold distinct cells, so they never block one another) and FR-004 (the cache mutex is never held across a spawn).
- Requires no new dependency — `std::sync::{Arc, Mutex}` are already imported in `mod_why.rs` for the existing cache (FR-009).
- Blocking on a `Mutex` is the correct wait primitive here: the waiter has nothing else to do, the wait is bounded by the preflight's own subprocess timeout (`run_bounded`), and there is no async runtime in this call path.

**Alternatives considered**:
- **`std::sync::Once` / `OnceLock` per scope**: expresses single-flight directly, but `Once` cannot carry a fallible result out of the initializer ergonomically, and `OnceLock<T>` has no stable "block until initialized by another thread" API on stable Rust (`wait` is unstable). Rejected on stable-toolchain grounds.
- **Hold the cache mutex across the subprocess**: trivially correct for single-flight, and simplest. Rejected because it violates FR-003/FR-004 — one scope's 11s preflight would block every other scope's cache read, converting a fixed-cost fix into a new serialization bottleneck on multi-scope repositories.
- **Condvar with an explicit state enum** (`NotStarted`/`InFlight`/`Done`): the textbook single-flight shape and strictly more expressive. Rejected as unnecessary: the `Mutex<Option<_>>` cell yields identical observable behavior for this access pattern with materially less machinery, and the extra states buy nothing because there is no cancellation path.
- **A single global `Mutex` guarding all preflights**: rejected for the same reason as holding the cache mutex — serializes unrelated scopes (FR-003).

---

## R2 — Poison-safety and panic behavior

**Decision**: Mutex acquisition uses `.expect("<descriptive message>")`, matching the existing convention in `mod_why.rs` (the `waybill-cli` crate root denies `clippy::unwrap_used`, so `.unwrap()` is not available). A panic inside a preflight poisons that scope's cell; subsequent waiters for the same scope then panic on acquisition, which propagates the failure rather than silently degrading classification.

**Rationale**: NFR-002 requires that a panic not leave waiters *permanently blocked*. Poisoning satisfies this: waiters fail fast rather than hanging. Propagating is also the behavior Principle III (Fail Closed) demands — a preflight that panicked has produced no verdict, and silently treating its scope as "classification passed" would emit an SBOM whose build-inclusion data is quietly wrong.

The pre-milestone code has the same property (a panicking preflight unwinds its worker), so this is behavior-preserving rather than a new failure mode.

**Alternatives considered**:
- **Recover from poisoning** (`.unwrap_or_else(|e| e.into_inner())`) and retry the preflight: rejected — converts a panic into a silent retry storm and contradicts Fail Closed.
- **`parking_lot::Mutex`** (no poisoning): rejected, new dependency (FR-009).

---

## R3 — Counting actual spawns (FR-015)

**Decision**: `MainModuleAnalysis` gains a boolean reporting whether *this* call performed a preflight subprocess spawn (as opposed to reading a cached outcome or waiting on another worker's cell). The scan-level classifier sums those booleans into a new `GoModWhyOutcome` counter, reported as one additional field on the existing FR-013 summary line.

**Rationale**: This mirrors the m231 `workspace_active` flag verbatim — a per-analysis boolean aggregated into the outcome struct and surfaced on the same summary line (`workspace_modules=`). Reusing an established shape means no new plumbing pattern to review.

Counting spawns rather than cache reads is what FR-015 requires and what makes SC-003 meaningful: a counter that incremented on every *request* would read 39 both before and after the fix and would have caught nothing.

**Alternatives considered**:
- **A process-global `AtomicUsize`**: simpler to thread, but leaks scan-scoped state into a static, breaking the "repeated scans in one process" edge case and the m666 test-isolation posture. Rejected.
- **Counting inside `run_preflight` itself**: would require passing a counter handle down purely for telemetry. Rejected — the per-analysis boolean is already flowing upward for `workspace_active`, so the aggregation channel exists.

---

## R4 — Testing single-flight without a `go` toolchain

**Decision**: Make the preflight execution injectable at the seam the tests need — the single-flight coordination logic is exercised with a test-supplied work function that records invocation counts and sleeps, rather than spawning `go`. Concurrency assertions (N threads, one scope ⇒ exactly one invocation; two scopes ⇒ both proceed concurrently) run against that seam.

**Rationale**: Constitution Principle VII requires tests to run without elevated privileges in standard CI. A test that spawns real `go list all` would additionally require a Go toolchain and a large fixture, would take ~11s, and would be inherently racy as a correctness assertion. Testing the *coordination* separately from the *subprocess* is what makes FR-001/FR-002/FR-003 assertable deterministically.

The end-to-end behavior (real subprocess counts on a real fixture) is covered by SC-003's assertion on the FR-015 counter, which needs no injection.

**Alternatives considered**:
- **Integration test spawning real `go`**: rejected — toolchain-dependent, slow, and racy; would be skipped on most CI lanes, so it would guard nothing.
- **Assert only via the FR-015 counter end-to-end**: necessary but insufficient — it cannot distinguish "one spawn because single-flight worked" from "one spawn because only one worker ran," and cannot test FR-003 (cross-scope concurrency) at all.

---

## R5 — Shared `go.work` directive vocabulary (Clarifications Q2)

**Decision**: Define the valid `go.work` directive keyword set once in `gowork.rs` — `go`, `toolchain`, `godebug`, `use`, `replace` — and have both parsers consult it. The strict parser (`gowork.rs`) uses it to decide whether an unrecognized leading token is `unknown-directive`; the lenient parser (`mod_why.rs`) uses it to confirm that a non-`use` line it is skipping is a directive it legitimately ignores.

**Rationale**: Satisfies FR-014 with the anti-recurrence property Q2 asked for — adding a directive from a future Go release is a one-place edit. Each parser keeps its distinct behavior (strict validates and reports reasons; lenient extracts members), so neither is restructured.

**Alternatives considered**:
- **Add `toolchain`/`godebug` to the strict parser's `if`-chain and stop**: satisfies FR-011/FR-012 but not FR-014's mechanism requirement; the parsers stay structurally independent and diverge again on the next directive. Explicitly rejected in Clarifications Q2.
- **Delete the strict parser, derive the annotation from the lenient one**: strongest agreement guarantee, but discards the malformed-reason vocabulary (`invalid-use-path`, `duplicate-use-path`, `invalid-replace-syntax`, `unknown-directive`) that FR-013 requires preserving. Rejected in Clarifications Q2.

---

## R6 — Directive semantics: `toolchain` and `godebug` are accept-and-ignore

**Decision**: The strict parser accepts `toolchain <name>` and `godebug <key>=<value>` as valid and stores nothing from them. `GoWorkDocument` gains no fields.

**Rationale**: Nothing downstream consumes either directive. `toolchain` selects a Go toolchain version; `godebug` sets runtime compatibility defaults. Neither affects member enumeration, scope detection, module identity, or any emitted SBOM field. Parsing them into unused struct fields would add surface with no consumer.

Repeated `godebug` lines are permitted by Go and must not be treated as malformed (spec edge case "duplicate directives") — accept-and-ignore handles this with no special case, unlike `use` paths which carry an explicit `duplicate-use-path` rejection.

**Alternatives considered**:
- **Capture `toolchain`/`godebug` values into `GoWorkDocument`**: rejected — no consumer, and Principle V discourages carrying metadata with no emission path.

---

## R7 — Byte-identity risk assessment for US2

**Decision**: US2 changes the emitted `waybill:go-workspace-mode` annotation value on exactly those repositories whose `go.work` contains `toolchain` or `godebug`. FR-007 and SC-002 carve this out as the single permitted diff.

**Rationale**: Any golden fixture whose `go.work` carries these directives will legitimately change. The k8s corpus golden is the known instance (its annotation currently reads `malformed: unknown-directive` and will become the detected form). Task ordering must therefore treat golden regeneration as an expected, reviewed diff for US2 — and reviewers must confirm the diff is confined to that annotation, per memory `feedback_verify_golden_churn_normalized` (mask content-addressed IDs and sort before diffing).

**Outcome (recorded at implement-time)**: this prediction was WRONG, in a harmless direction. No in-repo golden required regeneration — k8s is not among the corpus goldens (`go-cobra`, `pants-example-python`, …), no golden file contains `malformed: unknown-directive`, and no test-fixture `go.work` uses `toolchain` or `godebug`. The full masked diff against the pre-milestone baseline was exactly one line: the annotation value on the ad-hoc k8s scan. The reviewer guidance above still stands for any future fixture that adopts these directives.

**Alternatives considered**:
- **Gate US2 behind a flag to preserve byte-identity**: rejected — FR-008 forbids new operator surface, and emitting a knowingly-wrong annotation by default to protect a golden inverts the purpose of the fix.

---

## Non-decisions / explicit deferrals

- **Sub-workspace parallelization of the m774 source-import phase** (9.4s, the largest remaining phase after this milestone): out of scope, candidate follow-up. Requires splitting the root workspace's 12,959-file tree, which m774 documented as needing walker-side inventory support.
- **Reducing `go mod why` invocation count** (38 calls, 68.5s summed wall): untouched. This milestone single-flights only the preflight. Whether the 38 per-workspace `go mod why` calls can share work is a separate question with its own correctness surface.
- **Auto-accepting unknown future `go.work` directives**: rejected by FR-013 — genuinely malformed input must still be reported. Future directives are a one-line vocabulary edit (R5).
- **Bounding the waiter's wait with an independent timeout**: unnecessary — the preflight subprocess is already bounded by the existing `run_bounded` timeout and the shared budget, so the wait is transitively bounded (NFR-001).
