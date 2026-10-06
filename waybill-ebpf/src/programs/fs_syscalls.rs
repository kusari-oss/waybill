//! Milestone 1070 (#614) — `chdir`, `fchdir` and rename, reported to
//! userspace when they succeed.
//!
//! Userspace resolves relative opens against each process's working
//! directory, modelled from forks and from these calls
//! (`waybill-cli/src/trace/cwd.rs`). Renames matter because the compiler
//! writes its libraries under a temporary name and renames them into place,
//! so a write set built from opens alone would hold only the temporary names
//! (specs/1070-trace-relative-source-reads/research.md R6).
//!
//! Each call is observed at both ends. The entry tracepoint stores the
//! arguments by thread id, and the exit tracepoint reports the call only if it
//! returned 0, always removing the stored arguments. Path strings are read at
//! exit, straight into ring-buffer memory: a rename record (560 bytes) does
//! not fit on the 512-byte BPF stack. The strings are still mapped at
//! `sys_exit`, because it runs on the same thread before it returns to
//! userspace.
//!
//! Syscall tracepoint layout (stable, `/sys/kernel/tracing/events/syscalls/
//! */format`): an 8-byte common header, `int __syscall_nr` at 8, then each
//! argument as an 8-byte slot from offset 16. `sys_exit_*` has `long ret` at 16.

use aya_ebpf::{
    helpers::{bpf_ktime_get_ns, bpf_probe_read_user_str_bytes},
    macros::tracepoint,
    programs::TracePointContext,
};

use waybill_common::events::{FileEvent, FileEventType, RenameEvent, AT_FDCWD};

use crate::helpers::{current_comm, current_pid, current_tid, increment_drop_counter, should_trace};
use crate::maps::{PendingRename, FILE_EVENTS, FILE_EVENT_DROPS, PENDING_CHDIR, PENDING_RENAME};
use crate::programs::file_ops::classify_and_drop_if_noise;

const ARG0: usize = 16;
const ARG1: usize = 24;
const ARG2: usize = 32;
const ARG3: usize = 40;
const RET: usize = 16;

#[inline(always)]
fn read_ret(ctx: &TracePointContext) -> i64 {
    unsafe { ctx.read_at::<i64>(RET).unwrap_or(-1) }
}

// ---------------------------------------------------------------- chdir

#[tracepoint]
pub fn sys_enter_chdir(ctx: TracePointContext) -> u32 {
    if should_trace() {
        if let Ok(ptr) = unsafe { ctx.read_at::<u64>(ARG0) } {
            let tid = current_tid();
            let _ = unsafe { PENDING_CHDIR.insert(&tid, &ptr, 0) };
        }
    }
    0
}

#[tracepoint]
pub fn sys_exit_chdir(ctx: TracePointContext) -> u32 {
    let tid = current_tid();
    let Some(ptr) = (unsafe { PENDING_CHDIR.get(&tid).copied() }) else {
        return 0;
    };
    let _ = unsafe { PENDING_CHDIR.remove(&tid) };
    if read_ret(&ctx) != 0 {
        return 0;
    }
    emit_dir_change(FileEventType::Chdir, ptr);
    0
}

#[tracepoint]
pub fn sys_exit_fchdir(ctx: TracePointContext) -> u32 {
    if should_trace() && read_ret(&ctx) == 0 {
        emit_dir_change(FileEventType::Fchdir, 0);
    }
    0
}

/// A `Chdir` record carries the target as the process passed it; a `Fchdir`
/// record carries no path. The noise classifier never applies: userspace needs
/// every directory change to resolve paths, whatever the directory is.
#[inline(always)]
fn emit_dir_change(kind: FileEventType, path_ptr: u64) {
    let Some(mut buf) = FILE_EVENTS.reserve::<FileEvent>(0) else {
        increment_drop_counter(&FILE_EVENT_DROPS);
        return;
    };
    let event = buf.as_mut_ptr();
    unsafe {
        (*event).event_type = kind;
        (*event).timestamp_ns = bpf_ktime_get_ns();
        (*event).pid = current_pid();
        (*event).tid = current_tid();
        (*event).comm = current_comm();
        (*event).path = [0u8; 256];
        (*event).path_truncated = 0;
        (*event)._path_padding = [0; 3];
        (*event).flags = 0;
        (*event).dfd = AT_FDCWD;
        (*event).bytes_transferred = 0;
        (*event).content_hash = [0; 32];
        (*event).inode = 0;
        if path_ptr != 0 {
            let copied = bpf_probe_read_user_str_bytes(path_ptr as *const u8, &mut (*event).path)
                .map(|s| s.len())
                .unwrap_or(0);
            (*event).path_truncated = if copied >= 255 { 1 } else { 0 };
        }
    }
    buf.submit(0);
}

