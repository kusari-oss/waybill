//! Milestone 113 — user-supplied directory exclusion for `mikebom scan`.
//!
//! The CLI flag `--exclude-path <PATH_OR_PATTERN>` and its env-var
//! counterpart `MIKEBOM_EXCLUDE_PATH` carry one or more entries into
//! the scan pipeline. Each entry is classified at parse time:
//!
//! - If the text contains any of `*`, `?`, `[`, the entry is a
//!   [`ExclusionEntry::Pattern`] compiled via the `globset` crate
//!   and matched against the candidate directory's path-relative-to
//!   -scan-root at arbitrary depth.
//! - Otherwise it is a [`ExclusionEntry::Literal`] interpreted as a
//!   path relative to the scan root.
//!
//! The classification rule mirrors `gitignore`, `rsync --exclude`,
//! `find -path`, and every shell-glob tool an operator is likely to
//! know.
//!
//! Every walker that contributes components to the emitted SBOM
//! consults an [`ExclusionSet`] at its descent decision point and
//! skips matched subtrees before any per-walker emission occurs.
//! When an [`ExclusionSet`] is empty (the default), every walker
//! behaves identically to the pre-milestone-113 build (FR-003 /
//! SC-002 byte-identity).
//!
//! See `specs/113-exclude-path-flag/` for the full spec, plan, and
//! contracts.

use std::path::PathBuf;

use globset::{Glob, GlobSet, GlobSetBuilder};
use thiserror::Error;

/// Errors surfaced when parsing a user-supplied exclusion entry.
///
/// All variants name the offending entry verbatim so the CLI parser
/// can produce a single-line error message satisfying SC-005.
#[derive(Debug, Error)]
pub enum ExcludePathError {
    /// `--exclude-path ""` or a path-list-separator-only env-var
    /// entry. Empty entries can't have intended meaning and would
    /// otherwise match every directory (every path is-prefixed-by
    /// the empty string), so we reject at parse time.
    #[error("--exclude-path entry was empty")]
    EmptyEntry,

    /// A pattern entry (contained at least one of `*`, `?`, `[`)
    /// failed to compile through `globset::Glob`. The most common
    /// cause is an unbalanced `[` or `]`.
    #[error("--exclude-path entry {entry:?}: {source}")]
    MalformedPattern {
        entry: String,
        #[source]
        source: globset::Error,
    },
}

/// One user-supplied exclusion entry, classified at parse time.
///
/// String forms (the original CLI/env text) are preserved alongside
/// the compiled representation via [`ExclusionSet::as_normalized_strings`]
/// so the Principle-X transparency annotation can emit operator-typed
/// values verbatim.
#[derive(Debug, Clone)]
pub enum ExclusionEntry {
    /// No metacharacters present in the source string. Matched
    /// literally (after platform-separator normalization to forward
    /// slashes) against the candidate directory's path relative to
    /// the scan root.
    Literal(PathBuf),

    /// Source string contained at least one of `*`, `?`, `[`.
    /// Compiled to a `globset::Glob`; matched against the candidate
    /// directory's normalized relative path at arbitrary depth.
    Pattern(Glob),
}

impl ExclusionEntry {
    /// Classify and compile a single entry. Empty input is rejected
    /// with [`ExcludePathError::EmptyEntry`]; malformed patterns
    /// surface [`ExcludePathError::MalformedPattern`] naming the
    /// verbatim entry.
    pub fn parse(s: &str) -> Result<Self, ExcludePathError> {
        if s.is_empty() {
            return Err(ExcludePathError::EmptyEntry);
        }
        if s.chars().any(|c| matches!(c, '*' | '?' | '[')) {
            let glob = Glob::new(s).map_err(|source| ExcludePathError::MalformedPattern {
                entry: s.to_string(),
                source,
            })?;
            Ok(ExclusionEntry::Pattern(glob))
        } else {
            // Strip any leading separator so the path always
            // matches against a relative `strip_prefix` result.
            let trimmed = s.trim_start_matches(['/', '\\']);
            // Normalize platform-specific separators to forward slashes
            // for cross-platform parity (FR-009).
            let normalized = trimmed.replace('\\', "/");
            Ok(ExclusionEntry::Literal(PathBuf::from(normalized)))
        }
    }
}

