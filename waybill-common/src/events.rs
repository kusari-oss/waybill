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
///
/// Every record in the `FILE_EVENTS` ring buffer starts with this value as
/// a `u32`, so userspace dispatches on it before choosing which record to
/// decode (milestone 1070). Userspace reads it as a raw `u32` rather than as
/// this enum, because an unknown value is not a valid `FileEventType`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FileEventType {
    Open = 0,
    Read = 1,
    Write = 2,
    Close = 3,
    /// A process was created. Carried by a [`LineageEvent`].
    Fork = 4,
    /// A successful `chdir`. Carried by a [`FileEvent`] whose `path` is the
    /// target as the process passed it.
    Chdir = 5,
    /// A successful `fchdir`. Carried by a [`FileEvent`] with an empty path.
    Fchdir = 6,
    /// A successful rename. Carried by a [`RenameEvent`].
    Rename = 7,
    /// An open request the kernel discarded because it did not succeed.
    ///
    /// Emitted only when the diagnostic slot is set (`OPEN_DIAG`); userspace
    /// logs it and never admits it to a read or write set. For this variant
    /// alone, [`FileEvent::flags`] carries the negated errno the syscall
    /// returned rather than the open flags — the field is read by event type,
    /// so a consumer that dispatches first cannot confuse the two.
    OpenFailed = 8,
}

impl FileEventType {
    /// The record kind for a raw leading `u32`, or `None` for a value this
    /// build does not know.
    pub fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            0 => Self::Open,
            1 => Self::Read,
            2 => Self::Write,
            3 => Self::Close,
            4 => Self::Fork,
            5 => Self::Chdir,
            6 => Self::Fchdir,
            7 => Self::Rename,
            8 => Self::OpenFailed,
            _ => return None,
        })
    }
}

/// `dfd` value meaning "relative to the working directory".
pub const AT_FDCWD: i32 = -100;

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
    /// `1` when the path filled the buffer and may have been cut off.
    pub path_truncated: u8,
    pub _path_padding: [u8; 3],
    /// Open flags (`O_ACCMODE`, `O_CREAT`, `O_TRUNC`, ...) for `Open`
    /// records; the negated errno for [`FileEventType::OpenFailed`] records
    /// (#1252); 0 otherwise (milestone 1070).
    pub flags: u32,
    /// Directory fd the path is relative to; [`AT_FDCWD`] for the working
    /// directory (milestone 1070).
    pub dfd: i32,
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

/// Milestone 1070: a process was created. Emitted by the
/// `sched_process_fork` tracepoint into `FILE_EVENTS`, so it is ordered
/// with the file records it governs.
///
/// `parent_pid` is the parent's process id (tgid), read in the parent's
/// context. It is not the tracepoint's own `parent_pid` argument, which is
/// the forking thread's id.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct LineageEvent {
    pub event_type: FileEventType,
    pub parent_pid: u32,
    pub timestamp_ns: u64,
    pub child_pid: u32,
    pub _padding: u32,
}

/// Milestone 1070: a successful rename, emitted at syscall exit into
/// `FILE_EVENTS`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct RenameEvent {
    pub event_type: FileEventType,
    pub pid: u32,
    pub timestamp_ns: u64,
    pub tid: u32,
    pub old_dfd: i32,
    pub new_dfd: i32,
    pub old_truncated: u8,
    pub new_truncated: u8,
    pub _padding: [u8; 2],
    pub comm: [u8; 16],
    pub old_path: [u8; 256],
    pub new_path: [u8; 256],
}

#[cfg(feature = "std")]
impl RenameEvent {
    pub fn old_path_str(&self) -> &str {
        nul_terminated(&self.old_path)
    }

    pub fn new_path_str(&self) -> &str {
        nul_terminated(&self.new_path)
    }

    pub fn comm_str(&self) -> &str {
        nul_terminated(&self.comm)
    }
}

#[cfg(feature = "std")]
fn nul_terminated(bytes: &[u8]) -> &str {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..len]).unwrap_or("<invalid>")
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

