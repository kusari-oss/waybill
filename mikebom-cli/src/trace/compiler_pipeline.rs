//! Milestone 210 — user-space compiler-pipeline aggregator.
//!
//! Consumes:
//! - `CompilerExecEvent`s from the `COMPILER_EXEC_EVENTS` ring buffer
//!   (populated by `mikebom-ebpf/src/programs/compiler_exec.rs` —
//!   `sched_process_exec` + `sched_process_exit` tracepoints)
//! - `FileEvent`s from the existing `FILE_EVENTS` ring buffer that
//!   carry a `compiler_invocation_id` stamp (added in-kernel per
//!   research R3 when the emitting pid is a compiler descendant)
//!
//! Produces: [`CompilerPipelineData`] carrying the compiler-
//! invocation DAG + per-invocation read/write sets, ready to attach
//! to `BuildTracePredicate.compiler_pipeline`.
//!
//! Cross-platform: compiles on macOS + Windows with default features
//! (matches the existing `aggregator.rs` pattern — the types must
//! exist everywhere because the attestation types reference them
//! unconditionally per research R10). Linux-only code paths live in
//! `loader.rs` + `processor.rs` behind `#[cfg(target_os = "linux")]`.

// See aggregator.rs for the rationale on this attribute.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mikebom_common::attestation::compiler_pipeline::{
    CompilerFamily, CompilerInvocation, CompilerPipelineData, CompletenessState,
    FilterCategory, InvocationDagEdge, PartialReason, ReadKind, ReadSetEntry,
    WriteSetEntry,
};
use mikebom_common::events::{CompilerExecEvent, CompilerExecEventKind, FileEvent, FileEventType};
use mikebom_common::types::hash::{ContentHash, HashAlgorithm, HexString};
use mikebom_common::types::timestamp::Timestamp;

/// Configuration for the FR-016 trace-noise filter.
#[derive(Clone, Debug, Default)]
pub struct FilterConfig {
    /// FR-016 escape hatch — when `true`, bypass all denylist
    /// categories (system, cache, ephemeral, secrets-adjacent) and
    /// include every observed read in the emitted read-set.
    pub include_system_reads: bool,
    /// Home directory prefix used for `~/.cache`, `~/.ssh`, etc.
    /// glob-expansion. Populated from `dirs::home_dir()` at pipeline
    /// startup.
    pub home_dir: Option<PathBuf>,
}

/// User-space aggregator state for the compiler-pipeline trace.
/// See data-model E1..E6.
pub struct CompilerPipelineAggregator {
    /// invocation_id → mostly-complete CompilerInvocation record.
    /// Populated at `Exec`-event time; extended by file-op events;
    /// finalized at trace end.
    invocations: BTreeMap<u64, PartialInvocation>,
    /// pid → invocation_id lookup. Populated on `Exec` events; the
    /// eBPF-side `sched_process_fork` tracepoint propagates
    /// invocation_id to child pids in-kernel (so this map only
    /// carries the ROOT pids of each compiler invocation subtree —
    /// user-space doesn't need to know the descendant pids).
    pid_to_invocation_id: std::collections::HashMap<u32, u64>,
    /// Monotonically-increasing counter for new invocation ids.
    /// Assigned in user-space at `Exec`-event receive time (kernel
    /// doesn't know this counter — R1 note).
    next_invocation_id: u64,
    /// FR-016 filter config.
    filter_config: FilterConfig,
    /// FR-008 signal: total events dropped due to ring-buffer overflow.
    events_dropped: u64,
    /// FR-016a counter: reads filtered because the path matched a
    /// secret-adjacent pattern.
    secrets_read_filtered: u64,
    /// FR-016 accounting: which filter categories actually fired at
    /// least once during this trace.
    filter_categories_applied: std::collections::HashSet<FilterCategory>,
    /// FR-017 signal: was mikebom attached AFTER some compilers had
    /// already started? Set at trace-start if any `Exec` event
    /// reference a pid that had already been observed.
    attach_late: bool,
}