/// The active set of user-supplied exclusion entries threaded
/// read-only through every walker. Default-constructed (empty) is
/// the no-op state preserving pre-feature behavior.
#[derive(Debug, Default, Clone)]
pub struct ExclusionSet {
    /// All entries in source order so the Principle-X transparency
    /// annotation emits deterministically.
    entries: Vec<ExclusionEntry>,

    /// Pre-compiled GlobSet over every Pattern entry. `None` when
    /// no Pattern entries were supplied so an `is_match` call can
    /// short-circuit.
    pattern_set: Option<GlobSet>,

    /// Literal entries projected to forward-slash form for direct
    /// equality / `starts_with` checks at match time.
    literal_paths: Vec<String>,
}

impl ExclusionSet {
    /// Empty set — the default. Walkers receiving this borrow
    /// produce pre-feature output (FR-003). Equivalent to
    /// `ExclusionSet::default()`.
    #[allow(dead_code)] // Reserved for explicit empty-set construction in tests.
    pub fn new_empty() -> Self {
        Self::default()
    }

    /// Construct from an iterator of raw entry strings. Entries are
    /// classified and compiled in order. Duplicate literal paths
    /// are folded; the entries vector preserves source ordering for
    /// transparency-annotation determinism.
    pub fn from_iter<I, S>(iter: I) -> Result<Self, ExcludePathError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut entries: Vec<ExclusionEntry> = Vec::new();
        let mut literal_paths: Vec<String> = Vec::new();
        let mut pattern_builder = GlobSetBuilder::new();
        let mut has_pattern = false;
        for raw in iter {
            let s = raw.as_ref();
            let entry = ExclusionEntry::parse(s)?;
            match &entry {
                ExclusionEntry::Literal(p) => {
                    let key = p.to_string_lossy().into_owned();
                    if !literal_paths.contains(&key) {
                        literal_paths.push(key);
                    }
                }
                ExclusionEntry::Pattern(glob) => {
                    pattern_builder.add(glob.clone());
                    has_pattern = true;
                }
            }
            entries.push(entry);
        }
        let pattern_set = if has_pattern {
            Some(
                pattern_builder
                    .build()
                    .expect("globs validated individually at parse time"),
            )
        } else {
            None
        };
        Ok(Self {
            entries,
            pattern_set,
            literal_paths,
        })
    }

    /// Fast no-op probe used by walkers and the transparency-
    /// annotation emitter. `true` when no entries were supplied.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Match the candidate directory's forward-slash-normalized
    /// path-relative-to-scan-root against every entry. Returns
    /// `true` if any entry matches.
    pub fn matches(&self, candidate_rel_path: &str) -> bool {
        if self.is_empty() {
            return false;
        }
        let normalized = candidate_rel_path.replace('\\', "/");
        let trimmed = normalized.trim_start_matches('/');
        for literal in &self.literal_paths {
            if trimmed == literal.as_str()
                || trimmed.starts_with(&format!("{literal}/"))
            {
                return true;
            }
        }
        if let Some(set) = &self.pattern_set {
            if set.is_match(trimmed) {
                return true;
            }
        }
        false
    }

    /// Source-order snapshot of every active entry rendered as a
    /// string suitable for the Principle-X transparency annotation.
    /// Literal entries appear in their forward-slash-normalized
    /// form; pattern entries appear in their original glob syntax.
    pub fn as_normalized_strings(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| match e {
                ExclusionEntry::Literal(p) => p.to_string_lossy().into_owned(),
                ExclusionEntry::Pattern(g) => g.glob().to_string(),
            })
            .collect()
    }

    /// Borrow into the source-order entry list for callers that
    /// need structural access (e.g. parity-extractor introspection).
    #[allow(dead_code)] // Reserved for milestone-113 follow-up parity-catalog work.
    pub fn entries(&self) -> &[ExclusionEntry] {
        &self.entries
    }
}

