use aya_ebpf::macros::map;
use aya_ebpf::maps::{Array, BloomFilter, HashMap, PerCpuArray, RingBuf};

use waybill_common::maps::{ConnInfo, SslBufferInfo, TraceConfig};

/// Ring buffer for network trace events (TLS plaintext captures).
/// 8 MB default — sized for high-throughput builds.
#[map]
pub static NETWORK_EVENTS: RingBuf = RingBuf::with_byte_size(8 * 1024 * 1024, 0);

/// Ring buffer for file access events (opens).
/// 128 MB — file events fire much more frequently than network events,
/// and every unrelated process on the host contributes to the stream
/// (do_filp_open is a system-wide kprobe). The userspace drain cadence
/// is 5 ms; if the buffer fills between drains we lose events. 128 MB
/// gives room for ~400k events (~280 B each), comfortably above what a
/// busy container produces during a single HTTPS download.
#[map]
pub static FILE_EVENTS: RingBuf = RingBuf::with_byte_size(128 * 1024 * 1024, 0);

/// Per-thread SSL buffer info stored between uprobe entry and return.
/// Key: thread ID (u64), Value: SslBufferInfo
#[map]
pub static SSL_BUFFERS: HashMap<u64, SslBufferInfo> = HashMap::with_max_entries(1024, 0);

/// Per-socket connection metadata.
/// Key: socket cookie (u64), Value: ConnInfo
#[map]
pub static CONN_INFO: HashMap<u64, ConnInfo> = HashMap::with_max_entries(4096, 0);

/// Bloom filter for in-kernel deduplication of content hashes.
/// Drops duplicate network/file events to reduce ring buffer pressure.
/// NOTE: BloomFilter::contains/insert require &mut self; callers must use
/// get_ptr_mut or similar patterns to obtain mutable access from a static.
#[map]
pub static SEEN_HASHES: BloomFilter<[u8; 32]> = BloomFilter::with_max_entries(65536, 0);

/// Per-CPU scratch buffer for reading TLS plaintext without exceeding
/// the 512-byte BPF stack limit.
#[map]
pub static SCRATCH_BUF: PerCpuArray<[u8; 512]> = PerCpuArray::with_max_entries(1, 0);

/// PIDs to trace. Set by userspace for cgroup-isolated build processes.
/// Key: PID (u32), Value: 1 (present = trace this PID)
#[map]
pub static PID_FILTER: HashMap<u32, u8> = HashMap::with_max_entries(256, 0);

/// Runtime configuration passed from userspace.
/// Index 0 holds the TraceConfig struct.
#[map]
pub static CONFIG: Array<TraceConfig> = Array::with_max_entries(1, 0);

/// Milestone 210 — PID → compiler-invocation-id map. Populated in
/// kernel by the `sched_process_exec` tracepoint when a whitelisted
/// compiler starts; extended to child PIDs by the `sched_process_fork`
/// tracepoint (research R3).
///
/// This comment used to say the map was "consumed by every file-op kprobe
/// (see `file_ops.rs`) to gate whether the file event should be emitted at
/// all — non-compiler-descendant events are dropped at zero userspace cost
/// per R3". **That was not true**, and had not been for as long as the
/// sentence existed: `grep -rn COMPILER_INVOCATIONS waybill-ebpf/src`
/// returns `compiler_exec.rs` and this declaration, nothing else. No
/// file-op program reads it, so nothing gated on compiler ancestry and the
/// capture admitted every process on the machine (#1185).
///
/// Corrected rather than deleted, because the sentence read as a verified
/// fact and a reader who believed it — including the author of the #1185
/// controls — would model the capture as subtree-scoped when it is not.
/// It is restored to a claim when a file-op program actually gates on a
/// traced-descendant set; see the scope check in
/// `scripts/ebpf-integration-test.sh`, which measures what the capture
/// admits instead of asserting it in prose.
///
/// What the map IS used for: `compiler_exec.rs` reads it to assign and
/// propagate invocation ids, and to walk ancestry for `ppid` resolution.
///
/// Value = the invocation-id assigned userspace-side at exec-event
/// receive time. On fork, the child inherits its parent's value so
/// descendant tracking is transitive across arbitrarily deep spawn
/// chains (cargo → rustc → linker → linker's helper).
#[map]
pub static COMPILER_INVOCATIONS: HashMap<u32, u64> =
    HashMap::with_max_entries(4096, 0);