/// A partially-populated CompilerInvocation. Read/write sets grow
/// as events arrive; finalized to a `CompilerInvocation` at trace
/// end via `Self::finalize_invocation`.
struct PartialInvocation {
    invocation_id: u64,
    compiler: CompilerFamily,
    pid: u32,
    ppid: u32,
    parent_invocation_id: Option<u64>,
    cgroup_id: u64,
    start_timestamp: Timestamp,
    end_timestamp: Option<Timestamp>,
    argv_full_path: Option<PathBuf>,
    argv: Vec<String>,
    cwd: Option<PathBuf>,
    exit_code: Option<i32>,
    /// path → (hash, kind) — dedup on path.
    read_set: std::collections::HashMap<PathBuf, (ContentHash, ReadKind)>,
    /// path → (Option<hash>, survived_trace_window) — dedup on path.
    write_set: std::collections::HashMap<PathBuf, (Option<ContentHash>, bool)>,
    /// FR-008 per-invocation drop counter (subset of the aggregator-
    /// wide `events_dropped`; used to attribute drops to specific
    /// invocations for the C132 `affected_component_count` signal).
    events_dropped: u64,
}

impl CompilerPipelineAggregator {
    /// Construct a fresh aggregator. `filter_config` controls the
    /// FR-016 denylist behavior.
    pub fn new(filter_config: FilterConfig) -> Self {
        Self {
            invocations: BTreeMap::new(),
            pid_to_invocation_id: std::collections::HashMap::new(),
            next_invocation_id: 1,
            filter_config,
            events_dropped: 0,
            secrets_read_filtered: 0,
            filter_categories_applied: std::collections::HashSet::new(),
            attach_late: false,
        }
    }

    /// Handle a `CompilerExecEvent` from the `COMPILER_EXEC_EVENTS`
    /// ring buffer. Dispatches to Exec vs Exit branches.
    pub fn handle_compiler_event(&mut self, event: &CompilerExecEvent) {
        match event.kind {
            CompilerExecEventKind::Exec => self.handle_exec(event),
            CompilerExecEventKind::Exit => self.handle_exit(event),
        }
    }

    /// Handle a file-op event that the eBPF side stamped with a
    /// `compiler_invocation_id`. In the MVP the stamp isn't part of
    /// the existing `FileEvent` shape — the eBPF side threads it via
    /// a parallel per-pid map lookup at ring-buffer emit time. This
    /// user-space method takes the pid + FileEvent and looks up the
    /// invocation_id from the per-pid table populated on Exec events.
    ///
    /// Reads that pass the FR-016 filter accumulate into the target
    /// invocation's read_set (with hash lookup deferred to close-time
    /// via the existing hasher.rs). Writes accumulate into write_set.
    pub fn handle_file_event(&mut self, event: &FileEvent) {
        let Some(invocation_id) = self.pid_to_invocation_id.get(&event.pid).copied() else {
            // Not a compiler-descendant — the in-kernel filter should
            // have dropped this event already, but defense-in-depth
            // skip it here in case the filter races.
            return;
        };

        let path_str = event.path_str();
        if path_str == "<invalid>" || path_str.is_empty() {
            return;
        }
        let path = PathBuf::from(path_str);

        // FR-016 denylist check — categorize + drop if matched
        // (unless include_system_reads bypass is set).
        if let Some(category) = self.classify_filter_category(&path) {
            self.filter_categories_applied.insert(category);
            if category == FilterCategory::SecretsAdjacent {
                self.secrets_read_filtered += 1;
            }
            if !self.filter_config.include_system_reads {
                return;
            }
        }

        let Some(invocation) = self.invocations.get_mut(&invocation_id) else {
            return;
        };

        // Convert the fixed-size content_hash bytes → ContentHash
        // newtype. Zero-hash means "not yet hashed" (in-kernel didn't
        // populate; user-space hasher.rs will fill in at close-time).
        let hash = content_hash_from_bytes(&event.content_hash);

        match event.event_type {
            FileEventType::Read | FileEventType::Open => {
                invocation.read_set.insert(path, (hash, ReadKind::File));
            }
            FileEventType::Write => {
                // Placeholder — final hash + survived_trace_window
                // decided at close-time.
                invocation
                    .write_set
                    .entry(path)
                    .or_insert((Some(hash), true));
            }
            FileEventType::Close => {
                // Close events currently no-op — hash population +
                // survived_trace_window resolution live in a future
                // task (T047 wiring around hasher.rs at close-time).
            }
        }
    }

