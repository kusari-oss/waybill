//! `DirIndex` — in-memory (directory → filenames) index. Satisfies the
//! FR-003 + Clarify Q1 contract: sibling-lookup MUST NOT trigger a
//! `read_dir()` syscall. See `data-model.md` §"DirIndex" and
//! `contracts/registry-api.md` C2 + C3.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct DirIndex {
    entries: HashMap<PathBuf, Arc<Vec<OsString>>>,
}

impl DirIndex {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Arc-shared, sorted list of filenames in the same directory as
    /// `file`. Returns `None` if the walker did not visit that directory
    /// (excluded or beyond max_depth). Contract C2.
    pub fn siblings_of(&self, file: &Path) -> Option<Arc<Vec<OsString>>> {
        file.parent().and_then(|dir| self.entries.get(dir).cloned())
    }

    /// True iff the walker's index shows `dir` contains a file named
    /// `filename`. Cheap check used by two-phase readers. Since the
    /// entries are sorted (contract C3), we could binary-search, but
    /// typical directory sizes are small enough that a linear scan is
    /// simpler and hard to beat.
    pub fn contains(&self, dir: &Path, filename: &OsStr) -> bool {
        self.entries
            .get(dir)
            .map(|v| v.iter().any(|f| f.as_os_str() == filename))
            .unwrap_or(false)
    }

    /// Insert the sorted filename list for a directory. Contract C3
    /// requires `sorted_filenames` to be pre-sorted; enforced via
    /// `debug_assert!` (release builds trust callers).
    pub fn insert(&mut self, dir: PathBuf, sorted_filenames: Vec<OsString>) {
        debug_assert!(
            sorted_filenames.windows(2).all(|w| w[0] <= w[1]),
            "DirIndex::insert requires sorted input (contract C3); \
             dir={dir:?} has unsorted filenames",
        );
        self.entries.insert(dir, Arc::new(sorted_filenames));
    }

    /// Number of directories in the index — used by `WalkerMetrics::tick_dir`
    /// and by tests.
    pub fn dir_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for DirIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// T008 — contract C3: `insert` requires a pre-sorted filename list.
    #[test]
    fn dir_index_sorts_by_construction() {
        let mut idx = DirIndex::new();
        idx.insert(
            PathBuf::from("/tmp/a"),
            vec![OsString::from("apple"), OsString::from("banana"), OsString::from("cherry")],
        );
        let out = idx.siblings_of(&PathBuf::from("/tmp/a/apple")).unwrap();
        assert_eq!(*out, vec![OsString::from("apple"), OsString::from("banana"), OsString::from("cherry")]);
    }

    /// T008 (companion) — `insert` must panic on unsorted input in debug.
    #[test]
    #[should_panic(expected = "DirIndex::insert requires sorted input")]
    fn dir_index_rejects_unsorted_input_in_debug() {
        let mut idx = DirIndex::new();
        idx.insert(
            PathBuf::from("/tmp/a"),
            // Deliberately reversed.
            vec![OsString::from("zebra"), OsString::from("apple")],
        );
    }

    /// T009 — contract C2: sibling-lookup reads from the index, not from
    /// the filesystem. We populate the index, then never touch the
    /// (non-existent) directory on disk — sibling-lookup still succeeds.
    #[test]
    fn sibling_lookup_reads_from_index_not_disk() {
        let mut idx = DirIndex::new();
        let dir = PathBuf::from("/this/path/does/not/exist/on/disk");
        idx.insert(
            dir.clone(),
            vec![
                OsString::from("Cargo.lock"),
                OsString::from("Cargo.toml"),
            ],
        );

        // Query via a file path whose parent is the fake dir.
        let siblings = idx.siblings_of(&dir.join("Cargo.toml")).unwrap();
        assert_eq!(siblings.len(), 2);
        assert!(siblings.iter().any(|f| f == "Cargo.lock"));
        assert!(siblings.iter().any(|f| f == "Cargo.toml"));

        // Explicit `contains` check.
        assert!(idx.contains(&dir, OsStr::new("Cargo.lock")));
        assert!(idx.contains(&dir, OsStr::new("Cargo.toml")));
        assert!(!idx.contains(&dir, OsStr::new("nonexistent.txt")));
    }

    #[test]
    fn siblings_of_returns_none_for_unvisited_dir() {
        let idx = DirIndex::new();
        assert!(idx.siblings_of(&PathBuf::from("/never/visited/foo.txt")).is_none());
    }

    #[test]
    fn dir_count_matches_inserts() {
        let mut idx = DirIndex::new();
        assert_eq!(idx.dir_count(), 0);
        idx.insert(PathBuf::from("/a"), vec![OsString::from("f1")]);
        idx.insert(PathBuf::from("/b"), vec![OsString::from("f2")]);
        assert_eq!(idx.dir_count(), 2);
    }
}
