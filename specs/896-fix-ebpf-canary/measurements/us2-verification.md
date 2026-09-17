# US2 verification (T034-T037a)

## T034 — SC-003 / SC-004a: a self-inflicted failure reports as the canary's

| | |
|---|---|
| Run | [`35260145867`](https://github.com/kusari-oss/waybill/actions/runs/35260145867) |
| Dispatched | `--ref 896-fix-ebpf-canary -f version=latest -f dry_run=false -f break_env=true` |
| Conclusion | **failure** |
| Attribution | `latest=failure control=failure` → **canary** |
| Issue opened | [#903](https://github.com/kusari-oss/waybill/issues/903) `[canary] the eBPF canary cannot run` |

Row 4 of the attribution table, exercised for real. Note `Build eBPF kernel
object` shows conclusion `success` because of `continue-on-error`; its
**outcome** was `failure`, which is what attribution reads.

## T035 / T035a — SC-005 and FR-004c

`grep -ci 'bpf-linker/issues'` on #903's body → **0**. No instruction to file
upstream, which is the whole point.

The body carried the FR-004c statement verbatim:

> **The component under test went untested this run.**
>
> The build the project already ships could not be produced in this environment
> either, so nothing was learned about `0.11.1` in either direction.

### Two defects this dispatch exposed

Both in the FR-005 evidence, both fixed:

1. **`tail -c 2000` showed compile progress, not the error.** cargo keeps
   compiling other crates after the failure that matters, so the end of the log
   is noise. Now error lines are pulled first, with the tail as fallback.
2. **Raw ANSI escapes** (`^[[92m Compiling`) rendered inside the code fence.
   Now stripped in both spellings — the real ESC byte cargo writes into the
   tee'd file, and the literal `^[` that `gh run view` emits later.

Re-verified on run [`35260679602`](https://github.com/kusari-oss/waybill/actions/runs/35260679602):

```
--- error excerpt that would be reported ---
error: "/home/runner/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.lock" does not exist, unable to build with the standard library, try:
eBPF build failed with status: exit status: 101
```

## T036 — FR-004b: both titles open at once

With #903 open and #685 reopened, `gh issue list --label canary --state open`
listed both:

```
#903  [canary] the eBPF canary cannot run
#685  [canary] bpf-linker eBPF build regression
```

A broken canary does not clear an outstanding upstream regression, and vice
versa. #903 has since been closed as a test artifact.

## Attribution table — all 8 cases

Driven through the step's extracted shell script before any dispatch:

| latest | control | artifact | cause | |
|---|---|---|---|---|
| success | — | true | `none` | row 1 |
| success | — | false | `canary` | row 2 |
| failure | success | — | `component` | row 3 |
| failure | failure | — | `canary` | row 4 — **confirmed live on run 35260145867** |
| not run | — | — | `canary` | row 5, parity failed |
| not run | — | — | `canary` | row 5, install failed |
| failure | success but empty artifact | — | `canary` | guard: `component` cannot rest on an empty exit status |
| failure | did not run | — | `canary` | |

## Report rendering — 4 cases

Rendered through a node harness with stubbed `github`/`context`/`core`:
component (new issue), canary (new issue), 35-day streak (escalates),
3-day streak (does not escalate).

## Branch dispatches can no longer touch production issues

#685 was closed twice by branch dispatches during this work (runs
`35257496241` and `35258931279`). `dry_run` is opt-in and only helps whoever
remembers to pass it. Both report jobs now additionally require
`github.ref_name == github.event.repository.default_branch`, which cannot be
forgotten and fails closed. Confirmed on run `35260496065`: both report jobs
`skipped`.
