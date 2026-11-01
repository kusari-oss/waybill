use aya_ebpf::{
    helpers::bpf_ktime_get_ns,
    macros::kprobe,
    programs::ProbeContext,
};
// Milestone 211 — `bpf_d_path` import removed alongside the retired
// vfs_open kprobe. The kernel only allows this helper in LSM/fentry/
// fexit/tracing programs; a kprobe on vfs_open failed with `unknown
// func bpf_d_path#147` on every kernel we tested.

use waybill_common::events::{FileEvent, FileEventType, AT_FDCWD};
use waybill_common::filter::path_matches_filter_category;

use crate::helpers::{
    current_comm, current_pid, current_tid, increment_drop_counter,
    increment_filter_category_hit, should_trace,
};
use crate::maps::{FILE_EVENTS, FILE_EVENT_DROPS, FILTER_WIDEN};

/// Milestone 213 (issue #616) — read the `FILTER_WIDEN[0]` config slot
/// to decide whether the System category filter should be gated for
/// this trace. `1` = System filter disabled (operator opted in via
/// `--include-system-reads`); `0` or absent = System filter active
/// (default). Non-zero non-1 values MUST be treated as `1` per
/// contracts/filter-hits-map.md's fail-open-widening rule.
#[inline(always)]
fn widen_system() -> bool {
    match FILTER_WIDEN.get(0) {
        Some(&v) => v != 0,
        None => false,
    }
}

/// Milestone 213 (issue #616) — apply the kernel-side classifier to
/// `path`. Returns `true` if the caller should DROP this event (the
/// classifier matched a category and incremented the category's per-CPU
/// hit counter). Returns `false` if the caller should proceed to
/// `FILE_EVENTS.reserve()` (either no category matched, or the widen
/// flag suppressed the only matching category).
///
/// Called from `open_syscalls::finish_open` and
/// `fs_syscalls::finish_rename` once the path bytes are populated. The
/// open kprobes that used to call it classified before reserving;
/// `finish_open` reserves first and discards on a match, because the path
/// is not known until after the reserve (#1252). Zero cost when no
/// category matches.
#[inline(always)]
pub(crate) fn classify_and_drop_if_noise(path: &[u8; 256]) -> bool {
    match path_matches_filter_category(path, widen_system()) {
        Some(cat) => {
            increment_filter_category_hit(cat as u8);
            true
        }
        None => false,
    }
}

/// kprobe on vfs_write — captures file write operations.
///
/// ssize_t vfs_write(struct file *file, const char __user *buf,
///                   size_t count, loff_t *pos)
#[kprobe]
pub fn vfs_write_entry(ctx: ProbeContext) -> u32 {
    match try_vfs_write(&ctx) {
        Ok(0) => 0,
        _ => 0,
    }
}

fn try_vfs_write(ctx: &ProbeContext) -> Result<u32, i64> {
    if !should_trace() {
        return Ok(0);
    }

    let file_ptr: u64 = ctx.arg(0).ok_or(1i64)?;
    let count: u64 = ctx.arg(2).ok_or(1i64)?;

    let pid = current_pid();
    let tid = current_tid();
    let comm = current_comm();
    let timestamp = unsafe { bpf_ktime_get_ns() };

    // Read file path from struct file -> f_path -> dentry -> d_name
    // This is kernel-version dependent; using simplified approach
    let path = [0u8; 256];
    // In a real implementation, we'd use bpf_d_path or walk the dentry
    // For now, mark as needing userspace path resolution from fd
    let _ = file_ptr; // suppress unused warning

    if let Some(mut buf) = FILE_EVENTS.reserve::<FileEvent>(0) {
        let event = buf.as_mut_ptr();
        unsafe {
            (*event).event_type = FileEventType::Write;
            (*event).timestamp_ns = timestamp;
            (*event).pid = pid;
            (*event).tid = tid;
            (*event).comm = comm;
            (*event).path = path;
            (*event).path_truncated = 0;
            (*event)._path_padding = [0; 3];
            (*event).flags = 0;
            (*event).dfd = AT_FDCWD;
            (*event).bytes_transferred = count;
            (*event).content_hash = [0; 32]; // computed in userspace
            (*event).inode = 0; // populated from file struct
        }
        buf.submit(0);
    } else {
        increment_drop_counter(&FILE_EVENT_DROPS);
    }

    Ok(0)
}

/// kprobe on vfs_read — captures file read operations.
#[kprobe]
pub fn vfs_read_entry(ctx: ProbeContext) -> u32 {
    match try_vfs_read(&ctx) {
        Ok(0) => 0,
        _ => 0,
    }
}

fn try_vfs_read(ctx: &ProbeContext) -> Result<u32, i64> {
    if !should_trace() {
        return Ok(0);
    }

    let file_ptr: u64 = ctx.arg(0).ok_or(1i64)?;
    let count: u64 = ctx.arg(2).ok_or(1i64)?;

    let pid = current_pid();
    let tid = current_tid();
    let comm = current_comm();
    let timestamp = unsafe { bpf_ktime_get_ns() };

    let path = [0u8; 256];
    let _ = file_ptr;

    if let Some(mut buf) = FILE_EVENTS.reserve::<FileEvent>(0) {
        let event = buf.as_mut_ptr();
        unsafe {
            (*event).event_type = FileEventType::Read;
            (*event).timestamp_ns = timestamp;
            (*event).pid = pid;
            (*event).tid = tid;
            (*event).comm = comm;
            (*event).path = path;
            (*event).path_truncated = 0;
            (*event)._path_padding = [0; 3];
            (*event).flags = 0;
            (*event).dfd = AT_FDCWD;
            (*event).bytes_transferred = count;
            (*event).content_hash = [0; 32];
            (*event).inode = 0;
        }
        buf.submit(0);
    } else {
        increment_drop_counter(&FILE_EVENT_DROPS);
    }

    Ok(0)
}

// #1252 — the `do_sys_openat2` and `do_filp_open` kprobes were REMOVED.
//
// They were the only source of file-open events, and on GitHub's
// `ubuntu-24.04` image `20261025.353` `do_filp_open` stopped attaching:
// file-open events for an identical fixture build fell from 902 to 443
// and the attestation still reported itself complete. The names are
// internal kernel functions, so their presence as traceable symbols was
// never promised. Capture now attaches to syscall tracepoints in
// `programs/open_syscalls.rs`, which also carries the note on which
// interfaces this project treats as stable.
//
// Nothing replaced them *here* on purpose: this file is where the kprobes
// lived, and the two approaches should not read as variants of one. What
// is left is the shared noise classifier above and the two vfs probes,
// which are declared and never attached.
//
// Milestone 211 (issue #611) retired a third kprobe, on `vfs_open`, for a
// different reason worth keeping: it called `bpf_d_path`, which the kernel
// restricts to LSM/fentry/fexit/tracing program types, so the program
// never loaded on any kernel tested. If a future miss needs a kernel-side
// path resolution, `fentry` is on that helper's allowlist — but it is
// still an attach to an internal symbol, so prefer another syscall
// tracepoint.
