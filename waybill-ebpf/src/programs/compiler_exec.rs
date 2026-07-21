//! Milestone 210 — kernel-side compiler-pipeline eBPF programs.
//!
//! Three tracepoints that together capture the full compiler-invocation
//! DAG for a traced build:
//!
//! - **`sched_process_exec`** — fires when any process exec's a new
//!   binary. Filters by comm-field against the whitelist (R2) and
//!   emits a `CompilerExecEvent { kind: Exec }` on match. Also inserts
//!   the new PID into `COMPILER_INVOCATIONS` so descendant file-ops
//!   get gated in.
//!
//! - **`sched_process_fork`** — fires on every process fork. If the
//!   parent PID is in `COMPILER_INVOCATIONS`, propagates the same
//!   invocation-id to the child PID (research R3 — transitive
//!   descendant tracking through arbitrarily deep spawn chains).
//!
//! - **`sched_process_exit`** — fires when a tracked PID exits.
//!   Emits `{ kind: Exit }` + removes from the map so we don't grow
//!   unboundedly.
//!
//! Design decisions locked in `specs/210-compiler-pipeline-trace/research.md`:
//!
//! - **R1**: stable `sched_process_exec` tracepoint (not the
//!   kernel-version-fragile `execve` kprobe).
//! - **R2**: kernel-side prefilter on the fixed 16-byte comm field.
//!   Full-path argv[0] verification happens userspace-side.
//! - **R3**: `COMPILER_INVOCATIONS` HashMap propagates
//!   descendant-tracking to file-op kprobes at zero userspace cost.
//! - **R7**: `COMPILER_EXEC_EVENTS` ring buffer sized for typical
//!   build fanout; overflow surfaces per FR-008.
//!
//! Invocation-id assignment: kernel emits `bpf_ktime_get_ns()` at
//! exec time as the invocation-id (u64, monotonically increasing
//! within a boot, unique per invocation). Userspace consumes it
//! verbatim.

