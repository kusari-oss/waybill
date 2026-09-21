//! Milestone 924 (#932) — per-directory claimed/unclaimed accumulation.
//!
//! Accumulation only. No classification, no policy, no judgement — those live
//! in `significance` and `ecosystems`. This module's single job is to retain
//! the value `dispatch_file` already computes (research R1) and tally it by
//! directory.
//!
//! **Ordered containers throughout.** `BTreeMap`/`BTreeSet` rather than their
//! hashed counterparts, so iteration order is a function of the keys and not
//! of a random seed. FR-020's determinism requirement is satisfied here or not
//! at all — and a `HashMap` that happens to iterate consistently on one host is
//! exactly the sort of thing that passes locally and fails in CI.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::scan_fs::walk_registry::ReaderId;

/// Why a file or directory was not examined. Enumerated rather than collapsed
/// into a single "other" count: FR-003 requires skips to be broken down, and a
/// bucket labelled "other" is where a census stops being checkable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SkipReason {
    /// Matched an operator exclusion (`--exclude-path`) or the built-in skip set.
    ExcludedByPolicy,
    /// `read_dir` failed — permissions, a broken symlink, a vanished path.
    Unreadable,
    /// Already visited via another path (symlink convergence).
    AlreadyVisited,
    /// Depth limit reached.
    DepthLimit,
}

impl SkipReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ExcludedByPolicy => "excluded_by_policy",
            Self::Unreadable => "unreadable",
            Self::AlreadyVisited => "already_visited",
            Self::DepthLimit => "depth_limit",
        }
    }
}

/// What was observed about one directory. Raw tallies; significance and
/// classification are decided later from these.
#[derive(Debug, Default, Clone)]
pub(crate) struct DirCensus {
    /// Files directly in this directory. Excludes descendants.
    pub(crate) files_direct: u64,
    /// Of `files_direct`, how many at least one reader claimed.
    pub(crate) files_claimed: u64,
    /// Of `files_direct`, how many no reader claimed.
    pub(crate) files_unclaimed: u64,
    /// Every reader that claimed at least one file here.
    pub(crate) claimed_by: BTreeSet<ReaderId>,
    /// Basenames observed, for later marker matching (FR-007) and the
    /// extension histogram (FR-011). Sorted by construction.
    pub(crate) filenames: BTreeSet<String>,
}

/// Repository-wide accumulation.
///
/// # What "claimed" means here, and why it is not simply "dispatched to"
///
/// Research R1 equated `dispatched_to.is_empty()` with "unclaimed". That is
/// mechanically true and **semantically wrong**, discovered at implement time:
/// the `go_binary` reader registers `**/*` on purpose, because Go binaries
/// have no reliable filename pattern and its callback does the real filtering
/// by content. Counting that as a claim makes every file in every repository
/// claimed, and FR-002's "files claimed by no reader" permanently empty — a
/// census that cannot answer its own question.
///
/// So a **claim** here is a *filename-pattern* claim. Readers whose patterns
/// match everything are content probes, not claimants: they are excluded from
/// claim attribution and recorded separately, because "something looked at
/// this file" and "something recognised this file" are different facts and
/// only the second one is a coverage signal.
#[derive(Debug, Default)]
pub(crate) struct Census {
    dirs: BTreeMap<PathBuf, DirCensus>,
    per_reader_files: BTreeMap<ReaderId, u64>,
    files_walked: u64,
    files_claimed: u64,
    files_unclaimed: u64,
    skipped: BTreeMap<SkipReason, u64>,
    /// Readers whose patterns match everything. Excluded from claims.
    catch_all: BTreeSet<ReaderId>,
    /// Directories skipped by policy, with the file count each held.
    excluded_dirs: BTreeMap<PathBuf, u64>,
}

impl Census {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Declare the readers that match everything, so they are not counted as
    /// claimants. Determined by probing the registry, not hard-coded — a
    /// hard-coded list is the sort that silently rots when a reader changes
    /// its patterns.
    pub(crate) fn with_catch_all(mut self, ids: BTreeSet<ReaderId>) -> Self {
        self.catch_all = ids;
        self
    }

