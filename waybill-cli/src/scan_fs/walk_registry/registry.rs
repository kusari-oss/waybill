//! `ReaderRegistry` — the dispatch table mapping filename patterns to
//! interested readers. See `data-model.md` §"ReaderRegistry" and
//! `contracts/registry-api.md` §"Public API surface".
//!
//! Design note (differs from data-model.md's aspirational shape): we
//! do NOT build a single composite `GlobSet` union across registrations.
//! Reason: `globset::GlobSet` doesn't expose its constituent globs after
//! construction, so composing a union while preserving per-registration
//! attribution requires either (a) also storing a parallel `Vec<Glob>`
//! per registration (duplication), or (b) per-glob metadata tables. For
//! ~28 readers with ~1-5 patterns each, iterating registrations and
//! calling each `patterns.is_match(basename)` is O(R) per file, cheap
//! enough that the composite optimization isn't warranted. If perf
//! profiling later shows this is hot, revisit.

use std::collections::HashSet;

use super::ReaderRegistration;

#[derive(Debug, thiserror::Error)]
pub enum ReaderRegistryError {
    #[error("duplicate reader id registered: {0}")]
    DuplicateReaderId(&'static str),
    #[error("empty registration: reader {0} has neither on_file nor on_dir")]
    EmptyRegistration(&'static str),
}

/// Builder — accumulate registrations in insertion order, then `build()`
/// validates + freezes them into a `ReaderRegistry`.
pub struct ReaderRegistryBuilder {
    entries: Vec<ReaderRegistration>,
}

impl ReaderRegistryBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn register(mut self, r: ReaderRegistration) -> Self {
        self.entries.push(r);
        self
    }

    /// Validate + freeze. Rejects duplicates (contract C8), empty
    /// registrations (both callbacks `None`).
    pub fn build(self) -> Result<ReaderRegistry, ReaderRegistryError> {
        let mut seen: HashSet<&'static str> = HashSet::with_capacity(self.entries.len());
        for reg in &self.entries {
            let id_str = reg.reader_id.as_str();
            if !seen.insert(id_str) {
                return Err(ReaderRegistryError::DuplicateReaderId(id_str));
            }
            if reg.on_file.is_none() && reg.on_dir.is_none() {
                return Err(ReaderRegistryError::EmptyRegistration(id_str));
            }
        }
        Ok(ReaderRegistry {
            registrations: self.entries,
        })
    }
}

impl Default for ReaderRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Frozen registry — the shared walker iterates `registrations()` in
/// insertion order (contract C1) once per file/dir.
#[derive(Debug)]
pub struct ReaderRegistry {
    registrations: Vec<ReaderRegistration>,
}

impl ReaderRegistry {
    /// Insertion-ordered view of every registration. Callers iterate this
    /// to preserve C1 dispatch determinism.
    pub fn registrations(&self) -> &[ReaderRegistration] {
        &self.registrations
    }

    /// Ordered list of every registered reader's `ReaderId`. Used to
    /// pre-populate the per-reader output map + metrics counters.
    pub fn reader_ids(&self) -> Vec<super::ReaderId> {
        self.registrations.iter().map(|r| r.reader_id).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::scan_fs::walk_registry::{globset_from_patterns, ReaderId, ReaderRegistration};

    fn dummy_on_file(_path: &std::path::Path, _ctx: &super::super::SharedWalkerContext<'_>) {}

    /// T012 — contract C8: duplicate `ReaderId` at `build()` is rejected.
    #[test]
    fn duplicate_reader_id_rejected_at_build() {
        let id = ReaderId::new("test-reader");
        let builder = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: id,
                state: None,
                patterns: globset_from_patterns(&["**/foo.toml"]).unwrap(),
                on_file: Some(dummy_on_file),
                on_dir: None,
                descend_into: None,
            })
            .register(ReaderRegistration {
                reader_id: id, // duplicate!
                state: None,
                patterns: globset_from_patterns(&["**/bar.toml"]).unwrap(),
                on_file: Some(dummy_on_file),
                on_dir: None,
                descend_into: None,
            });
        let err = builder.build().unwrap_err();
        assert!(matches!(err, ReaderRegistryError::DuplicateReaderId("test-reader")));
    }

    #[test]
    fn empty_registration_is_rejected() {
        let builder = ReaderRegistryBuilder::new().register(ReaderRegistration {
            reader_id: ReaderId::new("empty-reader"),
            state: None,
            patterns: globset_from_patterns(&["**/*"]).unwrap(),
            on_file: None,
            on_dir: None,
            descend_into: None,
        });
        let err = builder.build().unwrap_err();
        assert!(matches!(err, ReaderRegistryError::EmptyRegistration("empty-reader")));
    }

    #[test]
    fn build_succeeds_with_valid_registrations() {
        let registry = ReaderRegistryBuilder::new()
            .register(ReaderRegistration {
                reader_id: ReaderId::new("r1"),
                state: None,
                patterns: globset_from_patterns(&["**/Cargo.toml"]).unwrap(),
                on_file: Some(dummy_on_file),
                on_dir: None,
                descend_into: None,
            })
            .register(ReaderRegistration {
                reader_id: ReaderId::new("r2"),
                state: None,
                patterns: globset_from_patterns(&["**/*.lock"]).unwrap(),
                on_file: Some(dummy_on_file),
                on_dir: None,
                descend_into: None,
            })
            .build()
            .unwrap();
        assert_eq!(registry.registrations().len(), 2);
        assert_eq!(registry.reader_ids(), vec![ReaderId::new("r1"), ReaderId::new("r2")]);
    }

    /// Empty registry is a valid state — the walker still runs, all
    /// dispatches are no-ops. Used during US1 pre-first-migration.
    #[test]
    fn empty_registry_is_valid() {
        let registry = ReaderRegistryBuilder::new().build().unwrap();
        assert!(registry.is_empty());
        assert!(registry.reader_ids().is_empty());
    }
}
