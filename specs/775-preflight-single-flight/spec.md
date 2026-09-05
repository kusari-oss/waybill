# Feature Specification: Single-Flight Go Preflight + go.work Directive Tolerance

**Feature Branch**: `775-preflight-single-flight`
**Created**: 2026-09-05
**Status**: Draft
**Input**: User description: "m775 — fix the `go list all` preflight cache stampede that defeats m771 US3 sharing (22 invocations instead of 6 on Kubernetes, 198s of redundant subprocess wall time), and fix the strict `go.work` parser that reports valid modern `go.work` files as malformed."

## Clarifications

### Session 2026-09-05

- Q: SC-003 (preflight invocation count) is currently only verifiable by externally shimming the `go` binary — nothing in the product reports how many preflights ran, so there is no automated regression guard. Add observability, keep it test-only, or leave it manual? → A: **Option A** — add a preflight-invocation counter to the classifier's existing summary log line (alongside `analyzed=`, `skipped=`, `elapsed_ms=`) and assert on it in an automated test, making SC-003 CI-verifiable with no external tooling. Rationale: this defect was a silent regression of m771 US3's own stated design intent and survived a full speckit cycle plus merge precisely because nothing observed the invocation count. A permanent counter closes the observability gap that allowed it to ship.
- Q: FR-014 requires both `go.work` parsers to agree on validity, but states the property without a mechanism — and the mechanism changes milestone size materially (add two keywords vs. delete a parser). Minimal keyword addition, shared directive vocabulary, or unify into one parser? → A: **Option B** — extract the set of valid `go.work` directive keywords into a single definition both parsers consult, so adding a future Go directive is a one-place change. Each parser keeps its distinct behavior: the strict one validates and reports malformed reasons (preserving the FR-013 reason vocabulary), the lenient one extracts members and ignores non-`use` directives. Rejected: minimal keyword addition (leaves the parsers structurally independent and free to diverge again on the next directive Go adds); parser unification (loses the strict parser's malformed-reason vocabulary that FR-013 requires preserving).

## Motivation

Continues issue #793. Per `docs/development/perf-methodology.md`, the m775 pre-spec profiling ran Steps 1–3 (per-phase tracing, then subprocess-level instrumentation, then an inclusion/exclusion verification build) against `test-kubernetes` (39 `go.mod` files) on macOS aarch64, warm caches.

### Step 2 — per-phase decomposition (post-m774 baseline, 26.3s default scan)

| Phase | Wall | % |
|---|---:|---:|
| **`go mod why` classifier (m112/m771)** | **13,686ms** | **52%** |
| m774 parallel source-imports | 9,438ms | 36% |
| scan_fs finalization (edge rewrites) | 1,561ms | 6% |
| startup + shared walker (walker itself 187ms) | 910ms | 3% |
| root-select + file-tier walker + emission | ~1,200ms | 5% |

### Step 3 — subprocess-level instrumentation

The classifier's 13.7s did not reconcile with direct measurement: a single `go mod why` against the full 421-module query measures 0.17s, and a shell reproduction of the entire 39-workspace workload completes in 3.0s. Instrumenting the `go` binary with a logging shim during a real scan resolved the discrepancy:

| Subcommand | Calls | Total wall |
|---|---:|---:|
| **`go list all` (reliability preflight)** | **22** | **198.1s** |
| `go mod why` | 38 | 68.5s |
| `go version` | 1 | 0.02s |

Milestone 771 US3 introduced a shared `go list all` preflight cached per `go.work` scope. On Kubernetes the correct count is **6** — one shared preflight for the 34-member `go.work` scope, plus five loose (non-member) workspaces running their own per FR-008 fallback. The observed count is **22**: **16 redundant invocations**, each measuring ~11s under concurrency versus 0.75s in isolation.

### Root cause

The shared-preflight cache at `waybill-cli/src/scan_fs/package_db/golang/mod_why.rs:687` implements: check cache under mutex → **release the mutex** → run the ~11s subprocess → re-acquire → insert. The in-code comment characterizes the resulting race as "very rare — the preflight window is where the race lives" with "a bounded 1-extra-preflight-per-scope worst case."

That characterization does not hold. Milestone 771 US2 — shipped in the same milestone — introduced a bounded thread pool that starts all `available_parallelism()` workers simultaneously (18 on the reference host). Every worker reaches the cache-miss branch at t≈0, before any worker has completed its preflight and populated the cache. The stampede is not a rare race; it is the structurally guaranteed common case. **m771 US2's thread pool defeats m771 US3's cache.**

The double-check occurs *after* the expensive work rather than gating entry to it, which is the defining characteristic of a cache-stampede defect.

### Step 3 verification

A scratch build replacing the check-release-work-reacquire pattern with a per-scope single-flight cell (branch `scratch-m775-preflight-stampede`, commit `d572e95`) was measured against the same fixture and host:

| Metric | Baseline (main) | Single-flight | Delta |
|---|---:|---:|---:|
| Wall time (best of 2) | 26.29s | **18.73s** | **−29%** |
| Classifier phase | 13,686ms | 6,209ms | −55% |
| `go list all` invocations | 22 | 5 | −17 |
| `go list all` total wall | 198.1s | 7.3s | −96% |
| System CPU time | 222s | 86s | −61% |
| CycloneDX output | — | **byte-identical** | 817/817 components |

### Secondary defect — `go.work` directive intolerance

The same profiling surfaced a wire-visible correctness defect. The strict parser at `waybill-cli/src/scan_fs/package_db/golang/gowork.rs:180` accepts only `go`, `use`, and `replace` directives and returns `unknown-directive` for anything else. Kubernetes' `go.work` contains `godebug default=go1.26` — a directive Go has supported since 1.23. Consequently every scan of that repository emits:

```
waybill:go-workspace-mode = malformed: unknown-directive
```

for a completely valid `go.work` file. The `toolchain` directive (Go 1.21+) fails identically. The same emitted SBOM contradicts itself: `waybill:workspaces-detected` correctly enumerates all 38 workspaces, because a **second, lenient** parser at `mod_why.rs:69` ignores unrecognized directives. Two parsers for one file format disagree, and the strict one drives the operator-facing annotation.

This defect is independent of the perf defect (the lenient parser is what feeds scope detection, so `go.work` misparsing is **not** a contributing cause of the stampede), but it lives in the same file family and is independently testable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Single-flight preflight per go.work scope (Priority: P1)

An operator scans a Go monorepo governed by a `go.work` file (reference fixture: Kubernetes, 39 `go.mod` files, 34 of them `go.work` members). The reliability preflight runs exactly once for the shared scope regardless of how many workers begin analyzing member workspaces concurrently. Workers that arrive while a preflight is in flight wait for its result and reuse it rather than launching duplicate subprocesses.

**Why this priority**: This is the milestone's entire wall-time value — a verified 29% reduction on the reference fixture, and the phase it targets is 52% of the scan. It is also a defect fix, not an enhancement: the current behavior contradicts m771 US3's stated design intent ("exactly one preflight per scope").

**Independent Test**: Instrument the `go` binary with a logging shim, scan `test-kubernetes`, and count `go list all` invocations. Pre-fix: 22. Post-fix: at most one per distinct `go.work` scope plus one per loose workspace. Wall-time and byte-identity verified per SC-001 and SC-002.

**Acceptance Scenarios**:

1. **Given** a `go.work` scope with N ≥ 2 member workspaces and a worker pool of W ≥ 2 workers, **When** the operator runs a scan, **Then** exactly one `go list all` invocation occurs for that scope, and every member workspace observes the same preflight outcome.
2. **Given** the preflight for a scope fails (unresolvable packages), **When** other workers for that same scope request the preflight result, **Then** all of them observe the identical failure outcome and are skipped per m771 FR-007, with no duplicate subprocess spawned to rediscover the failure.
3. **Given** two independent `go.work` scopes in one scan, **When** workers process both concurrently, **Then** the two preflights run concurrently with respect to each other — one scope's preflight never blocks an unrelated scope's.
4. **Given** a workspace that is not a member of any `go.work` scope, **When** the operator runs a scan, **Then** it runs its own preflight exactly once, unchanged from the m771 FR-008 fallback behavior.
5. **Given** any fixture in the existing regression suite, **When** the operator runs a scan before and after this change, **Then** the emitted CycloneDX, SPDX 2.3, and SPDX 3 documents are byte-identical modulo document-identity fields.
6. **Given** a multi-workspace Go repository governed by a `go.work` file, **When** the operator runs a scan with informational logging enabled, **Then** the classifier's summary log line reports the number of reliability-preflight subprocess invocations, and that number equals one per distinct scope plus one per loose workspace.

---

### User Story 2 - Tolerate modern go.work directives (Priority: P2)

An operator scans a Go monorepo whose `go.work` declares `godebug` (Go 1.23+) or `toolchain` (Go 1.21+). The emitted workspace-mode annotation reports the workspace as detected, with its member count, rather than reporting the file as malformed.

**Why this priority**: Independently valuable and independently testable, but it changes an annotation value rather than wall time, and affects operator-facing metadata rather than component or relationship data. P2 because US1 carries the milestone's measured value; this story can ship in the same milestone or be deferred without blocking US1.

**Independent Test**: Scan a fixture whose `go.work` contains `godebug default=go1.26` and assert the emitted `waybill:go-workspace-mode` annotation reports detected-with-member-count rather than `malformed: unknown-directive`. Verifiable without running US1.

**Acceptance Scenarios**:

1. **Given** a `go.work` containing a `godebug` directive, **When** the operator scans the repository, **Then** the workspace-mode annotation reports the workspace as detected with the correct member count.
2. **Given** a `go.work` containing a `toolchain` directive, **When** the operator scans the repository, **Then** the annotation likewise reports detected with the correct member count.
3. **Given** a `go.work` containing a genuinely malformed construct (an unparseable `use` path, or a directive that is not part of the `go.work` format), **When** the operator scans, **Then** the annotation still reports the appropriate malformed reason — tolerance is scoped to known-valid directives and does not degrade into accepting anything.
4. **Given** a `go.work` with only the directives already supported before this milestone, **When** the operator scans, **Then** the annotation value is unchanged from pre-milestone output.
5. **Given** any repository scanned before and after this change, **When** the two emitted SBOMs are compared, **Then** the only permitted difference is the workspace-mode annotation value on repositories whose `go.work` previously misparsed.
6. **Given** the shared directive vocabulary is extended with a hypothetical future directive, **When** a `go.work` containing that directive is parsed by either parser, **Then** both treat it as valid — no second, parser-specific edit is required to keep them in agreement.

---

### Edge Cases

- **Preflight panics or the subprocess is killed**: the waiting workers must not deadlock on the single-flight primitive, and must not observe a permanently-empty result slot. Failure must propagate to every waiter for that scope.
- **Budget exhaustion mid-preflight**: when the shared time budget expires while a preflight is in flight, waiting workers observe the resulting skip outcome rather than each launching their own doomed attempt.
- **Single-workspace scan**: no worker pool is created, so no contention exists; behavior must match pre-milestone exactly.
- **Zero `go.work` files**: every workspace is loose and runs its own preflight per FR-008 — this milestone changes nothing on that path.
- **Nested `go.work` files**: a workspace resolving to the nearest enclosing `go.work` continues to do so; single-flighting is keyed on the resolved scope, so nesting behavior is unchanged.
- **Repeated scans in one process**: the cache is per-scan; a second scan re-runs the preflight rather than reusing a stale result from the prior scan.
- **`go.work` with `godebug` but no `use` block**: parses as detected with zero members — the existing zero-member case, not a new one.
- **Duplicate directives** (two `godebug` lines): must not be treated as malformed; Go permits repeated `godebug` lines.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST invoke the `go list all` reliability preflight at most once per distinct `go.work` scope per scan, regardless of worker-pool size or arrival timing.
- **FR-002**: Workers requesting a preflight result for a scope whose preflight is already in flight MUST wait for that invocation and reuse its outcome, rather than launching a duplicate subprocess.
- **FR-003**: Preflights for distinct scopes MUST be able to proceed concurrently; the mechanism guaranteeing single-flight for one scope MUST NOT serialize preflights for unrelated scopes.
- **FR-004**: The system MUST NOT hold the shared-cache lock across a subprocess spawn, so that cache reads for already-resolved scopes remain non-blocking while an unrelated preflight is in flight.
- **FR-005**: A failed preflight outcome MUST be propagated identically to every worker in that scope, preserving the m771 FR-007 skip semantics unchanged.
- **FR-006**: Workspaces not governed by any `go.work` scope MUST continue to run their own preflight, preserving the m771 FR-008 fallback path unchanged.
- **FR-007**: The system MUST preserve byte-identity of every emitted SBOM across all three formats for every existing fixture, with the sole permitted exception of the US2 workspace-mode annotation value on repositories whose `go.work` previously misparsed.
- **FR-008**: The system MUST NOT introduce new operator-facing flags or environment variables.
- **FR-009**: The system MUST NOT introduce new dependencies at the workspace lockfile level.
- **FR-010**: The system MUST preserve the existing per-workspace log-line shapes and firing conditions emitted by the classifier. Every field currently present in the m771 FR-013 summary line MUST retain its name, meaning, and firing conditions; per Clarifications Q1 the summary line MAY gain the additional field required by FR-015, but MUST NOT rename, remove, or change the semantics of any existing field.
- **FR-011**: The `go.work` parser that drives the operator-facing workspace-mode annotation MUST accept the `godebug` directive without reporting the file as malformed.
- **FR-012**: That same parser MUST accept the `toolchain` directive without reporting the file as malformed.
- **FR-013**: That parser MUST continue to report genuinely malformed input with the appropriate existing reason code; directive tolerance MUST be limited to directives the `go.work` format actually defines, and MUST NOT degrade into accepting arbitrary unrecognized input.
- **FR-014**: Both `go.work` parsers in the codebase MUST agree on whether a given file is valid, so that a single scan cannot emit an annotation claiming the file is malformed while simultaneously enumerating workspaces parsed from it. Per Clarifications Q2 this MUST be achieved by both parsers consulting a single shared definition of the valid `go.work` directive vocabulary, such that recognizing a newly-added Go directive is a one-place change. Each parser retains its own distinct behavior beyond that shared vocabulary: the strict parser validates and reports malformed reasons; the lenient parser extracts member paths and ignores directives it does not need.
- **FR-015**: The classifier's summary log line MUST report the number of reliability-preflight subprocess invocations performed during the scan, so an operator (and an automated test) can observe the SC-003 invariant without external process instrumentation. The count MUST reflect actual subprocess spawns, not cache reads or waiter wake-ups.

### Non-Functional Requirements

- **NFR-001**: No worker may deadlock or spin waiting for a preflight result. A worker's wait MUST terminate when the in-flight preflight completes, fails, or exhausts the shared budget.
- **NFR-002**: A panic in a preflight MUST NOT leave the single-flight mechanism in a state that permanently blocks subsequent waiters for that scope within the same scan.

### Key Entities

- **Preflight single-flight cell**: a per-scope coordination point holding either "not yet attempted" or a completed outcome. The first worker to claim a scope performs the work while holding the cell; concurrent workers for that scope block on it and read the memoized outcome.
- **Shared preflight cache**: the existing per-scan, scope-keyed record of completed preflight outcomes. Continues to serve fast non-blocking reads for scopes whose preflight has already finished.
- **`go.work` scope**: unchanged from m771 — a `go.work` file's directory plus its enumerated member workspaces.
- **`go.work` directive vocabulary**: the single shared definition of which directive keywords the `go.work` format defines (`go`, `toolchain`, `godebug`, `use`, `replace`). Both parsers consult it; extending it for a future Go release is a one-place change. Introduced per Clarifications Q2 as the mechanism satisfying FR-014.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the Kubernetes reference fixture, a default offline scan completes in **≤ 21 seconds** on macOS aarch64 with warm caches, down from a 26.3s baseline. (Scratch verification measured 18.73s; the criterion carries headroom for host variance.)
- **SC-002**: Emitted CycloneDX, SPDX 2.3, and SPDX 3 documents are byte-identical before and after the change across every existing fixture, modulo document-identity fields and the US2 annotation value.
- **SC-003**: On the Kubernetes reference fixture, the number of reliability-preflight subprocess invocations is **at most one per distinct `go.work` scope plus one per loose workspace** — 6 or fewer, down from 22. Per Clarifications Q1 this count is reported by the classifier's summary log line (FR-015) and asserted by an automated test, so the criterion is verifiable in CI rather than by one-off external instrumentation.
- **SC-004**: Total wall time spent in reliability-preflight subprocesses on the reference fixture drops by **at least 90%** (198.1s baseline).
- **SC-005**: Two independent scans of the same tree produce byte-identical output, modulo document-identity fields.
- **SC-006**: Zero new dependencies at the workspace lockfile level.
- **SC-007**: The project's mandatory pre-PR verification passes: zero lint errors and every test suite reporting all tests passed with none failed.
- **SC-008**: Scanning a repository whose `go.work` declares `godebug` or `toolchain` reports the workspace as detected with the correct member count, rather than malformed.
- **SC-009**: On a single-workspace fixture, wall time is within ±3% of the pre-milestone measurement — the single-flight mechanism adds no measurable cost where there is no contention.

## Assumptions

- Reference hardware is macOS aarch64 or Linux x86_64 with 8 or more logical cores, matching the m669 benchmark reference class. The stampede's magnitude scales with worker-pool size, so lower-core hosts see a proportionally smaller (but still positive) improvement.
- The reference fixture is `kusari-sandbox/test-kubernetes` at the revision used for m774's measurements: 39 `go.mod` files, a 34-member `go.work` scope, and 5 loose workspaces.
- The correct preflight count on that fixture is 6. The scratch verification observed 5, which is within expectation — the arithmetic distinguishing members from loose workspaces depends on how the root `.` entry resolves, and the criterion is stated as an upper bound (SC-003) rather than an exact count.
- `go mod why` invocation count and per-invocation cost are unchanged by this milestone. Only the preflight is single-flighted. Aggregate `go mod why` wall time summed across concurrent processes may shift as CPU contention changes; this is not a regression signal, because summed wall across concurrent processes is not a cost metric. Scan wall time is the metric of record.
- The perf defect and the parser defect are causally independent. The lenient parser drives scope detection, so `go.work` misparsing does not contribute to the stampede, and fixing the parser alone would deliver no wall-time improvement.
- The `go.work` directive set treated as valid is the set Go itself defines: `go`, `toolchain`, `godebug`, `use`, and `replace`. Per Clarifications Q2 this set lives in one shared definition consulted by both parsers, so a directive added by a future Go release is accommodated by editing that definition alone. This milestone does not attempt to auto-accept unknown directives, because FR-013 requires that genuinely malformed input still be reported — future-proofing is a one-line edit, not permissiveness.
- The scratch verification build on `scratch-m775-preflight-stampede` (commit `d572e95`) is reference-only and is not the deliverable. The milestone re-implements the fix cleanly; the branch is dropped once the milestone lands.
- The m774 source-import phase (9.4s) becomes the largest remaining phase after this milestone. Attacking it requires sub-workspace parallelization to break the root workspace's ~10.9s serial floor — explicitly out of scope here, and a candidate follow-up milestone.
