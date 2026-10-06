//! Working directory of each traced process, built only from what the trace
//! observed (milestone 1070, #614).
//!
//! Cargo compiles workspace members with paths relative to the workspace
//! root, so a trace that cannot resolve relative opens misses every source
//! read of a workspace member. The kernel cannot report a working directory
//! robustly from a kprobe (no `bpf_d_path`, and `task->fs` offsets need
//! CO-RE), and `/proc/<pid>/cwd` is gone by the time a 6 ms compile is
//! drained, besides naming the wrong PID namespace. So the directory is
//! modelled here from three observed events:
//!
//! - a fork inherits the parent's directory;
//! - a successful `chdir` sets it, resolving a relative target against the
//!   current one;
//! - a successful `fchdir` makes it unknown, since its target is an fd.
//!
//! The traced root is seeded the same way: the tracer spawns it with an
//! explicit working directory, so the child performs a `chdir` the probes
//! see. Anything this model cannot establish is `Unknown`, and an open
//! relative to it is reported as unresolved, never resolved against a
//! guess (Constitution Principle III).
//!
//! See specs/1070-trace-relative-source-reads/research.md R2 and
//! data-model.md.

// Used by the trace loop, which only builds on Linux with `ebpf-tracing`.
#![cfg_attr(not(all(target_os = "linux", feature = "ebpf-tracing")), allow(dead_code))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use waybill_common::events::AT_FDCWD;

const O_ACCMODE: u32 = 0o3;
const O_WRONLY: u32 = 0o1;
const O_RDWR: u32 = 0o2;
const O_CREAT: u32 = 0o100;
const O_TRUNC: u32 = 0o1000;

/// Whether open `flags` mean the file is being written: opened write-only or
/// read-write, or with creation or truncation requested. The values are
/// Linux's generic ones, the same on x86-64 and arm64 (research R5).
pub fn is_write(flags: u32) -> bool {
    let mode = flags & O_ACCMODE;
    mode == O_WRONLY || mode == O_RDWR || flags & (O_CREAT | O_TRUNC) != 0
}

/// A process's working directory, as far as the trace observed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkingDir {
    Known(PathBuf),
    Unknown,
}

/// The outcome of resolving a path an event carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Resolved(PathBuf),
    Unresolved,
}

/// Working directory per process id (tgid). A pid never seen is `Unknown`.
#[derive(Debug, Default)]
pub struct CwdTracker {
    dirs: HashMap<u32, WorkingDir>,
}

