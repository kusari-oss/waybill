# Pre-change baseline (T001, T002)

> **CORRECTED — see "CORRECTION (T021)" at the foot of this file before
> quoting anything here.** Run C (1302s) is real but is ~98%
> **ClearlyDefined**, not deps.dev (deps.dev is ~16.6s). The "run B isolates
> deps.dev" argument in this file is **false**. Tracked as **#930**.


**Binary**: `waybill 0.9.0`, release build from `main` with the enrichment path
unmodified. Preserved at `<scratch>/m923/waybill-prechange` for the T022
teeth-check and the T009 byte-identity comparison.

**Target**: a public polyglot repository — ~6,200 files, Go plus two yarn
workspaces, 2,291 packages resolved, 2,331 dependency edges.

## The four flag combinations

| run | flags | wall clock | licensed components |
|---|---|---|---|
| A | `--offline` | **3.0s** | 0 |
| B | online, `--no-deps-dev` | **2.2s** | 0 |
| C | online, no enrichment flags (today's default) | **1302s** | 2283 |
| D | online, `--enrich-batch --enrich-no-cache` | **8.5s** | 2283 |

### Run B is the one that makes this a measurement

B holds the network **on** and disables only deps.dev. The delta B→C is
therefore deps.dev alone: **~1300 of the 1302 seconds**. Everything else
waybill does on this repository — walking 6,200 files, parsing `go.sum` and two
yarn lockfiles, resolving 2,291 components and 2,331 edges, emitting
CycloneDX — is about **two seconds**.

Comparing A to C would have attributed the whole 1302s to "enrichment" while
folding in every other network path. That is the flag-toggle attribution error
`docs/development/perf-methodology.md` exists to prevent.

> **And this paragraph then commits it one level down.** B→C folds in
> ClearlyDefined exactly as A→C folds in the rest. The per-source timings that
> settle it were in the log being summarised when this was written.

### Output equivalence, C vs D

```
purls:    C=2291  D=2291   differing = 0
licenses: C=2283  D=2283   differing = 0
edges:    C=2331  D=2331
```

~~**154× faster for a byte-equivalent document.**~~ **The multiplier is
wrong** — it is ClearlyDefined's cost, not deps.dev's; the measured deps.dev
effect is 2.4x. The *byte-equivalence* half holds, and it is the property
FR-002 turns into a test (T003) before the default moves.

## Provenance of these numbers

Measured earlier in this session against `main`, before this branch existed.
The enrichment path has not changed since — `waybill-cli/src/enrich/` is
untouched on this branch at the time of writing — so they are the pre-change
baseline rather than a re-derivation of it.

Re-running case C costs 22 minutes to reproduce a number already captured
under identical conditions. **T021 re-runs the matrix post-change**, which is
where the comparison that matters gets made.

## T002 — the state being changed

A default scan (no enrichment flags) currently takes the **per-component**
deps.dev path. On the reference repository that path costs **~16.6s** — not
the 1302 seconds of run C, which is ~98% ClearlyDefined (#930).

---

## CORRECTION (T021) — run C is real, but it is not deps.dev

Run C above (1302s) happened and was timed correctly. **It is almost entirely
ClearlyDefined.** The run's own log:

```
deps.dev licence enrichment complete  elapsed_ms=16550  network_lookups=2291
ClearlyDefined enrichment starting    unique_coords=2291  concurrency=8
   ... 1283 seconds ...
ClearlyDefined enriched components with concluded licenses  count=1203
real 1302.31
```

Re-run with **both** caches cold: deps.dev 16.65s (0.6% from the original),
ClearlyDefined 695s, total 714.50s. ClearlyDefined is 97–98% of the runtime
in both runs and varies about 2x between them; deps.dev is 1–2% and stable.

### What specifically is wrong above

The claim that run B isolates deps.dev — and therefore that B->C is "deps.dev
alone: ~1300 of the 1302 seconds" — **is false**. `--no-deps-dev` does not
disable ClearlyDefined (`scan_cmd.rs:2361`:
`clearly_defined: !args.no_clearly_defined`). Run B measured 2.2s because it
ran after the cold run had populated the 47 MB ClearlyDefined disk cache, not
because ClearlyDefined was off.

The per-source timings that settle this were in the log being summarised when
that paragraph was written. The paragraph congratulating itself on avoiding
the flag-toggle attribution error commits it.

"154x faster for a byte-equivalent document" is wrong in its multiplier. The
byte-equivalence half holds and is what FR-002 turns into a test.

**Do not cite the 1302s as a deps.dev figure.** The reproducible deps.dev
comparison is in `after.md`: 14,465ms -> 6,082ms and 2,291 -> 23 requests.
The 1302s belongs to #930.
