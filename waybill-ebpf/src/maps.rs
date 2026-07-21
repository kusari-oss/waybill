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
/// tracepoint (research R3). Consumed by every file-op kprobe (see
/// `file_ops.rs`) to gate whether the file event should be emitted at
/// all — non-compiler-descendant events are dropped at zero userspace
/// cost per R3.
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
