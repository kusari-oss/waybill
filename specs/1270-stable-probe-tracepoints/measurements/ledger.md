# Capture ledger — what is known about the trace, and how

Every row is either **measured** (a command produced it, and the command is
named), **inferred** (it follows from measured rows but was not itself
observed), or **assumed** (believed, never checked).

The ledger exists because six months of work on this feature produced
numbers that do not reconcile — across runs, across architectures, or with
each other — and the reason turned out to be that the measurements had an
uncontrolled variable in them rather than that any single number was wrong.
A ledger makes disagreement specific: "row F8 is wrong" is resolvable,
"I don't buy the story" is not.

Rules:

- A row moves from **inferred** to **measured** only when a check
  distinguishes it from the alternatives. Corroboration is not
  distinction: a result consistent with two mechanisms establishes
  neither.
- A row that is contradicted is struck through and kept, with what
  replaced it. Deleting a wrong row loses the reason the next person
  would otherwise re-derive it.
- Predictions are written **before** the run that tests them. A
  prediction recorded afterwards is a description.

Host for every arm64 row: Colima aarch64, 2 CPUs, Ubuntu 24.04, kernel
6.8, `Dockerfile.ebpf-test`. x86-64 rows come from the GitHub Actions eBPF
lane.

---

## Scope: what the capture admits

| # | fact | status | source |
|---|---|---|---|
| F1 | Tracing a single-process command captured **29** operations: **3** from the traced command, **26** from `comm=waybill` — the tracer itself | measured | `#1185 scope` block, `scripts/ebpf-integration-test.sh`, image `waybill-ebpf-test:scope` |
| F2 | Tracer exclusion writes `tracer_pid: std::process::id()` (`trace/loader.rs:112`) and compares it against `bpf_get_current_pid_tgid() >> 32` (`waybill-ebpf/src/helpers.rs`, `should_trace`) — two different pid namespaces | measured | source read |
| F3 | One `cargo` process is pid **35** to the tracer and pid **529990** to the kernel | measured | `runscope.log`, `Child started pid=` vs `comm=cargo` event |
| F4 | `PID_FILTER` (256 entries, "PIDs to trace") is written at `trace/loader.rs:89` and read by no eBPF program | measured | `grep -rn PID_FILTER waybill-ebpf/src` |
| F5 | `TraceConfig.trace_children` is written at `trace/loader.rs:114` and read by no eBPF program | measured | `grep -rn trace_children waybill-ebpf/src` |
| F6 | `COMPILER_INVOCATIONS` is read only by `compiler_exec.rs`. No file-op program gates on it, though the map's doc comment claimed "Consumed by every file-op kprobe (see `file_ops.rs`)" | measured | `grep -rn COMPILER_INVOCATIONS waybill-ebpf/src`; comment corrected in `maps.rs` |
| F12 | The identical control command captured **239** operations, then **415**, minutes apart on one image | measured | findings.md runs 9 and 10 |
| I1 | The namespace mismatch in F2 is **why** F1's 26 appeared — the tracer exclusion has never fired in a containerised run | **measured** (was inferred) | the P1/P2 A/B below: one variable, three runs each |
| F14 | The tracer's own captured activity is **exactly 26 operations**, in all three container runs. Deterministic, not a sample | measured | P2 ×3 |
| F15 | Ambient capture from unrelated host processes is present in **both** configurations and varies by orders of magnitude between runs seconds apart: **0, 174, 16** (container ns) and **0, 396, 0** (host ns). Observed comms: `systemd`, `bash`, `locale`, `locale-check`, `docker`, `containerd`, `systemd-journal`, `systemd-network`, `systemd-timesyn` | measured | P1 ×3, P2 ×3 |
| F17 | m211 did not decide against subtree scoping. Its T006d records the *intent* — "the 'run a command and trace it' path auto-captures the entire subtree" — achieved by turning the filter **off**, because no subtree-aware filter existed. Machine-wide capture is a side effect, not the goal | measured | `specs/211-vfs-open-verifier-fix/tasks.md:50`; `scan.rs:446` |
| F18 | A subtree tracker already exists in userspace (`trace/pid_tracker.rs`): root pid plus descendants, discovered by polling `/proc/<pid>/task/<tid>/children`, with `add_pid` for fork events. It is not consulted in command-execute mode — `scan.rs:708` passes an empty set | measured | source read |
| F19 | `PidTracker`'s pids come from `/proc` inside the container, so they are container-namespace, while kernel events carry initial-namespace pids (F3). The two sets barely intersect | measured (F3) + **inferred** that this is why m211 saw *empty* `file_access.operations[]` rather than root-only | see P5 |
| F20 | `/proc`-polling subtree discovery cannot see a process that is born and dies between polls. A build is made of short-lived compiler invocations, and m1070 already documents the same race for `/proc/<pid>/cwd` being gone before a 6 ms compile is drained | measured (source + m1070 research R2) |
| F16 | `/proc/self/status` `NSpid` inside the container reports **one** entry — the container's own view. The initial-namespace pid is not reachable from inside, so a fix cannot read it there | measured | `docker run --entrypoint bash … -c 'grep NSpid /proc/self/status'` |