// ---------------------------------------------------------------- rename

/// `renameat(olddfd, oldname, newdfd, newname)`.
#[tracepoint]
pub fn sys_enter_renameat(ctx: TracePointContext) -> u32 {
    store_rename(&ctx, true);
    0
}

/// `renameat2(olddfd, oldname, newdfd, newname, flags)`. Same leading layout.
#[tracepoint]
pub fn sys_enter_renameat2(ctx: TracePointContext) -> u32 {
    store_rename(&ctx, true);
    0
}

/// `rename(oldname, newname)`. x86-64 only; arm64 has no such syscall.
#[tracepoint]
pub fn sys_enter_rename(ctx: TracePointContext) -> u32 {
    store_rename(&ctx, false);
    0
}

#[tracepoint]
pub fn sys_exit_renameat(ctx: TracePointContext) -> u32 {
    finish_rename(&ctx);
    0
}

#[tracepoint]
pub fn sys_exit_renameat2(ctx: TracePointContext) -> u32 {
    finish_rename(&ctx);
    0
}

#[tracepoint]
pub fn sys_exit_rename(ctx: TracePointContext) -> u32 {
    finish_rename(&ctx);
    0
}

#[inline(always)]
fn store_rename(ctx: &TracePointContext, with_dfds: bool) {
    if !should_trace() {
        return;
    }
    let pending = unsafe {
        if with_dfds {
            PendingRename {
                old_dfd: ctx.read_at::<i64>(ARG0).unwrap_or(AT_FDCWD as i64),
                old_ptr: ctx.read_at::<u64>(ARG1).unwrap_or(0),
                new_dfd: ctx.read_at::<i64>(ARG2).unwrap_or(AT_FDCWD as i64),
                new_ptr: ctx.read_at::<u64>(ARG3).unwrap_or(0),
            }
        } else {
            PendingRename {
                old_dfd: AT_FDCWD as i64,
                old_ptr: ctx.read_at::<u64>(ARG0).unwrap_or(0),
                new_dfd: AT_FDCWD as i64,
                new_ptr: ctx.read_at::<u64>(ARG1).unwrap_or(0),
            }
        }
    };
    let tid = current_tid();
    let _ = unsafe { PENDING_RENAME.insert(&tid, &pending, 0) };
}

#[inline(always)]
fn finish_rename(ctx: &TracePointContext) {
    let tid = current_tid();
    let Some(p) = (unsafe { PENDING_RENAME.get(&tid).copied() }) else {
        return;
    };
    let _ = unsafe { PENDING_RENAME.remove(&tid) };
    if read_ret(ctx) != 0 || p.old_ptr == 0 || p.new_ptr == 0 {
        return;
    }
    let Some(mut buf) = FILE_EVENTS.reserve::<RenameEvent>(0) else {
        increment_drop_counter(&FILE_EVENT_DROPS);
        return;
    };
    let event = buf.as_mut_ptr();
    unsafe {
        (*event).event_type = FileEventType::Rename;
        (*event).pid = current_pid();
        (*event).timestamp_ns = bpf_ktime_get_ns();
        (*event).tid = tid;
        (*event).old_dfd = p.old_dfd as i32;
        (*event).new_dfd = p.new_dfd as i32;
        (*event)._padding = [0; 2];
        (*event).comm = current_comm();
        (*event).old_path = [0u8; 256];
        (*event).new_path = [0u8; 256];
        let old = bpf_probe_read_user_str_bytes(p.old_ptr as *const u8, &mut (*event).old_path)
            .map(|s| s.len())
            .unwrap_or(0);
        let new = bpf_probe_read_user_str_bytes(p.new_ptr as *const u8, &mut (*event).new_path)
            .map(|s| s.len())
            .unwrap_or(0);
        (*event).old_truncated = if old >= 255 { 1 } else { 0 };
        (*event).new_truncated = if new >= 255 { 1 } else { 0 };
        // Same noise rules as opens, applied to where the file ends up.
        if classify_and_drop_if_noise(&(*event).new_path) {
            buf.discard(0);
            return;
        }
    }
    buf.submit(0);
}