    /// FR-018 — inject a synthetic stdin-input read-set entry when
    /// a compiler invocation consumed input from stdin.
    /// `bytes_read` is the total byte count observed on the read()
    /// side (captured via the existing FileEvent stream when the
    /// path resolves to `/dev/stdin` or when the kernel emits a
    /// pipe-read event with the invocation's pid).
    pub fn record_stdin_input(&mut self, invocation_id: u64, bytes_read: u64) {
        if let Some(invocation) = self.invocations.get_mut(&invocation_id) {
            let path = PathBuf::from("<stdin>");
            let sentinel = ContentHash {
                algorithm: HashAlgorithm::Sha256,
                value: HexString::new(&"0".repeat(64))
                    .expect("64-char zero string is valid hex"),
            };
            invocation.read_set.insert(
                path,
                (sentinel, ReadKind::StdinInput { bytes_read }),
            );
        }
    }

    /// Record that N events were dropped due to ring-buffer overflow.
    /// Contributes to the FR-008 C132 completeness signal.
    pub fn record_events_dropped(&mut self, count: u64) {
        self.events_dropped += count;
    }

    /// Signal that the trace attached AFTER at least one compiler
    /// was already running (FR-017). Emits `CompletenessState::Partial`
    /// with `PartialReason::AttachLate` at finalize.
    pub fn mark_attach_late(&mut self) {
        self.attach_late = true;
    }

    /// Number of invocations captured so far. Test-only accessor.
    #[cfg(test)]
    pub fn invocation_count(&self) -> usize {
        self.invocations.len()
    }

    /// Consume the aggregator + produce the deterministic
    /// [`CompilerPipelineData`] per data-model E6 + research R8.
    /// Invocations sorted by (start_timestamp, pid); read/write
    /// sets sorted by path; dag_edges sorted by (parent, child);
    /// filter_categories_applied sorted lex.
    pub fn finalize(mut self) -> CompilerPipelineData {
        let completeness = self.derive_completeness(self.invocations.len());

        let mut invocations: Vec<CompilerInvocation> = std::mem::take(&mut self.invocations)
            .into_values()
            .map(finalize_invocation)
            .collect();
        // R8 ordering: (start_timestamp, pid) tuple ascending.
        invocations.sort_by(|a, b| {
            a.start_timestamp
                .as_datetime()
                .cmp(b.start_timestamp.as_datetime())
                .then(a.pid.cmp(&b.pid))
        });

        // Build dag_edges from parent_invocation_id links; sort by
        // (parent, child) per R8.
        let mut dag_edges: Vec<InvocationDagEdge> = invocations
            .iter()
            .filter_map(|inv| {
                inv.parent_invocation_id.map(|parent| InvocationDagEdge {
                    parent_invocation_id: parent,
                    child_invocation_id: inv.invocation_id,
                })
            })
            .collect();
        dag_edges.sort_by(|a, b| {
            a.parent_invocation_id
                .cmp(&b.parent_invocation_id)
                .then(a.child_invocation_id.cmp(&b.child_invocation_id))
        });

        // R8 filter_categories_applied sorted lex via ordinal.
        let mut filter_categories_applied: Vec<FilterCategory> =
            self.filter_categories_applied.iter().copied().collect();
        filter_categories_applied.sort();

        CompilerPipelineData {
            invocations,
            dag_edges,
            completeness,
            secrets_read_filtered: self.secrets_read_filtered,
            include_system_reads_flag: self.filter_config.include_system_reads,
            filter_categories_applied,
        }
    }

