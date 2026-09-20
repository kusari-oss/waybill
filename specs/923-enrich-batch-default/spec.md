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
3. **Given** repeated batch failures within one scan, **When** it runs,
   **Then** the scan does not spend the fallback cost more times than
   necessary.

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
- **FR-008**: The change MUST be inert when enrichment is disabled —
  `--offline`, deps.dev off, or sources restricted.
- **FR-009**: The change MUST NOT alter emitted content for any existing
  scan. Only speed changes, plus the degradation signal on failure.
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
  completes in **seconds rather than minutes** — the measured gap today is
  1302s against 8.5s.
- **SC-002**: Default-path and opt-out-path documents are equivalent: zero
  differing package identities, zero differing licence values, identical edge
  count.
- **SC-003**: Licence coverage is unchanged from today's default — measured at
  2283 of 2291 packages on the reference repository.
- **SC-004**: A scan with enrichment disabled produces byte-identical output
  to before this change.
- **SC-005**: With the batch endpoint failing, a scan still produces full
  enrichment content and records the degradation.
- **SC-006**: Both paths are covered by tests that fail if either breaks.
- **SC-007**: A script passing the current opt-in flag continues to work.
- **SC-008**: Small repositories are no slower than before.

## Assumptions

- The measurements in Context are reproducible with the four flag
  combinations shown, and run B is the one that makes the attribution valid.
  They are recorded in the issue rather than re-derived here.
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
