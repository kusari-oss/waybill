# Contract: what attaches, and what a failure to attach means

Feature: `specs/1270-stable-probe-tracepoints/` (spec-kit sequence 1270,
which is **not** an issue number) · Issue **#1252**

The loader's attach sequence is a contract because the attestation
reports it. A consumer reads `kprobe_attach_failures[]` and
`compiler_pipeline.completeness` to decide whether a recorded input set
is a total or a floor, so which attachments are fatal, which are
best-effort, and which are expected to be absent is observable
behaviour rather than an implementation detail.

## The attach table

| Program | Category | Event | Tier |
|---|---|---|---|
| `sys_enter_openat` | `syscalls` | `sys_enter_openat` | fatal |
| `sys_exit_openat` | `syscalls` | `sys_exit_openat` | fatal |
| `sys_enter_openat2` | `syscalls` | `sys_enter_openat2` | best-effort |
| `sys_exit_openat2` | `syscalls` | `sys_exit_openat2` | best-effort |
| `sys_enter_open` | `syscalls` | `sys_enter_open` | x86_64 only, best-effort — **pending T001** |
| `sys_exit_open` | `syscalls` | `sys_exit_open` | x86_64 only, best-effort — **pending T001** |
| `sys_enter_creat` | `syscalls` | `sys_enter_creat` | x86_64 only, best-effort — **pending T001** |
| `sys_exit_creat` | `syscalls` | `sys_exit_creat` | x86_64 only, best-effort — **pending T001** |
| `sys_enter_connect` | `syscalls` | `sys_enter_connect` | non-fatal |

The `open` and `creat` rows are conditional on the x86_64 tracepoint
inventory (T001). They are measurably absent on aarch64; if the
inventory shows them absent on x86_64 too, they do not ship and these
four rows come out of this table rather than becoming dead code.

Unchanged and still attached: the m210 `sched_*` tracepoints
(best-effort), the m1070 `chdir`/`rename` family (fatal), and the
OpenSSL uprobes.

Removed: `openat2_entry`, `do_filp_open_entry`, `tcp_connect`,
`tcp_connect_ret`, and the `attach_kprobe` helper itself.

## Tier semantics

**Fatal.** The trace stops with an error naming the interface. Losing
all open capture would emit a document whose empty file list reads as
"this build opened nothing", which Principle III forbids and which
#1310's bail already implements for the kprobe pair. The `openat`
*pair* is fatal as a unit: an enter without its exit cannot produce a
record, so a half-attached pair is a failure, not a degradation.

**Best-effort.** Attach failure is logged at WARN, the program name is
appended to `kprobe_attach_failures[]`, and the compiler-pipeline
completeness degrades to `partial / probe_attach_failed`. The trace
continues.

**Non-fatal (connect).** Same as best-effort, and explicitly does not
stop the trace: a build that fetches nothing over the network is a
normal build, so the absence of connect capture degrades the record
rather than invalidating it (FR-008).

**Expected absence is not a failure.** `open` and `creat` do not exist
on arm64 — measured, `measurements/findings.md` run 1 — so their
absence there MUST NOT appear in `kprobe_attach_failures[]` and MUST
NOT degrade completeness. This is why they sit behind
`#[cfg(target_arch = "x86_64")]` rather than behind a runtime
best-effort attach: a compile-time guard cannot mistake an
architecture's shape for a fault. `loader.rs:286-291` is the precedent.

## Field naming

`kprobe_attach_failures[]` keeps its name and carries tracepoint
program names, per clarification Q1. It has been a union since m212
(counter maps) and m210 (`sched_*` tracepoints); after this feature it
holds no kprobe names at all. Its doc comment MUST say what it is — a
list of capture interfaces that did not become usable — and MUST NOT
say "kprobes" (FR-011a). Renaming it is out of scope: the attestation
is a stability surface and a rename breaks consumers for a cosmetic
gain.

## What a test must enforce

- **No kprobe is attached.** `attach_kprobe` is deleted, so this is
  structural; the test asserts the helper's absence rather than
  inspecting a list of attach calls, because a list can grow an entry
  while the test still passes.
- **A half-attached `openat` pair fails closed.** Verified by making
  one of the two unavailable deliberately, not by waiting to observe
  it.
- **An architecture's missing syscall is not reported as a failure.**
  Verified on arm64, where `open` and `creat` are measurably absent.
