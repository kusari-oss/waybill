# Milestone 1072 measurements (#1144)

Run `./probe.sh` to reproduce. Probes 1 and 4 read milestone 1070's
committed strace of a real kernel trace (Colima aarch64, Ubuntu 24.04,
kernel 6.8.0-136); probes 2 and 3 run on any host, because inode identity
and file survival need no eBPF. Probe numbering follows the order the
probes were written, not the questions: probe 1 → Q1, probes 2–3 → Q2,
probe 4 → Q3. Output below captured 2026-10-11 (probes 1–3) and
2026-10-12 (probe 4) on macOS (darwin 25.5.0) against
`two_binaries_diverge` (2 lib crates, 2 bins).

## Q1. Can one path appear in two invocations' write sets?

**No**, and it is structurally prevented. Do not specify for it.

| probe | result |
|-------|--------|
| 1a parse coverage | `raw=70 parsed=70` — every write-mode open parsed |
| 1b real multi-writer paths | `/dev/fd/5` only, by pids 154/155/174/175 |
| 1c positive control | injected writer **surfaced** |

`/dev/fd/5` is a pipe, not an artifact. No real path is opened for
writing by more than one pid.

Probe 1c matters: a zero result in 1b is indistinguishable from a
broken `sed`, so the control re-emits one real artifact open under a
fake pid and confirms the detector fires. It does.

**Why it is structural, not luck.** Cargo embeds a metadata hash in
every output filename (`liblibsafe-603d5d27712eef2e.rlib`,
`safe_only-049f485b9f94a33f`) and compiles each
(crate, feature-set, target) exactly once per build. Two invocations
writing one final path would require two compilations sharing a
metadata hash, which is the thing fingerprinting exists to prevent.

**Limit of this evidence.** It covers one fixture. Milestone 1070 also
straced a larger build (`-p waybill-common`: 31 rustc, 12 cc, 8 build
scripts) and recorded the same write patterns in `strace_summary.txt`,
but `common.strace` itself was not committed, so the larger build was
not re-analysed here. Build scripts writing a shared path in `OUT_DIR`
remain the one unmeasured route to a multi-writer path; they are not
compiler invocations, so they do not reach a `write_set`.

## Q2. How do compiler outputs actually reach their final names?

Three distinct mechanisms, from `strace_summary.txt` plus probes 2–3:

- **Binaries** — the linker opens `deps/<bin>-<hash>` directly with
  `O_RDWR|O_CREAT|O_TRUNC`. No rename.
- **Libraries** — rustc writes a randomized temporary and `renameat()`s
  it into place; *the final library path is never opened for writing*:
  - `deps/rmeta*/full.rmeta` → `deps/lib<crate>-<hash>.rmeta`
  - `deps/.tmp*.temp-archive/tmp.a` → `deps/lib<crate>-<hash>.rlib`
- **User-facing name** — cargo materialises `release/<bin>` from
  `deps/<bin>-<hash>` *after* compilation. Mechanism is
  platform-dependent: `linkat` on Linux (m1070), a copy on macOS
  (probe 2: `ino=559840360` vs `ino=559840356`, both `links=1`,
  identical content `ba4d0a47…`).

So `two_binaries_diverge` exercises both the direct-open and the
rename-into-place paths, and is an adequate fixture for US1 and US3.

Probe 3 confirms the rustc temporaries are gone by trace end
(`deps/rmeta*` does not exist), which is exactly the case #1144
reports as falsely `survived_trace_window: true`.

## Q3. Can one path appear in two invocations' *read* sets?

**Yes, routinely.** This is the mirror of Q1 and the opposite answer.

Probe 4 walks each reader pid up the `clone()` edges to the `rustc` that
owns it, because `strace -f` lists worker threads as separate pids — without
that walk, one rustc's threads look like several invocations.

`deps/liblibsafe-2d5f1ecd358a29fb.rlib` is read under **three distinct
rustc invocations**:

| reader pid | owning invocation |
|-----------|-------------------|
| 157 | rustc 154 |
| 177 | rustc 174 |
| 180 | rustc 175 |
| 152 | cargo (pid 139), not a compiler invocation |
| 196, 197 | `/usr/bin/ld`, linker children |