// ---------------------------------------------------------------------------
// Transparency-annotation thread-local
// ---------------------------------------------------------------------------
//
// Constitution Principle X (Transparency) requires every SBOM emitted
// from a scan with active exclusion entries to carry an envelope-level
// `mikebom:exclude-path` annotation listing those entries (FR-014).
// The metadata emitters live deep below the CLI boundary and don't
// otherwise know about the ExclusionSet. Rather than thread it through
// every emitter signature (CDX metadata, SPDX 2.3 creationInfo, SPDX 3
// document Annotation — three separate code paths), we expose a
// scoped thread-local set at the CLI boundary and read by each
// emitter at metadata-build time. The RAII guard guarantees the
// thread-local is cleared after each scan invocation so successive
// in-process scans (e.g. inside an integration test) don't leak state.

thread_local! {
    static ACTIVE_ANNOTATION: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Install the current scan's exclusion-entry list for emitter
/// consumption. Returns an RAII guard that clears the thread-local
/// when dropped. The caller MUST keep the guard alive for the
/// duration of the SBOM emission to avoid mid-scan races.
pub fn install_annotation(entries: Vec<String>) -> AnnotationGuard {
    ACTIVE_ANNOTATION.with(|cell| {
        *cell.borrow_mut() = Some(entries);
    });
    AnnotationGuard
}

/// Snapshot the currently-installed entry list, if any. Returns
/// `None` when no exclusion is active (no annotation should be
/// emitted).
pub fn current_annotation() -> Option<Vec<String>> {
    ACTIVE_ANNOTATION.with(|cell| cell.borrow().clone())
}

/// RAII drop-guard returned by [`install_annotation`].
pub struct AnnotationGuard;

impl Drop for AnnotationGuard {
    fn drop(&mut self) {
        ACTIVE_ANNOTATION.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn literal_entry_classified_when_no_metacharacters() {
        let e = ExclusionEntry::parse("tests/fixtures").expect("parse");
        assert!(matches!(e, ExclusionEntry::Literal(_)));
    }

    #[test]
    fn pattern_entry_classified_when_star_present() {
        let e = ExclusionEntry::parse("**/testdata").expect("parse");
        assert!(matches!(e, ExclusionEntry::Pattern(_)));
    }

    #[test]
    fn pattern_entry_classified_when_question_mark_present() {
        let e = ExclusionEntry::parse("foo?").expect("parse");
        assert!(matches!(e, ExclusionEntry::Pattern(_)));
    }

    #[test]
    fn pattern_entry_classified_when_bracket_present() {
        let e = ExclusionEntry::parse("foo[ab]").expect("parse");
        assert!(matches!(e, ExclusionEntry::Pattern(_)));
    }

    #[test]
    fn empty_entry_rejected() {
        assert!(matches!(
            ExclusionEntry::parse(""),
            Err(ExcludePathError::EmptyEntry)
        ));
    }

    #[test]
    fn malformed_pattern_rejected_with_verbatim_entry() {
        let err = ExclusionEntry::parse("foo[").expect_err("should fail");
        match err {
            ExcludePathError::MalformedPattern { entry, .. } => {
                assert_eq!(entry, "foo[");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn literal_strips_leading_separators() {
        let e = ExclusionEntry::parse("/tests/fixtures").expect("parse");
        match e {
            ExclusionEntry::Literal(p) => {
                assert_eq!(p.to_string_lossy(), "tests/fixtures");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn literal_normalizes_backslash_to_forward_slash() {
        let e = ExclusionEntry::parse(r"tests\fixtures").expect("parse");
        match e {
            ExclusionEntry::Literal(p) => {
                assert_eq!(p.to_string_lossy(), "tests/fixtures");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn empty_set_is_empty_true() {
        let set = ExclusionSet::new_empty();
        assert!(set.is_empty());
    }

    #[test]
    fn empty_set_matches_returns_false_for_any_candidate() {
        let set = ExclusionSet::new_empty();
        assert!(!set.matches("anywhere"));
        assert!(!set.matches("tests/fixtures"));
        assert!(!set.matches(""));
    }

    #[test]
    fn literal_matches_anchored_at_scan_root() {
        let set = ExclusionSet::from_iter(["tests/fixtures"]).expect("build");
        assert!(set.matches("tests/fixtures"));
        assert!(set.matches("tests/fixtures/foo"));
        assert!(set.matches("tests/fixtures/foo/bar/baz"));
        assert!(!set.matches("services/a/tests/fixtures"));
        assert!(!set.matches("other"));
    }

    #[test]
    fn pattern_matches_at_arbitrary_depth() {
        let set = ExclusionSet::from_iter(["**/testdata"]).expect("build");
        assert!(set.matches("testdata"));
        assert!(set.matches("services/a/testdata"));
        assert!(set.matches("apps/web/internal/testdata"));
    }

    #[test]
    fn multiple_entries_combine_by_union() {
        let set = ExclusionSet::from_iter(["tests/fixtures", "**/testdata"]).expect("build");
        assert!(set.matches("tests/fixtures"));
        assert!(set.matches("services/a/testdata"));
        assert!(!set.matches("services/a"));
    }

    #[test]
    fn cross_platform_backslash_in_candidate_normalizes() {
        let set = ExclusionSet::from_iter(["tests/fixtures"]).expect("build");
        assert!(set.matches(r"tests\fixtures"));
    }

    #[test]
    fn cross_platform_backslash_in_entry_normalizes() {
        let set = ExclusionSet::from_iter([r"tests\fixtures"]).expect("build");
        assert!(set.matches("tests/fixtures"));
    }

    #[test]
    fn duplicate_literal_entries_deduped() {
        let set =
            ExclusionSet::from_iter(["tests/fixtures", "tests/fixtures"]).expect("build");
        assert_eq!(set.as_normalized_strings().len(), 2);
        let literals: Vec<_> = set
            .entries()
            .iter()
            .filter_map(|e| match e {
                ExclusionEntry::Literal(p) => Some(p.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        // entries preserve source order (both kept for annotation),
        // but the literal_paths fast-match set is deduplicated.
        assert_eq!(literals.len(), 2);
        // verify the internal literal_paths fast-match set is deduped
        assert_eq!(set.literal_paths.len(), 1);
    }

    #[test]
    fn as_normalized_strings_preserves_source_order() {
        let set = ExclusionSet::from_iter(["**/testdata", "tests/fixtures", "examples"])
            .expect("build");
        assert_eq!(
            set.as_normalized_strings(),
            vec!["**/testdata", "tests/fixtures", "examples"]
        );
    }

    #[test]
    fn no_match_no_op_does_not_clear_set() {
        // FR-008: an entry that matches no candidate must not
        // produce a warning/error and must not flip is_empty.
        let set = ExclusionSet::from_iter(["definitely/does/not/exist"]).expect("build");
        assert!(!set.is_empty());
        assert!(!set.matches("anywhere/else"));
        assert!(!set.matches("definitely"));
        assert!(!set.matches("definitely/does"));
        // The entry remains in the set for the transparency annotation.
        assert_eq!(set.as_normalized_strings(), vec!["definitely/does/not/exist"]);
    }

    #[test]
    fn from_iter_propagates_first_error() {
        let err = ExclusionSet::from_iter(["valid/path", "", "**/testdata"])
            .expect_err("should fail on empty entry");
        assert!(matches!(err, ExcludePathError::EmptyEntry));
    }

    #[test]
    fn from_iter_propagates_malformed_pattern() {
        let err = ExclusionSet::from_iter(["valid/path", "foo["])
            .expect_err("should fail on malformed");
        assert!(matches!(err, ExcludePathError::MalformedPattern { .. }));
    }
}
