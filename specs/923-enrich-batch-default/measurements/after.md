# Post-change measurement (T021)

**Binary**: `waybill` release build from `923-enrich-batch-default` with the
default flipped. Compared against `<scratch>/m923/waybill-prechange`, the
preserved pre-change binary from T002, and against the post-change binary's
own `--no-enrich-batch` opt-out path.

**Headline**: the change is real and reproducible, but **it is ~2x, not 154x.**
The 1302s figure this feature was specified against is real — and is almost
entirely **ClearlyDefined**, not deps.dev. deps.dev has cost ~16.6s on this
repository throughout. See "The baseline was real. The attribution was wrong
— twice." below; SC-001 has been corrected and #930 filed for the real cost.

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

## The baseline was real. The attribution was wrong — twice.

`baseline.md` records run C at **1302s** and this feature was specified
against it as the cost of per-component deps.dev enrichment. That run
happened, and its timing was correct. **What it measured was not deps.dev.**

The run's own log carries per-source timings:

```
19:01:29.868  deps.dev licence enrichment complete  elapsed_ms=16550  network_lookups=2291
19:01:29.870  ClearlyDefined enrichment starting    unique_coords=2291  concurrency=8
19:22:53.003  ClearlyDefined enriched components with concluded licenses  count=1203
real 1302.31
```

Re-run here with **both** enrichment caches cold, against the preserved
pre-change binary:

| | original (19:01) | repro (01:57) |
|---|---|---|
| total wall clock | 1302.31s | 714.50s |
| **deps.dev** | **16.55s** | **16.65s** |
| **ClearlyDefined** | **1283.13s** | **695.00s** |
| scan proper (residual) | 2.63s | 2.85s |
| deps.dev share of total | 1.3% | 2.3% |
| **ClearlyDefined share** | **98.5%** | **97.3%** |

deps.dev reproduces to within **0.6%**. It has cost ~16.6s on this repository
all along. ClearlyDefined is the 1300 seconds, and it varies roughly 2x run to
run (695s–1283s) because it depends on a different upstream's availability.

### The first error: attributing by flag toggle instead of reading the log

The baseline argued that run B (`--no-deps-dev`, 2.2s) isolated deps.dev, so
the B->C delta was deps.dev alone. It does not. `--no-deps-dev` does not
disable ClearlyDefined — `scan_cmd.rs:2361` reads
`clearly_defined: !args.no_clearly_defined`. Run B was fast because it ran
*after* the cold run had already populated the 47 MB ClearlyDefined disk
cache.

The per-source timings needed to catch this were in the log being summarised
at the time. No inference was required — only reading it.

This is exactly the failure `docs/development/perf-methodology.md` exists to
prevent, and `baseline.md` contains a paragraph claiming to have avoided it.

### The second error: re-measuring with one of the two caches still warm

The first version of this section claimed the 1302s "does not reproduce",
citing a 15.23s re-run, and offered deps.dev throttling as the likely cause.
That re-run cleared `~/.cache/waybill/deps-dev` and left
`~/.cache/waybill/clearly-defined` warm. 15.23s was a warm-ClearlyDefined
number compared against a cold-ClearlyDefined one.

**The throttling explanation was invented to fit a gap that the experiment
created.** There was never evidence for it. It is withdrawn.

### What this does to the feature

The measured effect of the flip is unchanged and stands on its own evidence:
deps.dev enrichment 14,465ms -> 6,082ms, and 2,291 requests -> 23. Every
number in the sections above was measured with paired runs and is unaffected.

What changes is the **premise**. #927 was opened to fix a 1302-second scan.
Batched deps.dev enrichment does not fix that scan: it removes ~8 seconds
from a runtime whose other ~1280 seconds are ClearlyDefined. The feature is
still worth having — a 2.4x cut and 100x fewer requests against a free public
API — but it should never have been sold as closing the 1302s gap, and the
real cost belongs in its own issue.

## Provenance

Every figure above was produced in this session against the branch binary and
the T002 preserved binary, alternating paths within the same minutes. Raw
logs at `<scratch>/m923/lg-batch-{1,2,3}.log`, `lg-perc-{1,2,3}.log`,
`repro-C.log`, `after-optout.log`.