The 7 rustc invocations in the trace are pids 142 144 149 154 155 174 175.
Both fixture binaries declare `libsafe = { path = ... }`, so its `.rlib` is
read by both of their compilations plus the lib's own, which is exactly the
shape expected. Across the whole trace, **213 paths have more than one
reader.**

**Consequence.** The close-time pass must count and charge per *distinct
path*, not per set entry. Charging per entry would bill one `.rlib`'s bytes
three times against the budget, and would let two entries for one path
disagree about its close-time state if the file changed between their
checks. Spec: FR-011, FR-011a, FR-013, SC-010.

Note the asymmetry this creates with Q1: a write path has exactly one
producing invocation, so `sha256_at_close` is attributable; a read path has
many consuming invocations, so the same hash is legitimately shared by all
of their entries.

## Consequences for the spec

1. No multi-writer FR. Recorded as an assumption with this evidence.
2. A hard link is **not** a rename: both names survive and share an
   inode. `renamed_from` is for renames only.
3. Two write-set paths may legitimately share an inode and hash
   identically. Not an error.
4. `release/<bin>` — the artifact an operator ships — appears in **no**
   compiler write set, because no compiler wrote it. A real provenance
   gap, but outside #1144; needs its own issue.
5. Hash and stat **once per distinct path** per trace, with the coverage
   counts over distinct paths (Q3).

## If something here is later found wrong

This file is also where negative results belong. The tracing subsystem is
experimental, so an approach recorded above may turn out not to work —
append the finding and what it cost rather than deleting the claim it
replaces. A measurement that was superseded still tells the next person
which question had already been asked.

## Not measured

The close-time hashing cost that SC-008 ties its ceiling to. That
requires a real kernel trace with the pass implemented, so it belongs
to the first implementation task, not to this probe.

## Q5. Can two traces of one build produce byte-identical payloads?

**No, and this predates milestone 1072.** Measured while implementing
T043, which originally asked the harness to assert exactly that.

Two consecutive `trace capture` runs of `two_binaries_diverge`, with
`target/` removed between them, on Colima 6.8.0 aarch64. The normalised
payloads differ in two places, both in the **observation** tier that
Principle II forbids the close-time pass to touch:

```
17c17
<           "invocation_id": 3114349875006
>           "invocation_id": 3118339958275
...
151c151
<   ".../target/release/deps/rmetaA36mif"
>   ".../target/release/deps/rmeta35kQFf"
156c156
<   ".../target/release/deps/rustceARhX8"
>   ".../target/release/deps/rustcdRlOq7"
194c194
<   ".../deps/.tmpZ6GhkV.temp-archive/tmp.a"
>   ".../deps/.tmpCc2MIw.temp-archive/tmp.a"
```

1. **`invocation_id` is `bpf_ktime_get_ns()` at exec time.** Milestone
   210's deliberate choice, commented at
   `waybill-cli/src/trace/compiler_pipeline.rs:371` — "unique +
   monotonically increasing within a boot". A boot-relative nanosecond
   clock cannot repeat across runs, so no two traces have ever agreed
   here.
2. **rustc's temporaries are `mkdtemp`-random.** `rmeta<6 random>`,
   `rustc<6 random>` and `.tmp<6 random>.temp-archive` appear in read
   sets, write sets and `renamed_from` because the kernel genuinely
   observed them. m1070 shipped those sets; the randomness arrived with
   them.

### Why the original requirement looked safe

It cited feature 006 US2. That text reads "byte-identical across re-runs
**when the build is reproducible**", and FR-006 constrains the *signing
step*: re-signing one canonical payload must not change the payload
bytes. Neither promises that two *traces* agree. The conflation is
between a build whose **outputs** are reproducible — this fixture's are —
and a build whose **process** is. The trace records the process.

### Disposition

C-1072-6 now requires what the pass can actually deliver: given the same
observation tier and budget, the close-time tier and the coverage record
are identical. SC-005a states it, and T043's harness section asserts it
by comparing the coverage record plus every path's close-time facts over
the paths both runs observed — an intersection, because the random
temporaries differ by name and never meet.