**Consequence if I1 holds:** every figure this feature has produced
includes the tracer's own file activity, and that activity scales with how
much the tracer drains, which scales with event volume. That is a feedback
term in every measurement taken so far.

## Step 1 — the subtree mechanism, measured with the gate still off

| # | fact | status | source |
|---|---|---|---|
| F21 | The handshake works. `traced subtree seeded: the root announced itself traced_pids=1 waited_ms=0`, in three separate traces, every time | measured | image `waybill-ebpf-test:wip` |
| F22 | Fork propagation reaches past the root. Fixture build: **inside=738, outside=435** opens. A root-only subtree could not produce 738 | measured | same run |
| F23 | The handshake does not pollute the counters it passes through. `#618 control: kernel_null_path after 5 forced openat(NULL): 5` — still exactly 5, so discarding the marker before that branch works, and `kernel_exit_without_request` saw no extra orphan | measured | same run |
| F24 | `outside=435` for the fixture build, against an ambient range of 0–396 measured earlier (F15). The gate has a large and variable term to remove | measured | same run |
| F25 | `vfs_read` / `vfs_write` are declared and **not attached** (`trace/loader.rs:240`), so opens are the only file-read source the gate must cover. `fs_syscalls`'s own `FILE_EVENTS` use is `emit_dir_change` — chdir/fchdir records userspace needs for path resolution, not reads | measured | source read |

### The dry-run figure was understating the gate, and only a cross-check found it

| # | fact | status | source |
|---|---|---|---|
| F26 | A single-process probe reports **26 `waybill` operations** in its attestation and **`kernel_outside_traced_subtree = 0`**. Reproduced twice | measured | `reconcile.sh`, image `wip` |
| F27 | Those 26 are `op=read` on `/proc/version_signature` and similar — aya's own kernel-version probing while `load_and_attach` runs | measured | same probe, grouped by comm and operation |
| F28 | They are counted and then subtracted: `sys_enter_openat` attaches first and fatally, so the tracer's remaining load-time reads increment the slot, and `read_kernel_event_drop_baseline` (`scan.rs:322`) then captures those counts as the baseline, which `merge_kernel_event_drops` removes | measured (arithmetic closes: 26 counted − 26 baselined = 0) | source read + F26 |

The attach-window baseline is correct for one slot and wrong for the other:

| slot | what baselining removes | correct |
|---|---|---|
| `kernel_exit_without_request` | orphans from the attach window — an artifact of when the probes went live | yes |
| `kernel_outside_traced_subtree` | the tracer's **real** load-time reads, which a subtree gate will drop | **no** |

So **`outside=435` on the fixture build is a floor, not the total**, understated
by whatever the tracer reads at startup. The slot is now read raw.

The way this was found matters more than the fix. It came from checking two
independent views of the same events against each other — the attestation's
operations grouped by `comm`, against the kernel's own counters — and not
from a theory. Had the gate shipped on the baselined figure, it would have
been built on a number wrong in a direction nothing predicted. My first
instinct was to explain the gap with `vfs_read`/`vfs_write`; those are
declared and **not attached** (F25), so that explanation was of a mechanism
that is not running.

### Why the step-1 harness assertion did not run

