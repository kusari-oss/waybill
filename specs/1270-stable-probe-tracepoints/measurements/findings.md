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

---

## Run 2 — x86_64, from the eBPF lane (2026-10-29) — T001 ANSWERED

The lane on PR #1313 was the first x86_64 execution of the `open` and
`creat` programs. It **failed**, and the failure was in this feature's
own code rather than in the kernel.

```
WARN could not attach the sys_exit_open tracepoint: program 'sys_exit_open' not found
WARN could not attach the sys_exit_creat tracepoint: program 'sys_exit_creat' not found
WARN some probes did not attach; capture is a floor, not a total
     failed=["sys_exit_open", "sys_exit_creat"]
completeness: partial  (reason=probe_attach_failed)
Collection done net=6 file=43
FAIL: .../libsafe/src/lib.rs is in no rustc read set (m1070 SC-001)
```

### The cause, in four steps

1. The four `open`/`creat` programs were gated
   `#[cfg(target_arch = "x86_64")]` **on the kernel side**. That crate
   compiles for `bpfel-unknown-none`, so the predicate is false on every
   host and the programs were in the object **nowhere**.
2. The loader's `#[cfg(target_arch = "x86_64")]` evaluates against the
   *host*, so on x86_64 it asked for programs that did not exist —
   `program 'sys_exit_open' not found`, which is aya reporting a missing
   program, not a missing kernel tracepoint.
3. The exit-first attach rule then skipped `sys_enter_open`, as designed.
4. x86_64 routes a real share of opens through `open(2)` rather than
   `openat(2)`, so those opens were never observed: **43 file
   operations against 102 on arm64**, with the fixture's crate roots
   missing from the read set.

`whether the syscall exists` is a property of the kernel a program
attaches to, not of the target it was compiled for. m1070 had this right
for `rename`: the kernel side is unconditional and only the *loader*
gates by host arch. The cfg was added here where the template
deliberately had none.

### Fix, and how it is verified

The arch gate is removed from the kernel side; the loader keeps its
gate, so arm64 still does not attempt an attach for a syscall its kernel
lacks. Verified by listing the object's symbols rather than by observing
an attach, because an attach can only be observed on one architecture at
a time:

```
$ readelf -s waybill-ebpf | grep -E 'sys_(enter|exit)_(open|openat|openat2|creat|connect)'
sys_enter_connect  sys_enter_creat  sys_enter_open   sys_enter_openat
sys_enter_openat2  sys_exit_creat   sys_exit_open    sys_exit_openat
sys_exit_openat2
```

All nine present. arm64 harness re-run after the fix: unchanged — 102
operations, 599 discards, `completeness: complete`,
`kprobe_attach_failures: []`, IPv6 recorded.

### What this says about the earlier claim

The PR described this block as "inference, labelled" and reasoned that
the tracepoints "should be present because those syscalls exist on
x86-64". The reasoning was sound and answered the wrong question: the
syscalls do exist and their tracepoints were available. What was missing
was the program.

**R10's superset assumption was therefore refuted on x86_64 and has now
been repaired rather than confirmed.** It still awaits a clean x86_64
run.

### What worked

The capture reported the gap rather than hiding it. `sys_exit_open`
reached `kprobe_attach_failures[]`, completeness read
`partial / probe_attach_failed`, and the harness named the missing crate
root. Before #1310 this same loss presented as `complete` with an empty
failure list, which is how it survived two releases.

## Run 6 — the #618 instrument on x86_64, and why its number cannot be read yet

First x86_64 lane run carrying the #618 drop counters (job 123780037537,
head `5944173c`). The lane had to be re-run once: the first attempt
(job 123731999178) died in the pre-existing `go install sbomqs` step on
a `proxy.golang.org` stream error, in both the plain and the eBPF
x86_64 lanes, and never reached the harness. A grep for
`event_drop_reasons` on that first log returned zero matches — which is
indistinguishable from a clean capture, and would have been reported as
one if the failing step had not been checked. The same shape as the
`openat`-257 control and the `-i`-less `docker run`.

### What the instrument reported

Two traced builds of the `two_binaries_diverge` fixture:

```
reasons=EventDropCounts { counts: {"kernel_exit_without_request": 1237} }
reasons=EventDropCounts { counts: {"kernel_exit_without_request": 918} }
```

Every other reason zero. No `kernel_path_unreadable`, no
`user_empty_path`, no `user_*` reason at all.

### The prediction this refutes — mine

