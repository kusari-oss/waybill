# Contract: joining an open request to its result

Feature: `specs/1270-stable-probe-tracepoints/` (spec-kit sequence 1270,
which is **not** an issue number) · Issue **#1252**

A read set's claim is that the build read these files. Today it carries
open *requests*, so a request that failed is in it. This contract
defines the join that narrows it to opens that succeeded, and the
record that makes the narrowing checkable.

## The join

1. **At `sys_enter_open*`**: if `should_trace()`, store a `PendingOpen`
   under the current thread id. Nothing is submitted. No path is read —
   the string is read at exit, where it is still mapped, straight into
   ring-buffer memory.
2. **At `sys_exit_open*`**: take the pending entry, **remove it
   unconditionally**, then read `ret` at offset 16.
   - `ret >= 0`: reserve, read the path into the reserved record, apply
     the m213 noise classifier, and either `submit` or `discard`.
   - `ret < 0`: increment `OPEN_FAILURES_DISCARDED`. If `OPEN_DIAG` is
     set, submit an `OpenFailed` record carrying the path and the
     negated errno. Otherwise submit nothing.
3. **No pending entry at exit**: return. The trace started mid-syscall,
   or `should_trace()` was false at entry.

### Invariants

- **One syscall, at most one record.** Entry submits nothing, exit
  submits once.
- **A failed open occupies no ring-buffer space** unless diagnostics
  are explicitly on. This is why the join is in the kernel
  (clarification Q3).
- **Removal precedes the result check**, so a failed open leaves no
  pending entry (FR-005c).
- **A stale entry is overwritten, never inherited.** A thread whose
  result never arrives leaves one entry that its next open replaces, so
  the map cannot accumulate and a later open cannot be attributed the
  earlier open's path.
- **Nothing is submitted at entry even when the exit tracepoint is
  absent.** A half-attached pair is an attach failure (see
  `probe-attachment.md`), not a silent degradation to request
  semantics. Where a host genuinely cannot provide the result,
  FR-004b's fallback applies and records a gap.

## The discard record

| Mechanism | Default | Answers |
|---|---|---|
| `OPEN_FAILURES_DISCARDED` → `TraceIntegrity.open_failures_discarded` | always on | "how much overstatement was removed", and "is the join firing at all" |
| `OpenFailed` records, gated by `OPEN_DIAG` | off | "is *this* removed entry attributable to a failed request" |

The counter is the positive control. A build known to attempt a failed
open must show a non-zero count; zero means the exit tracepoint is not
firing, which otherwise looks exactly like a clean run.

The per-path record is what SC-001b is verified against. Requirements
on it:

- It MUST name the path. A count alone passes equally for a correct
  join and for one that also drops successful opens.
- It MUST carry the result that caused the discard, so a removal can be
  attributed rather than assumed.
- It MUST NOT change what a run records when it is off: same read set,
  same write set, same event volume.
- It MUST NOT be required for a trace to run.
- An `OpenFailed` record MUST NOT reach a read set, a write set, or any
  attestation field. Userspace logs it and drops it.

## Semantics this contract does not change

**Connects keep attempt semantics.** `sys_enter_connect` records the
attempt; there is no exit program and no success filter. `connect` on a
non-blocking socket returns `-EINPROGRESS`, which is a connection in
flight rather than a failure, so narrowing connects the way opens are
narrowed would drop exactly the asynchronous connects a build's HTTP
client makes. The clarification narrowed opens only (research R6).

**`O_CREAT` writes are still writes.** The flags decide read versus
write, as they do today (`aggregator.rs`, FR-008 of m1070). The join
changes *whether* an entry appears, never which set it appears in.