    fn handle_exec(&mut self, event: &CompilerExecEvent) {
        let invocation_id = self.next_invocation_id;
        self.next_invocation_id += 1;

        // Parent-invocation lookup — if the ppid is in our pid map,
        // this is a child compiler invocation.
        let parent_invocation_id = self.pid_to_invocation_id.get(&event.ppid).copied();

        let compiler = classify_compiler_family(event.comm_str(), event.argv0_str());
        let argv0 = if event.argv0_hint_len > 0 {
            Some(PathBuf::from(event.argv0_str()))
        } else {
            None
        };

        let partial = PartialInvocation {
            invocation_id,
            compiler,
            pid: event.pid,
            ppid: event.ppid,
            parent_invocation_id,
            cgroup_id: event.cgroup_id,
            start_timestamp: Timestamp::now(),
            end_timestamp: None,
            argv_full_path: argv0,
            argv: Vec::new(), // full argv capture is a future extension
            cwd: None,        // future extension via /proc/<pid>/cwd
            exit_code: None,
            read_set: std::collections::HashMap::new(),
            write_set: std::collections::HashMap::new(),
            events_dropped: 0,
        };
        self.invocations.insert(invocation_id, partial);
        self.pid_to_invocation_id.insert(event.pid, invocation_id);
    }

    fn handle_exit(&mut self, event: &CompilerExecEvent) {
        if let Some(&invocation_id) = self.pid_to_invocation_id.get(&event.pid) {
            if let Some(invocation) = self.invocations.get_mut(&invocation_id) {
                invocation.end_timestamp = Some(Timestamp::now());
                invocation.exit_code = Some(event.exit_code);
            }
        }
    }

    fn classify_filter_category(&self, path: &Path) -> Option<FilterCategory> {
        let p = path.to_string_lossy();

        // R5 system + kernel category
        for prefix in ["/etc/", "/proc/", "/sys/", "/dev/"] {
            if p.starts_with(prefix) {
                return Some(FilterCategory::System);
            }
        }

        // R5 ephemeral category
        for prefix in ["/tmp/", "/var/tmp/"] {
            if p.starts_with(prefix) {
                return Some(FilterCategory::Ephemeral);
            }
        }
        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            if p.starts_with(&tmpdir) {
                return Some(FilterCategory::Ephemeral);
            }
        }

        // R5 user-cache category
        if let Some(home) = &self.filter_config.home_dir {
            let home_str = home.to_string_lossy();
            for suffix in [".cache/", ".local/share/"] {
                if p.starts_with(&*format!("{}/{}", home_str, suffix)) {
                    return Some(FilterCategory::UserCache);
                }
            }
        }

        // Q2 secrets-adjacent category — path prefix match.
        for prefix in [
            "/var/run/secrets/",
            "/run/secrets/",
            "/run/keys/",
        ] {
            if p.starts_with(prefix) {
                return Some(FilterCategory::SecretsAdjacent);
            }
        }
        if let Some(home) = &self.filter_config.home_dir {
            let home_str = home.to_string_lossy();
            for suffix in [".ssh/", ".aws/", ".gnupg/"] {
                if p.starts_with(&*format!("{}/{}", home_str, suffix)) {
                    return Some(FilterCategory::SecretsAdjacent);
                }
            }
            let home_prefix = home_str.to_string();
            for full in [
                format!("{}/.docker/config.json", home_prefix),
                format!("{}/.netrc", home_prefix),
                format!("{}/.kube/config", home_prefix),
            ] {
                if p == full {
                    return Some(FilterCategory::SecretsAdjacent);
                }
            }
        }

        // Q2 heuristic — key-file extension match on the basename.
        if let Some(basename) = path.file_name().and_then(|s| s.to_str()) {
            for pat in [".pem", ".key", ".crt", "_rsa", "_ed25519"] {
                if basename.ends_with(pat) {
                    return Some(FilterCategory::SecretsAdjacent);
                }
            }
        }