/// Milestone 210 — ring buffer for compiler exec + exit events
/// (see `waybill-ebpf/src/programs/compiler_exec.rs`). Separate from
/// `FILE_EVENTS` so overflow accounting per FR-008 is per-event-type.
/// 256 KB per research R7 — sized for typical build fanout (~100-1000
/// compiler invocations per scan).
#[map]
pub static COMPILER_EXEC_EVENTS: RingBuf =
    RingBuf::with_byte_size(256 * 1024, 0);

/// Milestone 210 (issue #610) — pids that DIRECTLY exec'd a
/// whitelisted compiler (as opposed to pids that inherited a
/// compiler-invocation-id via fork propagation into
/// `COMPILER_INVOCATIONS`). Populated by `sched_process_exec` on
/// whitelist match; queried by the ancestry-walk in the same
/// tracepoint to find the nearest tracked-compiler ancestor for
/// `ppid` resolution.
///
/// Without this distinction the walk terminates prematurely at any
/// pid that inherited a COMPILER_INVOCATIONS entry via fork (e.g.
/// rustc → cc-wrapper: cc-wrapper's pid gets rustc's invocation-id
/// propagated on fork; the walk from ld would see cc-wrapper in
/// COMPILER_INVOCATIONS and stop there instead of continuing up to
/// rustc). Same sizing rationale as COMPILER_INVOCATIONS (4096
/// concurrent direct-exec compiler pids).
///
/// Value = 1 (presence marker only; the actual invocation-id lives
/// in COMPILER_INVOCATIONS).
#[map]
pub static COMPILER_DIRECT_EXECS: HashMap<u32, u8> =
    HashMap::with_max_entries(4096, 0);

/// Milestone 210 (issue #610) — PID → parent-PID map populated by
/// `sched_process_fork` on EVERY fork (unfiltered), so the exec
/// tracepoint can look up `ppid` at exec time without walking
/// `task_struct` (kernel-version-fragile) or requiring BPF CO-RE.
///
/// Emitted event's `ppid` field is `PID_TO_PPID.get(&pid).unwrap_or(0)`.
/// Userspace then joins on `event.ppid` against its own pid-to-
/// invocation-id table to derive the parent invocation-id (research
/// R3). Without this map every emitted event carried `ppid: 0` and
/// `dag_edges: []` stayed empty (issue #610).
///
/// Sized at 32 K entries — every process on the host contributes an
/// entry until it exits, so this needs headroom above typical build-
/// host PID density. On overflow the fork tracepoint's `insert` fails
/// silently; downstream cost is a missing `ppid` on one event rather
/// than a load-time abort.
#[map]
pub static PID_TO_PPID: HashMap<u32, u32> =
    HashMap::with_max_entries(32768, 0);

// Milestone 212 (issue #615) — per-CPU drop counters. Each of the
// three ring buffers below (FILE_EVENTS, NETWORK_EVENTS,
// COMPILER_EXEC_EVENTS) gets a companion PerCpuArray<u64> that eBPF
// programs increment in the `else` branch of every
// `<RINGBUF>.reserve() → None` site. At trace end, userspace sums
// each map across all online CPUs and populates
// `TraceIntegrity.ring_buffer_overflows` with the aggregate —
// replacing the pre-m212 hardcoded `0` that hid a real drop bug
// (#614 investigation showed 80%+ of events being silently lost).
//
// Sizing: single-element per-CPU arrays. 8 bytes per CPU × N CPUs;
// on a 128-core host that's 1 KB per map. Negligible kernel resource.
//
// Per-CPU semantics eliminate cross-CPU atomic contention entirely
// — each CPU has its own u64 slot, no bpf_atomic_add needed. See
// research R1 + contracts/ebpf-verifier-notes.md V-2 for the
// increment pattern.

/// Milestone 212 — per-CPU counter incremented when `FILE_EVENTS.reserve()`
/// returns `None` (ring buffer full). Read at trace end via aya's
/// `PerCpuArray::get(&0, 0)` → summed across all online CPUs →
/// contributes to `TraceIntegrity.ring_buffer_overflows`.
#[map]
pub static FILE_EVENT_DROPS: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(1, 0);

/// Milestone 212 — per-CPU counter for `NETWORK_EVENTS.reserve() → None`.
#[map]
pub static NETWORK_EVENT_DROPS: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(1, 0);

