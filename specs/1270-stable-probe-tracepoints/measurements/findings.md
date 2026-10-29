# Measurements: syscall tracepoint inventory

Probe: `tracepoint-inventory.sh` in this directory. Re-run it when the
kernel changes; an undocumented interface is one that can move without
notice.

## Run 1 — aarch64, Linux 6.8.0-136-generic

Colima's VM kernel on the development host, 2026-10-29. Not a runner
image: this run answers "do these tracepoints exist and where are their
arguments", not "do they exist on the image the lane draws".

```
kernel: 6.8.0-136-generic aarch64
PRESENT sys_enter_openat   sys_exit_openat
PRESENT sys_enter_openat2  sys_exit_openat2
PRESENT sys_enter_connect  sys_exit_connect
ABSENT  sys_enter_open     sys_exit_open
ABSENT  sys_enter_creat    sys_exit_creat
ABSENT  sys_enter_rename   sys_exit_rename
PRESENT sys_enter_chdir sys_exit_chdir sys_exit_fchdir
PRESENT sys_enter_renameat sys_exit_renameat
PRESENT sys_enter_renameat2 sys_exit_renameat2
```

**Controls.** Two groups in that list are not under test. The m1070
calls (`chdir`, `renameat`, `renameat2`) are attached today and must
read PRESENT — they do, so the probe discriminates rather than printing
PRESENT for everything. And `rename` must read ABSENT on this
architecture, because `waybill-cli/src/trace/loader.rs` already guards
it behind `#[cfg(target_arch = "x86_64")]` — it does, so the probe is
reading this kernel rather than a cached or generic list.

**What it establishes.**

1. `openat` and `openat2` both offer an enter/exit pair here. The pair
   is what the kernel-side join needs: arguments at enter, result at
   exit.
2. `open` and `creat` do not exist on this architecture — arm64 has no
   such syscalls. FR-006's requirement is therefore live, not
   hypothetical, and the per-arch attach must follow the m1070 `rename`
   precedent rather than treating absence as failure.
3. `connect` offers an enter/exit pair, and `uservaddr` sits at offset
   24 — the address family and the address itself come from a
   UAPI-defined `struct sockaddr` in user memory, so capture reads no
   kernel struct layout at all. This is what removes both unstable
   dependencies in the network path at once: the symbol name
   `tcp_v4_connect` and the hardcoded `struct sock` offsets 4 and 12
   that the current kprobe reads.
4. The argument offsets match the constants m1070 already hardcodes
   (`ARG0=16, ARG1=24, ARG2=32, ARG3=40, RET=16`) exactly, on every
   tracepoint printed. The new code reuses those constants rather than
   deriving its own.

**What it does not establish.** Whether `sys_enter_open` and
`sys_enter_creat` exist on x86_64, where the lane runs. They are
expected to, because x86-64 has both syscalls, but that is an
expectation and this probe cannot reach an x86_64 kernel from this
host: a `--platform linux/amd64` container still reports the arm64 host
kernel's tracepoints. T-numbered task in `tasks.md` must run this probe
in the eBPF lane and record run 2 here before the per-arch attach list
is finalised.

## Still to measure

- **Run 2, x86_64 on the runner image.** The open-family presence list
  and the offsets, from the lane itself.
- **The successful-open baseline (SC-001a).** No measurement of
  successful-opens-only exists. The before/after comparison needs one,
  taken on a host where capture is intact, using the same fixture.
- **Ring-buffer event volume before and after (SC-008).** Today's count
  for the fixture build is the ceiling the join must not exceed.

---

## Run 3 — after the change, same host (2026-10-29)

aarch64, Linux 6.8.0-136-generic, image built from the working tree.
Harness exits 0; all probes attach; `completeness: complete` and
`kprobe_attach_failures: []` agree, so the #1252 honesty invariant holds.

| Quantity | Before | After |
|---|---|---|
| File operations | 573 | 102 |
| Distinct read paths (masked) | 109 | 31 |
| Distinct write paths (masked) | 31 | 31 |
| `open_failures_discarded` | — | 599 (203 distinct paths) |
| `ring_buffer_overflows` | 0 | 0 |
| Compiler invocations | 9 (7 rustc) | 9 (7 rustc) |

### SC-001b — every removal is attributable

`diff-sets.py` in this directory, run against the before sets, the after
sets and the discard record:

```
--- READ (masked) ---
  before=109 after=31 removed=78 added=0
  attributable to a failed request : 78
  UNEXPLAINED removals             : 0
--- WRITE (masked) ---
  before=31 after=31 removed=0 added=0
  UNEXPLAINED removals             : 0

SC-001b: PASS — every removal is attributable, nothing was added
```

**78 read entries left the set and all 78 appear by name in the discard
record. Nothing was added. The write set did not change at all.** That
is FR-004 and FR-004a satisfied together: the only thing lost is open
requests that failed.

