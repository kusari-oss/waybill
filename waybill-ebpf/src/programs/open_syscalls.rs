//! #1252 — file opens, observed at the syscall boundary.
//!
//! ## Which interfaces this treats as stable, and why
//!
//! Capture here attaches to **syscall tracepoints** and reads their
//! arguments at the offsets the kernel publishes in
//! `/sys/kernel/tracing/events/syscalls/*/format`. A syscall's name, its
//! argument order and the layout of the structures it is handed are UAPI:
//! the kernel does not get to change them without breaking every program
//! on the machine.
//!
//! What it must never go back to is a **kprobe on an internal kernel
//! function**. `do_filp_open` and `do_sys_openat2` are implementation
//! detail, and their presence as traceable symbols is promised to nobody.
//! That is not hypothetical: on GitHub's `ubuntu-24.04` image
//! `20261025.353` the `do_filp_open` kprobe stopped attaching, file-open
//! events for an identical fixture build fell from 902 to 443, and the
//! attestation reported itself complete anyway. The same objection applies
//! to reading kernel structures at hardcoded offsets, which is how the
//! retired `tcp_v4_connect` kprobe found a destination address.
//!
//! So: tracepoint names and their documented argument slots, yes. Internal
//! symbols, kernel struct offsets, and anything whose shape has to be
//! rediscovered after a kernel bump, no. If a future miss seems to call for
//! one, the interface to reach for is another syscall tracepoint — or BTF,
//! which at least tells you when the layout moved.
//!
//! ## One open, one record
//!
//! Each open is observed at both ends, following the m1070 pattern in
//! `fs_syscalls.rs`. The entry tracepoint stores the arguments by thread
//! id; the exit tracepoint learns the result and reports the open **only
//! if it succeeded**, always removing the stored arguments first. A request
//! that failed is counted and dropped, so it never occupies ring-buffer
//! space and never reaches a read set — a read set's claim is that the
//! build read these files, and a failed open read nothing.
//!
//! The path string is read at exit, straight into ring-buffer memory: a
//! `FileEvent` does not fit comfortably on the 512-byte BPF stack, and the
//! user pages are still mapped at `sys_exit` because it runs on the same
//! thread before returning to userspace.

use aya_ebpf::{
    helpers::{bpf_ktime_get_ns, bpf_probe_read_user, bpf_probe_read_user_str_bytes},
    macros::tracepoint,
    programs::TracePointContext,
};

use waybill_common::events::{FileEvent, FileEventType, AT_FDCWD};

use crate::helpers::{
    current_comm, current_pid, current_tid, gating_armed, in_traced_subtree,
    increment_drop_counter, increment_event_drop, should_trace,
};
use crate::maps::{
    PendingOpen, FILE_EVENTS, FILE_EVENT_DROPS, OPEN_DIAG, OPEN_FAILURES_DISCARDED,
    OPEN_REQUEST_ONLY, PENDING_INSERT_FORCE_FAIL, PENDING_OPEN, TRACED_PIDS,
    TRACED_SUBTREE_HITS, TRACE_HANDSHAKE_NONCE,
};
use crate::programs::file_ops::classify_and_drop_if_noise;

/// Syscall tracepoint argument slots. An 8-byte common header, `int
/// __syscall_nr` at 8, then one 8-byte slot per argument from 16.
/// `sys_exit_*` carries `long ret` at 16. Measured against every
/// tracepoint this module attaches to — see
/// `specs/1270-stable-probe-tracepoints/measurements/findings.md`.
/// `EventDropReason::KERNEL` slot indices. Named here rather than
/// imported as the enum because `waybill-common` is `no_std`-shared but
/// the enum carries `&'static str` names the kernel has no use for.
const KERNEL_PATH_UNREADABLE: u8 = 0;
const KERNEL_NULL_PATH: u8 = 1;
const KERNEL_EXIT_WITHOUT_REQUEST: u8 = 2;
const KERNEL_PENDING_INSERT_FAILED: u8 = 3;
const KERNEL_OUTSIDE_TRACED_SUBTREE: u8 = 4;
const KERNEL_TRACED_INSERT_FAILED: u8 = 5;

