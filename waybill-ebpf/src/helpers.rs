use aya_ebpf::helpers::{bpf_get_current_comm, bpf_get_current_pid_tgid};
use aya_ebpf::maps::PerCpuArray;

use crate::maps::CONFIG;

/// Check if the current process should be traced.
///
/// Drops events originating from the tracer itself (waybill userspace) to
/// prevent a feedback loop where ring buffer draining generates new events.
/// Broader PID filtering (e.g. limiting to a specific build command) is
/// applied in userspace against the aggregated stream.
#[inline(always)]
pub fn should_trace() -> bool {
    let pid = current_pid();
    if let Some(cfg) = CONFIG.get(0) {
        if cfg.tracer_pid != 0 && cfg.tracer_pid == pid {
            return false;
        }
    }
    true
}

/// Get the current PID (thread group ID).
#[inline(always)]
pub fn current_pid() -> u32 {
    let pid_tgid = unsafe { bpf_get_current_pid_tgid() };
    (pid_tgid >> 32) as u32
}

/// Get the current TID (thread ID).
#[inline(always)]
pub fn current_tid() -> u32 {
    let pid_tgid = unsafe { bpf_get_current_pid_tgid() };
    (pid_tgid & 0xFFFFFFFF) as u32
}

/// Get the current process command name.
#[inline(always)]
pub fn current_comm() -> [u8; 16] {
    bpf_get_current_comm().unwrap_or([0u8; 16])
}

/// Milestone 212 (issue #615) — increment a per-CPU drop counter
/// when a ring buffer's `reserve()` returns `None`.
///
/// Called from the `else` branch of every
/// `if let Some(mut buf) = <RINGBUF>.reserve() { ... }` site in the
/// eBPF programs. Because the map is per-CPU, no atomic op is needed
/// — each CPU has its own u64 slot updated with a plain increment.
/// `saturating_add` eliminates the theoretical u64-overflow UB (at
/// max observed drop rate a u64 takes ~600 years to overflow, but
/// the intrinsic is a one-instruction change so it's a free win).
///
/// See contracts/ebpf-verifier-notes.md V-2 for the canonical pattern.
#[inline(always)]
pub fn increment_drop_counter(map: &PerCpuArray<u64>) {
    if let Some(counter) = map.get_ptr_mut(0) {
        unsafe {
            *counter = (*counter).saturating_add(1);
        }
    }
}

/// Milestone 213 (issue #616) — increment the `FILTER_CATEGORY_HITS`
/// counter for a specific category slot. `cat` is the
/// `FilterCategoryTag` discriminant (0=System, 1=UserCache,
/// 2=Ephemeral, 3=CargoFingerprint) per contracts/filter-hits-map.md.
///
/// Called only on the drop path — after the kernel-side classifier
/// matched an open-syscall path to one of the four filter categories,
/// BEFORE returning early from the kprobe (before `FILE_EVENTS.reserve`).
/// Zero cost when the classifier returns `None`.
///
/// `get_ptr_mut(cat as u32)` returns `None` if the slot index is
/// out-of-bounds, which the `if let` gracefully skips — protects against
/// a future variant addition that forgets to bump `FILTER_CATEGORY_HITS`'s
/// `max_entries` from 4 to 5.
#[inline(always)]
pub fn increment_filter_category_hit(cat: u8) {
    if let Some(counter) = crate::maps::FILTER_CATEGORY_HITS.get_ptr_mut(cat as u32) {
        unsafe {
            *counter = (*counter).saturating_add(1);
        }
    }
}