Its first version grepped `inside=` out of the trace log. The container
colours `tracing` output, so the log holds `inside` and `=` separated by
escape sequences and the pattern never matched. Under the harness's
`set -euo pipefail` the no-match made the assignment exit 1, which killed
the script **before the FAIL branch below it could print** — a check whose
failure path was unreachable because the line feeding it killed the shell
first.

Two consequences, both fixed:

- The figure moved into the attestation (`traced_subtree_opens`), because
  an assertion built on log text broke the first time it ran, and because
  the log interleaves every trace in the run so no figure in it can be
  attributed to a particular trace.
- My own `RUN_EXIT=$?` after `docker run … | sed > log` was capturing
  **sed's** status, not the harness's — the exact trap CLAUDE.md names.
  It read 0 while the harness had died.

Neither of those was a product defect. Both were instruments reporting
success for a run that did not happen, which is the failure this ledger
exists to make visible.

## Step 2 — the gate, and the arithmetic that corrected the dry run

| # | fact | status | source |
|---|---|---|---|
| F29 | With the gate armed the capture is scoped: `SCOPE: CLEAN`, 3 operations, all from the traced command, **0** foreign | measured | image `wip`, harness PASSED, `HARNESS_EXIT=0` |
| F30 | The gate drops nothing the build needs. Fixture build `file_ops=102`, **identical** to the pre-gate figure, with m210, m1070 SC-001 and the m1072 read/write checks all passing | measured | same run |
| F31 | `sched_process_exit` fires per **task**, not per process. Reclaiming `TRACED_PIDS` by tgid meant cargo's first worker-thread exit removed cargo from the subtree: propagation stopped, `inside` fell 738 → **26**, `file_ops` 102 → **8**, and `compiler_pipeline` vanished from the attestation | measured | first gated run |
| F32 | `PID_TO_PPID` inserts by the new **task's** id and removed by the exiting task's **tgid**. A thread exit therefore deleted its own process's parent link while that process was still running, and thread entries were never reclaimed. Pre-existing, since m210 | measured (source) | `compiler_exec.rs` insert vs remove |
| F33 | **738 + 435 = 1173.** The dry run's `inside`/`outside` split and the gated run's `inside` sum exactly, so the dry run's `outside=435` was **the build's own opens**, wrongly counted as foreign by F31's eviction — not ambient traffic | measured | dry run vs gated run |
| F34 | `outside=28` after the gate, equal in both the fixture build and a single-process probe, and equal to the tracer's startup reads measured in P8. The gate does **not** reduce `outside` — it counts before returning — so 435 → 28 is F31's fix plus near-zero ambient, not a gate effect | measured | same run |

### H6 verified, and a third consequence of the namespace bug

| # | fact | status | source |
|---|---|---|---|
| F35 | The unarmed carve-out works: with no handshake nonce the kernel gate does not drop. A `--target-pid` trace counted `inside=0 outside=1917` and dropped none of them in-kernel | measured | `h6.sh`, image `wip` |
| F36 | `--target-pid` nevertheless captures **nothing** in a container: `user_pid_not_admitted: 1304`, `Collection done net=0 file=0`, `Error: no dependency activity observed`. The userspace filter compares kernel init-ns pids against `target_pids` read from the container's `/proc` | measured | same run |
| F37 | **Pre-existing.** The same probe against `waybill-ebpf-test:scope` — built before step 1 and before the gate — produces the identical result, down to the same count of 1304 | measured | A/B against the pre-change image |

F36 confirms P5 and with it F19's inference, by a route the original
prediction did not anticipate: `--target-pid` has `filter_by_pid` enabled
by default, so the broken comparison runs there without needing to be
forced. The namespace mismatch now has three measured consequences —
tracer exclusion (I1), the userspace pid filter (F36), and m211's original
"empty `file_access.operations[]`" symptom it was working around (F17).

The carve-out is still the right design — do not gate what cannot be
scoped — but it preserves a mode that is already broken in containers.
That is a follow-up with a measured defect behind it rather than a
suspicion.

### Struck by F33

My statement that `outside=435` was "the ambient term" and "what the gate
would drop" was wrong in both halves. It was mostly the build's own opens,
and the gate did drop exactly them — which is what the first gated run did,
removing `compiler_pipeline` entirely.

