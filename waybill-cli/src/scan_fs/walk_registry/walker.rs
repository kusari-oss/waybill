//! `SharedWalker` — the one-pass filesystem walker. See `data-model.md`
//! §"SharedWalker" and `contracts/registry-api.md` C1/C5/C6.
//!
//! Descent algorithm (single pass, contract C6 zero-extra-syscalls):
//!
//! ```text
//! walk_inner(current, depth_remaining):
//!     if depth_remaining == 0: return
//!     canonical = canonicalize(current); on err: return (m114)
//!     if visited.contains(canonical): return           (m054 loop guard)
//!     visited.insert(canonical)
//!     metrics.tick_dir()
//!
//!     entries = read_dir(current); on err: return      (m114)
//!     collect entries → sort by filename              (C3)
//!     dir_index.insert(canonical, sorted_filenames)   (feeds C2)
//!
//!     for file in files:
//!         ctx = SharedWalkerContext::new(...)
//!         dispatched = dispatch_file(...)              (C1 + C4)
//!         metrics.tick_file(dispatched)
//!
//!     for subdir in subdirs (unless skipped):
//!         walk_inner(subdir, depth_remaining - 1)
//!
//!     ctx = SharedWalkerContext::new(...)              (fresh borrow)
//!     dispatch_dir(current, ...)                       (C1 + C4)
//! ```

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::dir_index::DirIndex;
use super::dispatch;
use super::perf_metrics::WalkerMetrics;
use super::registry::ReaderRegistry;
use super::walk_context::SharedWalkerContext;
use super::ReaderId;
use crate::scan_fs::package_db::exclude_path::ExclusionSet;
use crate::scan_fs::package_db::project_roots::should_skip_default_descent;
use crate::scan_fs::package_db::PackageDbEntry;

/// Default max descent depth — matches the effective unbounded posture
/// of the legacy `safe_walk` (which defaulted to `usize::MAX` in the
/// m054/m114 config).
const DEFAULT_MAX_DEPTH: usize = usize::MAX;

pub struct SharedWalker<'reg, 'ex> {
    rootfs: PathBuf,
    registry: &'reg ReaderRegistry,
    exclude_set: &'ex ExclusionSet,
    max_depth: usize,
    visited: HashSet<PathBuf>,
    dir_index: DirIndex,
    metrics: WalkerMetrics,
    output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>,
}