const ARG0: usize = 16;
const ARG1: usize = 24;
const ARG2: usize = 32;
const RET: usize = 16;

/// `O_WRONLY | O_CREAT | O_TRUNC` — what `creat(2)` implies, since the
/// syscall takes no flags argument of its own.
const CREAT_FLAGS: u32 = 0o1 | 0o100 | 0o1000;

#[inline(always)]
fn read_ret(ctx: &TracePointContext) -> i64 {
    unsafe { ctx.read_at::<i64>(RET).unwrap_or(-1) }
}

/// Is this open the traced root announcing itself?
///
/// Recognised at **entry**, where `dfd` and `flags` are in hand as plain
/// integers — no path is read and no string is compared, so the cost on the
/// hot path is two comparisons. See `TRACE_HANDSHAKE_DFD` in
/// `waybill-common` for why the pid cannot simply be passed in.
#[inline(always)]
fn is_handshake(dfd: i64, flags: u32) -> bool {
    if dfd != waybill_common::maps::TRACE_HANDSHAKE_DFD {
        return false;
    }
    // A zero nonce is no armed handshake, so nothing can be seeded by a
    // caller that happens to guess the dfd.
    match TRACE_HANDSHAKE_NONCE.get(waybill_common::maps::TRACE_HANDSHAKE_NONCE_SLOT) {
        Some(&n) => n != 0 && n == flags,
        None => false,
    }
}

/// Record the handshaking process as the traced root.
#[inline(always)]
fn seed_traced_root() {
    let pid = current_pid();
    if unsafe { TRACED_PIDS.insert(&pid, &1u8, 0) }.is_err() {
        increment_event_drop(KERNEL_TRACED_INSERT_FAILED);
    }
}

/// Stash an in-flight open under the current thread id.
///
/// `insert` overwrites, which is what bounds the map *per thread*: a
/// thread that died inside a syscall leaves one entry, and that thread's
/// next open replaces it rather than inheriting it. It does not bound the
/// map across threads, and `PENDING_OPEN` holds 4096 of them for the whole
/// machine — these tracepoints are system-wide, so every thread on the
/// host competes for the same slots. A full BPF hash map rejects the
/// update, and until #1185 that rejection was discarded.
///
/// It is the only early return in this module that recorded nothing, and
/// the consequence is not a missing counter but a missing open: no stored
/// request means the exit has nothing to join, so the open never reaches
/// a read set. That is the shape of the loss #1185 reports — every open
/// after saturation is dropped, so a build loses whole invocations partway
/// through rather than a scattering of reads, and the count varies with
/// whatever else the host was doing.
#[inline(always)]
fn store_open(dfd: i64, path_ptr: u64, flags: u32) {
    // The handshake is answered before any other consideration, including
    // `should_trace()`: it carries no path, reads nothing, and is the one
    // syscall whose whole purpose is to be observed.
    //
    // A pending entry is stored for it anyway, carrying the handshake `dfd`
    // as its marker, so the matching exit finds a request and discards it
    // quietly. Returning here instead would leave an exit with nothing to
    // join, which increments `kernel_exit_without_request` -- polluting the
    // very counter this work exists to make readable.
    if is_handshake(dfd, flags) {
        seed_traced_root();
        let marker = PendingOpen {
            dfd,
            path_ptr: 0,
            flags,
            _padding: 0,
        };
        let tid = current_tid();
        let _ = unsafe { PENDING_OPEN.insert(&tid, &marker, 0) };
        return;
    }
    if !should_trace() {
        return;
    }

    // #1185 — the subtree gate.
    //
    // Dropping here, at entry, is deliberate: an open from outside the
    // subtree never consumes a `PENDING_OPEN` slot or ring-buffer space, so
    // this removes the resource competition as well as the noise. Every
    // thread on the machine shared those 4096 slots before.
    //
    // The gate is positive -- admit what is known to belong -- and that is
    // only safe because an unseeded root is unreachable: userspace refuses
    // to start a trace whose root never announced itself. Without that
    // check this branch is #1185's own failure, dropping every open while
    // the attestation still reports `complete`.
    //
    // An unarmed trace counts and does not drop, so `--target-pid` keeps
    // the behaviour it has always had rather than silently capturing
    // nothing.
    if in_traced_subtree(current_pid()) {
        increment_drop_counter(&TRACED_SUBTREE_HITS);
    } else {
        increment_event_drop(KERNEL_OUTSIDE_TRACED_SUBTREE);
        if gating_armed() {
            return;
        }
    }

    let pending = PendingOpen {
        dfd,
        path_ptr,
        flags,
        _padding: 0,
    };

    // FR-004b — a host that cannot tell us the result of an open. The
    // loader sets this when the entry tracepoint attached and the exit
    // one did not, and it means capture degrades to what it recorded
    // before this feature: the request, including requests that went on to
    // fail. Holding them instead would be the silent loss this feature
    // removes, in a smaller form. The gap is recorded in the attestation
    // by the loader, so the capture does not claim to be a total.
    if request_only() {
        emit_open(&pending, current_tid());
        return;
    }

    let tid = current_tid();
    if force_insert_failure() || PENDING_OPEN.insert(&tid, &pending, 0).is_err() {
        increment_event_drop(KERNEL_PENDING_INSERT_FAILED);
    }
}

