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
/// (see waybill-ebpf/src/programs/compiler_exec.rs).
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
/// Consumed by `waybill-cli/src/trace/compiler_pipeline.rs` which
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

/// Milestone 213: filter-category discriminant transported kernel↔user
/// via the `FILTER_CATEGORY_HITS` per-CPU array's slot index.
///
/// Discriminants 0-3 are PINNED per contracts/filter-category-tag.md;
/// renumbering breaks compatibility between mismatched kernel-side and
/// userspace-side builds.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum FilterCategoryTag {
    System = 0,
    UserCache = 1,
    Ephemeral = 2,
    CargoFingerprint = 3,
}

impl FilterCategoryTag {
    /// All variants in enum-discriminant order. Used by the userspace
    /// aggregator to iterate the 4 slots of the FILTER_CATEGORY_HITS map.
    pub const ALL: [FilterCategoryTag; 4] = [
        Self::System,
        Self::UserCache,
        Self::Ephemeral,
        Self::CargoFingerprint,
    ];

    /// Human-readable name emitted in
    /// `TraceIntegrity.filter_categories_applied[]`. Values match the
    /// userspace `ClassifyFilterCategory` enum variant names verbatim
    /// per m213 FR-007 so extractor tooling can join across the two
    /// layers with byte-identity comparison.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::UserCache => "UserCache",
            Self::Ephemeral => "Ephemeral",
            Self::CargoFingerprint => "CargoFingerprint",
        }
    }
}

impl TryFrom<u8> for FilterCategoryTag {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0 => Ok(Self::System),
            1 => Ok(Self::UserCache),
            2 => Ok(Self::Ephemeral),
            3 => Ok(Self::CargoFingerprint),
            other => Err(other),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Milestone 213 T004a — wire-shape pin guarding FR-005. Any change
    /// to `FileEvent` that alters `size_of` will fail this test,
    /// catching accidental wire-shape drift that would break every
    /// eBPF ring-buffer producer/consumer pair. Value captured on
    /// 2026-07-21 pre-m213 build on macOS aarch64 stable Rust.
    ///
    /// If this test starts failing, the correct response is to
    /// (a) understand exactly what shape change happened and why,
    /// (b) update EVERY kernel-side + userspace-side reader/writer
    ///     to match, and only then (c) update this pinned value.
    /// Never bump the pinned value in isolation.
    #[test]
    fn file_event_size_is_stable() {
        assert_eq!(std::mem::size_of::<FileEvent>(), 352);
    }

    #[test]
    fn filter_category_tag_u8_round_trip() {
        for cat in FilterCategoryTag::ALL {
            let raw = cat as u8;
            let round_tripped = FilterCategoryTag::try_from(raw).unwrap();
            assert_eq!(cat, round_tripped);
        }
    }

    #[test]
    fn filter_category_tag_name_matches_wire_contract() {
        // FR-007 pins these strings to match the userspace
        // ClassifyFilterCategory enum variant names verbatim.
        // Cross-layer joins via `filter_categories_applied[]` depend
        // on byte-identity between kernel-side names emitted here and
        // userspace-side names emitted by compiler_pipeline.rs.
        assert_eq!(FilterCategoryTag::System.name(), "System");
        assert_eq!(FilterCategoryTag::UserCache.name(), "UserCache");
        assert_eq!(FilterCategoryTag::Ephemeral.name(), "Ephemeral");
        assert_eq!(FilterCategoryTag::CargoFingerprint.name(), "CargoFingerprint");
    }

    #[test]
    fn filter_category_tag_try_from_unknown_discriminant_errors() {
        for bad in [4u8, 5, 42, 255] {
            match FilterCategoryTag::try_from(bad) {
                Err(v) => assert_eq!(v, bad),
                Ok(_) => panic!("unexpected Ok for discriminant {bad}"),
            }
        }
    }

    #[test]
    fn filter_category_tag_all_covers_all_variants() {
        // Guards against future-you adding a variant without updating
        // FilterCategoryTag::ALL, which would break the userspace
        // aggregator's iteration.
        assert_eq!(FilterCategoryTag::ALL.len(), 4);
    }
}
