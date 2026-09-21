# Post-change measurement (T021)

**Binary**: `waybill` release build from `923-enrich-batch-default` with the
default flipped. Compared against `<scratch>/m923/waybill-prechange`, the
preserved pre-change binary from T002, and against the post-change binary's
own `--no-enrich-batch` opt-out path.

**Headline**: the change is real and reproducible, but **it is ~2×, not 154×.**
The 1302s figure this feature was specified against does not reproduce. See
"The baseline was not reproducible" below — that finding supersedes SC-001 as
written and the spec has been corrected.

## Large repository — 2,291 packages, 2,331 edges

Paired runs, alternating batched / per-component, same session, same network,
`--enrich-no-cache` on both sides so neither path can read a warm disk cache.

| pair | batched (default) | per-component (`--no-enrich-batch`) |
|---|---|---|
| 1 | 8.53s | 16.58s |
| 2 | 8.03s | 16.18s |
| 3 | 6.76s | 15.29s |
| **median** | **8.03s** | **16.18s** |

**Enrichment phase in isolation**, read from the binary's own
`licence enrichment complete elapsed_ms` line rather than inferred from wall
clock — this is the number that attributes the delta to enrichment and nothing
else:

| path | enrichment phase | network lookups | enriched |
|---|---|---|---|
| batched | **6,082 ms** | 2291 | 2271 |
| per-component | **14,465 ms** | 2291 | 2271 |

**2.4× on the phase, 2.0× end-to-end.** Enrichment content is identical on
both sides: 2,331 components, 2,283 licensed, 2,291 PURLs, 2,331 edges — the
same figures the opt-out run produced, which is SC-002 holding at scale.

### Request count is the durable win

| path | HTTP requests for 2,291 packages |
|---|---|
| per-component | 2,291 |
| batched (`BATCH_SIZE = 100`) | **23** |

100× fewer requests. This is the part that does not depend on the day's
latency: the per-component path's cost scales with per-request round-trip
time, so the gap widens on a slow or lossy link and narrows on a fast one.
The 2× above was measured on a fast one, and is therefore closer to the
floor of the benefit than the ceiling.

## Small repository — 109 components (94 maven, 2 bazel, 13 none)

SC-008 asks that small repositories be no slower. Five paired runs, cold
cache:

| | runs | median |
|---|---|---|
| batched (default) | 1.20, 1.03, 0.97, 0.96, 0.94 | **0.97s** |
| per-component | 1.29, 1.39, 1.31, 1.30, 1.24 | **1.30s** |

**SC-008 passes** — batched is 0.33s *faster*, not slower. Two batch requests
replace 109 per-component ones.

### One measurement here was an outlier and is recorded as such

The first small-repo batched run measured **31.36s** and I stated on that
basis that SC-008 failed. It did not reproduce: five subsequent runs landed
between 0.94s and 1.20s. The 31.36s is left in this record rather than
deleted, because a single surprising number that never recurs is exactly what
the repeats exist to catch, and the first conclusion drawn from it was wrong.

## SC-005c — attempts do not grow with repository size

The circuit breaker sets `batch_circuit_open = false` on the first `None` and
every later group short-circuits *before* awaiting its future, so the attempt
count is 1 by construction regardless of chunk count. This is asserted at two
different chunk counts in
`depsdev_source.rs::a_persistent_batch_failure_is_attempted_exactly_once`
rather than measured against the live endpoint, because a live measurement
would depend on deps.dev actually failing.

## The baseline was not reproducible

`baseline.md` records run C — per-component, disk cache on, cold — at
**1302s**, and the spec's SC-001 quotes it as the gap this feature closes.
Re-run here with that exact flag set and a cold disk cache:

```
real 15.23   enrichment phase 13,673 ms   network_lookups 2291   enriched 2271
```

**15.23s, not 1302s.** An 85× discrepancy in the number the feature was
specified against.

What the per-component path actually costs is bounded by its concurrency:
`CONCURRENT_REQUESTS = 8` in a sliding window (`depsdev_source.rs`, the
`JoinSet` loop), so 2,291 lookups at ~50ms each is ~14s — which is what was
measured, three times, on both the pre-change and post-change binaries. 1302s
would require ~570ms per lookup sustained across the whole scan. The most
likely cause is deps.dev throttling that session; nothing in the code path
explains it, and it has not recurred.

The spec's Assumptions section claimed these numbers were "reproducible with
the four flag combinations shown". That assumption is **falsified**, and
SC-001 has been corrected to the measured ratio.

### This does not undo the feature

Two things stand independently of the bad number: enrichment is measurably
~2.4× faster on the phase, and it makes 23 requests where it used to make
2,291. The second is the one worth having — it is a 100× reduction in load
placed on a free public API, it is what makes the scan robust to a slow link,
and it is not a function of the day's network weather. The default flip is
still right. Its *justification* is request count and tail-latency
robustness, not a 154× speedup that was never real.

## Provenance

Every figure above was produced in this session against the branch binary and
the T002 preserved binary, alternating paths within the same minutes. Raw
logs at `<scratch>/m923/lg-batch-{1,2,3}.log`, `lg-perc-{1,2,3}.log`,
`repro-C.log`, `after-optout.log`.