Evidence that the narrower claim holds: `cost.md`'s five runs produced
`paths_total=102, paths_hashed=14, bytes_hashed=955494` with no column
moving, and the corrected harness assertion reports the coverage record
identical across runs with identical close-time facts for every shared
path.

### How much of the set is per-run

**50 of 102 paths appear in both runs.** The other 52 — just over half —
are `mkdtemp` names that exist in exactly one trace. That ratio is the
scale of the problem the old requirement would have had to solve: not a
field or two, but half the path set. It is also why the harness compares
an intersection and then asserts a floor on its size: 50 shared paths is
a real check, whereas a handful would have been close to vacuous.

**Recorded rather than quietly relaxed**, per `spec.md`'s experimental
posture: the subsystem is pre-stable, and an obligation found to be
unachievable belongs in this file with its measurement, not in a silent
edit.

### Open consequence for signing

If byte-identical re-runs are ever genuinely needed — reproducible-build
verification comparing two independently produced attestations — it is
`invocation_id` that has to change, not this pass: a content-derived
identifier (hash of argv + cwd + ordinal) rather than a clock. That is a
predicate-shape change well outside #1144, and it would not fix the
`mkdtemp` paths, which are irreducibly per-run facts about what happened.

## Q6. Does the 2 GiB default budget survive a real workspace?

**No — it was 1.4× a build this repo's own CI performs.** Measured after
T049 was marked done, by `budget-probe.sh`, because the question "where
did 2 GiB come from" has a worse answer than it looked.

`cost.md`'s original sizing cited the measurement it was required to
cite, and that measurement was of `two_binaries_diverge`: four crates,
955,494 bytes. The headroom was quoted as ~2,200×. A four-crate fixture
is the wrong denominator for a global default, and the project rule the
spec cites — measure external behaviour before designing around it —
applies to the scale of a real build just as much as to an API's page
size. 2 GiB satisfied the letter of C-1072-9 while resting on a toy.

Two traced builds of waybill itself, fresh target dir, budget flag at
64 GiB so nothing clamps:

| traced build | bytes demanded | % of 2 GiB |
|---|---|---|
| `two_binaries_diverge` | 955,494 | 0.04% |
| `--release --bin waybill` | 880,376,492 | 41.0% |
| `--release --workspace --all-targets` | 1,515,056,247 | **70.5%** |

Truncation costs real data, so the default is meant to be reached only
by something pathological. At 70.5% consumed by an ordinary CI build,
any workspace 1.5× this one hit it. **Revised to 16 GiB** (~11× the
CI-shaped demand); full detail and the SC-008 consequences are in
`cost.md`.

Two of `cost.md`'s numbers were also measured properly for the first
time: hashing throughput is **~120 MB/s**, not the ~478 MB/s derived
from a 2 ms sample, and the pass costs **1.8–2.4%** of a real build's
wall-clock rather than 0.2%. Both original figures were labelled
honestly — the throughput as derived, the ratio as fixture-specific —
and both were wrong by about 4× and 10× respectively. A correct label on
a wrong number still ships the wrong number.

### The probe measured a kernel filter twice before it measured a budget

Worth recording because the failure was indistinguishable from a product
defect. The first two runs put the fresh target dir at
`/tmp/m1072-probe-target` and reported:

```
paths_total=496 paths_hashed=419 bytes_hashed=9971025
total write entries: 0        (across 582 invocations, 358 crates)
```

Zero write-set entries for an entire release build, with
`events_dropped=0` and `completeness: complete`. That reads as a capture
defect in m210/m213 — and it is m213 working exactly as designed:
`Ephemeral` filters the `/tmp/` prefix in-kernel
(`waybill-common/src/filter.rs:139`), so every artifact write was dropped
before userspace saw it. The same build with the target dir under
`/waybill/` produced 3,432 write entries and 839 MiB.

Three things made it recoverable, all of them cross-checks the first
version of the probe did not have:

1. **cargo's own `Compiling` count** as ground truth for how much work
   the build did;
