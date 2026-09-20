# Pre-change baseline (T001, T002)

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

### Output equivalence, C vs D

```
purls:    C=2291  D=2291   differing = 0
licenses: C=2283  D=2283   differing = 0
edges:    C=2331  D=2331
```

**154× faster for a byte-equivalent document.** This is the property FR-002
turns into a test (T003) before the default moves.

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
path. That is run C above: 1302 seconds.
