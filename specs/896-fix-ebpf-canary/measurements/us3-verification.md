# US3 verification (T038-T042a)

## T042 — SC-006 / SC-006a: the rule evaluated against the live case

```
#685 created_at: 2026-08-13T06:36:09Z
elapsed days:    35
window:          30
escalates:       True
```

Reopening #685 (twice, during this work) does not alter `created_at`, so the
measure is stable against issue state churn.

The rendered escalation block, from the node harness at 35 days:

```
> ## ⚠️ Escalation due — 35 days
>
> This streak began 35 days ago and the documented
> 30-day window has elapsed. The documented fallback is
> now due — see `docs/development/ebpf-toolchain.md`.
>
> The clock runs from this report's own creation date. It does not
> wait on anyone having filed upstream, because a clock that does
> never starts when nobody does.
```

At 3 days the same code path emits `Streak: **3 day(s)**` and no escalation
block. SC-006 is therefore satisfied: the two are distinguishable without
counting comments.

### One honest qualification

This escalation will most likely never actually render on #685, and that is the
correct outcome rather than a gap.

#685 carries the **component-kind** title. After this feature merges, a canary
that still fails for its own reasons reports under the *canary* title at
elapsed 0, and #685 receives nothing; a canary that builds closes #685
outright. So the 35-day figure is verified as arithmetic over the live
timestamp and as rendering through the report code — not as a comment that
appeared on #685.

Claiming otherwise would be the same error this feature exists to fix: asserting
something the evidence does not cover.

## T042a — FR-009 / SC-007, reset half — DEFERRED to post-merge

Requires a green run to close the reports, then a deliberate break, then
reading the new report's elapsed days (expected: 0).

Not reachable from a feature branch: both report jobs now require the run to be
on the default branch, so no branch dispatch can create or close the issues this
check reads. The guard is worth more than the convenience — it exists because
branch dispatches closed #685 twice during this work.

Tracked in the post-merge task list (T047).
