# ClearlyDefined bulk endpoint — measurements for #933

Probed against `https://api.clearlydefined.io` on 2026-10-02. Every number
below is a single session's observation of a service whose variance is the
reason #930 exists. Re-run the probes before quoting any of them as a
baseline.

| script | question |
|---|---|
| `probe_semantics.py` | How do `GET /definitions/{coord}` and `POST /definitions` answer the same coordinates? |
| `probe_batches.py <cdx>...` | How do batches of 100 real coordinates behave cold, then warm? |
| `probe_concurrency.py <cdx>...` | How does wall clock scale with concurrent warm batches? |
| `abba.sh <repo>` | End to end, old binary vs new, alternating to share server warmth. |

## Semantics

- **The response is keyed by exactly the strings sent**, and every coordinate
  comes back. One CD has no record of comes back with a sparse body and no
  `licensed.declared`, the same shape `GET` returns for it (200, not 404).
- **Bulk keys are matched literally; the `GET` path is URL-decoded.** For
  `github.com/yudai/pp@v2.0.1+incompatible`, bulk with a raw `+` answers
  `MIT`; with `%2B` it answers nothing. `@types` hits and `%40types` misses.
  The one escape bulk needs is `/` inside a segment (a Go module prefix),
  which CD stores as `%2F` (`%2f` also matches). This is
  `CdCoord::bulk_key`, distinct from `url_path`.
- **npm scopes keep their `@`.** `npm/npmjs/@angular/core/16.0.0` resolves;
  `npm/npmjs/angular/core/16.0.0` has no `described` block at all. Fixed
  separately in #1092.
- **A malformed member is dropped, not fatal.** A batch containing
  `npm/npmjs/express` (four segments) returned 200 with the other member.
- **Cold `GET`s can stall.** `@types/node` and two Go coordinates each took
  11–30 s or timed out on the first request and answered in ~100 ms on the
  second. With the 5 s per-request timeout, a cold coordinate fails, and
  before #933 that failure was written to the 7-day disk cache as a miss.

## Batches of 100, 640 real coordinates (`probe_batches.py`)

Coordinates from the kubernetes, express and flask SBOMs.

| pass | batches 200 | failures | wall clock | declared |
|---|---|---|---|---|
| 1 (cold) | 5 of 7 | 120 s client timeout; 502 after 112 s | 234.5 s | 346 |
| 2 (minutes later) | 6 of 7 | 502 after 31 s, on a batch that was 0.40 s in pass 1 | 33.7 s | 405 |

Both batches that failed in pass 1 answered in 0.50 s and 0.71 s in pass 2.
Failures are per batch and transient, and they hit warm content as well as
cold. A failed batch costs whatever the client timeout is.

## Concurrency, warm (`probe_concurrency.py`)

| workers | wall clock for 7 batches |
|---|---|
| 1 | 122.6 s (one batch stalled to the 120 s timeout; the rest 0.31–0.67 s) |
| 2 | 1.67 s |
| 4 | 0.97 s |
| 7 | 0.69 s |

## End to end: grafana, 4,126 coordinates (`abba.sh`)

`--enrich-sources clearly-defined`, `WAYBILL_CLEARLY_DEFINED_NO_CACHE=1`.
Times are the ClearlyDefined phase only, from the log's start and end lines.
The order alternates so server-side warming is shared.

| order | binary | CD phase | components enriched |
|---|---|---|---|
| 1 | bulk | 93.7 s | 2,221 |
| 2 | per-coordinate (main) | 549.5 s | 2,171 |
| 3 | per-coordinate (main) | 21.6 s | 2,237 |
| 4 | bulk | 1.9 s | 2,247 |

In run 1, 40 of 42 batches answered; the other 2 failed twice, and their
200 coordinates went per-coordinate (29 s of the 93.7 s).

Runs 3 and 4 differed on 11 Go `+incompatible` versions that only the
per-coordinate path resolved. That is the literal-key finding above. After
`bulk_key`, a further warm pair gave:

| binary | CD phase | enriched | only in this run | same component, different licence |
|---|---|---|---|---|
| per-coordinate (main) | 16.2 s | 2,237 | 0 | 0 |
| bulk | 31.3 s | 2,258 | 21 | 0 |

The bulk run is a strict superset. Its wall clock is slower in this pair: a
stalled batch costs up to the 30 s bulk timeout before its retry. That is
the tail this design accepts in exchange for the cold-case difference.
