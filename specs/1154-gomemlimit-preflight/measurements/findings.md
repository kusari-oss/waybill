# `GOMEMLIMIT` for the `go list all` preflight — measurements

Issue #1154, ask 3 ("Consider setting `GOMEMLIMIT` for the preflight,
or checking for a known OOM failure first"). The second half shipped
earlier as `SkipReason::OutOfMemory`; this records what the first half
is worth and where the value comes from.

Harness: `probe.sh` beside this file. Re-run it when the toolchain,
architecture or workload changes.

**Platform.** go1.27 linux/arm64 in `golang:1.27-alpine`, under Docker
on a Colima VM (Ubuntu 24.04, cgroup v2 only, 12 GB). Workload:
`go list all` over Kubernetes v1.31.0, resolving 3613 packages. Every
figure below is from that one workload on that one platform, and the
arm64 `go` binary is smaller than an amd64 one — see the caveats.

## 1. The question that decided whether to build this at all

`GOMEMLIMIT` is a soft limit on Go-runtime-managed memory: it makes
the GC work harder rather than letting the heap grow. It therefore
does nothing if a workload's footprint is mostly *live* data, since
the GC cannot free that. If `go list all` holds its package graph
live, setting the variable would be a placebo that trades an OOM kill
for GC thrash and then dies anyway.

Measured, at the cgroup cap where the run dies unset:

| cap | `GOMEMLIMIT` | outcome | wall |
|------|--------------|---------|------|
| 160m | unset | **SIGKILL**, 0 packages | 9.2s |
| 160m | 144MiB | completed, 3613 packages | 8.4s |
| 160m | 128MiB | completed, 3613 packages | 12.0s |
| 160m | 112MiB | completed, 3613 packages | 14.1s |

The footprint is reclaimable, so the lever is real. Tighter limits buy
survival and are paid for in GC time.

The kill arrives as SIGKILL, which `preflight_ran_out_of_memory`
already detects via `signal() == Some(9)` — that detector is confirmed
against the real thing here, not just against a synthetic stderr
string.

## 2. What it buys, and the floor it cannot move

At 90% of the cap, descending:

| cap | `GOMEMLIMIT` | outcome |
|------|--------------|---------|
| 144m | 130MiB | completed |
| 128m | 115MiB | completed |
| 112m | 100MiB | **killed** |

Unset, the smallest cap that survives is 192m; with a limit, 128m. So
the limit moves the floor by about 1.5×. Below that the live set
genuinely exceeds what the GC can reclaim and no value helps — at
112m, 100MiB died while 78MiB and 67MiB completed, so the envelope can
be stretched further by tightening, but only at ~2× wall clock
(8.4s → 18.6s at the extreme).

## 3. Why the reserve is absolute, not a percentage

`GOMEMLIMIT` governs Go-managed memory; the OOM killer counts the
whole process. Survival tracks the absolute gap, not a fraction:

| cap | `GOMEMLIMIT` | headroom | outcome |
|------|--------------|----------|---------|
| 160m | 144MiB | 16.8 MB | completed |
| 128m | 115MiB | 13.6 MB | completed |
| 112m | 100MiB | 12.6 MB | **killed** |

The non-Go-heap cost of the `go` process therefore sits between 12.6
and 13.6 MB here. `GO_NON_HEAP_RESERVE_BYTES` is 16 MiB: that figure
rounded up, because an amd64 `go` binary is larger than the arm64 one
measured and a future toolchain may grow.

## 4. The division, which is the part that matters

waybill runs up to `worker_count()` = `min(workspaces,
available_parallelism())` preflights at once, **all in one cgroup**. A
limit computed against the full allowance lets every child believe it
may use all of it. Four concurrent `go list all`, counting how many of
the four the kernel killed:

| cap | unset | 90% undivided | `cap/workers − 16MiB` |
|------|-------|---------------|------------------------|
| 384m | 1 | 1 (345MiB) | **0** (80MiB) |
| 320m | 3 | 3 (288MiB) | 1 (64MiB) |
| 288m | 3 | — | 2 (56MiB) |
| 256m | 3 | 3 (230MiB) | 2 (48MiB) |

**An undivided limit measures the same as no limit at all.** It is the
division, not the limit, that does the work.

It also does not eliminate kills. `GOMEMLIMIT` is a soft target the
runtime may transiently overshoot, so a cgroup too small for the
workload still loses processes — 320m keeps losing one of four at every
value tried. Under concurrency the exact reserve stopped mattering too:
a ~12 MB reserve and a 16 MiB reserve lost identical counts at every
cap, so transient overshoot dominates there, not the fixed non-heap
cost that the single-process boundary in §3 measures so sharply.

**Scale note.** These caps are stress points chosen to make the
mechanism observable. In the 4 GiB environment the issue describes,
four workers get ~1 GiB each against a ~96 MiB live working set, so
the GC never approaches the limit and the cost rounds to nothing. The
protection is real; the measured penalty is not representative of the
operating range.

## 5. Where the limit is read from

| source | observation | verdict |
|--------|-------------|---------|
| `/sys/fs/cgroup/memory.max` | `4294967296` under `--memory=4g`; literal `max` when unlimited | the value, when the path is right |
| fixed root path | under `--cgroupns=host`, root `memory.max` is **unreadable**; the real limit is at `/sys/fs/cgroup/docker/<id>/memory.max` | insufficient alone |
| `/proc/self/cgroup` | `0::/` in a private cgroup namespace; `0::/docker/<id>` with `--cgroupns=host` | required, to locate the path |
| ancestor cgroups | a parent at `memory.max = 1073741824` has a child whose own `memory.max` reads `max` | an ancestor walk taking the **minimum** is required |
| `/proc/meminfo` `MemTotal` | inside a 4 GiB container, still reports the host's 12 GB | **wrong source**; never used |

The ancestor case is the ordinary shape of a systemd slice, and it is
the dangerous one: reading only the leaf reports "unlimited" in exactly
the constrained environment the feature exists for.

cgroup **v1** could not be measured — the only Linux host available is
cgroup-v2-only. That branch is written to fail safe: an unrecognised
value yields `None`, which leaves `GOMEMLIMIT` unset and behaviour
identical to today's.

## 6. Two traps this probe hit

**An empty bind mount reads as a clean result.** On macOS + Colima, a
mount of a path the VM does not share (`/private/tmp/...`) mounts
empty and silently. `go list all` then prints `warning: "all" matched
no packages` and **exits 0**. Several early runs "measured" a workload
that did not exist. Assert the package count, never the exit status.

**A vacuous test passes.** The test asserting an operator-set
`GOMEMLIMIT` is never overridden passed on macOS with the override
check deleted, because cgroup detection returns `None` there and the
function returned `None` for the wrong reason. Fixed by splitting
`resolve_go_memory_limit_with`, so the rule is exercised against a
supplied limit on any platform, with a control asserting that limit
does produce a value when the variable is absent.