2. **artifact bytes left in the fresh target dir**, which bounds what a
   complete capture would have faced — 884 MiB against 9.97 MB hashed,
   i.e. 1%, is not a plausible reading of an ample budget;
3. **write-set entry count**, asked directly, because #1144 exists to
   resolve write-set entries and zero of them means the probe measured
   nothing about its own subject.

`budget-probe.sh` now refuses a `/tmp` or `/var/tmp` target dir with exit
2 rather than printing a number, and prints all three cross-checks beside
the result. The lesson is the one already in the project's memory in a
different costume: a number that looks like "plenty of headroom" and a
number that means "the experiment measured nothing" are the same number,
so the probe has to carry its own positive control.

## Q7. Is the trace's observation tier stable across runs of one build, and does SC-008 hold on a real workspace?

**No to the first. Not reliably, to the second.** Measured by
`budget-probe-paired.sh` after Q6 revised the budget, because Q6's single
runs raised both questions without being able to answer either.

### Capture varies between identical runs, and nothing discloses it

Three consecutive `--release --workspace --all-targets` builds, same
container, same relocated toolchain, fresh target dir each, cargo
reporting `Compiling` 392 times in all three:

| # | invocations | paths_total | read entries | bytes hashed | `events_dropped` | `ring_buffer_overflows` |
|---|---|---|---|---|---|---|
| 1 | 1,261 | 22,447 | 174,898 | 1,953,456,369 | **0** | **0** |
| 2 | 1,311 | 23,200 | 184,482 | 1,996,591,335 | **0** | **0** |
| 3 | 1,311 | 23,200 | 184,216 | 1,996,591,239 | **0** | **0** |

Run 1 is missing **50 compiler invocations and 753 paths** relative to the
other two, and every integrity counter says nothing was lost. Runs 2 and 3
agree on `paths_total` and `paths_hashed` exactly, and still differ by 266
read entries and 96 bytes.

A wider spread appeared in the four-run `F R R F` set, where `paths_total`
ranged **15,029 → 23,200** across runs of the same build (two per layout).

**This is silent, not disclosed** — and the reason is narrower than it
first looked. Of the two columns above, only `ring_buffer_overflows` is a
measurement: m212 wired it to the per-CPU maps, and it genuinely read
zero, ruling out kernel-side reserve failure. `events_dropped` is a
literal `0` on this path, deferred to #618 with the deferral stated at
`waybill-cli/src/cli/scan.rs:713`. And `completeness` could not have said
anything else: `derive_completeness`
(`trace/compiler_pipeline.rs:517`) returns `Degraded` only when
`events_dropped > 0`, while the sole caller of `record_events_dropped` in
the tree is the unit test at `compiler_pipeline.rs:844` — so the
`Degraded` arm is unreachable in production and the test proving it *can*
fire is the only thing that ever reaches it.

So the trace did not fail to report loss; the reporting path for this
class of loss was never built. That leaves #618's second drop path — an
event written to the ring buffer that the drain loop does not consume
before the child exits and the programs unload — as the leading
candidate, with its first path measured at zero. Filed as #1185 with that
narrowing; the probe's `events_dropped` column should be read as "not
instrumented", not as "zero loss".

### Why this matters beyond #1144

It is not a close-time problem. The close-time pass is read-only over the
observation tier and faithfully reports whatever reaches it; SC-005a
promises determinism *given the same observation tier*, and that premise
is what these runs break. The consequences land on the consumers:

1. **m210/m212's completeness claim.** `completeness: complete` plus zero
   drop counters is the strongest statement this predicate makes, and it
   was true of a trace that saw 96% of another run of the same build.
2. **#1142, the SBOM wiring.** C130 (`waybill:source-read-set`) exists to
   attribute source files to a built component. If the read set moves by
   5% between runs, so does that annotation, and two SBOMs of one build
   differ for a reason no field in either document explains. Worth
   deciding before the wiring lands rather than after.
3. **Reproducible-build verification.** Q5 already established that
   payloads cannot be byte-identical across traces. Q7 is the stronger
   statement: the *set* is not stable either, so even a normalised
   comparison has no fixed baseline on a build this size.