    /// Record one file and the readers that claimed it.
    ///
    /// A file claimed by several readers counts **once** toward
    /// `files_claimed` — otherwise FR-003's reconciliation would break on any
    /// repository where two readers legitimately match the same file, which is
    /// a supported case, not an error.
    pub(crate) fn record_file(&mut self, dir: &Path, file_name: &str, claimed_by: &[ReaderId]) {
        self.files_walked += 1;
        let entry = self.dirs.entry(dir.to_path_buf()).or_default();
        entry.files_direct += 1;
        entry.filenames.insert(file_name.to_string());

        // Filename-pattern claims only. A content probe that matches `**/*`
        // is not a claim (see the type docs).
        let claimed_by: Vec<ReaderId> = claimed_by
            .iter()
            .copied()
            .filter(|id| !self.catch_all.contains(id))
            .collect();
        let claimed_by = claimed_by.as_slice();

        if claimed_by.is_empty() {
            self.files_unclaimed += 1;
            entry.files_unclaimed += 1;
        } else {
            self.files_claimed += 1;
            entry.files_claimed += 1;
            for id in claimed_by {
                entry.claimed_by.insert(*id);
                // Per-reader counts DO count a multiply-claimed file once per
                // reader — "how many files did this reader match" is a
                // different question from "how many files were claimed".
                *self.per_reader_files.entry(*id).or_insert(0) += 1;
            }
        }
    }

    /// Record a directory that was visited but whose contents were not walked.
    pub(crate) fn record_skip(&mut self, reason: SkipReason, files: u64) {
        *self.skipped.entry(reason).or_insert(0) += files;
        self.files_walked += files;
    }

    /// FR-005 — record a directory excluded by policy, with the files it
    /// holds, so `excluded_by_policy` is distinguishable from `unclaimed`.
    pub(crate) fn record_excluded_dir(&mut self, dir: &Path, files: u64) {
        self.excluded_dirs.insert(dir.to_path_buf(), files);
        *self.skipped.entry(SkipReason::ExcludedByPolicy).or_insert(0) += files;
        self.files_walked += files;
    }

    pub(crate) fn excluded_dirs(&self) -> &BTreeMap<PathBuf, u64> {
        &self.excluded_dirs
    }

    /// Ensure a directory appears in the census even with no files of its own.
    /// A directory that exists and is empty is an observation; silently
    /// omitting it would make the tree misleading.
    pub(crate) fn touch_dir(&mut self, dir: &Path) {
        self.dirs.entry(dir.to_path_buf()).or_default();
    }

    pub(crate) fn dirs(&self) -> &BTreeMap<PathBuf, DirCensus> {
        &self.dirs
    }

    pub(crate) fn per_reader_files(&self) -> &BTreeMap<ReaderId, u64> {
        &self.per_reader_files
    }

    pub(crate) fn files_walked(&self) -> u64 {
        self.files_walked
    }

    pub(crate) fn files_claimed(&self) -> u64 {
        self.files_claimed
    }

    pub(crate) fn files_unclaimed(&self) -> u64 {
        self.files_unclaimed
    }

    pub(crate) fn skipped(&self) -> &BTreeMap<SkipReason, u64> {
        &self.skipped
    }

}


// ---------------------------------------------------------------------------
// Scan-scoped activation
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Whether this process is producing a report.
///
/// Read **once per walker construction**, not per file — so an ordinary scan
/// pays a single relaxed atomic load for the whole traversal, and the per-file
/// cost stays the `Option::is_some` check that T007 proved inert.
///
/// A flag rather than a threaded parameter because the walker sits six call
/// frames below the CLI, and widening six signatures to carry a value that is
/// `None` in every production path is a worse trade than one atomic.
static REPORT_MODE: AtomicBool = AtomicBool::new(false);

/// Where a completed census is left for the report command to collect.
static COLLECTED: Mutex<Option<Census>> = Mutex::new(None);

