use crate::ip::IpAddr;

/// Type of network event observed by eBPF probes.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum NetworkEventType {
    ConnEstablished = 0,
    TlsRead = 1,
    TlsWrite = 2,
    ConnClosed = 3,
}

/// A network event emitted from eBPF ring buffer.
///
/// This struct is `#[repr(C)]` for shared use between kernel-space
/// eBPF programs and userspace. All fields are fixed-size.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NetworkEvent {
    pub event_type: NetworkEventType,
    pub timestamp_ns: u64,
    pub pid: u32,
    pub tid: u32,
    pub comm: [u8; 16],
    pub conn_id: u64,
    pub src_addr: IpAddr,
    pub src_port: u16,
    pub dst_addr: IpAddr,
    pub dst_port: u16,
    pub payload_size: u32,
    /// SHA-256 of the payload, computed in-kernel when feasible.
    pub payload_hash: [u8; 32],
    /// First 512 bytes of payload for HTTP header parsing.
    pub payload_fragment: [u8; 512],
    pub payload_truncated: u8,
    pub _padding: [u8; 3],
}

/// Type of file operation observed by eBPF probes.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FileEventType {
    Open = 0,
    Read = 1,
    Write = 2,
    Close = 3,
}

/// A file access event emitted from eBPF ring buffer.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct FileEvent {
    pub event_type: FileEventType,
    pub timestamp_ns: u64,
    pub pid: u32,
    pub tid: u32,
    pub comm: [u8; 16],
    pub path: [u8; 256],
    pub path_truncated: u8,
    pub _path_padding: [u8; 3],
    pub flags: u32,
    pub bytes_transferred: u64,
    /// SHA-256 of content when available.
    pub content_hash: [u8; 32],
    pub inode: u64,
}

#[cfg(feature = "std")]
impl NetworkEvent {
    /// Extract the process command name as a string.
    pub fn comm_str(&self) -> &str {
        let len = self.comm.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&self.comm[..len]).unwrap_or("<invalid>")
    }

    /// Extract the payload fragment as bytes (up to payload_size or 512).
    pub fn payload_bytes(&self) -> &[u8] {
        let len = core::cmp::min(self.payload_size as usize, 512);
        &self.payload_fragment[..len]
    }
}

#[cfg(feature = "std")]
impl FileEvent {
    /// Extract the file path as a string.
    pub fn path_str(&self) -> &str {
        let len = self.path.iter().position(|&b| b == 0).unwrap_or(256);
        core::str::from_utf8(&self.path[..len]).unwrap_or("<invalid>")
    }

    /// Extract the process command name as a string.
    pub fn comm_str(&self) -> &str {
        let len = self.comm.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&self.comm[..len]).unwrap_or("<invalid>")
    }
}

/// Milestone 210: kind of a compiler-pipeline event emitted from
/// the `sched_process_exec` + `sched_process_fork` tracepoints
/// (see mikebom-ebpf/src/programs/compiler_exec.rs).
///
/// `Fork` events propagate the parent's compiler-invocation-id to
/// the child in-kernel (research R3); user-space only sees `Exec`
/// events surfaced via the COMPILER_EXEC_EVENTS ring buffer plus
/// the correlated file-op events (which already reach user-space
/// via the existing FILE_EVENTS ring buffer, now stamped with a
/// compiler_invocation_id when the emitting pid is a compiler
/// descendant).
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum CompilerExecEventKind {
    /// A whitelisted compiler binary exec'd.
    Exec = 0,
    /// A tracked compiler-descendant process exited. Used to set
    /// `end_timestamp` + `exit_code` on the CompilerInvocation.
    Exit = 1,
}

/// Milestone 210: a compiler-pipeline event emitted from eBPF.
/// `#[repr(C)]` for shared use between kernel + userspace, matching
/// the NetworkEvent + FileEvent pattern above.
///
/// Consumed by `mikebom-cli/src/trace/compiler_pipeline.rs` which
/// assembles the compiler-invocation DAG + read/write sets.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CompilerExecEvent {
    pub kind: CompilerExecEventKind,
    pub timestamp_ns: u64,
    pub pid: u32,
    pub ppid: u32,
    pub cgroup_id: u64,
    /// Process comm-field (16-byte kernel-limited; matches Exec /
    /// Exit filtered against the whitelist in-kernel per research R2).
    pub comm: [u8; 16],
    /// Best-effort argv[0] path capture via `bpf_probe_read_user`.
    /// Populated on `Exec` events only; zero on `Exit`.
    ///
    /// Sized at 16 bytes (matching `comm`) because the aarch64 eBPF
    /// verifier rejected larger fixed-size zero-inits — the LLVM
    /// backend emits a bounded store-loop that runs past the verifier's
    /// per-program instruction budget when the array is 128 bytes.
    /// Anything past the first 16 chars of `argv[0]` gets truncated;
    /// full `/usr/local/cargo/bin/rustc` becomes `/usr/local/carg` which
    /// is still enough for the whitelist-recognition heuristic since
    /// `classify_compiler_family` at the userspace side falls back to
    /// `comm` (the last 15 chars of the exec target's basename) when
    /// `argv0_str` doesn't uniquely resolve. Grow this back once
    /// milestone T047a rewrites the emit path to use a size-independent
    /// memzero strategy that the verifier accepts.
    pub argv0_hint: [u8; 16],
    /// Populated length of `argv0_hint`; may be less than 16.
    pub argv0_hint_len: u16,
    /// `Exit` only — captured from `sched_process_exit`. Zero on
    /// `Exec` events.
    pub exit_code: i32,
    pub _padding: [u8; 2],
}

#[cfg(feature = "std")]
impl CompilerExecEvent {
    /// Extract the process command name as a string.
    pub fn comm_str(&self) -> &str {
        let len = self.comm.iter().position(|&b| b == 0).unwrap_or(16);
        core::str::from_utf8(&self.comm[..len]).unwrap_or("<invalid>")
    }

    /// Extract the argv[0] hint as a string. Truncated to
    /// `argv0_hint_len` bytes (max 16).
    pub fn argv0_str(&self) -> &str {
        let len = core::cmp::min(self.argv0_hint_len as usize, 16);
        let bytes = &self.argv0_hint[..len];
        // Trim trailing nulls that in-kernel read may have left.
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
        core::str::from_utf8(&bytes[..end]).unwrap_or("<invalid>")
    }
}