        None
    }

    fn derive_completeness(&self, total_invocation_count: usize) -> CompletenessState {
        if self.attach_late {
            return CompletenessState::Partial {
                reason: PartialReason::AttachLate,
            };
        }
        if self.events_dropped > 0 {
            return CompletenessState::Degraded {
                dropped: self.events_dropped,
                affected_component_count: total_invocation_count,
            };
        }
        CompletenessState::Complete
    }
}

/// Convert a partial in-flight invocation into the final
/// serializable shape. Read/write sets are converted from
/// path-keyed hashmaps into sorted vecs (R8 determinism).
fn finalize_invocation(p: PartialInvocation) -> CompilerInvocation {
    let mut read_set: Vec<ReadSetEntry> = p
        .read_set
        .into_iter()
        .map(|(path, (sha256, kind))| ReadSetEntry {
            path,
            sha256,
            kind,
        })
        .collect();
    read_set.sort_by(|a, b| a.path.cmp(&b.path));

    let mut write_set: Vec<WriteSetEntry> = p
        .write_set
        .into_iter()
        .map(|(path, (sha256, survived))| WriteSetEntry {
            path,
            sha256,
            survived_trace_window: survived,
        })
        .collect();
    write_set.sort_by(|a, b| a.path.cmp(&b.path));

    CompilerInvocation {
        invocation_id: p.invocation_id,
        compiler: p.compiler,
        pid: p.pid,
        ppid: p.ppid,
        parent_invocation_id: p.parent_invocation_id,
        cgroup_id: p.cgroup_id,
        start_timestamp: p.start_timestamp,
        end_timestamp: p.end_timestamp,
        argv_full_path: p.argv_full_path,
        argv: p.argv,
        cwd: p.cwd,
        exit_code: p.exit_code,
        read_set,
        write_set,
        events_dropped: p.events_dropped,
    }
}

/// Classify a compiler's `comm` string + `argv[0]` into a
/// `CompilerFamily` per R2's two-stage match. comm is preferred
/// (16-byte kernel-limited); argv[0] hint disambiguates truncated
/// variants like `x86_64-linux-gnu-g` (kernel-truncated `g++`).
fn classify_compiler_family(comm: &str, argv0: &str) -> CompilerFamily {
    // Direct comm-field match (most cases).
    match comm {
        "rustc" => return CompilerFamily::Rustc,
        "gcc" => return CompilerFamily::Gcc,
        "g++" => return CompilerFamily::Gpp,
        "clang" => return CompilerFamily::Clang,
        "clang++" => return CompilerFamily::Clang, // Note: could split into Gpp-equiv if a separate family emerges
        "go" => return CompilerFamily::Go,
        "ld" | "ld.lld" | "ld.gold" | "ld.bfd" => return CompilerFamily::Ld,
        "mold" => return CompilerFamily::Mold,
        "cc1" | "cc1plus" => return CompilerFamily::Cc1,
        "cpp" => return CompilerFamily::Cpp,
        "as" => return CompilerFamily::As,
        _ => {}
    }

    // Second-stage disambiguation via argv[0] basename for
    // truncated comm variants + multi-arch prefixes.
    if let Some(basename) = std::path::Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
    {
        if basename.contains("rustc") {
            return CompilerFamily::Rustc;
        }
        if basename.ends_with("g++") || basename.ends_with("-g++") {
            return CompilerFamily::Gpp;
        }
        if basename.ends_with("gcc") || basename.ends_with("-gcc") {
            return CompilerFamily::Gcc;
        }
        if basename.ends_with("clang++") {
            return CompilerFamily::Clang;
        }
        if basename.ends_with("clang") {
            return CompilerFamily::Clang;
        }
    }

    CompilerFamily::Unknown
}