Not investigated further here — root-causing it means instrumenting the
ring-buffer drain and the exec path, which is m210/m212 territory, not
#1144's. Recorded so the next person starts from a measurement instead of
an assumption, and filed as **#1185** with the three-run table, the
layout dependence, and the two reproduction traps.

### SC-008's 5% ceiling is exceeded in 2 of 9 observations

Every `--workspace --all-targets` observation taken, both layouts, three
containers:

| source | layout | wall (s) | pass (ms) | ratio |
|---|---|---|---|---|
| standalone | filtered | 685.019 | 12,734 | 1.8% |
| paired A #1 | filtered | 204.820 | 3,484 | 1.7% |
| paired A #4 | filtered | 190.313 | 2,901 | 1.5% |
| paired A #2 | relocated | 175.096 | 4,330 | 2.4% |
| paired B #1 | relocated | 685.358 | 17,239 | 2.5% |
| paired B #3 | relocated | 857.598 | 24,029 | 2.8% |
| paired A #3 | relocated | 524.688 | 16,743 | 3.1% |
| standalone | relocated | 118.927 | 6,291 | **5.2%** |
| paired B #2 | relocated | 345.142 | 24,812 | **7.1%** |

The first overrun looked like an outlier produced by the fastest
wall-clock in the set, and an earlier revision of `cost.md` said so. The
7.1% retired that reading: its wall-clock, 345 s, sits mid-range.

**The mechanism is not noise, and it is worth stating precisely.** The
pass's absolute cost on this build is **6.3–24.8 s** for ~2 GB hashed.
The ratio is that over a build whose duration ranged 118.9–857.6 s for
*identical work* on this host. So the criterion is violated exactly when
the host is fast:

> a ratio-of-wall-clock ceiling penalises a quick build for being quick,
> while the pass's own cost is unchanged or better.

A host that compiles this workspace in three minutes would see ~13% with
the pass behaving perfectly. That is a property of how SC-008 was written,
not of the pass.

### Not re-baselined

`spec.md` requires an overrun to be recorded here and the ceiling
re-decided deliberately. This entry is the record; the ceiling is
**unchanged** pending that decision. The options, with the measurement
each would rest on:

1. **Keep 5% and accept 2-in-9 overruns on this host.** Defensible only
   if the reference host is defined and is not this VM — its throughput
   spread is 80–479 MB/s, 6×, which no criterion can be quoted against.
2. **Re-express the ceiling in absolute terms** — e.g. the pass adds no
   more than N seconds per GB hashed. Measured: 3.2–12.4 s/GB here. This
   survives a fast host, which is the failure mode above, and it is the
   form the pass can actually control.
3. **Keep the ratio but quote it against a defined reference class**, as
   m669's bench harness does (Linux x86_64 GHA, median-of-5). Then
   measure it there, not on Colima.

My recommendation is (2), with (3) if a ratio is wanted for operator
communication. Both require a measurement this file does not yet have:
`budget-probe-paired.sh` on the m669 reference class.

### Hashing throughput: no single figure

Nine samples on one host, 80–479 MB/s — a 6× spread. The ~478 MB/s in
`cost.md`'s original "Derived, not measured" section happens to sit at the
top of that range, and the ~120 MB/s that replaced it sits near the
bottom. **Both were single-point readings of a quantity that does not hold
still here**, and quoting either as "the" throughput was the same mistake
twice. No duration bound for a full budget spend is quoted anywhere as a
result.

## Q8. Is the loss Q7 found actually undisclosed, and is the drain window the cause?

