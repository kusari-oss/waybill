# Phase 0 Research: Capture that survives a kernel change

Feature: `specs/1270-stable-probe-tracepoints/` (spec-kit sequence 1270,
which is **not** an issue number) · Issue **#1252** · 2026-10-29

Every decision below is grounded in one of two things: code that exists
in this repository, cited by path and line, or the measurement in
`measurements/findings.md`. Where neither is available the item says so
and names the task that must establish it. No number here is inferred
from documentation.

---

## R1 — What exactly is being removed

**Decision**: remove four kernel-side programs and the loader helper
that attaches them.

| Program | Attached to | Site |
|---|---|---|
| `openat2_entry` | `do_sys_openat2` | `loader.rs:199` |
| `do_filp_open_entry` | `do_filp_open` | `loader.rs:208` |
| `tcp_connect` | `tcp_v4_connect` | `loader.rs:181` |
| `tcp_connect_ret` | `tcp_v4_connect` | `loader.rs:182` |

`attach_kprobe` at `loader.rs:336` has no other caller, so it goes too.
That is what makes FR-012 structural rather than a review habit: with
the helper deleted, no kprobe can be attached, and a test asserting the
helper's absence cannot be satisfied by a program that quietly
reintroduces one.

**Not removed**: `vfs_read_entry` and `vfs_write_entry`
(`file_ops.rs:113`, `:59`). They are declared but never attached — the
loader says so at `loader.rs:183-189` — so they are not a live
dependency on an unstable name. A kprobe target is resolved at attach
time, not at load time, so leaving them in the object costs nothing.
The spec's assumption records this asymmetry deliberately.

