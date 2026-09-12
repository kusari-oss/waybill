# T001 — Pre-change enrichment baseline

Measured 2026-09-12 with the pre-change release binary
(`target/release/waybill`, built 2026-09-11, before any of this
feature's code). Target: a pristine export of this repository
(`git archive HEAD`), 57 MB, 804 components offline / 945 after
enrichment, 769 carrying a PURL.

## The headline: cache state dominates everything else

| run | wall | components | licensed | upstream cache |
|---|---|---|---|---|
| offline (`--offline`) | **0.64s** | 804 | 23 | n/a |
| full, **cold** | **445.81s** | 945 | 586 | first touch |
| full, **warm** | **42.52s** | 945 | 586 | same coords, minutes later |

**The same scan is 10.5× faster on its second run**, with no code
change, no local cache, and no flag difference. deps.dev serves
`cache-control: public, max-age=3600` from a Google edge; a repeat scan
inside that hour is served from the edge rather than from origin.

Per-component enrichment cost:

- cold: ~580 ms/component
- warm: ~55 ms/component

The warm figure matches the ~60 ms median measured for a bare
`GetVersion` in `measurements/`. **The cold figure does not**, and that
gap is the whole story: raw request latency measured against
already-requested coordinates describes the warm path only.

## Consequence: every comparison must state its cache state

A measurement that runs the slow arm cold and the fast arm warm will
show a ~10× improvement that is entirely an artefact. This is not
hypothetical — it happened while taking these numbers. An attempted
decomposition produced:

| run | wall |
|---|---|
| graph disabled (licence path only) | 48.59s |
| licence disabled (graph path only) | 5.91s |
| **sum** | **54.50s** |
| full (cold, measured first) | **445.81s** |

The parts appeared to explain 12% of the whole. They do not fail to
sum because of some interaction between the paths — they fail to sum
because the full run was cold and both partial runs were warm. The
decomposition is void and was not used.

**Protocol for every later measurement in this feature (T017, T029,
T040):**

1. State the cache state of every arm, and make all arms match.
2. Prefer warm-vs-warm. It is reproducible within the hour; cold is not
   reproducible at all without waiting out a 3600-second TTL or finding
   coordinates nobody has requested.
3. Never compare an arm measured today against a number recorded
   yesterday.
4. Record wall time *and* component/licence counts, so a speed-up
   bought by enriching less is visible rather than invisible.

## Baselines for the success criteria

Success criteria are ratios (SC-001 ≥20×, SC-008 ≥5×). The denominator
is the **warm sequential** figure, because it is the reproducible one:

- **Sequential warm baseline: 42.52s** for 769 enrichable components
  (~55 ms/component).
- SC-001 (batch path, ≥20×) → target **≤2.13s**
- SC-008 (concurrent default, ≥5×) → target **≤8.50s**

The cold figure (445.81s) is recorded for context and for the #766
narrative, but must not be used as a denominator: it cannot be
reproduced on demand, so a criterion resting on it cannot be re-verified.

## On the ~17 minutes in issue #766

Still not reproduced, and now better explained. 445.81s (7.4 min) cold
on 769 components extrapolates to roughly 95 minutes for 7,592 — far
*more* than 17. The reported 17 minutes therefore sits between our cold
and warm rates, which is what a partially-warm upstream cache would
produce. The figure is plausible and not precisely reproducible, which
is exactly why the criteria are ratios.

---

# T017 — Concurrency checkpoint (Phase 4 Block A)

Measured 2026-09-12, **warm vs warm**, both arms run back to back after
a discarded warm-up pass, `--no-deps-dev-graph` (SC-001/SC-008 scope).
Sequential arm is the preserved pre-change release binary, so this is a
genuine A/B rather than a comparison against a recorded number.

| arm | wall | licensed |
|---|---|---|
| sequential (pre-change) | 38.33s | 586 |
| concurrent, 8-way sliding window | **10.04s** | 586 |
| concurrent, 24-way (experiment only) | 8.21s | 586 |

**Licence coverage is identical in every arm.** The speed-up is not
bought by enriching less, which is the failure mode a wall-clock-only
comparison would hide.

## Result: 3.63×, and SC-008 (≥5×) does not pass

### Corrected measurement (the numbers above are whole-scan, not the licence path)

The table above subtracts the **offline** run as a base. That is wrong:
`--offline` disables all network, not just deps.dev, so it folds in the
ClearlyDefined enricher. Isolating properly with
`--no-deps-dev-graph --no-clearly-defined`, warm, both arms back to back:

| arm | wall | licensed |
|---|---|---|
| sequential (pre-change) | 34.83s | 584 |
| concurrent, 8-way | **9.60s** | 584 |

**3.63×**, coverage identical.

A subtraction attempted mid-investigation gave 7.96× and was wrong.
ClearlyDefined is a *fallback* enricher — it only queries components
that still lack a licence — so its cost collapses when deps.dev
succeeds (5.14s with deps.dev off, 0.13s with it on). The phases are
not additive and cannot be subtracted from one another. Only a run with
the other paths actually disabled measures this one.

### The implementation is correct; the ceiling is upstream

Instrumented with an in-flight counter: `max_in_flight=8,
requests=709`. The sliding window keeps the full bound saturated.

What upstream does with it:

| | ms/req | speedup |
|---|---|---|
| python, 1 connection, serial | 60.4 | 1× |
| **python, 8 separate connections** | **6.7** | **9.1×** |
| waybill, 8-way over one pooled HTTP/2 connection | 12.9 | 3.8× |
| waybill, 8-way forced HTTP/1.1 + pool 32 | — | *slower* (11.67s vs 10.04s) |
| waybill, 24-way | — | 1.22× over 8-way |

Python's sequential rate (60.4 ms) matches waybill's (48.4 ms), so
network conditions are not the difference. Eight independent
connections reach 9.1×; one multiplexed connection reaches 3.8×;
raising our bound 3× buys 1.22×. That pattern points at a per-connection
server-side limit.

**It is not confirmed.** Forcing HTTP/1.1 with a 32-connection pool
should have tested it directly and came back *slower*, which the
per-connection theory does not explain. Either that experiment did not
actually produce multiple connections, or the mechanism is something
else. Recorded as unexplained rather than asserted.

### Not adopted

24-way is recorded and rejected: FR-003b makes the bound conservative
because deps.dev publishes no rate limit, and tripling load for 1.22%
— 1.22× — is the trade that requirement exists to refuse. Bending the
ceiling to reach a success criterion invented from a different
concurrency model would be worse.


## What the numbers say

**A chunk barrier cost ~16%.** The first implementation used
`chunks(8)`, which waits for the slowest member of each group before
starting the next — cost is max-of-8 per group, not mean-of-8. Measured
3.22×. Replacing it with a sliding window that keeps 8 continuously in
flight gave 3.82×.

**Our ceiling is not the bottleneck.** Raising it from 8 to 24 — 3×
the load — bought only 1.22× (10.04s → 8.21s). Effective concurrency is
~4 at a ceiling of 8 and ~5 at a ceiling of 24. Something server- or
connection-side is capping it; deps.dev serves HTTP/2 and `reqwest`
multiplexes over a single connection, which is the leading suspect but
is **not confirmed**.

The 24-way result is recorded as an experiment and **not adopted**:
FR-003b makes the ceiling deliberately conservative because deps.dev
publishes no rate limit, and tripling load for 1.22× is precisely the
trade that requirement exists to refuse.

## Why the earlier prediction was wrong

`measurements/` recorded 126 req/s at 8-way from a Python harness using
**8 independent connections**, implying ~7.7×. waybill uses one pooled
`reqwest` client. The harness measured a different concurrency model
than the one that shipped, so its figure never described this code.
Same class of error as quoting a component-level number for a
system-level criterion — narrower than presented.

## Consequence for SC-008

SC-008 asks ≥5× for the default concurrent path. The measured ceiling
is ~3.8× at the conservative bound and ~4.7× even at 3× the load. The
criterion was set from the Python harness figure and is not reachable
by this path. It needs revising down to what per-component concurrency
can actually deliver against this service, or the batch path has to
carry the default — which FR-002 explicitly forbids until v3alpha has
been exercised against real corpora.

---

# Batch request-layer ceiling (pre-implementation, T029 will verify)

Measured 2026-09-12 on the **same 709 coordinates** the T017 A/B used,
so the two are directly comparable.

| | wall | requests |
|---|---|---|
| sequential (python, 1 connection) | 42.84s | 709 |
| **batch=100, 8-way (python)** | **0.43s** | **8** |

**100× at the request layer**, 702/709 resolved.

## Projected end-to-end — NOT measured

The request layer is not the scan. A scan also walks, parses and emits,
which costs 0.48s offline and which no enrichment change touches.

| configuration | today (measured) | projected | ratio |
|---|---|---|---|
| licence lookups alone | ~34.3s | 0.43s | ~100× |
| scan, `--no-deps-dev-graph` | 38.33s | ~1.0s | **~37×** |
| scan, default (graph on) | ~41s | ~7s | ~6× |

The projections assume the batch path in waybill reaches the same
request-layer rate the python harness does. **T017 is the reason to
distrust that assumption**: the harness predicted 7.7× for concurrency
and waybill delivered 3.8×, because the harness used eight independent
connections and waybill uses one pooled client. The batch path issues 8
requests rather than 709, so per-connection limits should matter far
less — but that is reasoning, not measurement, and T029 is where it
gets checked.

SC-001 is set at ≥20× against the 38.33s baseline (≤1.92s) — well under
the ~37× projection, so it records a floor rather than a best case.

---

# T029 — Batch path, MEASURED (not projected)

2026-09-12. Every arm is the **same binary and the same code path**,
differing only in strategy, and every number comes from an explicit
`elapsed_ms` emitted by the enrichment phase itself. Nothing here is
inferred by subtracting one whole-scan run from another.

Config: `--no-deps-dev-graph --no-clearly-defined` (deps.dev licences
only). 709 enrichable components. Warm; warm-up run discarded.

| enrichment phase | median | samples | vs sequential |
|---|---|---|---|
| sequential (bound = 1) | 30,140 ms | 3 | 1× |
| concurrent, 8-way | **3,524 ms** | 5 | **8.55×** |
| **batched (`--enrich-batch`)** | **766 ms** | 5 | **39.35×** |

Batched vs concurrent: **4.60×**. Batch detail: `batched=709,
fell_back=0` — every key resolved in bulk, no fallbacks.

| | components | licensed | externalRefs |
|---|---|---|---|
| concurrent | 803 | 584 | 2264 |
| batched | 803 | 584 | 2264 |

**SC-003 PASS** — identical enrichment content. The speed is not bought
by enriching less.

## Criteria

- **SC-001** (batch ≥20×): **PASS at 39.35×**
- **SC-003** (identical coverage): **PASS**
- **SC-008** (default path ≥5×): **PASS at 8.55×**

## Why the phase timer replaced subtraction

Earlier arms were measured by differencing whole-scan runs against a
"floor". That floor is not stable: on this repository it moved between
**5.13s and 23.53s** across runs, because 20 synthetic Go fixtures
named `example.com/waybill-fixture-*` make the Go toolchain attempt
real network resolution and 404. Subtracting an unstable floor produced
three successive wrong speedup figures (3.22×, 3.63×, and a briefly-held
7.96×). The phase timer removes the subtraction entirely.

**SC-008 was lowered to ≥3.5× on the strength of one of those wrong
figures and is restored to ≥5× here**, which the measured 8.55× clears.

## Note on concurrency exceeding its bound

8-way measures 8.55×, slightly superlinear. The `bound = 1` arm has no
pipelining at all, so it pays full round-trip latency per request with
no overlap; 8-way also benefits from connection reuse. The comparison
is honest — both arms are the shipped code — but the bound is not a
theoretical 8× ceiling and should not be read as one.
