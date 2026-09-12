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
