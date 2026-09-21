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
#[derive(Debug, Default)]
pub(crate) struct Census {
    dirs: BTreeMap<PathBuf, DirCensus>,
    per_reader_files: BTreeMap<ReaderId, u64>,
    files_walked: u64,
    files_claimed: u64,
    files_unclaimed: u64,
    skipped: BTreeMap<SkipReason, u64>,
}

impl Census {
    pub(crate) fn new() -> Self {
        Self::default()
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

    /// FR-003 — the invariant that makes the census trustworthy.
    ///
    /// `files_walked == files_claimed + files_unclaimed + Σ skipped`.
    ///
    /// Exposed rather than merely asserted internally so the emitter can
    /// refuse to write a report that does not reconcile, and so a test can
    /// check it directly rather than inferring it from output.
    pub(crate) fn reconciles(&self) -> bool {
        let skipped: u64 = self.skipped.values().sum();
        self.files_walked == self.files_claimed + self.files_unclaimed + skipped
    }
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
        assert!(c.reconciles());
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
        assert!(c.reconciles());
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