**Rationale**: the two open kprobes are the measured failure (#1252);
the two connect kprobes are the same class of defect not yet triggered,
and worse — `tcp_connect.rs:36-38` reads `struct sock` at hardcoded
offsets 4 and 12, so it depends on both an internal symbol *and* an
internal struct layout, with a comment admitting "offsets vary by
kernel version".

**Alternative rejected**: keep the kprobes as a fallback. Ruled out in
clarification Q2 — a fallback keeps the unstable dependency and its
cross-interface deduplication alive to buy coverage on no kernel anyone
runs.

---

## R2 — What replaces them

**Decision**: syscall tracepoints, enter paired with exit.

| Capture | Programs |
|---|---|
| file open | `sys_enter_openat` / `sys_exit_openat`, `sys_enter_openat2` / `sys_exit_openat2`, and on x86_64 `sys_enter_open` / `sys_exit_open`, `sys_enter_creat` / `sys_exit_creat` |
| network connect | `sys_enter_connect` |

Presence and argument offsets measured in
`measurements/findings.md` run 1: the `openat`, `openat2` and `connect`
pairs exist on aarch64 6.8, and `open`/`creat` do not exist there at
all. The x86_64 half of that table is an expectation until run 2 is
taken in the lane.

**Rationale**: a syscall tracepoint's name and argument layout are
UAPI. `do_filp_open` is an internal function whose presence as a traceable
symbol is not promised to anyone, which is why it stopped being one.

**Alternative considered — `fentry`/`fexit` on the same functions.**
Cheaper than kprobes and would permit `bpf_d_path` (see m211's note at
`file_ops.rs:265`). Rejected: it is the *same symbol dependency* with a
different attach mechanism, so it does not address the defect. Worth
revisiting only for the separate question of read/write capture on
already-open files, which this feature leaves out of scope.

**Alternative considered — one program on `raw_syscalls:sys_enter`.**
One attach point for every syscall, dispatching on `__syscall_nr`.
Rejected: it fires for every syscall on the host, which is a volume
multiplier on a path with a drop history (#614, m212, m213), and the
per-syscall tracepoints give typed argument offsets for free.

---

## R3 — How the request and its result are joined

**Decision**: follow the pattern m1070 already shipped in
`waybill-ebpf/src/programs/fs_syscalls.rs`, verbatim in shape.

That file is the template, not an analogy: `sys_enter_chdir` stores one
argument keyed by thread id in `PENDING_CHDIR`
(`fs_syscalls.rs:49-57`), `sys_exit_chdir` retrieves it, **removes it
unconditionally**, then checks `read_ret(&ctx) != 0` and returns early
if the call failed (`:59-71`). `store_rename` / `finish_rename` do the
same with a four-field struct. The constants `ARG0=16 ARG1=24 ARG2=32
ARG3=40 RET=16` at `fs_syscalls.rs:35-39` are the offsets this feature
needs, and the measurement confirms them on every tracepoint it printed.

Three details that file already settled, which the open join inherits:

1. **Unconditional removal before the result check.**
   `PENDING_CHDIR.remove` at `:65` happens before the early return, so
   a failed call leaves nothing
   behind. This is FR-005c's requirement, and it is already the
   established idiom rather than a new invention.
2. **Strings are read at exit, directly into ring-buffer memory.**
   `finish_rename` reserves first and reads into `(*event).old_path`
   because a 560-byte record does not fit on the 512-byte BPF stack
   (`fs_syscalls.rs:11-17`). User memory is still mapped at `sys_exit`,
   which runs on the same thread before returning to userspace.
3. **Post-reserve drops use `buf.discard(0)`.** `finish_rename` applies
   the noise classifier after reserving and discards on a match
   (`:217-219`). The open join needs the same, because the path is not
   known until after the reserve.

**Alternative rejected**: join in userspace. Clarification Q3 — it
roughly doubles open-family ring-buffer volume to trade one silent loss
for another.

---

## R4 — Where the flags come from, and the trap in it

**Decision**: read `flags` from the tracepoint's own argument slot for
`open`/`openat`/`creat`. For `openat2`, read `struct open_how.flags`
through a **user**-space read, not a kernel read.

This is the one place where porting the existing code verbatim produces
a bug that compiles. `try_openat2` at `file_ops.rs:184-191` reads
`how_ptr` with `bpf_probe_read_kernel`, and is correct to: a kprobe on
`do_sys_openat2` sees the pointer after the kernel has copied the
structure in. The `sys_enter_openat2` tracepoint sees the argument as
userspace passed it — `struct open_how *` in user memory, per the
measured format file. A kernel read of a user pointer fails, and the
existing code swallows the failure with `.unwrap_or(0)`, which means
**every openat2 open would silently be classified as a read**. Write
sets would lose entries with no error anywhere.

`sys_enter_openat` needs no dereference at all: `flags` is a direct
field at offset 32 (`ARG2`).

**Mitigation**: the classification is observable, so a task must assert
it rather than read the code. The fixture build writes files; a test
that an `O_WRONLY|O_CREAT` open through `openat2` lands in the write set
discriminates, and fails if the read helper is wrong.

---

## R5 — Address families, and why IPv6 is free on the wire

**Decision**: read `sa_family` from the `struct sockaddr` at
`sys_enter_connect` argument 1 (offset 24, measured), then read
`sockaddr_in` or `sockaddr_in6` accordingly.

No event-struct change is needed. `waybill_common::ip::IpAddr` already
carries `octets: [u8; 16]` plus an `is_v6` discriminant and already
exposes `new_v6` (`waybill-common/src/ip.rs:8-14`, `:33`); the current code
simply never calls it, because `tcp_v4_connect` is reached only by
IPv4 connections. FR-002a is a capture change, not a schema change.

**`conn_id` must keep its current meaning.** `tcp_connect.rs:48` uses
the timestamp as the connection id and inserts into `CONN_INFO`, which
the TLS uprobes read to correlate plaintext with a destination. The
replacement must populate `CONN_INFO` the same way or TLS correlation
breaks silently — the connections would still be recorded, just no
longer joined to their payloads.

---

## R6 — Connects keep attempt semantics, deliberately

**Decision**: capture the connect at `sys_enter_connect` only. Do not
add a `sys_exit_connect` program, and do not narrow connects to those
that succeeded.

**Rationale**: clarification Q3 narrowed *opens*. Nothing in the spec
asks for the same on connects, and the current behaviour is already
attempt-based — `tcp_connect_ret` at `tcp_connect.rs:94-99` reads the
return value and discards it, with a comment saying as much. Preserving
that is FR-004's "lose nothing" applied to the network path.

**Why it is called out rather than left implicit**: `connect` on a
non-blocking socket returns `-EINPROGRESS`, which is a connection in
flight, not a failure. An implementer who generalised the open join to
connects would drop exactly the asynchronous connects that a build's
HTTP client makes. The address is read at enter, where user memory is
mapped, so the exit tracepoint buys nothing here.

---

## R7 — The discard record (FR-004c)

**Decision**: two mechanisms, because they answer different questions.

1. **An always-on count** of discarded failed opens, via the m212
   `PerCpuArray<u64>` pattern (`maps.rs:143`, read by
   `trace/counters.rs:116`'s `read_percpu_sum`), surfaced in `TraceIntegrity`.
   Cheap, and it is the positive control: a zero discard count on a
   fixture build known to attempt failed opens means the exit
   tracepoint is not firing, which otherwise looks exactly like a clean
   run.
2. **An opt-in per-path record**, which is what SC-001b is verified
   against. Gated by a config map slot following the m213 `FILTER_WIDEN`
   pattern (`maps.rs:191-192`, one `PerCpuArray<u8>` read by
   `widen_system()` at `file_ops.rs:26-32`). When the slot is set, the
   kernel submits the discarded open as a distinct event type carrying
   the errno instead of dropping it; userspace logs each one and never
   admits it to a read or write set.

**Rationale**: Q4 requires attribution per entry, naming the path. A
count alone passes equally for a correct join and for one that also
drops successful opens. Reusing both existing patterns means no new
map kind and no new config surface.

**Alternative rejected**: always emit the failed opens and filter in
userspace. That is the userspace join Q3 rejected, reintroduced under
another name — it would restore the volume the kernel join exists to
avoid.

---

## R8 — Event volume, and why the count should fall

**Decision**: expect fewer ring-buffer events after the change, and
measure it rather than assert it.

Two reductions compound. First, `openat2_entry` and
`do_filp_open_entry` both fire for one `openat` call on a modern kernel
— `do_sys_openat2` calls `do_filp_open` — and both submit a full
`FileEvent` (`file_ops.rs:236`, `:342`). The read set is a set, so the
duplicate is invisible in the output and visible only in the volume.
One syscall becomes one event. Second, failed opens are discarded in
the kernel instead of being submitted.

This is the mechanism behind SC-008, and it is why SC-008 is stated as a
measurement: an implementation that sent both the request and the result
to userspace would pass every content criterion while doubling volume.

**Measurement owed**: today's event count for the fixture build, as the
ceiling. Recorded in `measurements/findings.md` under "still to
measure".

---

## R9 — Per-architecture attach, and what counts as a failure

**Decision**: three tiers, matching the spec's FR-006 / FR-007 /
FR-008 split and the precedent at `loader.rs:286-291`.

- **Fatal if absent**: the `openat` pair. Measured present on aarch64;
  it is the syscall every libc open wrapper uses. Losing all open
  capture must stop the trace (FR-007, Principle III).
- **Best-effort, recorded if absent**: `openat2`, and `open`/`creat`
  behind `#[cfg(target_arch = "x86_64")]`. Absence on an architecture
  that has no such syscall is not a failure and must not be recorded as
  one (FR-006) — measured: both are absent on aarch64.
- **Non-fatal**: `connect`. Its loss degrades the trace and is recorded
  (FR-008).

**Open question for run 2**: whether `open`/`creat` are present on the
x86_64 runner image. If they are absent there too, the cfg block is
dead code and should not ship.

---

## R10 — The superset claim, which this phase does not settle

The spec's removal decision rests on the replacement interfaces
covering every open the removed probes caught. Research supports it on
mechanism — `do_filp_open` is reached *from* the open syscalls, so
anything that passed through it entered through one of them — but
mechanism is not measurement, and the one case that would break the
claim is an in-kernel caller of `do_filp_open` with no syscall above
it.

This stays an explicit assumption with a task attached, as the spec
requires. The task is a before/after read-set comparison on a host
where capture is intact, which is work SC-001b already demands; the
superset claim is confirmed or refuted by the same diff. If refuted,
the removal is what needs revisiting, not the migration.