use aya_ebpf::{
    helpers::{bpf_get_current_comm, bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::tracepoint,
    programs::TracePointContext,
};

use waybill_common::events::{CompilerExecEvent, CompilerExecEventKind};

use crate::helpers::increment_drop_counter;
use crate::maps::{
    COMPILER_DIRECT_EXECS, COMPILER_EXEC_DROPS, COMPILER_EXEC_EVENTS, COMPILER_INVOCATIONS,
    PID_TO_PPID,
};

/// Compiler whitelist per FR-002. Matched against the 16-byte
/// comm-field in-kernel; longer names truncate (e.g.
/// `x86_64-linux-gn`) and require userspace argv[0] disambiguation
/// per R2.
///
/// Order matters only for readability — all entries are checked
/// unconditionally.
/// Whitelist stored as fixed-size `[[u8; 16]; N]`.
///
/// Rationale: an earlier `const COMPILER_WHITELIST: &[&[u8]]` version
/// was rejected by the eBPF verifier on aarch64. The inner
/// `entry[j]` deref requires the verifier to prove the fat pointer's
/// data ptr is a valid pointer, which it can't reason about — it sees
/// the loaded value as a plain scalar and refuses the load
/// (`R0 invalid mem access 'scalar'`).
///
/// Storing each entry inline as a 16-byte array means indexing is
/// direct constant-offset arithmetic against the array's flat memory
/// layout — no fat-pointer deref, no `.len()` load. Entries are
/// NUL-padded to 16 bytes so byte-wise compare works against the
/// kernel's NUL-padded 16-byte `comm` field.
const COMPILER_WHITELIST: [[u8; 16]; 15] = [
    *b"rustc\0\0\0\0\0\0\0\0\0\0\0",
    *b"gcc\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"g++\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"clang\0\0\0\0\0\0\0\0\0\0\0",
    *b"clang++\0\0\0\0\0\0\0\0\0",
    *b"go\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"ld\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"ld.lld\0\0\0\0\0\0\0\0\0\0",
    *b"ld.gold\0\0\0\0\0\0\0\0\0",
    *b"ld.bfd\0\0\0\0\0\0\0\0\0\0",
    *b"mold\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"cc1\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"cc1plus\0\0\0\0\0\0\0\0\0",
    *b"cpp\0\0\0\0\0\0\0\0\0\0\0\0\0",
    *b"as\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
];

/// Check whether the current process's comm-field matches any
/// entry in the whitelist.
///
/// Compares 16-byte `comm` against 16-byte whitelist entries as TWO
/// `u64` word loads per entry (2 branches per entry × 15 entries = 30
/// branches). The earlier byte-by-byte version blew the verifier's 1M
/// instruction budget because each `comm[j] != entry[j]` in the nested
/// loop created a state fork the verifier had to symbolically track
/// through 240 iterations. Word-wide compare collapses that to 30
/// branches which the verifier accepts trivially.
///
/// SAFETY: `comm` is `#[repr(C, align(_))]`-adjacent within the
/// tracepoint context; the u64 read is aligned by convention (aya
/// exposes `comm` from `bpf_get_current_comm()` which returns a
/// 16-byte-aligned buffer). Whitelist entries are `#[repr(C)]` array
/// literals with natural u64 alignment.
#[inline(always)]
fn matches_whitelist(comm: &[u8; 16]) -> bool {
    let c_ptr = comm.as_ptr() as *const u64;
    let c0 = unsafe { core::ptr::read_unaligned(c_ptr) };
    let c1 = unsafe { core::ptr::read_unaligned(c_ptr.add(1)) };

    let mut w = 0;
    while w < COMPILER_WHITELIST.len() {
        let e_ptr = COMPILER_WHITELIST[w].as_ptr() as *const u64;
        let e0 = unsafe { core::ptr::read_unaligned(e_ptr) };
        let e1 = unsafe { core::ptr::read_unaligned(e_ptr.add(1)) };
        if c0 == e0 && c1 == e1 {
            return true;
        }
        w += 1;
    }
    false
}

/// Return the parent PID's compiler-invocation-id if the parent is
/// a tracked compiler descendant, else `0` (root of a new DAG).
#[inline(always)]
fn parent_invocation_id(ppid: u32) -> u64 {
    unsafe { COMPILER_INVOCATIONS.get(&ppid) }
        .copied()
        .unwrap_or(0)
}

/// `sched_process_exec` tracepoint handler. See R1 + R2.
///
/// Layout of `sched_process_exec` context (from
/// `/sys/kernel/debug/tracing/events/sched/sched_process_exec/format`):
/// ```text
/// field:pid_t pid;         // offset 8; new process pid (post-exec)
/// field:pid_t old_pid;     // offset 12; pid before exec (usually same)
/// ```
/// The `comm` field lives on the current task, so `bpf_get_current_comm()`
/// gives us the post-exec comm. Same for `pid_tgid` — post-exec.
#[tracepoint]
pub fn sched_process_exec(ctx: TracePointContext) -> u32 {
    match try_sched_process_exec(&ctx) {
        _ => 0,
    }
}

fn try_sched_process_exec(_ctx: &TracePointContext) -> Result<u32, i64> {
    let comm = bpf_get_current_comm().unwrap_or([0u8; 16]);

    // R2 prefilter — kernel-side comm-field match against the
    // whitelist. Reject non-compiler execs at zero user-space cost.
    if !matches_whitelist(&comm) {
        return Ok(0);
    }

    let pid_tgid = unsafe { bpf_get_current_pid_tgid() };
    let pid = (pid_tgid >> 32) as u32;
    let ts_ns = unsafe { bpf_ktime_get_ns() };

    // Milestone 210 (issue #610) — resolve `ppid` by walking up the
    // PID_TO_PPID chain looking for the nearest tracked-compiler
    // ancestor. Real compiler pipelines have non-compiler
    // intermediates (e.g. rustc → cc → collect2 → ld); returning
    // ld's IMMEDIATE ppid (collect2's pid — unknown to userspace)
    // breaks the DAG. Walking up to find the rustc ancestor gives
    // userspace a pid its `pid_to_invocation_id` table can join on.
    // Returns the IMMEDIATE ppid as a fallback when no compiler
    // ancestor is found within the depth limit — this at least
    // gives userspace something to correlate against instead of 0.
    //
    // Depth limit 16 covers typical linker toolchain chains (cargo
    // → rustc → cc → collect2 → ld = 5 hops) plus headroom for
    // Bazel/Buck-style wrapper stacks. The verifier accepts this
    // bounded loop because each iteration is a single map lookup +
    // one branch; total ~30 instructions per iteration × 16 = ~500
    // instructions, well under the 1M budget.
    // Bounded ancestry walk: start with the immediate parent as a
    // fallback (userspace still gets *some* correlatable pid even
    // when no compiler ancestor exists), then walk up the chain
    // looking for a direct-exec compiler ancestor. When found,
    // overwrite `ppid` with that ancestor's pid so userspace's
    // pid_to_invocation_id join produces the strongest possible
    // DAG edge. When NOT found within 16 hops (typical cargo
    // topology: cargo directly forks rustc AND ld's wrapper chain
    // — the compilers are process-siblings, not ancestors), fall
    // back to the immediate ppid which userspace can still use for
    // "known-untracked parent" bookkeeping.
    //
    // Depth limit 16 covers typical linker toolchain chains (cargo
    // → rustc → cc → collect2 → ld = 5 hops) plus headroom for
    // Bazel/Buck wrapper stacks. Verifier accepts easily because
    // each iteration is one map lookup + one branch (~30 insns per
    // iter × 16 = ~500 insns, well under the 1M budget).
    let immediate_ppid = unsafe { PID_TO_PPID.get(&pid).copied() }.unwrap_or(0);
    let mut ppid: u32 = immediate_ppid;
    let mut cursor: u32 = pid;
    let mut i = 0;
    while i < 16 {
        let parent = unsafe { PID_TO_PPID.get(&cursor).copied() }.unwrap_or(0);
        if parent == 0 {
            break;
        }
        if unsafe { COMPILER_DIRECT_EXECS.get(&parent).is_some() } {
            ppid = parent;
            break;
        }
        cursor = parent;
        i += 1;
    }

    // Assign invocation-id = ktime (unique per invocation within a
    // boot). Kernel writes COMPILER_INVOCATIONS[pid] = ts_ns; userspace
    // consumes ts_ns as the invocation-id verbatim.
    let invocation_id = ts_ns;
    let _ = unsafe { COMPILER_INVOCATIONS.insert(&pid, &invocation_id, 0) };
    // Milestone 210 (issue #610) — mark this pid as a DIRECT-exec
    // compiler so the ancestry walk in subsequent exec events can
    // distinguish it from fork-propagated descendants.
    let one: u8 = 1;
    let _ = unsafe { COMPILER_DIRECT_EXECS.insert(&pid, &one, 0) };

    if let Some(mut buf) = COMPILER_EXEC_EVENTS.reserve::<CompilerExecEvent>(0) {
        let ev = buf.as_mut_ptr();
        unsafe {
            (*ev).kind = CompilerExecEventKind::Exec;
            (*ev).timestamp_ns = ts_ns;
            (*ev).pid = pid;
            (*ev).ppid = ppid;
            (*ev).cgroup_id = 0; // TODO: bpf_get_current_cgroup_id() in follow-up
            (*ev).comm = comm;
            (*ev).argv0_hint = [0u8; 16]; // TODO: read argv[0] via bpf_probe_read_user
            (*ev).argv0_hint_len = 0;
            (*ev).exit_code = 0;
            (*ev)._padding = [0u8; 2];
        }
        buf.submit(0);
    } else {
        increment_drop_counter(&COMPILER_EXEC_DROPS);
    }
    Ok(0)
}

/// `sched_process_fork` tracepoint handler. See R3.
///
/// Layout (from `/sys/kernel/debug/tracing/events/sched/sched_process_fork/format`):
/// ```text
/// field:pid_t parent_pid;  // offset 24
/// field:pid_t child_pid;   // offset 44
/// ```
/// Propagates the parent's invocation-id to the child so file-op
/// kprobes fire on any descendant of a whitelisted compiler process.
#[tracepoint]
pub fn sched_process_fork(ctx: TracePointContext) -> u32 {
    match try_sched_process_fork(&ctx) {
        _ => 0,
    }
}

fn try_sched_process_fork(ctx: &TracePointContext) -> Result<u32, i64> {
    // Read parent + child pid from the tracepoint's raw args.
    // Offsets are stable per the tracepoint's format file.
    let parent_pid: u32 = unsafe { ctx.read_at::<u32>(24).map_err(|e| e as i64)? };
    let child_pid: u32 = unsafe { ctx.read_at::<u32>(44).map_err(|e| e as i64)? };

    // Milestone 210 (issue #610) — always record the fork lineage,
    // regardless of whether the parent is a tracked compiler. The
    // `sched_process_exec` tracepoint reads back from this map to
    // populate `ppid` in the emitted event, which is what userspace
    // joins on to derive `parent_invocation_id` + build `dag_edges`.
    // Insert-with-any (BPF_ANY = 0) so we overwrite on pid reuse.
    let _ = unsafe { PID_TO_PPID.insert(&child_pid, &parent_pid, 0) };

    // Compiler-descendant propagation: if the parent is a tracked
    // compiler, the child inherits its invocation-id so downstream
    // file-op kprobes fire on the child too.
    let parent_id = parent_invocation_id(parent_pid);
    if parent_id != 0 {
        let _ = unsafe { COMPILER_INVOCATIONS.insert(&child_pid, &parent_id, 0) };
    }
    Ok(0)
}

/// `sched_process_exit` tracepoint handler.
///
/// Removes the exiting pid from `COMPILER_INVOCATIONS` (prevents
/// unbounded growth) + emits a `CompilerExecEvent { kind: Exit }`
/// when the exiting pid was a tracked compiler-invocation root.
#[tracepoint]
pub fn sched_process_exit(ctx: TracePointContext) -> u32 {
    match try_sched_process_exit(&ctx) {
        _ => 0,
    }
}

fn try_sched_process_exit(_ctx: &TracePointContext) -> Result<u32, i64> {
    let pid_tgid = unsafe { bpf_get_current_pid_tgid() };
    let pid = (pid_tgid >> 32) as u32;

    // Milestone 210 (issue #610) — purge PID_TO_PPID unconditionally
    // on any exit so the map doesn't grow unbounded. The map holds
    // every fork's child→parent link regardless of compiler
    // whitelist, so pruning has to be as broad as insertion. Runs
    // BEFORE the early-return for non-compiler pids so cleanup
    // still fires on ordinary process exits.
    let _ = unsafe { PID_TO_PPID.remove(&pid) };

    let invocation_id = unsafe { COMPILER_INVOCATIONS.get(&pid).copied() };
    let Some(id) = invocation_id else {
        return Ok(0);
    };

    let ts_ns = unsafe { bpf_ktime_get_ns() };

    // Emit an Exit event so userspace sets end_timestamp + exit_code
    // on the CompilerInvocation.
    if let Some(mut buf) = COMPILER_EXEC_EVENTS.reserve::<CompilerExecEvent>(0) {
        let ev = buf.as_mut_ptr();
        unsafe {
            (*ev).kind = CompilerExecEventKind::Exit;
            (*ev).timestamp_ns = ts_ns;
            (*ev).pid = pid;
            (*ev).ppid = 0;
            (*ev).cgroup_id = 0;
            (*ev).comm = bpf_get_current_comm().unwrap_or([0u8; 16]);
            (*ev).argv0_hint = [0u8; 16];
            (*ev).argv0_hint_len = 0;
            // TODO: read the actual exit_code from ctx in a follow-up.
            (*ev).exit_code = 0;
            (*ev)._padding = [0u8; 2];
        }
        buf.submit(0);
    } else {
        increment_drop_counter(&COMPILER_EXEC_DROPS);
    }

    // Only remove ROOT invocation pids on exit. Descendant pids
    // that inherited via fork stay in the map until their subtree
    // exits — the kernel eventually purges them via natural pid
    // reuse OR the map's LRU eviction if we ever add that.
    // For MVP, always remove; userspace's aggregator is idempotent
    // on unknown pids.
    let _ = unsafe { COMPILER_INVOCATIONS.remove(&pid) };
    let _ = unsafe { COMPILER_DIRECT_EXECS.remove(&pid) };
    let _ = id;
    Ok(0)
}
