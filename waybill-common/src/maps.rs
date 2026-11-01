use crate::ip::IpAddr;

/// Per-thread SSL buffer info stored between uprobe entry and return.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SslBufferInfo {
    /// Pointer to the buffer passed to SSL_read/SSL_write
    pub buf_ptr: u64,
    /// Length of the buffer
    pub buf_len: u32,
    pub _padding: u32,
    /// Pointer to the SSL context (for SNI extraction)
    pub ssl_ptr: u64,
}

/// Per-socket connection metadata.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ConnInfo {
    pub pid: u32,
    pub tid: u32,
    pub dst_addr: IpAddr,
    pub dst_port: u16,
    pub _padding: [u8; 6],
    pub established_ns: u64,
}

/// Runtime configuration passed from userspace to eBPF programs.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct TraceConfig {
    /// Maximum bytes of payload to capture in the fragment (default 512)
    pub max_payload_capture: u32,
    /// PID of the tracer itself — events from this PID are always dropped
    /// in-kernel to prevent a feedback loop between probe emission and
    /// userspace ring buffer draining.
    pub tracer_pid: u32,
    /// Whether to compute content hashes in-kernel (0=no, 1=yes)
    pub capture_file_content_hash: u8,
    /// Whether to trace child processes (0=no, 1=yes)
    pub trace_children: u8,
    pub _padding: [u8; 2],
}

/// #1185 — the traced root announces itself to the kernel.
///
/// The tracer cannot learn its own, or its child's, initial-namespace pid:
/// `std::process::id()` and `/proc/self/status`'s `NSpid` both report the
/// container's view, while `bpf_get_current_pid_tgid()` reports the initial
/// namespace's. Comparing them is what made the tracer-exclusion filter a
/// no-op in every containerised run.
///
/// So neither side translates. The child performs one `openat` with this
/// `dfd` and the trace's nonce in `flags`, and the kernel records whatever
/// pid *it* sees — which is the only view that matters, and is correct by
/// construction. The syscall fails with `EBADF`, which is intended: the
/// handshake carries no path and reads nothing.
///
/// `dfd` is a value no caller passes by accident. Real directory fds are
/// non-negative and `AT_FDCWD` is -100; nothing in between is meaningful.
pub const TRACE_HANDSHAKE_DFD: i64 = -0x7ADC0DE;

/// Per-trace nonce, checked alongside `TRACE_HANDSHAKE_DFD` so one trace's
/// handshake cannot arm another's. Spoofing it is harmless in the direction
/// that matters: the gate is positive, so a forged handshake adds only the
/// forger to the traced set and can exclude nothing.
pub const TRACE_HANDSHAKE_NONCE_SLOT: u32 = 0;

/// #1252 — an open the entry tracepoint observed and the exit tracepoint
/// has not resolved yet, stored under the calling thread id.
///
/// The key is the thread id rather than the pid because a syscall's entry
/// and exit run on one thread.
///
/// Lives here rather than beside the map because userspace reads the map's
/// occupancy at trace end (#1185), and that needs the value type and its
/// `aya::Pod` impl on this side of the boundary. `waybill-ebpf` re-exports
/// it, as it already does `TraceConfig`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PendingOpen {
    pub dfd: i64,
    pub path_ptr: u64,
    /// `O_*` flags as the caller passed them. Read from the tracepoint's own
    /// argument slot for every call but `openat2`, whose flags live behind a
    /// user pointer (see `open_syscalls.rs`).
    pub flags: u32,
    pub _padding: u32,
}

// Safety: these types are #[repr(C)], Copy, with explicit padding fields.
// The aya crate requires Pod for types used in eBPF maps.
#[cfg(all(feature = "aya-user", target_os = "linux"))]
unsafe impl aya::Pod for TraceConfig {}
#[cfg(all(feature = "aya-user", target_os = "linux"))]
unsafe impl aya::Pod for ConnInfo {}
#[cfg(all(feature = "aya-user", target_os = "linux"))]
unsafe impl aya::Pod for SslBufferInfo {}
#[cfg(all(feature = "aya-user", target_os = "linux"))]
unsafe impl aya::Pod for PendingOpen {}
