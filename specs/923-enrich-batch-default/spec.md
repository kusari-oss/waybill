# Feature Specification: Enrichment is fast by default

**Feature Branch**: `923-enrich-batch-default`
**Created**: 2026-09-20
**Status**: Draft
**Input**: User description: "927"

Addresses [#927](https://github.com/kusari-oss/waybill/issues/927).

## Context

Enrichment is the slowest thing waybill does by two orders of magnitude, and
the fast path already exists but is off by default.

Measured on a public polyglot repository — ~6,200 files, Go plus two yarn
workspaces, 2,291 packages:

| run | network | deps.dev | wall clock | licensed components |
|---|---|---|---|---|
| A | off | — | 3.0s | 0 |
| B | **on**, deps.dev disabled | off | **2.2s** | 0 |
| C | on, **default (per-component)** | on | **1302s** | 2283 |
| D | on, **batch**, cold cache | on | **8.5s** | 2283 |

Run B is what makes the attribution safe: it holds the network on and disables
only deps.dev, so the delta B→C is deps.dev alone — **~1300 of the 1302
seconds**. Everything else waybill does on that repository takes about two
seconds.

The outputs are equivalent, not merely similar:

```
serial (C) vs batch (D)
  purls:    2291 vs 2291   differing = 0
  licenses: 2283 vs 2283   differing = 0
  edges:    2331 vs 2331
```

**154× faster for the same document.**

### Why it is off today, and why that reason is not what it appears

The obvious reading — "it was opt-in while it bedded in, and it has now bedded
in" — is not what the code records. The flag's own documentation says:

> `GetVersionBatch` lives on deps.dev's `v3alpha` surface, which its own
> documentation says "may change in incompatible ways from time to time". A
> batch failure falls back to the per-component path, so an upstream change
> degrades speed rather than breaking the scan.

That is a **standing property of the upstream API**, not a temporary caution.
Waiting does not make `v3alpha` stable. So this feature is not "the trial
period ended" — it is a deliberate decision that the mitigation is good enough
to carry the default.

The mitigation is real and already built (milestone 839): a batch failure
falls back to the per-component path, the fallback is counted, and it is
reported as the `BatchUnavailable` degradation mode whose own documentation
states the invariant — *enrichment content is unaffected; this costs speed,
not coverage*.

### What is actually at stake

An operator who benchmarks waybill against other tools without knowing the
flag concludes it is ~150× slower than its nearest comparable. On the same
repository, with the fast path on, waybill finds more dependency edges (2331
vs 1214), more unique packages, and far more licences — in less wall clock.
The default is the only thing hiding that.

## Clarifications

### Session 2026-09-20

- Q: What happens after repeated batch failures in one scan? → A: **Circuit-break after the first failure** — stop attempting the batched path for the remainder of the scan and fall back for everything, with a log line saying so. The risk this feature accepts is exactly "upstream changes shape and every batch call starts failing"; in that world a per-chunk retry pays the batch penalty once per chunk (~23 times on the reference repository) before arriving at the slow path anyway, whereas breaking after the first failure costs one wasted round-trip and lands at roughly today's default. Chosen over the status-quo per-chunk retry, which handles a transient single-chunk blip marginally better and a persistent outage much worse, and over an N-failure threshold, which buys that transient tolerance at the cost of a tuning constant nobody would know how to pick. **Additionally**: the scan must log when the circuit breaks, and there must be a standing check that tells us when the batch endpoint leaves `v3alpha`, so the risk this feature accepts is re-examined when its premise changes rather than being inherited indefinitely.
- Q: When does the circuit break take effect, given that batch chunks run 8-at-a-time? → A: **Withdrawn — the premise was false, and the strict guarantee stands.** The question assumed batch chunks execute concurrently because the code groups them by `CONCURRENT_REQUESTS`. They do not: the async blocks are collected into a `Vec` and awaited one at a time, with no `join_all`, `FuturesUnordered` or `spawn` anywhere in the module. Timing corroborates independently — 23 chunks and ~6.3s of enrichment is ~0.27s per chunk, one round-trip each; eight-way concurrency would predict ~0.8s. Execution is therefore sequential, a failure is observed before the next request is issued, and breaking after the first failure costs **exactly one** wasted attempt. The weaker "at most one concurrency group" wording was written to accommodate a concurrency that does not exist and has been reverted. Recorded rather than quietly fixed because a reader comparing this spec to the code would otherwise find a doc comment (`depsdev_source.rs:187`) still asserting that concurrency is bounded at `CONCURRENT_REQUESTS`, which is true only in the sense that 1 ≤ 8.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A scan enriches at batch speed without being asked (Priority: P1)

Someone runs a scan with no enrichment flags and gets licence and provenance
data in seconds rather than minutes.

**Why this priority**: it is the feature. Everything else protects it.

**Independent Test**: scan a repository with ~2,000 packages, no flags. Wall
clock in the seconds, licence coverage unchanged from today's slow default.

**Acceptance Scenarios**:

1. **Given** a repository with thousands of packages, **When** it is scanned
   with no enrichment flags, **Then** enrichment completes in seconds and the
   emitted document is equivalent to the one the per-component path produces.
2. **Given** the same repository, **When** the two paths' outputs are
   compared, **Then** package identities, licences and edges are identical.
3. **Given** `--offline` or deps.dev disabled, **When** a scan runs, **Then**
   behaviour is unchanged — this feature does not introduce network calls
   where there were none.

---

### User Story 2 - The slow path stays reachable and exercised (Priority: P2)

An operator who needs the per-component path can still select it, and it does
not rot.

**Why this priority**: the per-component path is the fallback that makes the
default safe. A fallback nobody can select is a fallback nobody tests, and it
is the thing standing between an upstream API change and a broken scan.

**Independent Test**: select the per-component path explicitly; enrichment
content matches the batch path.

**Acceptance Scenarios**:

1. **Given** an operator who opts out, **When** they scan, **Then** the
   per-component path runs and produces equivalent content.
2. **Given** a script passing today's opt-in flag, **When** it runs after this
   change, **Then** it still works rather than erroring on an unknown flag.
3. **Given** the test suite, **When** it runs, **Then** both paths are
   exercised, so neither can silently break.

---

### User Story 3 - An operator can tell when the fast path stopped working (Priority: P3)

When the batch endpoint fails and work falls back, the scan says so.

**Why this priority**: the whole safety argument is "a failure costs speed,
not coverage". That is only true if the operator can distinguish "slow because
upstream changed" from "slow for no reason" — otherwise the failure mode is a
mysterious 20-minute scan.

**Independent Test**: with the batch endpoint failing, a scan completes with
full content and reports the degradation.

**Acceptance Scenarios**:

1. **Given** a failing batch endpoint, **When** a scan runs, **Then** it
   completes with the same enrichment content as a successful run.
2. **Given** that scan, **When** its document is read, **Then** it records
   that the fast path was unavailable and work fell back.
3. **Given** a batch failure, **When** the scan continues, **Then** it stops
   attempting the batched path for the rest of that scan rather than retrying
   per chunk, and logs that it has done so.
4. **Given** a scan whose circuit broke, **When** its wall clock is compared
   to a scan with enrichment forced down the per-component path, **Then** the
   difference is a single wasted round-trip, not one per chunk.

---

### Edge Cases

- A repository small enough that batching saves nothing — the default must not
  make small scans slower.
- `--offline`, deps.dev disabled, or enrichment sources restricted: the
  default must be inert.
- A cached scan, where most components need no fetch at all.
- The batch endpoint changing shape incompatibly — the standing `v3alpha` risk
  this feature accepts.
- A partial batch response: some components resolved, some not.
- A batch failure on one chunk while earlier chunks succeeded — the successes
  are kept; only later chunks are skipped.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Enrichment MUST use the batched path by default.
- **FR-002**: The emitted document MUST be equivalent whichever path ran —
  identical package identities, licence values and dependency edges. This is
  the property that makes the default safe, and it MUST be asserted rather
  than assumed.
- **FR-003**: An operator MUST be able to select the per-component path
  explicitly.
- **FR-004**: The existing opt-in flag MUST continue to be accepted, so
  scripts that pass it today do not break.
- **FR-005**: Both paths MUST be exercised by the test suite. The
  per-component path is the fallback that makes the default defensible; an
  untested fallback is not one.
- **FR-006**: A batch failure MUST fall back to the per-component path with no
  loss of enrichment content.
- **FR-007**: A scan whose fast path failed MUST record that fact in its
  output, so a slower-than-expected scan is explicable rather than mysterious.
- **FR-007a**: After a batch failure, the scan MUST stop attempting the
  batched path for its remainder. A persistent upstream failure MUST cost
  exactly one wasted attempt, not one per chunk.

  This is achievable because batch requests are issued sequentially — a
  failure is observed before the next request goes out. Should that ever
  change, this requirement becomes "at most one concurrency group" and the
  change MUST be deliberate rather than a silent weakening.
- **FR-007b**: The scan MUST emit a log line when it stops using the batched
  path, naming the failure that caused it. The document-scope record (FR-007)
  tells a consumer afterwards; the log tells the operator while it is
  happening, and the two audiences are different.
- **FR-007c**: There MUST be a standing check that detects when the batch
  endpoint leaves `v3alpha`. This feature accepts a risk whose entire
  justification is that the upstream surface is unstable; when that stops
  being true the decision deserves re-examination, and nobody will think to
  look unless something tells them.
- **FR-008**: The change MUST be inert when enrichment is disabled —
  `--offline`, deps.dev off, or sources restricted.
- **FR-009**: On a **successful** scan, the change MUST NOT alter emitted
  content. Only speed changes.
- **FR-009a**: On a scan whose batch path **failed**, the emitted document MAY
  differ from before this change in exactly one way: it carries the
  degradation record of FR-007. That record was previously unreachable on a
  default scan, because the batched path was never used. This is the sole
  content difference the feature introduces, it is confined to the failure
  path, and it is strictly more information than before.
- **FR-010**: Documentation MUST record that the batched path runs against an
  upstream surface its publisher describes as liable to change incompatibly,
  and that the fallback is what makes that acceptable. The rationale must
  survive the decision, so a future reader does not mistake the default for an
  oversight.

### Key Entities

- **Enrichment path**: how component metadata is fetched — batched, or one
  request per component. Same data, different request shape.
- **Fallback**: the per-component path, used when the batched one fails.
  Costs time, not content.
- **Degradation record**: the document-scope statement that the fast path was
  unavailable.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a repository of ~2,000 packages, a default scan's enrichment
  is **at least 2x faster** than the opt-out path and issues **at least 50x
  fewer requests** — measured at 2.4x on the enrichment phase (6,082ms against
  14,465ms) and 23 requests against 2,291. **Corrected after T021.** This
  criterion previously read "1302s against 8.5s"; the 1302s figure did not
  reproduce (re-measured at 15.23s with its own flags and a cold cache) and
  was almost certainly deps.dev throttling that session. See
  `measurements/after.md`. The request-count bound is the durable half — it
  does not depend on the day's latency.
- **SC-002**: Default-path and opt-out-path documents are equivalent: zero
  differing package identities, zero differing licence values, identical edge
  count.
- **SC-003**: Licence coverage is unchanged from today's default — measured at
  2283 of 2291 packages on the reference repository.
- **SC-004**: A scan with enrichment disabled produces byte-identical output
  to before this change.
- **SC-005**: With the batch endpoint failing, a scan still produces full
  enrichment content and records the degradation.
- **SC-005a**: With the batch endpoint failing persistently, the scan makes
  **exactly one** batch attempt, not one per chunk — measurable as attempt
  count. On a repository whose work spans ~23 chunks, that is the difference
  between ~23 failed attempts and one.
- **SC-005c**: The bound holds regardless of how many chunks the work spans:
  attempts do not grow with repository size once the circuit has broken.
- **SC-005b**: An operator watching a scan whose fast path failed sees a log
  line saying so.
- **SC-006**: Both paths are covered by tests that fail if either breaks.
- **SC-007**: A script passing the current opt-in flag continues to work.
- **SC-008**: Small repositories are no slower than before.

## Assumptions

- ~~The measurements in Context are reproducible with the four flag
  combinations shown, and run B is the one that makes the attribution valid.
  They are recorded in the issue rather than re-derived here.~~
  **FALSIFIED by T021.** Run B's attribution method is sound and run D (8.5s)
  reproduces exactly. Run C (1302s) does not: re-run with its own flags on a
  cold cache it measures 15.23s. The per-component path is bounded by
  `CONCURRENT_REQUESTS = 8`, which puts its floor near 14s, not 1302s. Treat
  any single unreplicated timing of an external service as provisional --
  this is the failure mode `docs/development/perf-methodology.md` and the
  CLAUDE.md "measure external behaviour" rule both warn about, reached here
  by trusting one observation instead of repeating it.
- Today's opt-in flag becomes a no-op rather than an error, and a new opt-out
  selects the per-component path. Removing the old flag would break scripts
  for no benefit.
- Batching applies to deps.dev enrichment only. Other enrichment sources are
  unaffected.
- No size threshold below which the default reverts. One batched request for a
  handful of components is not slower than a handful of individual ones, and a
  threshold is a tuning parameter nobody would know to measure.
- Caching behaviour is unchanged; the batched path populates and reads the
  same cache.
- The `v3alpha` risk is accepted, not eliminated. This feature's position is
  that a fallback which costs speed rather than coverage is an adequate
  mitigation — and that position is recorded so it can be revisited if the
  fallback ever stops holding.
