# Performance-spec authoring — measure before you spec

**Purpose**: Prevent the "wrong bottleneck" class of mistake that
milestone 772 hit — where a perf spec was authored, planned, tasked,
and implemented against a target that turned out to be 0.3% of the
actual cost (the shared walker measured 63ms of a 20-second scan;
the real bottleneck was `golang::graph_resolver` at 15+ seconds).

This document is for anyone authoring a perf-oriented waybill spec
via the speckit lifecycle (`/speckit.specify` → `/speckit.plan` →
`/speckit.tasks` → `/speckit.implement`). Read it BEFORE `/speckit.specify`.

---

## The lesson (m772 case study)

**Symptom observed**: `time waybill --offline --no-go-mod-why sbom scan
--path /tmp/k8s` reported 18.7 seconds wall-time. `time` also showed
99% CPU utilization on 1 of 8 cores. The natural conclusion:
"single-threaded walker dominates; parallelize it."

**Spec written**: 771 lines across spec.md + plan.md + research.md +
data-model.md + contracts/ + quickstart.md + tasks.md. Target: bring
the "walker-isolated" wall time from 18.7s to ≤ 5s via a bounded
work-stealing thread pool over the m664 `SharedWalker::run`.

**Implementation**: byte-identical + all 76 regression tests passed.
Zero user-visible improvement on the target workload.

**Root cause of the wasted effort**: the "walker-isolated" decomposition
was actually the sum of every non-classifier phase in `scan_fs::scan_path`.
Broken down via per-phase tracing:

| Phase | Wall time |
|---|---:|
| `walk_registry::SharedWalker::run` (the parallelization target) | **63ms** |
| `golang::legacy` (parse go.mod files) | ~700ms |
| **`golang::graph_resolver`** (transitive dep resolution) | **~15 seconds** |
| `scan_fs` finalization | ~2 seconds |
| Emission | ~1 second |

The walker was 0.3% of the wall time. Parallelizing it could not
possibly move the observation.

## The rule

**Before spec'ing a perf milestone, decompose the observation with
per-phase tracing.** `time` + flag-toggling is coarse binary attribution
— it lumps every non-toggled phase together, which invites
misattribution when the code path has more than one active subsystem.

## The methodology

### Step 1 — Reproduce the slow scan with `RUST_LOG=info`

```sh
time RUST_LOG=info waybill --offline --no-go-mod-why sbom scan \
    --path /path/to/slow/fixture --no-deep-hash \
    --format cyclonedx-json --output /tmp/out.cdx.json 2>/tmp/scan.log
```

### Step 2 — Extract per-phase timing from the log

Every waybill subsystem emits a `tracing::info!` line at its
completion (m664 walker, m112 classifier, per-reader legacy walks,
graph_resolver, emitters). The `tracing` timestamps in the log let
you compute per-phase deltas.

Example decomposition script (copy-paste):

```sh
python3 << 'EOF'
import re
ansi = re.compile(r'\x1b\[[0-9;]*m')
ts_re = re.compile(r'(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z)')
mod_re = re.compile(r'waybill::([a-zA-Z_:]+)')
from datetime import datetime
first = None
prev = None
with open("/tmp/scan.log") as f:
    for line in f:
        clean = ansi.sub("", line)
        tsm = ts_re.search(clean)
        modm = mod_re.search(clean)
        if not tsm or not modm:
            continue
        dt = datetime.strptime(tsm.group(1), "%Y-%m-%dT%H:%M:%S.%fZ")
        if first is None:
            first = dt
        elapsed = int((dt - first).total_seconds() * 1000)
        delta = int((dt - prev).total_seconds() * 1000) if prev else 0
        prev = dt
        marker = " <<<" if delta > 500 else ""
        print(f"{elapsed:>6}ms  +{delta:>5}ms  {modm.group(1)[:50]:<52}{marker}")
EOF
```

Read the output: any `+NNNms` gap over 500ms flags a genuine
bottleneck. Sum the deltas by module and you have the per-subsystem
budget breakdown.

### Step 3 — Verify the bottleneck with an inclusion/exclusion test

Once you've identified the suspected bottleneck, prove it via a
build that skips ONLY that subsystem. If wall-time drops by the
predicted amount, the attribution is correct. If it doesn't, keep
decomposing.

