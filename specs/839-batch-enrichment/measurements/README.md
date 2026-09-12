# Measurements: deps.dev enrichment strategies

Taken 2026-09-12 against the live deps.dev API. Every number here was
observed. Nothing in this directory is a prediction.

## Why this exists

An earlier draft of this feature set batch size, success criteria and a
performance baseline from arithmetic rather than observation — one
measured figure (17 min / 7,592 components) extrapolated through
assumptions about concurrency scaling and batch cost that had no evidence
behind them, and presented in a table headed "so the choice isn't a
guess". Two of the three resulting decisions were wrong. These scripts
replace that.

## Reproducing

```bash
python3 specs/839-batch-enrichment/measurements/measure.py      # latency by mode
python3 specs/839-batch-enrichment/measurements/pagecheck.py    # page-size probe
python3 specs/839-batch-enrichment/measurements/strategies.py   # end-to-end A/B
python3 specs/839-batch-enrichment/measurements/scale.py        # batch/concurrency scaling
```

Stdlib only; run from the repository root. Coordinates come from our own
`Cargo.lock` (555 CARGO) plus the committed corpus goldens (302 NPM, 132
PYPI, 59 MAVEN, 12 GO) — 1,060 unique real packages, no synthetic names.
Connections are pooled, modelling `reqwest` rather than paying a TLS
handshake per request.

## Finding 1 — the batch endpoint pages at exactly 100

Undocumented. Found by probing (`pagecheck.py`):

| requested | responses | nextPageToken |
|---|---|---|
| 50 | 50 | empty |
| 99 | 99 | empty |
| 100 | 100 | empty |
| **101** | **100** | **non-empty** |
| 250 | 100 | non-empty |
| 1000 | 100 | non-empty |

Each page needs the previous page's token, so **pages are serial**. A
5000-entry batch is therefore 50 sequential round-trips, not one.

This is observed behaviour, not a documented contract. It can change
without notice, which is why FR-006 (consume all pages) remains required
even though a correctly-sized batch never triggers it.

## Finding 2 — measured throughput (`strategies.py`)

800 unique real coordinates. The 7,592 column is linear extrapolation —
an assumption, flagged as such.

| strategy | 800 components | → 7,592 | vs sequential |
|---|---|---|---|
| per-component, sequential | 43.46s | ~412s | 1× |
| per-component, 8-way | 5.68s | ~54s | 7.7× |
| per-component, 16-way | 2.79s | ~26s | 15.6× |
| **batch=100, 8-way** | **0.43s** | **~4.0s** | **~102×** |
| batch=500, 8-way | 1.20s | ~11.4s | 36× |
| batch=5000, 8-way | 1.64s | ~15.6s | 26× |

All runs enriched the same 791/800 components, so the speed differences
are not bought with coverage.

**Larger batches are slower**, because of Finding 1. Batch size 100 is
the page boundary: below it, round-trips are wasted; above it, pagination
serialises.

Single-request latency (`measure.py`): sequential median **60 ms**, mean
64 ms, p90 77 ms over 40 requests.

## Finding 3 — concurrency scaling (`scale.py`)

3,000 components (1,060 unique, repeated), batch=100:

| workers | wall | → 7,592 |
|---|---|---|
| 8 | 1.05s | ~2.6s |
| 16 | 0.57s | ~1.5s |
| 32 | 0.48s | ~1.2s |

Concurrency past 8 still helps, but deps.dev publishes no rate limit, so
FR-003b keeps the conservative ceiling. 8-way already lands at ~4s.

## Caveats — read before citing these numbers

1. **The 7,592 column is extrapolation**, linear from runs of 800 and
   3,000. Not measured at target scale.
2. **Finding 3 reuses coordinates** (1,060 unique repeated to 3,000), so
   deps.dev's CDN was likely warm. Finding 2 uses 800 all-unique
   coordinates and is the more trustworthy of the two.
3. **This is one network from one location.** Latency elsewhere will
   differ; the *ratios* should survive, the absolute times may not.
4. **The 17-minute figure in the spec is not reproduced here.** Sequential
   measures ~412s (~6.9 min). The difference may be network, coordinate
   mix, CDN warmth, or per-scan overhead outside the HTTP calls. Until
   T001 settles it, no success criterion should quote either number.
