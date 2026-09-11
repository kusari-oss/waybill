# Contract: `xtask compare`

**Feature**: 780-comparative-bench-harness

## Invocation

```
cargo run -p xtask --release -- compare [--config <path>] [--target <name>]... [--repeats <n>]
```

| Flag | Default | Meaning |
|---|---|---|
| `--config` | `xtask/compare/tools.local.toml` | Operator tool set. Absent → error naming the example file. |
| `--target` | all in corpus | Restrict to named targets. Repeatable. |
| `--repeats` | 5 | Timing repeats per tool per target. Minimum 3; below that the spread figure is not meaningful. |

Exit codes: `0` comparison produced (verdict may still be Withheld — that is a
result, not a failure); `1` self-check failed or configuration invalid; `2`
no tool ran successfully.

## C-1: Self-check precedes everything (FR-012, FR-013)

Before any tool is invoked against any real target, the harness scans the
committed fixture whose module set is known by construction and confirms it
recovers that set exactly.

```
self-check: fixture xtask/compare/fixtures/known-answer-go
  expected 7 identities, recovered 7  OK
```

On mismatch it prints the symmetric difference and exits 1. No tool is
measured. An instrument that cannot demonstrate its own accuracy does not
rank anything.

The fixture MUST contain two versions of one package, so a regression to
version-stripped identity fails here rather than silently changing every
subsequent number.

## C-2: Interleaved execution (FR-001a, FR-003)

For each target, repeats are interleaved across tools and never run
concurrently:

```
round 1: tool-a, tool-b, tool-c
round 2: tool-a, tool-b, tool-c
...
```

Not `tool-a ×5` then `tool-b ×5`. Block execution lets drift between blocks
masquerade as a difference between tools — which is how a 2.7× contention
error entered the measurements that motivated this feature.

## C-3: Reduction to package identity (FR-006, FR-006a, FR-007)

Every tool's CycloneDX output is reduced by the same rule:

1. Take each component's `purl`. No purl → count toward
   `identityless_components`, never toward packages.
2. Normalise: lowercase type, normalise namespace, **retain version**, drop
   qualifiers and subpath.
3. Collect into a set.

Report `distinct_packages` and, beside it, the `raw_components` it reduced
from. A tool emitting 2,314 entries for 427 packages is telling you something
about itself; the harness surfaces that ratio rather than hiding it.

## C-4: Accuracy scoring (FR-008, FR-008a, FR-008b)

Only when the target declares a truth method. Output always names the method
and whether it is a superset:

```
tool-a  vs truth(go-sum-union, SUPERSET, n=479)
        found 468   missed 11   extra 0
        NOTE: truth is a superset of the built set; `extra` is not
              necessarily wrong and `found` rewards over-reporting.
```

Scores derived by different methods are never compared; attempting it yields
`WithheldReason::TruthMethodMismatch`.

## C-5: Verdict (FR-002, FR-002a, FR-004, FR-017)

```
VERDICT: withheld (2 reasons)
  - host is Noisy, not reference class
  - timing spread for tool-a exceeded 1.25 (observed 1.61)
Figures below are recorded for context and are NOT a comparison.
```

Or:

```
VERDICT: comparable
  host reference class; all spreads within tolerance; self-check passed
```

The harness never emits "faster", "better", "more accurate", or any
comparative adjective about a tool. It reports quantities and conditions.
Interpretation is a human act performed afterwards, which is precisely where
the errors that motivated this feature occurred.

## C-6: Output location (FR-014, FR-016)

Sole output: `target/compare/run-<timestamp>.json`, plus a rendered summary
on stdout. `target/` is gitignored.

The harness MUST NOT write into `docs/`, MUST NOT write into the repository
tree outside `target/`, and MUST NOT be referenced by any workflow file.
A test asserts the last of these by scanning `.github/workflows/`.

## C-7: Timing presentation (FR-001b)

Ratios within a session, with spread:

```
timing (within-session ratios, interleaved, n=5)
  tool-a : tool-b   3.9x   (spread: a 1.08, b 1.04)
  absolute medians  a=21.3s  b=5.4s   [context only]
```

Absolutes are labelled context. On a non-reference host they have been
observed to move by more than 2× between runs of an identical command, so
they do not support a comparative claim and the harness does not let them
appear to.

## C-8: Enriched-mode handling (FR-002a, FR-002b)

A tool whose `network = "enriched"` has its timings labelled indicative and
subject to a separate, wider tolerance. Its coverage and accuracy figures are
authoritative and carry no caveat:

```
tool-a [enriched]  timing INDICATIVE (network-dependent)
                   coverage authoritative: 468 distinct, 428 with licence
```
