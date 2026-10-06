//! A file operation as the aggregators consume it: the kernel record after
//! userspace has resolved its path and classified it (milestone 1070, #614).
//!
//! Both aggregators (the attestation's file operations, and the compiler
//! pipeline's read and write sets) take this one type, so they agree on what
//! the path is, whether it is resolved, and whether it is a read or a write.

// Used by the trace loop, which only builds on Linux with `ebpf-tracing`.
#![cfg_attr(not(all(target_os = "linux", feature = "ebpf-tracing")), allow(dead_code))]

use std::path::PathBuf;

use waybill_common::events::{FileEvent, FileEventType};

use super::cwd::is_write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedKind {
    Read,
    Write,
    /// A successful rename into `path`. `from` is the resolved old path, or
    /// `None` if it could not be resolved.
    Rename { from: Option<PathBuf> },
}

#[derive(Clone, Debug)]
pub struct ObservedOp {
    pub kind: ObservedKind,
    /// The resolved absolute path or, when `unresolved`, the path exactly as
    /// the build passed it.
    pub path: String,
    /// The path as the kernel captured it, before resolution.
    pub raw_path: String,
    /// `true` when `path` is relative and its directory is unknown.
    pub unresolved: bool,
    pub pid: u32,
    pub tid: u32,
    pub comm: String,
    pub timestamp_ns: u64,
    pub bytes: u64,
    pub content_hash: [u8; 32],
}

impl ObservedOp {
    /// An operation from an `Open`/`Read`/`Write` record taken at face value,
    /// with no path resolution. Opens are reads or writes by their flags. For
    /// callers (and tests) that hold absolute-path records. Returns `None` for
    /// records that are not file operations, and for empty paths.
    pub fn from_file_event(ev: &FileEvent) -> Option<Self> {
        let kind = kind_of(ev)?;
        let path = ev.path_str();
        if path.is_empty() || path == "<invalid>" {
            return None;
        }
        Some(Self::with_path(ev, kind, path.to_string(), false))
    }

    /// The same record with an already-resolved path.
    pub fn with_path(ev: &FileEvent, kind: ObservedKind, path: String, unresolved: bool) -> Self {
        Self {
            kind,
            path,
            raw_path: ev.path_str().to_string(),
            unresolved,
            pid: ev.pid,
            tid: ev.tid,
            comm: ev.comm_str().to_string(),
            timestamp_ns: ev.timestamp_ns,
            bytes: ev.bytes_transferred,
            content_hash: ev.content_hash,
        }
    }
}

/// Read or write, for the record kinds that are file operations.
pub fn kind_of(ev: &FileEvent) -> Option<ObservedKind> {
    match ev.event_type {
        FileEventType::Open if is_write(ev.flags) => Some(ObservedKind::Write),
        FileEventType::Open | FileEventType::Read => Some(ObservedKind::Read),
        FileEventType::Write => Some(ObservedKind::Write),
        FileEventType::Close
        | FileEventType::Fork
        | FileEventType::Chdir
        | FileEventType::Fchdir
        | FileEventType::Rename => None,
    }
}