/// Read the #1185 forced-failure slot, which makes the branch above
/// reachable without saturating a machine-wide map. Absent or unreadable
/// reads as `0`, the behaviour of a host nobody is testing.
#[inline(always)]
fn force_insert_failure() -> bool {
    match PENDING_INSERT_FORCE_FAIL.get(0) {
        Some(&v) => v != 0,
        None => false,
    }
}

/// Read the FR-004b fallback slot. An absent or unreadable slot reads as
/// `0` — the joined behaviour — because that is what an intact host does.
#[inline(always)]
fn request_only() -> bool {
    match OPEN_REQUEST_ONLY.get(0) {
        Some(&v) => v != 0,
        None => false,
    }
}

/// Report the open if it succeeded; count and drop it if it did not.
#[inline(always)]
fn finish_open(ctx: &TracePointContext) {
    // The same guard `store_open` applies, and for the counter's sake
    // rather than the capture's. Without it the filtering is asymmetric:
    // the tracer's own opens store no request at entry and then orphan
    // their exit here, so `KERNEL_EXIT_WITHOUT_REQUEST` counts a drop
    // for an event that was never meant to be captured. Measured on the
    // x86-64 lane: see measurements/findings.md run 6.
    if !should_trace() {
        return;
    }
    let tid = current_tid();
    // SAFETY: `HashMap::get` is unsafe because the returned reference
    // borrows map memory the kernel may reuse; copied out immediately.
    let Some(p) = (unsafe { PENDING_OPEN.get(&tid).copied() }) else {
        // An exit whose entry was never stored. Two benign sources are
        // bounded and one-time: a syscall already in flight when the
        // exit program attached, and the window between attaching the
        // exit and the entry program — the probes are system-wide, so
        // that window counts every open on the machine. Userspace
        // baselines this slot once attach completes and reports only the
        // delta.
        //
        // The delta alone still does not say which mechanism it came
        // from, because `store_open` failing to stash the request lands
        // here too, and that one is real loss rather than a syscall the
        // probes arrived late for. `kernel_pending_insert_failed` counts
        // that case at its own site, so the two numbers separate the
        // causes: this slot minus that one is late arrival, and that one
        // is capture the host could not hold (#1185).
        increment_event_drop(KERNEL_EXIT_WITHOUT_REQUEST);
        return;
    };
    // Unconditionally, and before the result is inspected: a failed open
    // must not leave an entry behind (FR-005c).
    let _ = PENDING_OPEN.remove(&tid);

    // The handshake's own exit. Discarded before the null-path branch
    // below, which it would otherwise reach -- the marker carries no path
    // by design. That branch feeds `kernel_null_path`, whose positive
    // control asserts a count of exactly 5, so letting the handshake
    // through would break an unrelated assertion and make the drop
    // counters mean one more thing than they say.
    if p.dfd == waybill_common::maps::TRACE_HANDSHAKE_DFD {
        return;
    }

    if p.path_ptr == 0 {
        increment_event_drop(KERNEL_NULL_PATH);
        return;
    }

    let ret = read_ret(ctx);
    if ret < 0 {
        discard_failed_open(&p, ret);
        return;
    }

    emit_open(&p, tid);
}