/// Per-reader emitted-component counts, taken from the walker's own
/// per-reader output map rather than inferred from paths.
///
/// **Known limitation, stated rather than papered over**: readers that
/// accumulate during the walk and emit in a later `finalize` step (haskell,
/// erlang, scala) produce their entries outside the walker, so they appear
/// here with zero emitted. That is a real gap in FR-004's coverage for those
/// three readers; it is recorded in the report rather than silently rounded
/// into a number that looks complete.
static EMITTED: Mutex<Option<BTreeMap<ReaderId, u64>>> = Mutex::new(None);

pub(crate) fn enable_report_mode() {
    REPORT_MODE.store(true, Ordering::Relaxed);
    if let Ok(mut slot) = COLLECTED.lock() {
        *slot = None;
    }
}

pub(crate) fn report_mode_enabled() -> bool {
    REPORT_MODE.load(Ordering::Relaxed)
}

/// Called by the walker when its traversal finishes.
pub(crate) fn deposit(census: Census, emitted: BTreeMap<ReaderId, u64>) {
    if let Ok(mut slot) = COLLECTED.lock() {
        *slot = Some(census);
    }
    if let Ok(mut slot) = EMITTED.lock() {
        *slot = Some(emitted);
    }
}

pub(crate) fn take_emitted() -> BTreeMap<ReaderId, u64> {
    EMITTED.lock().ok().and_then(|mut s| s.take()).unwrap_or_default()
}

/// Collect the census and reset, so a second report in one process cannot
/// silently inherit the first one's counts.
pub(crate) fn take_collected() -> Option<Census> {
    COLLECTED.lock().ok().and_then(|mut s| s.take())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    const R1: ReaderId = ReaderId::new("r1");
    const R2: ReaderId = ReaderId::new("r2");

    #[test]
    fn reconciles_across_claimed_unclaimed_and_skipped() {
        let mut c = Census::new();
        c.record_file(Path::new("/a"), "go.mod", &[R1]);
        c.record_file(Path::new("/a"), "README", &[]);
        c.record_skip(SkipReason::ExcludedByPolicy, 7);
        // Arithmetic spelled out rather than calling a helper: a test that
        // asserts `x.reconciles()` is checking the function against itself.
        let skipped: u64 = c.skipped().values().sum();
        assert_eq!(c.files_walked(), c.files_claimed() + c.files_unclaimed() + skipped);
        assert_eq!(c.files_walked(), 9);
        assert_eq!(c.files_claimed(), 1);
        assert_eq!(c.files_unclaimed(), 1);
    }

    /// FR-003 / data-model edge case. Two readers legitimately claiming the
    /// same file is supported, not an error — but counting it twice would
    /// break reconciliation on every such repository.
    #[test]
    fn a_file_claimed_by_two_readers_counts_once_toward_the_total() {
        let mut c = Census::new();
        c.record_file(Path::new("/a"), "pom.xml", &[R1, R2]);
        assert_eq!(c.files_claimed(), 1, "the file is one file");
        let skipped: u64 = c.skipped().values().sum();
        assert_eq!(c.files_walked(), c.files_claimed() + c.files_unclaimed() + skipped);
        assert_eq!(c.per_reader_files()[&R1], 1);
        assert_eq!(c.per_reader_files()[&R2], 1, "but both readers matched it");
    }

    #[test]
    fn a_directory_with_no_files_is_still_recorded() {
        let mut c = Census::new();
        c.touch_dir(Path::new("/empty"));
        assert!(c.dirs().contains_key(Path::new("/empty")));
        assert_eq!(c.dirs()[Path::new("/empty")].files_direct, 0);
    }

    /// FR-020. Ordered containers are the whole mechanism; a regression to
    /// `HashMap` would pass a single-run test and fail determinism in CI.
    #[test]
    fn iteration_order_is_by_key_not_insertion() {
        let mut c = Census::new();
        for d in ["/z", "/a", "/m"] {
            c.record_file(Path::new(d), "f", &[]);
        }
        let seen: Vec<_> = c.dirs().keys().map(|p| p.to_string_lossy().to_string()).collect();
        assert_eq!(seen, vec!["/a", "/m", "/z"]);
    }
}
