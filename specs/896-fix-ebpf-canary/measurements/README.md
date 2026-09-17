# Measurements — 896-fix-ebpf-canary (#685)

Evidence captured **before** any change, so every check this feature adds has
something to fail against. Per the project's "keep the probe" rule, these are
committed next to the spec rather than left in a scratch directory.

## What is here

| File | What it proves |
|---|---|
| `run-history.md` | The canary has produced **0 green runs in 36 attempts**, 2026-08-13 → 2026-09-17. There is no green baseline anywhere in its history. |
| `baseline-failure.log` | Runs 2–36 fail on a missing `rust-src` component, at the eBPF build step. bpf-linker itself installed fine. |
| `baseline-failure-llvm.log` | Run 1 failed on something else entirely — absent system LLVM, at the bpf-linker install step. |

## The finding these three establish together

One issue (#685), one title — *"[canary] bpf-linker eBPF build regression"* —
and **two different canary-environment faults** underneath it. Neither was an
upstream regression. Every one of the 36 reports told the reader to file at
`aya-rs/bpf-linker`.

The transition between the two causes is commit `e1024ab5` (PR #686,
2026-08-13), which made `install-method: binary` the default in
`.github/actions/install-bpf-linker/action.yml`. That is downstream mitigation
**(b)** from `docs/development/ebpf-toolchain.md:104`, executed as designed —
and it gated off the `rustup component add rust-src` that had lived on the
`cargo` install path (`action.yml:57`). The action's own comment records the
assumption it took on:

> *"The binary path doesn't need it (the calling workflow installs its own
> toolchain for the actual eBPF build)."* — `action.yml:54-56`

True of `ci.yml` and `release.yml`. False of the canary, which installs stable
only (`ebpf-canary.yml:70-73`) and then relies on `cargo +nightly` inside xtask.

## Why this rules out a step-position attribution rule (FR-003b)

A rule keyed on *which step failed* would have called run 1 the canary's fault
(install step) and runs 2–36 upstream's (build step). Half right — and wrong on
the cause that ran 35 times. That is the observation behind the spec's choice of
a control build over a positional rule.

## Reproducing

```bash
gh run list --workflow=ebpf-canary.yml --limit 60 \
  --json conclusion,createdAt,databaseId,event
gh run view 35189734662 --log-failed   # missing rust-src
gh run view 31674387553 --log-failed   # missing llvm-config
```

Note: `gh run view --log-failed` writes ANSI escapes as the **literal two
characters** `^[` when piped, not as byte `0x1b`. Strip with
`(?:\x1b|\^\[)\[[0-9;]*m`, or the captures keep their colour codes.

## A second false-report path, found during verification

The T017 verification dispatch (run `35257496241`, 2026-09-17) was the canary's
**first green run in 36 attempts** — and it closed #685 as a side effect,
despite being dispatched with `dry_run=true` from an unmerged branch.

Cause: `dry_run` gated `report-failure` only. `report-success` ran on a bare
`if: success()`, so any green run from any ref closed the open report.

This is the same class of defect as the mis-attribution the feature exists to
fix, pointed the other way: the canary asserting something it has not
established. A report was closed as fixed by a run from a branch whose fix was
not merged. #685 was reopened and `report-success` now honours `dry_run`.

Worth noting because the spec's own edge cases anticipate the general shape
("a green run closes an outstanding report that nothing has fixed") but neither
the spec, the plan, nor `/speckit-analyze` caught this specific instance. It
surfaced only from running the thing.