/// Milestone 212 — per-CPU counter for `COMPILER_EXEC_EVENTS.reserve() → None`.
#[map]
pub static COMPILER_EXEC_DROPS: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(1, 0);

// Milestone 213 (issue #616) — kernel-side trace-noise filter maps.
//
// The `path_matches_filter_category` classifier in file_ops.rs drops
// events matching System / UserCache / Ephemeral / CargoFingerprint
// path patterns BEFORE they enter FILE_EVENTS, freeing ring-buffer
// capacity for the actual rustc + linker events the operator cares
// about. See specs/213-kernel-noise-filter/ for the full contract.
//
// FILTER_CATEGORY_HITS: 4-slot per-CPU u64 counter map, one slot per
// FilterCategoryTag variant (data-model.md E2 + contracts/filter-hits-
// map.md). Slot index = FilterCategoryTag discriminant as u32.
// Userspace reads this at trace-end and emits the sorted-deduplicated
// set of category names whose sum > 0 as
// TraceIntegrity.filter_categories_applied[] (FR-006).
//
// FILTER_WIDEN: 1-slot per-CPU u8 flag map (data-model.md E3). Written
// once at loader-time from ScanArgs.include_system_reads; read by the
// classifier at every open to gate the System-category compare per
// FR-010. Value 0 = filter fully active; value 1 = System category
// disabled (UserCache/Ephemeral/CargoFingerprint remain active).

/// Milestone 213 — per-CPU u64 counter incremented every time the
/// classifier drops a matched file-open event. 4 slots; index = category
/// discriminant. Read at trace-end via `PerCpuArray::get(&idx, 0)` +
/// summed across online CPUs → `TraceIntegrity.filter_categories_applied[]`.
#[map]
pub static FILTER_CATEGORY_HITS: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(4, 0);

/// Milestone 213 — per-CPU u8 config flag. Slot 0 holds the widen flag:
/// 0 = System filter active (default), 1 = System filter disabled.
/// Written once at loader time from ScanArgs.include_system_reads; read
/// by the kernel-side classifier at every open per FR-010. Per-CPU
/// semantics eliminate cross-CPU contention on the read.
#[map]
pub static FILTER_WIDEN: PerCpuArray<u8> =
    PerCpuArray::with_max_entries(1, 0);

/// Milestone 1070 — user pointer of a `chdir` target, stored at syscall entry
/// and read at exit only if the call succeeded. Keyed by thread id, because a
/// syscall's entry and exit happen on the same thread. The exit handler always
/// removes the entry.
#[map]
pub static PENDING_CHDIR: HashMap<u32, u64> = HashMap::with_max_entries(4096, 0);

/// Milestone 1070 — arguments of an in-flight rename, by thread id. See
/// `PENDING_CHDIR`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PendingRename {
    pub old_dfd: i64,
    pub old_ptr: u64,
    pub new_dfd: i64,
    pub new_ptr: u64,
}

#[map]
pub static PENDING_RENAME: HashMap<u32, PendingRename> = HashMap::with_max_entries(4096, 0);

/// #1252 — arguments of an in-flight open, by thread id. Same shape and same
/// reasoning as `PendingRename` above: the entry tracepoint stores what the
/// process asked for, the exit tracepoint learns whether it worked.
///
/// The key is the thread id rather than the pid because a syscall's entry and
/// exit run on one thread. The bound was taken to be threads currently
/// inside an open, not opens — a build with 4096 threads blocked in `openat`
/// at one instant would be remarkable.
///
/// #1185 questions that, because it holds only while every entry is
/// eventually removed. The exit tracepoint removes one; a thread that dies
/// inside the syscall does not, and nothing reclaims it for the rest of the
/// trace. These probes are machine-wide, so every thread on the host
/// contributes. Whether that accumulates anywhere near 4096 is a question
/// about the host rather than the build, and it is now measured instead of
/// assumed: `pending_open_entries_at_close` in the attestation is this map's
/// occupancy when the trace ended, and `kernel_pending_insert_failed` counts
/// the opens lost when it would take no more.
pub use waybill_common::maps::PendingOpen;

#[map]
pub static PENDING_OPEN: HashMap<u32, PendingOpen> = HashMap::with_max_entries(4096, 0);