/// Convert a fixed 32-byte content hash from the eBPF event shape
/// into a `ContentHash` newtype. All-zero bytes mean "in-kernel
/// didn't populate; user-space hasher will fill in at close-time"
/// per research R4.
fn content_hash_from_bytes(bytes: &[u8; 32]) -> ContentHash {
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    ContentHash {
        algorithm: HashAlgorithm::Sha256,
        value: HexString::new(&hex).expect("32 bytes → 64 hex chars is always valid"),
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn make_exec_event(pid: u32, ppid: u32, comm: &str) -> CompilerExecEvent {
        let mut comm_bytes = [0u8; 16];
        let cb = comm.as_bytes();
        let n = std::cmp::min(cb.len(), 16);
        comm_bytes[..n].copy_from_slice(&cb[..n]);
        CompilerExecEvent {
            kind: CompilerExecEventKind::Exec,
            timestamp_ns: 1_000_000_000,
            pid,
            ppid,
            cgroup_id: 42,
            comm: comm_bytes,
            argv0_hint: [0u8; 128],
            argv0_hint_len: 0,
            exit_code: 0,
            _padding: [0u8; 2],
        }
    }

    fn make_file_event(
        event_type: FileEventType,
        pid: u32,
        path: &str,
    ) -> FileEvent {
        let mut path_bytes = [0u8; 256];
        let pb = path.as_bytes();
        let n = std::cmp::min(pb.len(), 256);
        path_bytes[..n].copy_from_slice(&pb[..n]);
        FileEvent {
            event_type,
            timestamp_ns: 2_000_000_000,
            pid,
            tid: pid,
            comm: [0u8; 16],
            path: path_bytes,
            path_truncated: 0,
            _path_padding: [0u8; 3],
            flags: 0,
            bytes_transferred: 0,
            content_hash: [0u8; 32],
            inode: 0,
        }
    }

    #[test]
    fn empty_aggregator_finalizes_to_complete_state() {
        let agg = CompilerPipelineAggregator::new(FilterConfig::default());
        let data = agg.finalize();
        assert_eq!(data.completeness, CompletenessState::Complete);
        assert_eq!(data.invocations.len(), 0);
        assert_eq!(data.dag_edges.len(), 0);
        assert_eq!(data.secrets_read_filtered, 0);
    }

    #[test]
    fn exec_event_creates_invocation() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        assert_eq!(agg.invocation_count(), 1);
        let data = agg.finalize();
        assert_eq!(data.invocations.len(), 1);
        assert_eq!(data.invocations[0].compiler, CompilerFamily::Rustc);
        assert_eq!(data.invocations[0].pid, 100);
        assert_eq!(data.invocations[0].ppid, 50);
        assert_eq!(data.invocations[0].parent_invocation_id, None); // no parent invocation known
    }

    #[test]
    fn child_exec_gets_parent_invocation_id() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        // Parent: cargo (well, in our whitelist, `rustc` — but the
        // parent-child pattern still holds).
        agg.handle_compiler_event(&make_exec_event(100, 1, "rustc"));
        // Child: linker spawned by the same "parent" (ppid=100).
        agg.handle_compiler_event(&make_exec_event(200, 100, "ld"));
        let data = agg.finalize();
        assert_eq!(data.invocations.len(), 2);
        let child = data.invocations.iter().find(|i| i.pid == 200).unwrap();
        assert_eq!(child.parent_invocation_id, Some(1));
        // And a corresponding dag_edge:
        assert_eq!(data.dag_edges.len(), 1);
        assert_eq!(data.dag_edges[0].parent_invocation_id, 1);
        assert_eq!(data.dag_edges[0].child_invocation_id, 2);
    }

    #[test]
    fn file_read_event_lands_in_invocation_read_set() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.handle_file_event(&make_file_event(FileEventType::Read, 100, "/src/main.rs"));
        let data = agg.finalize();
        let inv = &data.invocations[0];
        assert_eq!(inv.read_set.len(), 1);
        assert_eq!(inv.read_set[0].path, PathBuf::from("/src/main.rs"));
        assert_eq!(inv.read_set[0].kind, ReadKind::File);
    }

    #[test]
    fn file_event_from_unknown_pid_is_ignored() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        // No Exec event first — the file event references an
        // untracked pid.
        agg.handle_file_event(&make_file_event(FileEventType::Read, 999, "/src/main.rs"));
        let data = agg.finalize();
        assert_eq!(data.invocations.len(), 0);
    }

    #[test]
    fn system_path_filtered_by_default() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.handle_file_event(&make_file_event(FileEventType::Read, 100, "/etc/passwd"));
        let data = agg.finalize();
        assert_eq!(data.invocations[0].read_set.len(), 0);
        assert!(data.filter_categories_applied.contains(&FilterCategory::System));
    }

    #[test]
    fn secrets_path_increments_counter_and_is_filtered() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.handle_file_event(&make_file_event(
            FileEventType::Read,
            100,
            "/var/run/secrets/token",
        ));
        let data = agg.finalize();
        assert_eq!(data.invocations[0].read_set.len(), 0);
        assert_eq!(data.secrets_read_filtered, 1);
        assert!(data
            .filter_categories_applied
            .contains(&FilterCategory::SecretsAdjacent));
    }

    #[test]
    fn key_file_extension_heuristic_flags_secrets_adjacent() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.handle_file_event(&make_file_event(
            FileEventType::Read,
            100,
            "/some/random/path/id_rsa",
        ));
        let data = agg.finalize();
        assert_eq!(data.secrets_read_filtered, 1);
    }

    #[test]
    fn include_system_reads_bypass_lets_filtered_paths_through() {
        let cfg = FilterConfig {
            include_system_reads: true,
            home_dir: None,
        };
        let mut agg = CompilerPipelineAggregator::new(cfg);
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.handle_file_event(&make_file_event(FileEventType::Read, 100, "/etc/passwd"));
        let data = agg.finalize();
        assert_eq!(data.invocations[0].read_set.len(), 1);
        // Counter still increments — auditor mode SHOULD reflect
        // that the filter would have fired.
        assert!(data.filter_categories_applied.contains(&FilterCategory::System));
        assert!(data.include_system_reads_flag);
    }

    #[test]
    fn attach_late_flag_derives_partial_completeness() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.mark_attach_late();
        let data = agg.finalize();
        assert_eq!(
            data.completeness,
            CompletenessState::Partial {
                reason: PartialReason::AttachLate,
            }
        );
    }

    #[test]
    fn events_dropped_derives_degraded_completeness() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "rustc"));
        agg.record_events_dropped(7);
        let data = agg.finalize();
        assert_eq!(
            data.completeness,
            CompletenessState::Degraded {
                dropped: 7,
                affected_component_count: 1,
            }
        );
    }

    #[test]
    fn stdin_input_marker_lands_in_read_set() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        agg.handle_compiler_event(&make_exec_event(100, 50, "gcc"));
        agg.record_stdin_input(1, 1234);
        let data = agg.finalize();
        let inv = &data.invocations[0];
        assert_eq!(inv.read_set.len(), 1);
        assert_eq!(inv.read_set[0].path, PathBuf::from("<stdin>"));
        assert_eq!(
            inv.read_set[0].kind,
            ReadKind::StdinInput { bytes_read: 1234 }
        );
    }

    #[test]
    fn finalize_produces_deterministic_ordering() {
        let mut agg = CompilerPipelineAggregator::new(FilterConfig::default());
        // Intentional out-of-order pids.
        agg.handle_compiler_event(&make_exec_event(300, 1, "rustc"));
        agg.handle_compiler_event(&make_exec_event(100, 1, "gcc"));
        agg.handle_compiler_event(&make_exec_event(200, 1, "clang"));
        agg.handle_file_event(&make_file_event(FileEventType::Read, 300, "/b.rs"));
        agg.handle_file_event(&make_file_event(FileEventType::Read, 300, "/a.rs"));
        let data = agg.finalize();
        // read_set sorted lex by path:
        let inv300 = data.invocations.iter().find(|i| i.pid == 300).unwrap();
        assert_eq!(inv300.read_set[0].path, PathBuf::from("/a.rs"));
        assert_eq!(inv300.read_set[1].path, PathBuf::from("/b.rs"));
    }
}
