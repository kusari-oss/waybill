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

## T042a — FR-009 / SC-007, reset half — VERIFIED post-merge

Run on `main` after PR #906 merged, with both report jobs live (no `dry_run`,
default branch, so nothing was suppressed). Three dispatches, in order:

| # | Run | Dispatch | Result |
|---|---|---|---|
| 1 | [`35265779655`](https://github.com/kusari-oss/waybill/actions/runs/35265779655) | `version=latest` | **success** — `report-success` ran and logged `No open canary issue to close — nothing to do.` |
| 2 | [`35266195857`](https://github.com/kusari-oss/waybill/actions/runs/35266195857) | `+ break_env=true` | **failure** → opened [#907](https://github.com/kusari-oss/waybill/issues/907) `[canary] the eBPF canary cannot run` |
| 3 | [`35266358705`](https://github.com/kusari-oss/waybill/actions/runs/35266358705) | `version=latest` | **success** → closed #907 |

**The reset**: #907 carried no streak line and no escalation block — elapsed 0.
It did not inherit #685's 35 days, because a change of attributed cause means a
different title, which means a different issue, which means a different
`created_at`. FR-009's second clause and SC-007 hold, observed rather than
argued.

**The recovery comment** on #907:

> Canary green as of 2026-09-17T19:43:51.140Z (run .../35266358705).
>
> The target version built and its artifact is present, which also means the
> canary itself is working. Closing.

**The excerpt fix, in production**:

```
error: "/home/runner/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.lock" does not exist, unable to build with the standard library, try:
eBPF build failed with status: exit status: 101
```

Two lines, no ANSI, no compile noise — against the same defect whose first
report (#903) showed 15 lines of `Compiling` progress in escape codes.

After dispatch 3: **no open canary issues.**