impl CwdTracker {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&self, pid: u32) -> WorkingDir {
        self.dirs.get(&pid).cloned().unwrap_or(WorkingDir::Unknown)
    }

    /// `child` starts in `parent`'s directory. A reused pid is overwritten.
    pub fn fork(&mut self, parent: u32, child: u32) {
        let inherited = self.get(parent);
        self.dirs.insert(child, inherited);
    }

    /// A successful `chdir(path)`. A truncated target, or a relative one
    /// against an unknown directory, leaves the process `Unknown`.
    pub fn chdir(&mut self, pid: u32, path: &str, truncated: bool) {
        let next = if truncated || path.is_empty() {
            WorkingDir::Unknown
        } else if Path::new(path).is_absolute() {
            WorkingDir::Known(PathBuf::from(path))
        } else {
            match self.get(pid) {
                WorkingDir::Known(dir) => WorkingDir::Known(dir.join(path)),
                WorkingDir::Unknown => WorkingDir::Unknown,
            }
        };
        self.dirs.insert(pid, next);
    }

    /// A successful `fchdir`: the target is an fd, which the trace does not
    /// map to a path.
    pub fn fchdir(&mut self, pid: u32) {
        self.dirs.insert(pid, WorkingDir::Unknown);
    }

    /// Resolve the path an event carried. Absolute paths are taken as they
    /// are. A relative path resolves only when it is relative to the working
    /// directory (`dfd == AT_FDCWD`), that directory is known, and the path
    /// was not truncated. The join is lexical: no filesystem access and no
    /// `..` canonicalisation, so the path reads as the build named it.
    pub fn resolve(&self, pid: u32, dfd: i32, path: &str, truncated: bool) -> Resolution {
        if Path::new(path).is_absolute() {
            return Resolution::Resolved(PathBuf::from(path));
        }
        if truncated || dfd != AT_FDCWD {
            return Resolution::Unresolved;
        }
        match self.get(pid) {
            WorkingDir::Known(dir) => Resolution::Resolved(dir.join(path)),
            WorkingDir::Unknown => Resolution::Unresolved,
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn resolved(p: &str) -> Resolution {
        Resolution::Resolved(PathBuf::from(p))
    }

    #[test]
    fn fork_inherits_and_unknown_parent_gives_unknown() {
        let mut t = CwdTracker::new();
        t.chdir(1, "/ws", false);
        t.fork(1, 2);
        assert_eq!(t.resolve(2, AT_FDCWD, "src/lib.rs", false), resolved("/ws/src/lib.rs"));
        t.fork(99, 3);
        assert_eq!(t.resolve(3, AT_FDCWD, "src/lib.rs", false), Resolution::Unresolved);
    }

    /// Analysis U1: cargo spawns from worker threads. The fork record carries
    /// the parent's process id, so the child inherits the process's directory
    /// even though a thread did the forking.
    #[test]
    fn fork_from_a_worker_thread_inherits_the_process_directory() {
        let mut t = CwdTracker::new();
        t.chdir(100, "/ws", false); // cargo, pid 100
        t.fork(100, 150); // forked by cargo's thread 117; the record says 100
        assert_eq!(t.resolve(150, AT_FDCWD, "libsafe/src/lib.rs", false), resolved("/ws/libsafe/src/lib.rs"));
    }

    #[test]
    fn chdir_absolute_relative_and_against_unknown() {
        let mut t = CwdTracker::new();
        t.chdir(1, "/ws", false);
        t.chdir(1, "sub", false);
        assert_eq!(t.resolve(1, AT_FDCWD, "a.rs", false), resolved("/ws/sub/a.rs"));
        t.chdir(2, "sub", false); // pid 2 never seen
        assert_eq!(t.resolve(2, AT_FDCWD, "a.rs", false), Resolution::Unresolved);
    }

    #[test]
    fn truncated_chdir_and_fchdir_give_unknown() {
        let mut t = CwdTracker::new();
        t.chdir(1, "/ws", false);
        t.chdir(1, "/very/long/cut-off", true);
        assert_eq!(t.resolve(1, AT_FDCWD, "a.rs", false), Resolution::Unresolved);
        t.chdir(2, "/ws", false);
        t.fchdir(2);
        assert_eq!(t.resolve(2, AT_FDCWD, "a.rs", false), Resolution::Unresolved);
    }

    #[test]
    fn unseen_pid_is_unknown_but_absolute_paths_always_resolve() {
        let t = CwdTracker::new();
        assert_eq!(t.resolve(7, AT_FDCWD, "a.rs", false), Resolution::Unresolved);
        assert_eq!(t.resolve(7, 3, "/abs/a.rs", false), resolved("/abs/a.rs"));
    }

    #[test]
    fn relative_to_a_directory_fd_or_truncated_is_unresolved() {
        let mut t = CwdTracker::new();
        t.chdir(1, "/ws", false);
        assert_eq!(t.resolve(1, 5, "raw-dylibs", false), Resolution::Unresolved);
        assert_eq!(t.resolve(1, AT_FDCWD, "cut-off-relat", true), Resolution::Unresolved);
    }

    #[test]
    fn dotdot_is_kept_verbatim() {
        let mut t = CwdTracker::new();
        t.chdir(1, "/ws/a", false);
        assert_eq!(t.resolve(1, AT_FDCWD, "../b/c.rs", false), resolved("/ws/a/../b/c.rs"));
    }

    #[test]
    fn is_write_classification() {
        assert!(!is_write(0)); // O_RDONLY
        assert!(is_write(O_WRONLY));
        assert!(is_write(O_RDWR));
        assert!(is_write(O_CREAT)); // O_RDONLY|O_CREAT
        assert!(is_write(O_TRUNC)); // O_RDONLY|O_TRUNC
        assert!(is_write(O_RDWR | O_CREAT | O_TRUNC)); // the linker's output open
        assert!(!is_write(0o2000000)); // O_RDONLY|O_CLOEXEC
    }
}