impl<'reg, 'ex> SharedWalker<'reg, 'ex> {
    pub fn new(
        rootfs: &Path,
        registry: &'reg ReaderRegistry,
        exclude_set: &'ex ExclusionSet,
    ) -> Self {
        let reader_ids = registry.reader_ids();
        let mut output: HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>> =
            HashMap::with_capacity(reader_ids.len());
        for id in &reader_ids {
            output.insert(*id, Mutex::new(Vec::new()));
        }
        Self {
            rootfs: rootfs.to_path_buf(),
            registry,
            exclude_set,
            max_depth: DEFAULT_MAX_DEPTH,
            visited: HashSet::new(),
            dir_index: DirIndex::new(),
            metrics: WalkerMetrics::new(&reader_ids),
            output,
        }
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Run the shared walker once, populating the dir index + dispatching
    /// callbacks. Idempotent-ish: calling twice will re-walk (and re-skip
    /// via the visited set), but that isn't the intended use — the
    /// walker is designed to run exactly once per scan.
    pub fn run(&mut self) {
        // Snapshot rootfs to avoid a self-borrow tangle during recursion.
        let root = self.rootfs.clone();
        // Root descent: no scope restriction — every registration is active.
        self.walk_inner(&root, self.max_depth, None);
    }

    fn walk_inner(
        &mut self,
        current: &Path,
        depth_remaining: usize,
        // Contract C10 (m664 post-2026-08-23 API extension):
        // `Some(set)` restricts dispatch under this subtree to the
        // named readers — engaged when descending into a normally-
        // skipped dir via any registration's `descend_into` override.
        // `None` = no restriction (all readers active).
        scope: Option<&HashSet<ReaderId>>,
    ) {
        if depth_remaining == 0 {
            return;
        }

        // C5 (m054): canonicalize + visited-set for symlink-loop safety.
        let Ok(canonical) = std::fs::canonicalize(current) else {
            // m114 permissive: silently skip unreadable / non-existent dir.
            tracing::debug!(
                target: "waybill::scan_fs::walk_registry",
                path = %current.display(),
                "safe_walk-equivalent: canonicalize failed; skipping",
            );
            return;
        };
        if !self.visited.insert(canonical.clone()) {
            return;
        }

        // Check m113 ExclusionSet against the scan-root-relative path.
        // ExclusionSet::matches wants a scan-relative forward-slash path;
        // compute it from the canonicalized current path.
        if let Some(rel) = rel_scan_path(&canonical, &self.rootfs) {
            if self.exclude_set.matches(&rel) {
                tracing::debug!(
                    target: "waybill::scan_fs::walk_registry",
                    path = %current.display(),
                    "safe_walk-equivalent: skipping directory matched by ExclusionSet (m113)",
                );
                return;
            }
        }

        self.metrics.tick_dir();

        // Read the directory's entries. m114 permissive on error.
        let Ok(read_dir) = std::fs::read_dir(current) else {
            tracing::debug!(
                target: "waybill::scan_fs::walk_registry",
                path = %current.display(),
                "safe_walk-equivalent: read_dir failed; skipping contents",
            );
            return;
        };

        // We use CANONICAL paths for both DirIndex keys AND for the
        // paths dispatched to reader callbacks. Rationale: `siblings_of`
        // computes `file.parent()` to look up its DirIndex entry; if we
        // dispatch a non-canonical file path (e.g. macOS `/var/folders/…`
        // for a canonical `/private/var/folders/…`), the parent lookup
        // misses the canonical DirIndex key. Building child paths from
        // `canonical.join(child_name)` keeps the two coherent — and
        // adds no syscalls (we already have `canonical` in hand).
        let mut filenames: Vec<OsString> = Vec::new();
        // Per-subdir descent decision: (canonical_child_path,
        // Option<HashSet<ReaderId>> = restricted-scope-for-descent).
        // `None` means "descend with the current scope unchanged"
        // (normal-non-skipped subdir). `Some(set)` means "descend into
        // a normally-skipped dir with dispatch restricted to `set`".
        let mut subdirs: Vec<(PathBuf, Option<HashSet<ReaderId>>)> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        let registrations = self.registry.registrations();
        for entry in read_dir.flatten() {
            let name = entry.file_name();
            let Ok(file_type) = entry.file_type() else { continue };
            let child_path = canonical.join(&name);
            if file_type.is_dir() {
                if !should_skip_by_basename(&name) {
                    // Normal descent — inherit `scope`.
                    subdirs.push((child_path, None));
                } else {
                    // Normally-skipped dir — check whether any
                    // registration's `descend_into` opts back in.
                    // Contract C10 (m664 post-2026-08-23 API extension).
                    if let Some(descend_scope) =
                        compute_descend_into_scope(&name, registrations, scope)
                    {
                        subdirs.push((child_path, Some(descend_scope)));
                    }
                }
                filenames.push(name);
            } else if file_type.is_file() {
                filenames.push(name);
                files.push(child_path);
            } else if file_type.is_symlink() {
                // Symlink target may resolve to either a file or a
                // directory. Legacy `safe_walk` used `Path::is_file()`
                // / `Path::is_dir()` — both FOLLOW symlinks — so a
                // symlink-to-file was dispatched as a file and a
                // symlink-to-dir was descended into (with the m054
                // canonicalize-visited-set catching loops).
                //
                // Post-2026-08-23 fix: match legacy behavior by
                // stat-following the symlink target. Preserves
                // FR-006 byte-identity for fixtures with symlink-to-
                // file entries (e.g. pytorch `docs/requirements.txt`
                // → `../.ci/docker/requirements-docs.txt`).
                match std::fs::metadata(&child_path) {
                    Ok(target_meta) => {
                        if target_meta.is_dir() {
                            // Symlink to a directory — descend, subject
                            // to the same skip/descend_into rules as
                            // real dirs. m054 visited-set (keyed on
                            // canonicalized target) catches loops.
                            if !should_skip_by_basename(&name) {
                                subdirs.push((child_path, None));
                            } else if let Some(descend_scope) =
                                compute_descend_into_scope(&name, registrations, scope)
                            {
                                subdirs.push((child_path, Some(descend_scope)));
                            }
                        } else if target_meta.is_file() {
                            files.push(child_path);
                        }
                        // Broken symlinks + non-file/non-dir targets
                        // (block devs, sockets): skip dispatch. Legacy
                        // `Path::is_file()` returns false for these too.
                    }
                    Err(_) => {
                        // Dangling symlink or permission denied on
                        // target — legacy `Path::is_file()` also
                        // returned false here. Skip dispatch.
                    }
                }
                filenames.push(name);
            } else {
                // Block devs, sockets, FIFOs: not dispatched (matches
                // legacy — `Path::is_file()` returns false for these).
                filenames.push(name);
            }
        }

        // C3: sort the filename list before insertion into DirIndex.
        // sort_unstable is fine — filenames within one dir are unique.
        filenames.sort_unstable();
        // Deterministic file visitation order for FR-006 byte-identity.
        files.sort_unstable();
        subdirs.sort_by(|a, b| a.0.cmp(&b.0));

        // Insert the dir's sorted filename list BEFORE dispatching, so
        // any callback that consults `ctx.dir_index()` for the current
        // directory sees a fully populated view.
        self.dir_index.insert(canonical.clone(), filenames);

        // Dispatch file-visit events.
        for file in &files {
            let dispatched_to = {
                let ctx = SharedWalkerContext::new(
                    &self.dir_index,
                    self.exclude_set,
                    &self.output,
                    registrations,
                );
                dispatch::dispatch_file(file, registrations, &ctx, scope)
            };
            self.metrics.tick_file(&dispatched_to);
        }

        // Recurse into subdirs. Each subdir carries EITHER the current
        // `scope` (normal descent) OR a fresh restricted scope
        // (descend-into override triggered).
        for (subdir, subdir_scope) in &subdirs {
            let effective_scope = subdir_scope.as_ref().or(scope);
            self.walk_inner(subdir, depth_remaining - 1, effective_scope);
        }

        // AFTER children are done, invoke on_dir callbacks for the
        // current directory. Pass the CANONICAL path (matches DirIndex
        // keys, matches what child files were dispatched under).
        let ctx = SharedWalkerContext::new(
            &self.dir_index,
            self.exclude_set,
            &self.output,
            registrations,
        );
        let _dispatched_dir_readers = dispatch::dispatch_dir(&canonical, registrations, &ctx, scope);
        // Note: tick_dir was already called at descent-entry; we don't
        // double-count dispatches via tick_file for dir callbacks.
    }

    /// Consume the walker, emit the FR-009 diagnostic log, and return
    /// per-reader outputs. Mutex poisoning (from a panicked reader per
    /// C4) is accepted — `into_inner` recovers the vector.
    pub fn finish(self) -> HashMap<ReaderId, Vec<PackageDbEntry>> {
        self.metrics.emit();
        let mut out: HashMap<ReaderId, Vec<PackageDbEntry>> =
            HashMap::with_capacity(self.output.len());
        for (id, mutex) in self.output {
            let entries = match mutex.into_inner() {
                Ok(v) => v,
                Err(poisoned) => poisoned.into_inner(),
            };
            out.insert(id, entries);
        }
        out
    }

    // ---- test accessors ----
    #[cfg(test)]
    pub(crate) fn dir_index_ref(&self) -> &DirIndex {
        &self.dir_index
    }

    #[cfg(test)]
    pub(crate) fn metrics_ref(&self) -> &WalkerMetrics {
        &self.metrics
    }
}

/// True iff `basename` is a directory name matching the shared
/// default-descent skip set (same set as `should_skip_default_descent`).
fn should_skip_by_basename(basename: &std::ffi::OsStr) -> bool {
    basename
        .to_str()
        .map(should_skip_default_descent)
        .unwrap_or(false)
}

/// Contract C10 (m664 post-2026-08-23 API extension): compute the
/// dispatch-scope for a normally-skipped directory when one or more
/// registrations' `descend_into` overrides match its basename.
///
/// Returns:
/// - `Some(scope)`: at least one registration's `descend_into`
///   matches; walker should descend with dispatch restricted to
///   `scope` (the set of matching readers, intersected with the
///   caller's incoming scope when present).
/// - `None`: no registration wants to descend into this directory;
///   walker skips it (original behavior).
///
/// The intersection preserves invariant "a reader restricted to a
/// subtree does not get re-broadened by a nested descend_into"
/// (e.g., if we're already inside a maven-restricted target/, and
/// a nested build/ triggers go_binary's descend_into, go_binary is
/// NOT allowed dispatch inside a maven-scoped subtree unless it
/// was ALSO in the outer maven scope). In practice, since
/// `descend_into` normally only fires at the root level, this
/// intersection is defensive rather than commonly triggered.
fn compute_descend_into_scope(
    basename: &std::ffi::OsStr,
    registrations: &[super::ReaderRegistration],
    incoming_scope: Option<&HashSet<ReaderId>>,
) -> Option<HashSet<ReaderId>> {
    let mut matching: HashSet<ReaderId> = HashSet::new();
    for reg in registrations {
        let Some(ref globset) = reg.descend_into else {
            continue;
        };
        if globset.is_match(basename) {
            // Also enforce the incoming scope — a reader currently
            // scoped OUT cannot re-enter via a nested descend_into.
            if let Some(outer) = incoming_scope {
                if !outer.contains(&reg.reader_id) {
                    continue;
                }
            }
            matching.insert(reg.reader_id);
        }
    }
    if matching.is_empty() {
        None
    } else {
        Some(matching)
    }
}

/// Compute the scan-root-relative forward-slash path for a canonicalized
/// absolute path. Returns `None` if `path` is not under `rootfs`.
fn rel_scan_path(path: &Path, rootfs: &Path) -> Option<String> {
    let canonical_root = std::fs::canonicalize(rootfs).unwrap_or_else(|_| rootfs.to_path_buf());
    let rel = path.strip_prefix(&canonical_root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    //! Walker-level unit tests. Cover contracts C4 (panic isolation) and
    //! C5 (safe_walk-equivalent semantics). These live in-crate because
    //! `waybill-cli/src/lib.rs` intentionally does not expose `scan_fs`
    //! per Constitution Principle VI — internal test modules have full
    //! crate access, integration-tests-crate access would cascade-require
    //! lib-exposing every binary-internal module (see `lib.rs` doc-comment).

    use std::fs;
    use std::sync::{Arc, Mutex};

    use crate::scan_fs::package_db::exclude_path::ExclusionSet;
    use crate::scan_fs::walk_registry::{
        globset_from_patterns, ReaderId, ReaderRegistration, ReaderRegistryBuilder,
        SharedWalker, SharedWalkerContext,
    };

    // Milestone 666: per-test observation buffer for walker file-visit
    // callbacks. Each `#[test]` fn that observes visits constructs its
    // own instance on its stack frame; two Arc clones exist for the
    // duration of the walker's run — one held by the test (for post-
    // run assertions), one held by the `ReaderRegistration.state` slot
    // (for the walker's callback lookup via `ctx.state::<Mutex<Vec<String>>>`).
    // Both drop when the test exits. No cross-test sharing, no static
    // mutable state, no global lock. See `specs/666-walker-test-flake-fix/
    // contracts/test-visit-sink.md` for the C1-C6 contracts.
    type VisitSink = Arc<Mutex<Vec<String>>>;

    /// Milestone 666: shared helper called by each test's `record_visit_*`
    /// wrapper. Fetches the test's own sink from the ReaderRegistration's
    /// state slot (populated by the test with `Some(sink.clone())`) and
    /// pushes the visited filename. Silent no-op if the sink is absent
    /// (reader_id mismatch or downcast failure) — keeps the walker's
    /// dispatch loop unblocked. See contracts/test-visit-sink.md §C1.
    fn push_visit_to_sink(
        path: &std::path::Path,
        ctx: &SharedWalkerContext<'_>,
        reader_id: ReaderId,
    ) {
        let Some(sink) = ctx.state::<Mutex<Vec<String>>>(reader_id) else {
            return;
        };
        sink.lock()
            .unwrap()
            .push(path.file_name().unwrap().to_string_lossy().into_owned());
    }

    // Per-test callback wrappers. Each hardcodes ITS OWN reader_id at
    // compile time because `FileCallback` is a bare `fn` pointer (no
    // captures) and the walker's dispatch loop invokes the pointer
    // without passing the reader_id (see `dispatch::dispatch_file`).
    // So the reader_id must be baked in via distinct wrapper fns per
    // test. See contracts/test-visit-sink.md §C3.
    fn record_visit_loop(path: &std::path::Path, ctx: &SharedWalkerContext<'_>) {
        push_visit_to_sink(path, ctx, ReaderId::new("visitor-loop"));
    }
    fn record_visit_exclude(path: &std::path::Path, ctx: &SharedWalkerContext<'_>) {
        push_visit_to_sink(path, ctx, ReaderId::new("visitor-exclude"));
    }
    fn record_visit_noise(path: &std::path::Path, ctx: &SharedWalkerContext<'_>) {
        push_visit_to_sink(path, ctx, ReaderId::new("visitor-noise"));
    }

    // ------------------------------------------------------------
    // T013 — contract C4: reader-callback panic isolation
    // ------------------------------------------------------------

    static PANIC_ISOLATION_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    fn panicking_reader_cb(path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        PANIC_ISOLATION_LOG.lock().unwrap().push("panicker-called");
        if path.file_name().and_then(|n| n.to_str()) == Some("bomb.marker") {
            panic!("intentional test panic from panicking_reader_cb");
        }
    }

    fn healthy_reader_cb(_path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        PANIC_ISOLATION_LOG.lock().unwrap().push("healthy-called");
    }

    /// T013 — contract C4: when reader A panics on a specific file,
    /// reader B still receives its callback for that file AND subsequent files.
    #[test]
    fn panicking_reader_does_not_abort_walker() {
        PANIC_ISOLATION_LOG.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        fs::write(tmpdir.path().join("bomb.marker"), b"boom").unwrap();
        fs::write(tmpdir.path().join("safe.marker"), b"ok").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("panicker"),
                state: None,
                patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
                on_file: Some(panicking_reader_cb),
                on_dir: None,
                descend_into: None,
            })
            .register(ReaderRegistration {
                reader_id: ReaderId::new("healthy"),
                state: None,
                patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
                on_file: Some(healthy_reader_cb),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(tmpdir.path(), &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = PANIC_ISOLATION_LOG.lock().unwrap();
        let panicker_calls = log.iter().filter(|s| **s == "panicker-called").count();
        let healthy_calls = log.iter().filter(|s| **s == "healthy-called").count();
        assert_eq!(
            panicker_calls, 2,
            "panicker should be dispatched to both files (its panic on one \
             doesn't stop the walker); log={:?}",
            *log,
        );
        assert_eq!(
            healthy_calls, 2,
            "healthy reader must receive callbacks for both files despite \
             panicker's panic (contract C4); log={:?}",
            *log,
        );
    }

    // ------------------------------------------------------------
    // T014 — contract C5: safe_walk-equivalent semantics
    //
    // Each of the three tests below constructs its own per-test
    // `VisitSink` (see the type alias + `push_visit_to_sink` helper
    // above) and threads an Arc clone through `ReaderRegistration.state`
    // (m664 contract C4 slot). Post-run assertions read from the test's
    // own Arc — never from a static — so cargo's parallel test-runner
    // cannot race entries between tests. Milestone 666 (issue #720).
    // ------------------------------------------------------------

    #[test]
    #[cfg(unix)]
    fn walker_survives_symlink_loop() {
        let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("target.marker"), b"payload").unwrap();
        std::os::unix::fs::symlink("../child", child.join("loopback")).unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("visitor-loop"),
                state: Some(sink.clone()),
                patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
                on_file: Some(record_visit_loop),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = sink.lock().unwrap();
        let visits = log.iter().filter(|s| s.as_str() == "target.marker").count();
        assert_eq!(
            visits, 1,
            "symlink loop should not cause repeated visits; got {:?}",
            *log,
        );
    }

    #[test]
    fn walker_respects_exclusion_set() {
        let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        let excluded = root.join("excluded_dir");
        let kept = root.join("kept_dir");
        fs::create_dir(&excluded).unwrap();
        fs::create_dir(&kept).unwrap();
        fs::write(excluded.join("in_excluded.marker"), b"x").unwrap();
        fs::write(kept.join("in_kept.marker"), b"y").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("visitor-exclude"),
                state: Some(sink.clone()),
                patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
                on_file: Some(record_visit_exclude),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::from_iter(["excluded_dir"]).unwrap();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = sink.lock().unwrap();
        assert!(
            !log.iter().any(|s| s == "in_excluded.marker"),
            "excluded contents should not be visited; log={:?}",
            *log,
        );
        assert!(
            log.iter().any(|s| s == "in_kept.marker"),
            "kept contents should be visited; log={:?}",
            *log,
        );
    }

    #[test]
    fn walker_skips_default_noise_dirs() {
        let sink: VisitSink = Arc::new(Mutex::new(Vec::new()));

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        let git_dir = root.join(".git");
        let nm_dir = root.join("node_modules");
        fs::create_dir(&git_dir).unwrap();
        fs::create_dir(&nm_dir).unwrap();
        fs::write(git_dir.join("in_git.marker"), b"x").unwrap();
        fs::write(nm_dir.join("in_nm.marker"), b"y").unwrap();
        fs::write(root.join("top.marker"), b"z").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("visitor-noise"),
                state: Some(sink.clone()),
                patterns: globset_from_patterns(&["**/*.marker"]).unwrap(),
                on_file: Some(record_visit_noise),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = sink.lock().unwrap();
        assert!(
            !log.iter().any(|s| s == "in_git.marker"),
            ".git contents should be skipped; log={:?}",
            *log,
        );
        assert!(
            !log.iter().any(|s| s == "in_nm.marker"),
            "node_modules contents should be skipped; log={:?}",
            *log,
        );
        assert!(
            log.iter().any(|s| s == "top.marker"),
            "top-level file should be visited; log={:?}",
            *log,
        );
    }

    /// Sanity: an empty registry still runs the walker and returns an
    /// empty per-reader output map without dispatching anything.
    #[test]
    fn empty_registry_produces_empty_output() {
        let tmpdir = tempfile::tempdir().unwrap();
        fs::write(tmpdir.path().join("some.file"), b"x").unwrap();

        let registry = ReaderRegistryBuilder::new().build().unwrap();
        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(tmpdir.path(), &registry, &excludes);
        walker.run();
        let output = walker.finish();
        assert!(output.is_empty(), "empty registry → empty per-reader map");
    }

    /// Sibling-lookup end-to-end: a reader callback consults the
    /// DirIndex for its file's siblings without triggering extra I/O.
    /// This is FR-003 + Clarify Q1 exercised across the whole stack.
    static SIBLINGS_SEEN: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());

    fn record_siblings_cb(path: &std::path::Path, ctx: &SharedWalkerContext<'_>) {
        let sibs: Vec<String> = ctx
            .dir_index()
            .siblings_of(path)
            .map(|arc| arc.iter().map(|s| s.to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        SIBLINGS_SEEN.lock().unwrap().push(sibs);
    }

    #[test]
    fn sibling_lookup_end_to_end() {
        SIBLINGS_SEEN.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        fs::write(root.join("Cargo.toml"), b"[package]").unwrap();
        fs::write(root.join("Cargo.lock"), b"[[package]]").unwrap();
        fs::write(root.join("README.md"), b"# hi").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("sibs-checker"),
                state: None,
                patterns: globset_from_patterns(&["**/Cargo.toml"]).unwrap(),
                on_file: Some(record_siblings_cb),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let seen = SIBLINGS_SEEN.lock().unwrap();
        assert_eq!(seen.len(), 1, "expected one Cargo.toml callback");
        let sibs = &seen[0];
        assert!(sibs.iter().any(|s| s == "Cargo.toml"), "siblings should include self; got {sibs:?}");
        assert!(sibs.iter().any(|s| s == "Cargo.lock"), "siblings should include Cargo.lock; got {sibs:?}");
        assert!(sibs.iter().any(|s| s == "README.md"), "siblings should include README.md; got {sibs:?}");
    }

    // ------------------------------------------------------------
    // Contract C10 — descend_into API extension (m664 post-2026-08-23)
    // ------------------------------------------------------------

    static DESCENDER_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static NON_DESCENDER_LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn cb_descender(path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        DESCENDER_LOG.lock().unwrap().push(
            path.file_name().unwrap().to_string_lossy().into_owned(),
        );
    }

    fn cb_non_descender(path: &std::path::Path, _ctx: &SharedWalkerContext<'_>) {
        NON_DESCENDER_LOG.lock().unwrap().push(
            path.file_name().unwrap().to_string_lossy().into_owned(),
        );
    }

    /// C10 clause 1: a reader that declares `descend_into` for a
    /// normally-skipped basename receives dispatch for files under
    /// that subtree.
    #[test]
    fn descend_into_allows_requesting_reader() {
        DESCENDER_LOG.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        // `target/` is in the shared walker's default skip set.
        let target = root.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("secret.jar"), b"x").unwrap();
        fs::write(root.join("outer.jar"), b"y").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("descender"),
                state: None,
                patterns: globset_from_patterns(&["**/*.jar"]).unwrap(),
                on_file: Some(cb_descender),
                on_dir: None,
                descend_into: Some(globset_from_patterns(&["target"]).unwrap()),
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = DESCENDER_LOG.lock().unwrap();
        assert!(
            log.iter().any(|s| s == "outer.jar"),
            "descender should see outer.jar (root-level); got {log:?}",
        );
        assert!(
            log.iter().any(|s| s == "secret.jar"),
            "C10 clause 1: descender declared descend_into=[target], should see \
             secret.jar under target/; got {log:?}",
        );
    }

    /// C10 clause 2 (byte-identity preservation): a reader that did
    /// NOT declare descend_into does NOT see files under a subtree
    /// that another reader opened via descend_into.
    #[test]
    fn descend_into_scopes_out_non_requesting_readers() {
        DESCENDER_LOG.lock().unwrap().clear();
        NON_DESCENDER_LOG.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        let target = root.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("stale.txt"), b"x").unwrap();
        fs::write(root.join("visible.txt"), b"y").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("descender"),
                state: None,
                patterns: globset_from_patterns(&["**/*.txt"]).unwrap(),
                on_file: Some(cb_descender),
                on_dir: None,
                descend_into: Some(globset_from_patterns(&["target"]).unwrap()),
            })
            .register(ReaderRegistration {
                reader_id: ReaderId::new("non-descender"),
                state: None,
                patterns: globset_from_patterns(&["**/*.txt"]).unwrap(),
                on_file: Some(cb_non_descender),
                on_dir: None,
                descend_into: None, // NOT opting in to target/ descent.
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        // Descender sees BOTH files (root-level + target/ subtree).
        let d_log = DESCENDER_LOG.lock().unwrap();
        assert!(d_log.iter().any(|s| s == "visible.txt"));
        assert!(
            d_log.iter().any(|s| s == "stale.txt"),
            "descender should see stale.txt under its own descend_into; got {d_log:?}",
        );
        // Non-descender sees ONLY the root file — the target/ subtree
        // is scoped-out for it (C10 clause 2, byte-identity guarantee).
        let n_log = NON_DESCENDER_LOG.lock().unwrap();
        assert!(n_log.iter().any(|s| s == "visible.txt"));
        assert!(
            !n_log.iter().any(|s| s == "stale.txt"),
            "C10 clause 2: non-descender did NOT declare descend_into, must NOT \
             receive dispatch under target/; got {n_log:?}",
        );
    }

    /// C10 clause 3: normal (non-skipped) descent continues to
    /// dispatch to all readers when the outer scope is unrestricted.
    #[test]
    fn descend_into_absent_preserves_default_behavior() {
        NON_DESCENDER_LOG.lock().unwrap().clear();

        let tmpdir = tempfile::tempdir().unwrap();
        let root = tmpdir.path();
        let sub = root.join("normal_subdir"); // NOT in the skip set
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file.txt"), b"x").unwrap();

        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("visitor"),
                state: None,
                patterns: globset_from_patterns(&["**/*.txt"]).unwrap(),
                on_file: Some(cb_non_descender),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();

        let excludes = ExclusionSet::new_empty();
        let mut walker = SharedWalker::new(root, &registry, &excludes);
        walker.run();
        let _ = walker.finish();

        let log = NON_DESCENDER_LOG.lock().unwrap();
        assert!(
            log.iter().any(|s| s == "file.txt"),
            "C10 clause 3: no descend_into declared, no scoping applied; \
             normal descent into normal_subdir should dispatch; got {log:?}",
        );
    }
}