/// Submit one `Open` record. Reached from `finish_open` once the syscall is
/// known to have succeeded, and from `store_open` on a host that cannot
/// report results at all (FR-004b).
#[inline(always)]
fn emit_open(p: &PendingOpen, tid: u32) {
    if p.path_ptr == 0 {
        return;
    }
    let Some(mut buf) = FILE_EVENTS.reserve::<FileEvent>(0) else {
        increment_drop_counter(&FILE_EVENT_DROPS);
        return;
    };
    let event = buf.as_mut_ptr();
    unsafe {
        (*event).event_type = FileEventType::Open;
        (*event).timestamp_ns = bpf_ktime_get_ns();
        (*event).pid = current_pid();
        (*event).tid = tid;
        (*event).comm = current_comm();
        (*event).path = [0u8; 256];
        (*event)._path_padding = [0; 3];
        (*event).flags = p.flags;
        (*event).dfd = p.dfd as i32;
        (*event).bytes_transferred = 0;
        (*event).content_hash = [0; 32];
        (*event).inode = 0;
        let copied = bpf_probe_read_user_str_bytes(p.path_ptr as *const u8, &mut (*event).path)
            .map(|s| s.len())
            .unwrap_or(0);
        // #618 / #1185: reading the path at syscall exit assumes the user
        // pages are still mapped. When they are not the record would
        // carry an empty path, which userspace skips silently. Count it
        // here, where the cause is known, rather than leaving userspace to
        // infer it from an empty string.
        if copied == 0 {
            increment_event_drop(KERNEL_PATH_UNREADABLE);
            buf.discard(0);
            return;
        }
        // A copy that filled the buffer may have been cut off; userspace
        // will not resolve a truncated relative path (m1070).
        (*event).path_truncated = if copied >= 255 { 1 } else { 0 };
        // Same noise rules as the kprobes applied, and for the same
        // reason: a match drops before the event can cost ring-buffer
        // space. Applied after the reserve because the path is not known
        // until then -- the m1070 `finish_rename` shape.
        if classify_and_drop_if_noise(&(*event).path) {
            buf.discard(0);
            return;
        }
    }
    buf.submit(0);
}

/// Count a discarded request, and report it by name when diagnostics are on.
///
/// The count is unconditional. Without it a zero is unreadable: a build that
/// attempted no failed opens and a capture whose exit tracepoint never fired
/// look identical, and the second is the failure this feature exists to
/// remove.
#[inline(always)]
fn discard_failed_open(p: &PendingOpen, ret: i64) {
    increment_drop_counter(&OPEN_FAILURES_DISCARDED);

    let diag = match OPEN_DIAG.get(0) {
        Some(&v) => v != 0,
        None => false,
    };
    if !diag {
        return;
    }

    let Some(mut buf) = FILE_EVENTS.reserve::<FileEvent>(0) else {
        increment_drop_counter(&FILE_EVENT_DROPS);
        return;
    };
    let event = buf.as_mut_ptr();
    unsafe {
        (*event).event_type = FileEventType::OpenFailed;
        (*event).timestamp_ns = bpf_ktime_get_ns();
        (*event).pid = current_pid();
        (*event).tid = current_tid();
        (*event).comm = current_comm();
        (*event).path = [0u8; 256];
        (*event)._path_padding = [0; 3];
        // For this record kind alone, `flags` carries the negated errno.
        (*event).flags = (-ret) as u32;
        (*event).dfd = p.dfd as i32;
        (*event).bytes_transferred = 0;
        (*event).content_hash = [0; 32];
        (*event).inode = 0;
        let copied = bpf_probe_read_user_str_bytes(p.path_ptr as *const u8, &mut (*event).path)
            .map(|s| s.len())
            .unwrap_or(0);
        (*event).path_truncated = if copied >= 255 { 1 } else { 0 };
    }
    // Deliberately not noise-classified: the point of this record is to
    // account for every entry that left a read set, and a classifier that
    // dropped some of them would leave exactly the unexplained removals
    // SC-001b forbids.
    buf.submit(0);
}