**Cheap ways to skip a subsystem for a diagnostic run**:

- Set a very-tight budget env var so the subsystem hits its own
  degrade path (e.g., `WAYBILL_GO_MOD_WHY_BUDGET_MS=1` short-circuits
  the classifier — used in m771 empirical decomposition).
- Comment out the call site in a scratch commit + run.
- Use an existing operator flag when one exists (e.g.,
  `--no-go-mod-why`, `--no-binary-scan=all`, `--no-deep-hash`).

### Step 4 — Only THEN write the spec

At this point the spec's Motivation section can cite:
- The per-phase timing table from Step 2
- The inclusion/exclusion verification from Step 3
- A precise attribution of the wall time to a named subsystem

If the spec's target subsystem isn't in the biggest-delta rows of the
per-phase table, stop and re-decompose.

---

## Anti-patterns to avoid

### ❌ "Binary flag toggling implies phase attribution"

**Wrong**: "scan takes 20s; `--no-go-mod-why` drops it to 18s;
therefore the classifier is 2s and the rest is walker."

**Reality**: `--no-go-mod-why` skips only the classifier. Everything
else — walker, graph_resolver, emission — is still running. The 18s
"rest" is the union of every non-classifier phase, not any one
subsystem. This was the exact m791/m772 misattribution.

### ❌ "One-shot observation implies stable pattern"

**Wrong**: "one k8s scan showed 18s walker-isolated; therefore the
walker is 18s in general."

**Reality**: without per-phase tracing, the attribution is unverifiable
regardless of how many times you run the observation. Repeatable
measurements of the WRONG quantity are still wrong.

### ❌ "Perf spec doesn't need a decomposition step"

**Wrong**: "we've all agreed the walker is slow; skip the profiling
and go straight to `/speckit.specify`."

**Reality**: the m772 spec+plan+tasks+implementation took ~2 hours of
context. The decomposition step would have taken 5 minutes. **Every
perf spec MUST cite a per-phase decomposition as Motivation section
evidence.** If the profiling wasn't done, the spec isn't ready.

---

## Checklist for perf-spec authors

Before invoking `/speckit.specify`:

- [ ] I have per-phase timing data from Step 2 above, not just a `time`
      wall-clock number.
- [ ] I have inclusion/exclusion verification from Step 3 showing the
      suspected subsystem accounts for the predicted share of wall time.
- [ ] The spec's Motivation section will cite these two pieces of
      evidence, not just an aggregated observation.
- [ ] If the target subsystem's attributed cost is < 10% of the total
      wall time, I have re-decomposed to find the real bottleneck.
- [ ] The reference-class benchmark host + fixture are named and
      reproducible (same protocol as m669 + m771).

---

## When you have to skip Step 3 (inclusion/exclusion)

Some subsystems can't be cleanly excluded via a flag (e.g., early
`scan_fs::scan_path` init work). In that case:

- Add temporary `tracing::info!("phase_start", ...)` / `tracing::info!("phase_end", ...)` markers in a scratch commit around the suspected code
- Re-run with `RUST_LOG=info`
- Compute the delta between the two markers
- Drop the scratch commit

This is more work than a flag-toggle but still cheaper than writing a
spec against the wrong target.

---

## Reference

- **m791**: original issue where the misattribution was made ("walker
  is 18s single-threaded"). Later updated with the actual decomposition
  post-m772.
- **m772**: the spec+plan+tasks+implementation that targeted the wrong
  subsystem. Rolled back at implement-time when Step-2 tracing
  revealed the actual bottleneck. Spec artifacts kept at
  `specs/772-parallel-scan-walker/` as a historical record.
- **m669 benchmark harness**: `xtask bench` provides repeatable
  per-fixture wall-time measurements. Useful for validating that a
  perf fix moved the number, NOT for the initial decomposition.
- **m771**: successful example of the methodology — the empirical
  decomposition in issue #745's re-benchmark comment used exactly
  the flag-toggle approach that (subsequently) turned out to be
  insufficient. m771 got lucky because the classifier really was the
  bottleneck; m772 got unlucky because the walker wasn't. Neither
  outcome was predictable from the coarse decomposition alone.
