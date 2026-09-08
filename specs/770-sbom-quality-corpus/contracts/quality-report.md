# Contract: Quality Report

**File**: `target/quality/run-<waybill-sha-12>.json` (gitignored; uploaded as a CI artifact)
**Producer**: `xtask::quality::report`
**Stability**: `schema_version` is bumped on any breaking change.

## C-1 — Envelope

```json
{
  "schema_version": 1,
  "waybill_sha": "f8ebfa84...",
  "corpus_sha": "a1b2c3d4...",
  "sbomqs_version": "v2.0.6",
  "started_at": "2026-09-03T22:14:07Z",
  "finished_at": "2026-09-03T22:18:52Z",
  "runner": "Linux runner 6.8.0 x86_64",
  "measurements": [ ... ],
  "violations": [ ... ],
  "config_errors": []
}
```

**C-1.1** — `waybill_sha` and `corpus_sha` are mandatory (FR-025). A report that cannot say
which build produced it is not useful later.
**C-1.2** — `sbomqs_version` records the version **actually invoked**, which may differ from the
version the corpus expected. A difference is itself reported.

## C-2 — A measurement

```json
{
  "name": "python-ansible",
  "status": "measured",
  "wall_ms": 2737,
  "sbomqs": { "cyclonedx": 5.87 },
  "pkgs": 11,
  "files": 375,
  "edges": 386,
  "nodes_with_out_edges": 1,
  "max_depth": 1,
  "flat": true,
  "graph_completeness": "partial",
  "sbom_bytes": 184320
}
```

**C-2.1** — `sbomqs` is an object keyed by format name, never a bare number. Only `cyclonedx` is
populated this milestone; the object shape is what makes adding SPDX additive (FR-030).
**C-2.2** — `graph_completeness` is waybill's own self-report, recorded verbatim and **never**
compared against an expectation (research R3).
**C-2.3** — An unmeasurable target omits every measurement field and carries a reason:

```json
{
  "name": "cpp-mongo",
  "status": { "unmeasurable": { "scan_timed_out": { "budget_secs": 600 } } }
}
```

Measurement fields MUST be omitted, never zeroed. A zero is indistinguishable from a real
collapse to zero, which is the misreading Principle X exists to prevent.

## C-3 — A violation

```json
{
  "target": "pnpm-vue-core",
  "metric": "pkgs",
  "expected": { "min": 600, "max": 700 },
  "observed": 412
}
```

**C-3.1** — Every violation carries target, metric, expected bound, and observed value — enough
to act on without re-running (FR-024).
**C-3.2** — Flatness violations render `expected` as a bare boolean.
**C-3.3** — `violations` is sorted by `(target, metric)`; `measurements` by `name` (FR-026).

## C-4 — Human summary (stdout)

A table of every target, then a violations block. Example shape:

```text
target                     wall   sbomqs   pkgs  files  edges  depth  flat
go-cobra                  136ms     7.59      7      0      7      2    no
python-ansible           2737ms     5.87     11    375    386      1   YES
...

VIOLATIONS (1)
  pnpm-vue-core  pkgs  expected 600..700, observed 412
```

**C-4.1** — When there are no violations and no unmeasurable targets, the summary MUST say so
explicitly rather than printing nothing.

## C-5 — Exit codes

| Code | Meaning |
|---|---|
| 0 | Every measurement passed or was observe-only; nothing unmeasurable. |
| 1 | At least one violation, or at least one unmeasurable target (FR-019). |
| 2 | Configuration error — nothing was fetched or measured (FR-021). |

Exit 2 is distinct from exit 1 so a broken corpus file is never mistaken for a waybill
regression.