The dry-run-before-behaviour ordering was still right, and it still produced
a wrong number, because the mechanism being measured had a defect the
measurement could not see. What caught it was a **pre-existing** assertion
failing on the gated run. The new scope check reported `CLEAN` on that same
run: a scoped capture, zero foreign operations, and the build silently
gutted. Trusting the assertion written alongside the feature would have
shipped it.

Twice in this session the thing that found the defect was two independent
views of the same events disagreeing — the attestation against the kernel
counters (F26–F28), and the scope check against m210 (F31).

## Loss: what the capture drops

| # | fact | status | source |
|---|---|---|---|
| F7 | Forcing the in-flight-open map to refuse every request: `kernel_pending_insert_failed` **75**, `kernel_exit_without_request` **75**, operations captured **0** | measured | `#1185 control`, `WAYBILL_TRACE_FORCE_PENDING_INSERT_FAIL=1` |
| F8 | `pending_open_entries_at_close` on the fixture build: **2**, then **0**, of 4096 | measured, n=2 | runs 11 and 12 |
| F9 | `event_drop_reasons` is `{}` on every arm64 run to date | measured | runs 3–12 |
| F10 | A capture that lost **every** open still reports `completeness: complete`, with no `dropped` and no `reason` | measured | run 12, forced control |
| F11 | **91** sites in the eBPF programs where control leaves early or discards a result; **25** increment a named counter; the remainder is unclassified | measured | `grep -rnE 'return;\|let _ = \|unwrap_or\|is_err()\|else {' waybill-ebpf/src/programs/*.rs` |
| F13 | x86-64 reports `kernel_exit_without_request` ≈ **1113**; baselining the attach window removed ~124 of 1237 | measured | findings.md runs 6–8 |

## Phase 5 — the claim

| # | fact | status | source |
|---|---|---|---|
| F42 | `CompilerPipelineAggregator::record_events_dropped` is called from **no production path** — only from four unit tests. `TraceStats.events_dropped` is the literal `0` with the comment "Not measured" | measured | `grep -rn record_events_dropped waybill-cli/src`; `waybill-cli/src/cli/scan.rs` |
| F43 | Therefore `CompletenessState::Degraded` was **unreachable in a real trace**: `derive_completeness` read `attach_late`, `probe_attach_failed` and `events_dropped`, and the third is always zero. The only states a real trace could report were `Partial` and `Complete` | measured | `waybill-cli/src/trace/compiler_pipeline.rs` |
| F44 | Seven counters the trace **does** measure and reports in `trace_integrity` were not inputs to the claim: `ring_buffer_overflows`, the five kernel loss slots, and `pending_open_entries_at_close`. `scan.rs` logs "the capture is a floor, not a total" from `total_lost()` and then writes `complete` into the document | measured | same |
| F45 | `EventDropCounts::total_lost()` already existed and already excluded `kernel_outside_traced_subtree`. The exclusion the claim needed was written before the claim was wired; nothing consumed it except a log line | measured | `waybill-cli/src/trace/counters.rs:519` |
| F46 | `user_no_invocation_mapping`, which the harness comments describe as "non-zero on a healthy host", **is not an `EventDropReason`** and exists only in comments. So it cannot contaminate the loss total, and the comment describing it as a live false-alarm risk is stale | measured | `grep -rn user_no_invocation_mapping waybill-common waybill-cli` |

Each of F42–F44 is a reading of the source rather than a run, and all three
are mechanically checkable by the greps named — which is the distinction
that matters, because F10 is the *measured* consequence of them and was
already in the ledger before the cause was.

## Struck

| struck | replaced by |
|---|---|
| ~~"All 26 foreign operations are the tracer's own. Not ambient host processes."~~ | F15. That was read off **one** harness run that happened to capture no ambient activity. Three further runs show ambient capture of 0, 174 and 16 operations in the same configuration. The 26 figure was right; "not ambient host processes" was a single quiet sample reported as a property |
| ~~P1's first run showing 0 foreign operations means the tracer was the only foreign source~~ | F15. Same error, same cause: run 2 of the identical configuration captured 396 |

## The capture decomposes into three terms

Measured separately for the first time, tracing a single-process command:

| term | operations | behaviour | removed by |
|---|---|---|---|
| the traced command | **3, 3, 3** | deterministic | — it is the subject |
| the tracer itself | **26, 26, 26** | deterministic | a working tracer exclusion (namespace-correct) |
| ambient host processes | **0 … 396** | varies by orders of magnitude between runs seconds apart | subtree scoping; the tracer fix does nothing for it |

This answers Q2 and changes the plan: fixing tracer exclusion alone is
**not** sufficient. The ambient term is both larger and far more variable
than the tracer term, so Phase 2b moves from "only if the residual says so"
to required.

It also explains F12 without needing a theory: the identical control
reporting 239 then 415 operations is the ambient term, now observed over
six runs rather than inferred from two.

## Design conclusion for Phase 2b: scope in the kernel, not userspace

From F17–F20, with F19's inference labelled:

- **m211 is not an obstacle.** It wanted subtree capture and removed the
  filter to get descendants. Building a subtree filter delivers its intent
  rather than reversing its decision.
- **Userspace is the wrong layer, for two independent reasons.** Its pids
  are in the wrong namespace (F19), and `/proc` polling structurally
  cannot see short-lived processes (F20) — which is what a build consists
  of. Fixing the first leaves the second.
- **Userspace filtering would not fix capture quality either.** The
  kernel-side drop counters, the 4096-entry in-flight-open map and the
  ring buffer are all consumed before userspace sees an event, so ambient
  traffic keeps competing for them whatever userspace discards (P6).
- **The kernel has the right machinery already, and it is proven live.**
  `try_sched_process_fork` fires on **every** fork with no filter — that
  is how `PID_TO_PPID` is populated — and already copies a parent's
  `COMPILER_INVOCATIONS` entry to its child. A traced-descendant set is
  the same propagation in the same function.
- **Seeding must be namespace-free** (F16): a self-identifying handshake,
  where the tracer performs one distinctive syscall after attach and the
  kernel records whatever pid *it* sees.

## Phase 3 — the protocol, and what it caught

| # | fact | status | source |
|---|---|---|---|
| F38 | With the capture scoped, everything it *reports* is stable across N=10: `compiler_invocations` 9, `rustc_invocations` 7, `traced_subtree_opens` 1173, `unresolved_relative_opens` 5, `open_failures_discarded` 599, `ring_buffer_overflows` 0, `pending_open_entries_at_close` 0. The only field that still varies is `outside_traced_subtree` — what the capture declines | measured | `measurements/baseline.sh`, N=10 |
| F39 | The gate manufactured orphaned exits. An out-of-subtree open returns at syscall entry and stores no request, so its exit has nothing to join: one run of ten caught an ambient burst and reported **324** declined opens alongside **151** orphans, the two outliers in the same run, where the other nine reported 28 and 0 | measured | per-run rows, N=10 |
| F40 | Fixed, verified against forced load rather than a chance burst: **~33,000** declined opens across three runs produce **0** orphans, with `admitted` unchanged at 1173. A thousandfold over ambient, so the control could have shown the defect | measured | `p11.sh`, forced `find /usr` outside the subtree |
| F41 | `file_operations` is **102–103**, not deterministic. The N=10 table reported `distinct: 1`; six further runs contain three 103s, and a no-noise control shows the extra operation is not caused by ambient load | measured, n=16 | baseline N=10 + 6 probe runs |

### Struck by F41

My own Phase 3 report called `file_operations` deterministic at 102 on the
strength of ten runs. It is not. The protocol was built because single runs
are not measurements, and then its first output was over-read the same way
— one rung up, with N=10 instead of N=1.

The stability column is itself a measurement with an N attached. Quoting
"deterministic" from it requires stating that N: `traced_subtree_opens`
has held at 1173 across 16 runs, `file_operations` has not held across 16.

### Why the protocol was worth building before the baseline mattered

F39 needed three things at once, and no earlier approach had them:
repetition (nine of ten runs read a clean 0), a fixed command (so an
outlier was not merely a different measurement), and the raw per-run rows
(the summary said `max 151`; only the rows put 151 and 324 in the same
run, which is what identified the mechanism rather than the anomaly). The
third existed only because the first N=10 run lost its rows inside a
`--rm` container.

## Open questions, in dependency order