Before the run I recorded: "if entry events are being lost,
`kernel_exit_without_request` is large." That inference is **not
available**, because of a design decision in this same feature.

The open tracepoints are attached **exit first** (`loader.rs`, and the
comment "Exit first, same rule as openat2 above"). The reason is sound:
an entry attached before its exit would stash requests no exit program
can join, losing successful opens outright. The consequence is that in
the window between attaching `sys_exit_openat` and `sys_enter_openat` —
and again for `openat2`, `open` and `creat` — every open **anywhere on
the machine** exits with no stored request and increments this counter.
The probes are system-wide; the capture is scoped to the build's process
tree in userspace. So the counter aggregates the whole runner's ambient
open traffic across the attach window, and the capture does not.

That explains arm64 reading 0 and x86_64 reading 1237 with no defect in
between: a quiet local container versus a busy CI runner. It also means
the number cannot presently distinguish ambient attach-window orphans
from genuinely lost build entries. A global counter compared against a
process-scoped capture is not evidence.

A second, smaller asymmetry is real but latent: `store_open` returns
early on `!should_trace()` and `finish_open` has no such guard, so the
tracer's own opens orphan their exits. `tracer_pid` is populated
(`loader.rs:112`), so the path is live; the count stays small only
because the tracer is blocked in the ring-buffer drain for the window
and writes the attestation after detach.

### What is independently established

The x86_64 capture really is short, and that does not rest on the
counter:

| | arm64 (passing) | x86_64 (failing) |
|---|---|---|
| file operations | 102 | **29** |
| rustc invocations | 7 | 7 |
| ring_buffer_overflows | 0 | 0 |
| open_failures_discarded | — | 151 |
| unresolved_relative_opens | — | 2 |

All 7 rustc invocations were captured this run — whole invocations did
not vanish, unlike earlier #1185 observations. What is missing is the
contents of their read sets: **not one `.rs` read appears in the entire
recorded operation list.** Every surviving read is an absolute path
under `target/release/deps/`. `completeness` still reads `complete`,
which is the #1185 complaint exactly.

### Mechanisms eliminated for `libsafe/src/lib.rs` specifically

- **m213 kernel noise filter** — not a match. The fixture path
  `/waybill/waybill-cli/tests/fixtures/.../libsafe/src/lib.rs` hits no
  prefix pattern (`/etc/ /proc/ /sys/ /dev/ /lib/ /usr/ /bin/ /var/`)
  and no contains pattern (`/.cache/ /.local/ /fingerp /.finger
  /increme Cargo.to -toolcha .cargo/c Cargo.lo`). Checked against the
  catalog in `waybill-common/src/filter.rs`, not assumed. Note the
  filter does legitimately explain the absence of `Cargo.toml` reads,
  via the `Cargo.to` contains pattern.
- **Ring buffer** — `ring_buffer_overflows: 0`.
- **Failed-open discard** — the harness diagnostic reports "(no discard
  line names it)".
- **Relative-path resolution (m1070)** — would report the open as
  `Unresolved`, and only 2 operations are so flagged, neither of them a
  source file. An unresolvable open still appears; this one does not
  appear at all.

So the event never reached userspace, and no instrumented site claims
it. It is either inside the unattributable 1237, or upstream of every
counter.

### What would make the next run decisive

1. **Scope the orphan counter to the traced process tree**, or carry the
   pid with the drop, so ambient traffic stops being counted against a
   process-scoped capture.
2. **Baseline `EVENT_DROPS` immediately after attach completes** and
   report the delta. The attach-window orphans are expected by design
   and should not be reported as loss at all.
3. **Set `WAYBILL_TRACE_OPEN_DIAG=1` in the harness.** The diagnostic
   already asks for it ("needs WAYBILL_TRACE_OPEN_DIAG=1 above") and
   `OPEN_DIAG` is already written on that env var — this costs nothing
   to switch on and names what happened to each open.

Until (1) and (2) land, `kernel_exit_without_request` should be read as
"not yet attributable", not as evidence either way. The counter that
*is* trustworthy is the set of zeros around it: no path-read failure, no
null path, no userspace drop reason fired on a capture that lost ~70% of
its operations.

## Run 7 — making the counter attributable, and a second failure CI had masked

Three changes, measured on arm64 in the `Dockerfile.ebpf-test` container.

### The two that make `kernel_exit_without_request` mean something

1. **`finish_open` now applies `should_trace()`**, the guard `store_open`
   already had. Without it the filtering was asymmetric: the tracer's own
   opens stored no request at entry and then counted a drop at exit.

