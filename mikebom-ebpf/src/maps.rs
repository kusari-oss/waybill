use aya_ebpf::macros::map;
use aya_ebpf::maps::{Array, BloomFilter, HashMap, PerCpuArray, RingBuf};

use mikebom_common::maps::{ConnInfo, SslBufferInfo, TraceConfig};

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
/// (see `mikebom-ebpf/src/programs/compiler_exec.rs`). Separate from
/// `FILE_EVENTS` so overflow accounting per FR-008 is per-event-type.
/// 256 KB per research R7 — sized for typical build fanout (~100-1000
/// compiler invocations per scan).
#[map]
pub static COMPILER_EXEC_EVENTS: RingBuf =
    RingBuf::with_byte_size(256 * 1024, 0);