/// #1252 — per-CPU count of open requests discarded because the syscall
/// failed. Counted unconditionally, which is the point: a zero here on a
/// build known to attempt a failed open means the exit tracepoint is not
/// firing, and that is indistinguishable from a clean run if nothing counts
/// it. Reaches `TraceIntegrity.open_failures_discarded`.
#[map]
pub static OPEN_FAILURES_DISCARDED: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(1, 0);

/// #1252 — per-CPU u8 config flag, slot 0: `1` means this host could not
/// provide the result of an open, so capture falls back to recording
/// requests the way it did before this feature (FR-004b).
///
/// Set by the loader when `sys_enter_openat` attached and
/// `sys_exit_openat` did not. The alternative to a fallback would be to
/// abort, which throws away every open on a host that can still see most
/// of them; the alternative to a *flag* would be to hold requests that no
/// exit will ever collect, which is the silent loss this feature exists
/// to remove. So the kernel emits at entry instead, and the loader
/// records the gap — the capture is honest about being a floor.
#[map]
pub static OPEN_REQUEST_ONLY: PerCpuArray<u8> = PerCpuArray::with_max_entries(1, 0);

/// #1185 — the traced subtree: the spawned root and every descendant.
///
/// Seeded by the root itself through the handshake in `open_syscalls.rs`
/// (see `waybill_common::maps::TRACE_HANDSHAKE_DFD` for why the tracer
/// cannot seed it by pid), then extended on every fork by
/// `compiler_exec.rs::try_sched_process_fork` — the same propagation that
/// map already does for `COMPILER_INVOCATIONS`, in the same function.
///
/// Keyed by **tgid**, because every lookup is by tgid: file records carry
/// the process id, so a traced process's threads are covered by its own
/// entry. The fork tracepoint cannot tell a new thread from a new process,
/// so a thread's id is inserted too and never looked up. That is dead
/// weight rather than a correctness problem, and the exit tracepoint
/// reclaims it.
///
/// Sized above `COMPILER_INVOCATIONS` because this holds the whole build,
/// not just compiler descendants, and because the cost of running out is
/// a descendant that looks foreign. `KernelTracedInsertFailed` counts that
/// rather than leaving it silent.
#[map]
pub static TRACED_PIDS: HashMap<u32, u8> = HashMap::with_max_entries(16384, 0);

/// #1185 — opens observed from **inside** the traced subtree, slot 0.
///
/// Not an `EventDropReason`: nothing is dropped here, and the drop slots
/// are a vocabulary for loss. This is the positive half of the dry run,
/// and it is the only way to see whether fork propagation works while the
/// gate is still off — with capture unchanged, the attestation looks
/// identical whether the subtree contains one process or ninety.
///
/// A zero here with a seeded root means propagation is broken: the root
/// announced itself and no descendant inherited.
#[map]
pub static TRACED_SUBTREE_HITS: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);

/// #1185 — where the handshake dies, when it dies.
///
/// Separate from `EVENT_DROPS` on purpose, and not for tidiness: that map
/// is now an input to the completeness claim (`measured_loss` sums
/// `total_lost()`), so a diagnostic counted there would be reported as
/// capture loss. This one is read for diagnosis and feeds nothing.
///
/// It exists because the failure it diagnoses destroys its own evidence.
/// When the root is never established the run aborts before writing an
/// attestation, so every existing counter -- including
/// `kernel_traced_insert_failed`, which covers the seeding path itself --
/// is unreadable. Two hypotheses were refuted by separate means before
/// this was built; counts localise in one run what guessing did not in
/// two.
///
/// Slots, and what each distinguishes:
///
/// - `0` `sys_enter_openat` fired at all. Zero here means the entry
///   program does not run on this kernel, which would explain far more
///   than the handshake.
/// - `1` an open arrived whose `dfd` equalled `TRACE_HANDSHAKE_DFD`.
///   Zero with slot 0 non-zero means the marker's `dfd` is not arriving
///   as written.
/// - `2` the `dfd` matched and the nonce did not. Non-zero means compare
///   the value userspace wrote against what the slot reads back.
/// - `3` `is_handshake` returned true, so seeding was attempted. Non-zero
///   with an empty `TRACED_PIDS` means the insert failed (see
///   `kernel_traced_insert_failed`) or userspace is reading a different
///   map than the kernel wrote.
/// - `4` the low 32 bits of `dfd` equalled the sentinel's and the full
///   64 bits did not. The kernel's `dfd` is an `int` reported in an
///   8-byte slot the format declares `signed:0`; a kernel that
///   zero-extends the truncated `int` yields
///   `0x00000000F8523F42` where one passing the raw register yields
///   `0xFFFFFFFFF8523F42`. Non-zero here names that difference exactly,
///   and the comparison should be on the low 32 bits.
/// - `7` an open was observed from a pid that is **not** the tracer.
///   The unifying control. If this is zero while slot 0 counts
///   hundreds, the probe sees only the tracing process, which would
///   explain the handshake and the x86_64 shortfall -- 29 operations
///   against 102, with not one `.rs` read -- as one fault rather than
///   two.
/// - `6` an open arrived whose `flags` equalled the nonce, whatever its
///   `dfd`. This is the control for the one thing the marker's design
///   rests on: that an open with a deliberately invalid `dfd` is
///   observed at syscall entry. If slot 6 fires while slot 1 stays
///   zero, the kernel raises this tracepoint after validating `dfd`,
///   and an invalid-`dfd` marker cannot be seen on it by construction.
/// - `5` any open with a negative `dfd` other than `AT_FDCWD`. A
///   catch-all: if slot 4 is zero too, the marker is being transformed
///   some third way, or is not reaching this tracepoint at all.
#[map]
pub static HANDSHAKE_DIAG: PerCpuArray<u64> = PerCpuArray::with_max_entries(8, 0);