The counter and the log agree independently — 599 discard events
counted in the kernel, 599 discard records logged in userspace — which
is a consistency check between two mechanisms that could have
disagreed.

### SC-008 — volume

573 → 102 events, so the ceiling is met with a wide margin, and
`ring_buffer_overflows` stays 0. Two reductions compound, as predicted
in research R8: `do_sys_openat2` called `do_filp_open`, so one `openat`
used to submit two events, and failed opens are now discarded in the
kernel instead of being submitted. 599 discards against 102 recorded
events is the size of the second effect.

### R10 / the superset assumption — CONFIRMED on this host

The removal of the kprobes rested on the syscall tracepoints covering
every open the kprobes caught. Zero unexplained removals is that claim
measured: nothing the kprobes saw is missing except requests that
failed. Confirmed on aarch64 6.8; the x86_64 lane must reproduce it
before the assumption is retired generally.

### Two measurement traps, both hit

**The diff is dominated by run-to-run noise unless it is masked.** The
first comparison reported **52 unexplained read removals and 18
additions**, which read as a serious regression. They were rustc's and
cargo's random temp directories: `rmetaTmtspV` in one run and
`rmeta6OxaCo` in the next, `.tmpWRoguS.temp-archive` and
`.tmpwLsqVV.temp-archive`, `targeto3Q2sv` and `targetkpUXvC`. Masking
those four shapes takes the read diff to 78 removed / 0 added and the
write diff to 0 / 0. The masks are in `diff-sets.py`; a new random-name
shape will show up as an unexplained removal paired with a matching
addition, which is the signature to look for before believing a
regression.

**The discard log must be stripped of ANSI escapes before parsing.** The
first extraction matched whole log lines rather than paths, and so
reported "599 distinct discarded paths" when 599 was the event count and
203 the distinct-path count. A count that exactly equals another count
is worth a second look.

### Still owed from the lane

- Run 2 of the inventory on x86_64, and whether `open`/`creat` exist
  there (research R9; the `#[cfg(target_arch = "x86_64")]` block is
  unverified and would be dead code if they do not).
- The same before/after comparison on the x86_64 runner image,
  especially the image where the kprobe failed.
- An IPv6-only connect (SC-007). The fixture build is hermetic and
  makes no network connection at all, so `connections=0` in every run
  here — the IPv6 path is compiled and loaded but has not been observed
  firing.
- SC-003's 30 consecutive default-branch runs.

---

## Run 4 — IPv6, with a control (2026-10-29)

SC-007 asks for a build that connects only over IPv6, verified against a
deliberately IPv6-only case rather than inferred from one that uses both.
The harness now does this with a listener on `::1` inside the container,
so it needs no egress and cannot flake on DNS or a firewall.

| | Result |
|---|---|
| After the change | 1 connection recorded, destination `::1` |
| **Control: the same case on the pre-#1252 binary** | **0 connections recorded** |

The control is what makes the assertion worth having. `tcp_v4_connect` is
reached only by IPv4, so the old capture saw nothing at all for this
build — indistinguishable from an offline build — and a test written
without the control could have passed against code that recorded
connections for an unrelated reason.

The compiler fixture cannot carry this check: it is hermetic and reports
`connections=0` on every run, so the IPv6 path would have shipped
compiled, loaded, attached and never once observed firing.

## Run 5 — the FR-004b fallback, exercised deliberately

A host that attaches the open entry but not its exit can see requests and
not results. FR-004b says capture must degrade to recording requests,
record the gap, and not claim completeness. That branch had never
executed, so `WAYBILL_TRACE_FORCE_REQUEST_ONLY=1` makes it reachable —
SC-005 asks for this verified by making an interface unavailable on
purpose rather than by waiting to observe it.

| | Result |
|---|---|
| Attestation produced | yes — the trace did not abort |
| `kprobe_attach_failures` | `["sys_exit_openat"]` — the gap is named |
| `completeness` | `partial / probe_attach_failed` — does not claim complete |
| `open_failures_discarded` | 0 — correct: nothing is discarded in request mode |
| File operations | 289 |

### The three modes reconcile

This is the check that the model of the defect is right, rather than
merely consistent with the numbers:

| Mode | Operations | What it records |
|---|---|---|
| Pre-#1252 (two kprobes) | 573 | every request, **twice** |
| FR-004b fallback (entry only) | 289 | every request, once |
| #1252 (entry joined to exit) | 102 | every request that succeeded |

573 ≈ 2 × 289 is the duplication research R8 predicted:
`do_sys_openat2` called `do_filp_open`, so one `openat` submitted two
events and the read set hid it because a set deduplicates. 289 → 102 is
the failed requests leaving. Neither number was chosen; both fell out of
the same fixture on the same host, and each mode was reached by a
different mechanism.