// ------------------------------------------------------------------ openat

/// `openat(dfd, filename, flags, mode)`.
#[tracepoint]
pub fn sys_enter_openat(ctx: TracePointContext) -> u32 {
    unsafe {
        store_open(
            ctx.read_at::<i64>(ARG0).unwrap_or(AT_FDCWD as i64),
            ctx.read_at::<u64>(ARG1).unwrap_or(0),
            ctx.read_at::<i64>(ARG2).unwrap_or(0) as u32,
        );
    }
    0
}

#[tracepoint]
pub fn sys_exit_openat(ctx: TracePointContext) -> u32 {
    finish_open(&ctx);
    0
}

// ----------------------------------------------------------------- openat2

/// `openat2(dfd, filename, how, usize)`.
///
/// `how` is a **user** pointer here, unlike the kprobe on `do_sys_openat2`
/// this replaces, which saw the structure after the kernel had copied it in.
/// A kernel read of it compiles, fails at runtime, and with the failure
/// swallowed every `openat2` open would be classified as a read — write
/// sets would quietly lose entries. `struct open_how`'s first member is
/// `__u64 flags` at offset 0 (UAPI).
#[tracepoint]
pub fn sys_enter_openat2(ctx: TracePointContext) -> u32 {
    unsafe {
        let how_ptr = ctx.read_at::<u64>(ARG2).unwrap_or(0);
        let flags = if how_ptr == 0 {
            0
        } else {
            bpf_probe_read_user(how_ptr as *const u64).unwrap_or(0) as u32
        };
        store_open(
            ctx.read_at::<i64>(ARG0).unwrap_or(AT_FDCWD as i64),
            ctx.read_at::<u64>(ARG1).unwrap_or(0),
            flags,
        );
    }
    0
}

#[tracepoint]
pub fn sys_exit_openat2(ctx: TracePointContext) -> u32 {
    finish_open(&ctx);
    0
}

// ------------------------------------------------------- open, creat (x86)

/// `open(filename, flags, mode)`. No dfd argument.
///
/// **Not** gated on `target_arch`, and that is load-bearing: this crate
/// compiles for the BPF target, so `cfg(target_arch = "x86_64")` is false
/// here on every host and the program would be missing from the object
/// everywhere. The loader asked for it on x86_64, got `program
/// 'sys_exit_open' not found`, and x86_64 opens that go through `open(2)`
/// rather than `openat(2)` went uncaptured — `file=43` against 102 on
/// arm64, with the fixture's crate roots missing from the read set.
///
/// Whether the syscall exists is a property of the kernel the program
/// attaches to, not of the target it was compiled for. The *loader* gates
/// the attach by host arch, which is where m1070 put the same decision
/// for `rename`.
#[tracepoint]
pub fn sys_enter_open(ctx: TracePointContext) -> u32 {
    unsafe {
        store_open(
            AT_FDCWD as i64,
            ctx.read_at::<u64>(ARG0).unwrap_or(0),
            ctx.read_at::<i64>(ARG1).unwrap_or(0) as u32,
        );
    }
    0
}

#[tracepoint]
pub fn sys_exit_open(ctx: TracePointContext) -> u32 {
    finish_open(&ctx);
    0
}

/// `creat(pathname, mode)` — `open` with `O_WRONLY|O_CREAT|O_TRUNC`
/// implied. x86-64 only.
#[tracepoint]
pub fn sys_enter_creat(ctx: TracePointContext) -> u32 {
    unsafe {
        store_open(
            AT_FDCWD as i64,
            ctx.read_at::<u64>(ARG0).unwrap_or(0),
            CREAT_FLAGS,
        );
    }
    0
}

#[tracepoint]
pub fn sys_exit_creat(ctx: TracePointContext) -> u32 {
    finish_open(&ctx);
    0
}