/// #618 / #1185 — a reason an observed event never reached the recorded
/// sets, named so a consumer can tell which mechanism consumed it.
///
/// Every variant corresponds to a site that used to `continue` or
/// `return` with nothing incremented. That is why an attestation could
/// read `completeness: complete` while compiler invocations were missing
/// from it: the drops were real and the artifact had no vocabulary for
/// them. A zero here means the mechanism did not fire; an absent field
/// means this build does not measure it.
///
/// The kernel-side variants occupy slots `0..KERNEL_SLOTS` of the
/// `EVENT_DROPS` per-CPU array, in discriminant order, following the
/// m213 `FILTER_CATEGORY_HITS` pattern. The userspace variants are
/// counted in process memory and never cross the map boundary.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum EventDropReason {
    // ---- kernel side: slots in EVENT_DROPS, discriminant order ----
    /// The path string could not be read from user memory, so the event
    /// would carry an empty path. Reading at syscall exit assumes the
    /// pages are still mapped; this counts the times they were not.
    KernelPathUnreadable = 0,
    /// The syscall was handed a null path pointer.
    KernelNullPath = 1,
    /// A syscall exit arrived with no stored request for that thread, so
    /// there was nothing to join the result to. Expected at trace start
    /// for syscalls already in flight; a large count means entry events
    /// are being lost.
    KernelExitWithoutRequest = 2,
    /// The entry tracepoint could not stash the in-flight open, so the
    /// exit had nothing to join and the open was lost outright. The map
    /// is bounded and machine-wide, so this is what saturation looks
    /// like: once the slots are gone, every subsequent open anywhere on
    /// the host is dropped until they free up.
    ///
    /// This is the one site in the open path that returned early
    /// without recording anything (#1185). It double-counts with
    /// `KernelExitWithoutRequest` by design — the same lost open
    /// increments both, since an exit whose entry was never stored is
    /// also an exit with no request. That pairing is the point: the
    /// difference between the two is the orphan count attributable to
    /// syscalls already in flight when the probes attached, and this
    /// slot alone is loss the capture caused.
    KernelPendingInsertFailed = 3,
    /// An open was observed from a process outside the traced subtree.
    ///
    /// Until the subtree gate lands these opens are still captured, so
    /// this is a dry run: the count is what the gate *will* drop. That
    /// ordering is deliberate — a positive gate whose seed went missing
    /// drops everything silently, which is #1185's own shape, so the
    /// measurement comes before the behaviour.
    KernelOutsideTracedSubtree = 4,
    /// The traced-subtree set would not take a descendant, so that
    /// process and everything it spawns will look foreign.
    ///
    /// Counted because discarding an `insert` result is exactly how the
    /// in-flight-open map came to lose opens in silence (#1185).
    KernelTracedInsertFailed = 5,

    // ---- userspace side ----
    /// The record's leading kind was a value this build does not know.
    UserUnknownRecordKind = 16,
    /// The record was shorter than the struct it claims to be.
    UserShortRecord = 17,
    /// The event's pid was outside the operator's `--target-pid` set.
    UserPidNotAdmitted = 18,
    /// The event carried an empty or unparseable path. This is where a
    /// kernel-side `KernelPathUnreadable` lands if it is submitted
    /// anyway, so the two should move together.
    UserEmptyPath = 19,
    /// The compiler aggregator's replay buffer was at capacity.
    UserHeldAtCapacity = 21,
    /// Held for replay and never claimed, because the exec record it was
    /// waiting for never arrived.
    UserHeldUnreplayed = 22,
    /// The drain loop reached its deadline while events were still
    /// arriving, so the remainder was abandoned when the programs
    /// unloaded. This is #618's original subject.
    UserDrainAbandoned = 23,
}

impl EventDropReason {
    /// Number of kernel-side slots in the `EVENT_DROPS` map.
    pub const KERNEL_SLOTS: u32 = 6;

    /// The kernel-side variants, in slot order.
    pub const KERNEL: [EventDropReason; 6] = [
        Self::KernelPathUnreadable,
        Self::KernelNullPath,
        Self::KernelExitWithoutRequest,
        Self::KernelPendingInsertFailed,
        Self::KernelOutsideTracedSubtree,
        Self::KernelTracedInsertFailed,
    ];

    /// The userspace variants.
    pub const USER: [EventDropReason; 7] = [
        Self::UserUnknownRecordKind,
        Self::UserShortRecord,
        Self::UserPidNotAdmitted,
        Self::UserEmptyPath,
        Self::UserHeldAtCapacity,
        Self::UserHeldUnreplayed,
        Self::UserDrainAbandoned,
    ];

    /// The name emitted in `TraceIntegrity.event_drop_reasons`. Stable
    /// wire values: a consumer joins on these, so they are not derived
    /// from the variant name by a macro that a rename could silently
    /// change.
    pub fn name(self) -> &'static str {
        match self {
            Self::KernelPathUnreadable => "kernel_path_unreadable",
            Self::KernelNullPath => "kernel_null_path",
            Self::KernelExitWithoutRequest => "kernel_exit_without_request",
            Self::KernelPendingInsertFailed => "kernel_pending_insert_failed",
            Self::KernelOutsideTracedSubtree => "kernel_outside_traced_subtree",
            Self::KernelTracedInsertFailed => "kernel_traced_insert_failed",
            Self::UserUnknownRecordKind => "user_unknown_record_kind",
            Self::UserShortRecord => "user_short_record",
            Self::UserPidNotAdmitted => "user_pid_not_admitted",
            Self::UserEmptyPath => "user_empty_path",
            Self::UserHeldAtCapacity => "user_held_at_capacity",
            Self::UserHeldUnreplayed => "user_held_unreplayed",
            Self::UserDrainAbandoned => "user_drain_abandoned",
        }
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
        // 352 → 360 in milestone 1070: `dfd` added (and `flags` populated).
        assert_eq!(std::mem::size_of::<FileEvent>(), 360);
    }

    /// Milestone 1070 — the two new `FILE_EVENTS` record shapes, pinned for
    /// the same reason as `FileEvent`.
    #[test]
    fn lineage_and_rename_event_sizes_are_stable() {
        assert_eq!(std::mem::size_of::<LineageEvent>(), 24);
        assert_eq!(std::mem::size_of::<RenameEvent>(), 560);
    }

    /// Every record kind round-trips through its raw leading `u32`, which is
    /// how userspace dispatches; an unknown value is rejected, not cast.
    #[test]
    fn file_event_type_round_trips_through_raw() {
        for t in [
            FileEventType::Open,
            FileEventType::Read,
            FileEventType::Write,
            FileEventType::Close,
            FileEventType::Fork,
            FileEventType::Chdir,
            FileEventType::Fchdir,
            FileEventType::Rename,
            FileEventType::OpenFailed,
        ] {
            assert!(FileEventType::from_raw(t as u32) == Some(t));
        }
        // One past the last known variant. The guard matters because
        // userspace reads the leading u32 as a raw value and dispatches on
        // it: a record kind this build does not know must be skipped, not
        // decoded as whichever variant happens to sit at that number.
        assert!(FileEventType::from_raw(9).is_none());
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