Q7 found that two traces of one build observe different amounts while both
integrity counters read zero, and #1185 was filed from it naming the userspace
drain window (#618) as the leading candidate. Q7's evidence was comparative —
run against run — so it could show that capture *varies* but not where the
events went. `ringbuf-accounting-probe.sh` tests the hypothesis against a
workload with exact ground truth instead.

The control opens N distinct pre-created files; each `open()` fires both the
`openat2` and `do_filp_open` kprobes, so the trace must record 2N events. The
files sit under `/root/probe/` because that prefix matches none of the 19 m213
filter patterns. An earlier revision used `/etc/hostname`, which is a
`CAT_SYSTEM` prefix, and measured zero for the wrong reason — the same trap
Q6 hit with a target dir under `/tmp`, and the one #1185 warns about in its
reproduction notes. That makes three times a filter has been measured instead
of the thing intended; the prefix list is worth checking first, every time.

| scenario | expected | recorded | events_dropped | ring_buffer_overflows |
|---|---|---|---|---|
| seq, 100k opens, rb=64 KiB | 200,000 | 200,000 | 0 | 0 |
| seq, 100k opens, rb=64 MiB | 200,000 | 200,000 | 0 | 0 |
| 8-way, 15k each, rb=64 MiB | 240,000 | 240,000 | 0 | 0 |
| 8-way, 100k each, rb=8 MiB | 1,600,000 | ~766k | 0 | ~834k |
| exec-heavy, 5,000 execs | 10,000 | 10,000 | 0 | 0 |

### No undisclosed loss was reproducible

Across every configuration — a 64 KiB ring buffer against 200k sequential
events, 240k events from eight producers exiting at once, 5,000 `fork`+`execl`
children — every probe-path event was either recorded or counted. Loss
appeared only when the buffer genuinely overflowed, and then
`ring_buffer_overflows` covered the shortfall.

**#618's path 2 never fired.** That is the substantive result. #618's premise
quotes #614's ~1 ms/event; #991 cut that to ~13 µs/event and m213 removed the
bulk of the traffic, so the backlog it was filed to instrument appears
unreachable on this workload class. #618 anticipated exactly this — *"#616 is
the actual throughput fix and should reduce backlog dramatically; this counter
is transparency for cases where backlog still occurs after #616 lands"* — and
measurement says those cases are rare to absent here. Wiring the counter
remains worth doing because a hardcoded `0` is a claim the code cannot
support, but it will not explain Q7.

### The accounting relation is an inequality, and saying otherwise was wrong

Two runs produced `recorded + ring_buffer_overflows == expected` exactly, and
that was written up as an exact identity. The probe's third run failed it by
29 events. The counter is a **global** count of reserve failures while
`recorded` is filtered to `/root/probe/`, so the two are not commensurable:
the control's own loader and libc opens are also eligible to be dropped. The
two exact matches were luck — those non-probe events occur at trace start,
before the buffer fills. The honest form is `recorded + counters >= expected`,
and that is what the probe now asserts. A claim of exactness that holds twice
and fails on the third trial was never a measurement.

### What this means for #1185

#1185 reports a deficit with `ring_buffer_overflows == 0`. Here all
buffer-pressure loss *is* disclosed, and no configuration produced undisclosed
loss at all. So its deficit is not buffer pressure, and on this evidence not
the drain window either. That re-weights its own third candidate — exec events
missed before the probe observes the child. Its deficit was 50 *compiler
invocations*, a process-event quantity; this probe's clean exec rows cover
`fork`+`execl` of a trivial binary, which is a far weaker test than hundreds
of concurrent rustc processes.

### Not measured

- Anything resembling a compiler pipeline. The loads here are `open()` loops
  and trivial execs: no large reads or writes, no page-cache pressure, no
  rustc, no `cargo`. `budget-probe-paired.sh` with `SEQUENCE="R R R"` remains
  the right instrument for Q7's scenario and this probe does not replace it.
- `events_dropped` itself, which is hardcoded `0` (#618) and so appears in
  every row above as a constant rather than an observation.
- Whether #1143 (tracer self-exclusion under a PID namespace) reproduces. The
  `waybill-v1` predicate exposes no `comm` on file operations, so events cannot
  be attributed to the tracer process by path alone. Inconclusive, not negative.
- The default `--ring-buffer-size` is 8 MiB — ~26,000 events at ~320 B per
  `FileEvent` — not the 128 MiB declared in `maps.rs`. Worth knowing when
  reading any trace taken at defaults; not investigated further here.
- `scripts/ebpf-integration-test.sh` mounts neither debugfs nor tracefs, so
  `sched_process_exec` / `_fork` / `_exit` and `sys_enter_chdir` fail to attach
  with "tracefs not found". The repo's own container harness therefore likely
  runs without those tracepoints. Noted, not chased.