/// #1185 — the per-trace handshake nonce, written by the loader before the
/// child is spawned. Slot `TRACE_HANDSHAKE_NONCE_SLOT`. A zero nonce means
/// no handshake is armed, and no pid can be seeded.
#[map]
pub static TRACE_HANDSHAKE_NONCE: Array<u32> = Array::with_max_entries(1, 0);

/// #1185 — per-CPU u8 config flag, slot 0: `1` makes the entry tracepoint
/// refuse to stash the in-flight open, as a full `PENDING_OPEN` would.
///
/// A counter nobody has seen fire is not a measurement. The new
/// `kernel_pending_insert_failed` slot counts a map that would not take
/// the request, and saturating a 4096-entry machine-wide map on demand is
/// not something a test can arrange — so the branch is made reachable
/// instead, the way `OPEN_REQUEST_ONLY` above makes the FR-004b fallback
/// reachable without waiting for a host that lacks the exit tracepoint.
///
/// What it establishes: that the count reaches the attestation, and what
/// a capture looks like when the request cannot be held — every open
/// lost, which is the #1185 symptom. What it does NOT establish: that a
/// genuinely full map returns an error here. That claim rests on
/// `bpf_map_update_elem` rejecting an insert into a full hash map, and
/// only a real saturation would confirm it.
#[map]
pub static PENDING_INSERT_FORCE_FAIL: PerCpuArray<u8> = PerCpuArray::with_max_entries(1, 0);

/// #618 / #1185 — per-CPU counters for kernel-side drops that used to be
/// invisible, one slot per `EventDropReason::KERNEL` variant in
/// discriminant order. Same shape and same reasoning as m213's
/// `FILTER_CATEGORY_HITS`.
///
/// Each slot counts a site that returned early without recording
/// anything. The point is not the number but the distinction: a zero
/// here means the mechanism did not fire, where before there was no way
/// to tell that from nobody looking.
///
/// Sized from `KERNEL_SLOTS` rather than a literal, because
/// `increment_event_drop` reaches the slot through `get_ptr_mut`, which
/// returns `None` for an index past the end and silently counts
/// nothing. A variant added to the enum without widening the map would
/// therefore read zero forever — a counter that cannot see its own
/// subject, indistinguishable from a mechanism that never fired.
#[map]
pub static EVENT_DROPS: PerCpuArray<u64> =
    PerCpuArray::with_max_entries(waybill_common::events::EventDropReason::KERNEL_SLOTS, 0);

/// #1252 — per-CPU u8 config flag, slot 0: `1` makes a discarded open
/// reportable as an `OpenFailed` record carrying the path and the errno,
/// `0` (the default) drops it after counting. Same shape as `FILTER_WIDEN`
/// above, and written once at loader time.
///
/// It exists so a removed read-set entry can be attributed to a failed
/// request by name rather than accepted because a count matched.
#[map]
pub static OPEN_DIAG: PerCpuArray<u8> = PerCpuArray::with_max_entries(1, 0);