2. **`EVENT_DROPS` is baselined after attach completes**, read in
   `scan.rs` between `load_and_attach` returning and the child being
   spawned, so anything it already holds predates the build.
   `merge_kernel_event_drops` reports the delta.

The exit-first attach order is kept. It is correct — an entry program
live before its exit program would stash requests nothing can join,
losing successful opens outright — so the window is a consequence to
subtract, not a bug to fix.

Two decisions, both in the direction of not concealing loss:

- An unreadable baseline slot subtracts **nothing** rather than
  defaulting to zero, because a zero baseline would silently inflate the
  delta by the whole attach window. Over-reporting loss beats hiding it.
- `saturating_sub`, not an assertion. Per-CPU sums read at two instants
  can fall if a CPU comes online between them; that is "nothing since
  attach", not a fault.

Five unit tests cover the arithmetic. The one that matters is
mutation-verified: making `net()` return `now` makes
`baseline_nets_out_what_the_attach_window_already_counted` fail, so the
test exercises the subtraction rather than the plumbing around it.

### The third was already done

`WAYBILL_TRACE_OPEN_DIAG=1` has been set in the harness since #1252
(line 55). The diagnostic's "(needs WAYBILL_TRACE_OPEN_DIAG=1 above)" is
a reminder, which run 6 misread as a complaint. So run 6's "(no discard
line names it)" was already a real negative result: the missing crate
root is genuinely absent from the discarded failed opens, not merely
unlogged. Nothing to change, and the run-6 conclusion is strengthened.

### A second failure, which the first was masking

Running the harness to completion on arm64 surfaced:

```
FAIL: trace_integrity carries keys the pre-m1072 struct did not have (SC-007)
      ["open_failures_discarded"]
```

m1072 SC-007 pins `trace_integrity`'s key set against the **merge-base**
struct, deliberately, so the assertion cannot be circular. #1252 then
added `open_failures_discarded`, which a trace of `cat /etc/hostname`
populates legitimately — the loader probes several paths for libc and
most of those opens fail, which is what the counter is for.

Pre-existing at `5944173c`. CI has never reported it because **the
x86-64 lane exits at the m1070 SC-001 check further up the script and
never reaches this assertion.** One failure hid the other, so "the eBPF
lane is red" was two independent faults:

| | fails on | cause |
|---|---|---|
| m1070 SC-001 | x86-64 only | #1185 — the capture loses the source reads |
| m1072 SC-007 | both; arm64 reaches it first | #1252 added a field SC-007 pins against |

Fixed by naming the two fields this branch adds on purpose
(`open_failures_discarded`, `event_drop_reasons`) rather than widening
the check, so an addition nobody intended still fails.

### The assertion was also weaker than it read

It used `[.] | inside($pre)`. On strings jq's `inside` is substring
containment, so a stray `ring_buffer` key would have passed against
`ring_buffer_overflows`. Now exact membership via `index($k)`, proved
rather than asserted:

```
  ring_buffer_overflows      inside=true  index=true
  open_failures_discarded    inside=true  index=true
  event_drop_reasons         inside=true  index=true
  ring_buffer                inside=true  index=false   <- would have slipped through
  totally_new                inside=false index=false
```

Strictly stricter than before about everything it did not admit on
purpose.

### arm64 after the changes: unchanged

| | before | after |
|---|---|---|
| fixture file operations | 102 | 102 |
| `event_drop_reasons` | absent | absent |
| "capture is a floor" warnings | 0 | 0 |
| #618 control (`kernel_null_path`) | 5 | 5 |
| ring_buffer_overflows | 0 | 0 |
| harness exit | — | 0 (full run) |

Caveat on scope of evidence: in this run `open_failures_discarded`
happened to be absent from the `cat` trace, so the harness did not
re-exercise the SC-007 fix end-to-end. The evidence for that fix is the
predicate table above plus run 6 failing with the key present.

### What the next x86-64 lane run distinguishes

The delta is now attributable, so each outcome says something different:

- **~0** — entry events were never being lost; run 6's 1237 was the
  attach window and tracer self-noise. The loss then sits upstream of
  every instrumented site, which eliminates the whole join mechanism.
- **still large** — entry events are genuinely lost with all probes
  attached, pointing at `PENDING_OPEN` insert failure or the entry
  program not firing for some opens.
- **in between** — compare it against the 73 missing operations
  (102 − 29) to size how much of the shortfall the join accounts for.