| # | question | blocked on |
|---|---|---|
| Q1 | Is I1 true? | Phase 1 |
| ~~Q2~~ | ~~With the tracer excluded, is there any foreign capture left?~~ **Answered: yes, 0–396 operations, highly variable** | — |
| Q3 | Can the capture be scoped to the traced subtree, and why did m211 clear the userspace filter? | nothing — now known to be required |
| Q7 | Do the high-ambient runs correlate with a longer trace window, or with host activity? Not needed for any decision above; recorded so it is not mistaken for settled | — |
| Q4 | What is the baseline, with variance, on a scoped capture? | Q2/Q3 |
| Q5 | Which of F11's 91 sites are losses? | nothing — static work |
| Q6 | What must the attestation be able to claim? | Q4, Q5 — and it is the maintainer's call |

## Predictions

Recorded before the run that tests them.

| # | prediction | tests | outcome |
|---|---|---|---|
| P1 | Run outside a container (tracer's namespace **is** the initial one): `comm=waybill` operations = **0** | I1 | **CONFIRMED** — 0, 0, 0 |
| P2 | Same probe in a container: stays ≈ **26** | I1 | **CONFIRMED** — 26, 26, 26 |
| P3 | After fixing tracer exclusion alone, residual foreign operations > **0**. A **0** means something else filters that this model does not account for | Q2 | **CONFIRMED** — 0, 396, 0 with the exclusion firing; ambient capture survives it |
| P4 | After fixing tracer exclusion, x86-64's `kernel_exit_without_request` falls substantially. If it does not, the tracer was not the main contributor there | F13 | pending |
| P5 | Forcing `filter_by_pid` on in command-execute mode yields ≈ **0** operations in a container and > 0 with `--pid=host`. Root-only operations instead would mean m211's own diagnosis (a pid-exact filter) was the cause and F19 is not implicated | F19 | **CONFIRMED** by F36 without needing the forced variant: `--target-pid` enables `filter_by_pid` already, and captures zero with 1304 events rejected as not-admitted |
| P6 | A userspace-only subtree filter leaves `kernel_exit_without_request` unchanged, because the kernel counters are incremented before userspace sees an event | F13, F20 | pending |
| P8 | With the slot read raw, the single-process probe reports `kernel_outside_traced_subtree` ≈ **26**, matching the 26 `waybill` operations in its own attestation. The two views reconcile | F26–F28 | **CONFIRMED** — 0 → **28**, twice. The residual over 26 is structural: the counter increments at syscall entry, upstream of both `discard_failed_open` and the noise classifier inside `emit_open`, so counter ≥ operations is the expected direction |
| P9 | With the gate armed, the scope check reports `CLEAN` | the gate | **CONFIRMED** — 3 ops, 0 foreign |
| P10 | The fixture build still reports its invocations and read/write sets | regression risk | **FAILED first, then CONFIRMED** — F31 was the cause; `file_ops=102` after the fix |
| P11 | After the fix, `exit_without_request` stays 0 under heavy out-of-subtree open traffic | F39 | **CONFIRMED** — 0 across three runs at ~33k declined opens each. An earlier run that read 0 was **not** evidence: it used a stale image and saw no burst, so it could not have shown the defect |
| P7 | Step 2's gate must cover the chdir/fork lineage records too, not only opens. Gating opens alone leaves ambient lineage traffic flowing; gating lineage *without* the subtree established first would break cwd inheritance for traced processes | F25 | pending — design constraint, to be tested by the gate |

## Reversals

Recorded because a plan that quietly changes direction cannot be reviewed.

| what was said | why it changed |
|---|---|
| "The x86-64 lane is the decisive next step; push #1313" | Every figure it would produce carries the F1 feedback term, so it would be a fourth uninterpretable measurement |
| "Goal A (a complete trace) is unreachable; the capture not being a function of its subject is structural" | It is not structural. It is unwired filters (F4–F6) plus I1 |
| "The pending-open map saturating is the best lead for #1185" | F8 measured occupancy at 2 then 0 of 4096, which weakens it. Re-measure after the tracer is excluded, since the tracer's draining is inside that number |
| "Fix the map with an LRU type and an insert fallback" | Premature while F8 is contaminated by I1 |
