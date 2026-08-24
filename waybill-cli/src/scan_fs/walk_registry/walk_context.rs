//! `SharedWalkerContext` — the reader-facing handle passed to every
//! callback. See `data-model.md` §"SharedWalkerContext" and
//! `contracts/registry-api.md` §"Public API surface".
//!
//! Design note: the context does NOT hold `&SharedWalker` (which would
//! force interior mutability on every walker field). Instead it holds
//! direct references to the three things a reader callback needs:
//! `&DirIndex`, `&ExclusionSet`, and `&HashMap<ReaderId,
//! Mutex<Vec<PackageDbEntry>>>` (the output sink). The walker constructs
//! a fresh `SharedWalkerContext` per callback invocation — stack-allocated,
//! ~three pointers wide.

use std::collections::HashMap;
use std::sync::Mutex;

use super::{DirIndex, ReaderId, ReaderRegistration};
use crate::scan_fs::package_db::exclude_path::ExclusionSet;
use crate::scan_fs::package_db::PackageDbEntry;

pub struct SharedWalkerContext<'w> {
    dir_index: &'w DirIndex,
    exclude_set: &'w ExclusionSet,
    output: &'w HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>,
    /// Slice into the registry's registrations, held for per-reader
    /// state downcast via `state::<T>(reader_id)`. See
    /// `ReaderRegistration::state` for the design rationale.
    registrations: &'w [ReaderRegistration],
}

impl<'w> SharedWalkerContext<'w> {
    pub(crate) fn new(
        dir_index: &'w DirIndex,
        exclude_set: &'w ExclusionSet,
        output: &'w HashMap<ReaderId, Mutex<Vec<PackageDbEntry>>>,
        registrations: &'w [ReaderRegistration],
    ) -> Self {
        Self {
            dir_index,
            exclude_set,
            output,
            registrations,
        }
    }

    /// Retrieve this reader's per-scan state, downcast to its concrete
    /// type. Returns `None` if the reader registered no state OR if the
    /// downcast fails (reader-side bug — mismatched type at
    /// `registration()` vs at callback).
    ///
    /// O(R) lookup where R = number of registered readers; for typical
    /// callback shapes (one lookup per callback invocation) this is
    /// well under 1 µs.
    pub fn state<T: 'static>(&self, reader_id: ReaderId) -> Option<&T> {
        self.registrations
            .iter()
            .find(|r| r.reader_id == reader_id)
            .and_then(|r| r.state.as_ref())
            .and_then(|arc| arc.downcast_ref::<T>())
    }

    /// The in-memory (directory → filenames) index. FR-003 + Clarify Q1:
    /// sibling-lookup MUST NOT trigger a fresh `read_dir()` syscall.
    pub fn dir_index(&self) -> &DirIndex {
        self.dir_index
    }

    /// The m113 `ExclusionSet` handle. Readers can consult this for
    /// finer-grained decisions on non-matching-but-still-visited paths.
    pub fn exclude_set(&self) -> &ExclusionSet {
        self.exclude_set
    }

    /// Push a package-db entry into the reader's output sink.
    /// Debug-asserts that `reader_id` is registered — pushing under an
    /// unregistered ID is a bug (contract C8 corollary).
    pub fn push(&self, reader_id: ReaderId, entry: PackageDbEntry) {
        debug_assert!(
            self.output.contains_key(&reader_id),
            "SharedWalkerContext::push called with unregistered reader_id {:?} — \
             every push target must be pre-registered at scan init",
            reader_id.as_str(),
        );
        if let Some(sink) = self.output.get(&reader_id) {
            // Unwrap on Mutex::lock is standard practice for
            // panic-poisoned recovery — if the previous holder panicked
            // (contract C4 catch_unwind), we accept the poison and
            // continue writing. See std::sync::PoisonError::into_inner.
            match sink.lock() {
                Ok(mut guard) => guard.push(entry),
                Err(poisoned) => poisoned.into_inner().push(entry),
            }
        }
    }
}
