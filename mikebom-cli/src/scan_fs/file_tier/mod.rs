// US1.A scaffolding lint suppression — every entry point in this
// module is reachable from inside `scan_fs::file_tier` and from the
// per-module #[cfg(test)] suites, but no production caller invokes
// the walker yet. US1.B (next PR) wires it into `scan_cmd::scan`,
// after which this allow is removed.
#![allow(dead_code)]

//! Milestone 133 US1 — file-tier component emission infrastructure.
//!
//! **Scope** (US1.A scaffolding): this module is the home of the
//! orphan file-tier walker, content-shape classifier, and hybrid
//! dedupe index. **US1.A is infrastructure-only**: the walker is
//! callable from inside this module, the unit tests exercise every
//! module independently, but the scan pipeline does not yet invoke
//! the walker — that integration ships in US1.B alongside the new
//! `--file-inventory` CLI flag, default flip, multi-format SBOM
//! emission, and the new C-rows. Until US1.B lands, every entry
//! point here is dead code from the production pipeline's
//! perspective.
//!
//! **Architecture** (per `specs/133-file-tier-components/data-model.md`):
//!
//! - [`ContentShape`] — content-shape allowlist enum (ELF / PE /
//!   Mach-O / shared lib / archive / OS package / lone manifest /
//!   exec script) and the per-file [`content_shape::classify`]
//!   function that applies FR-005's allowlist + path-prefix
//!   exclusion + adjacent-lockfile rule.
//! - [`dedupe::DedupeIndex`] — FR-011 hybrid dedupe: paths
//!   claimed by any package-tier component via the
//!   `evidence.occurrences[]` field (populated by milestone
//!   133 US2.3 for every reader) OR per-file SHA-256 hashes
//!   carried by binary-tier components. Built once per scan
//!   AFTER all package + binary readers complete.
//! - [`walker::walk_file_tier`] — drives `safe_walk` over the
//!   rootfs, classifies each file via [`ContentShape`], hashes
//!   surviving files via streaming SHA-256, dedupes against the
//!   index, and accumulates [`FileTierEntry`] records keyed by
//!   SHA-256 (per FR-006 per-unique-hash dedupe).
//!
//! **`mikebom:component-paths` is a spec fabrication**: the
//! original FR-011 wording references a `mikebom:component-paths`
//! property that mikebom never emitted. US2 (already merged at
//! PR US2.1 / US2.2 / US2.3) ships `mikebom:source-files`,
//! `mikebom:layer-digest`, and standards-native
//! `evidence.occurrences[]`. The DedupeIndex here reads from the
//! third — `evidence.occurrences[]` — which after US2.3 covers
//! 2925 / 2926 components (99.96 %) on the audit baseline.

pub(crate) mod content_shape;
pub(crate) mod dedupe;
pub(crate) mod walker;

use std::path::PathBuf;

pub(crate) use content_shape::ContentShape;

/// Per-unique-content file-tier accumulator entry. One
/// `FileTierEntry` per unique SHA-256 observed on the rootfs;
/// multiple paths with identical content collapse to a single
/// entry whose `paths` Vec carries every observed path
/// (sort-stable per FR-007).
///
/// **Conversion to `ResolvedComponent`** lives in US1.B
/// (`crate::scan_fs::file_tier::mod::file_tier_entry_to_component`)
/// alongside the new C-rows that surface
/// `mikebom:component-tier`, `mikebom:file-paths`,
/// `mikebom:file-paths-truncated`. This entry type is the
/// in-process accumulator; emission shape comes later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileTierEntry {
    /// Lowercase-hex SHA-256 of the file's bytes. Computed via
    /// streaming hash (8 KB chunk buffer) to avoid loading huge
    /// files into memory. Mandatory per FR-008.
    pub(crate) sha256_hex: String,
    /// Every path observed for this content, sorted lex-ascending
    /// per FR-007. Rootfs-relative + no leading `/` per the FR-007
    /// / FR-012 convention.
    pub(crate) paths: Vec<PathBuf>,
    /// Content-shape classification that allowed this entry. One
    /// per entry — every path the entry covers had the same shape
    /// at insert time (matching basenames + matching magic bytes).
    pub(crate) shape: ContentShape,
    /// File size in bytes at scan time. Per-content (matches the
    /// SHA-256 anchor): all paths for the entry share the same
    /// content so size is invariant.
    pub(crate) size_bytes: u64,
}

impl FileTierEntry {
    /// Construct a new entry from the first path observed for a
    /// content hash. Subsequent observations of the same hash
    /// extend the existing entry via [`FileTierEntry::push_path`].
    pub(crate) fn new(
        sha256_hex: String,
        first_path: PathBuf,
        shape: ContentShape,
        size_bytes: u64,
    ) -> Self {
        Self {
            sha256_hex,
            paths: vec![first_path],
            shape,
            size_bytes,
        }
    }

    /// Append a path to an existing entry. Caller is responsible
    /// for ensuring the hash already matched and the entry is the
    /// one indexed under the same hash. The `paths` Vec is kept in
    /// insertion order during accumulation; the final sort runs
    /// once at finalization via [`FileTierEntry::finalize`].
    pub(crate) fn push_path(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    /// Sort `paths` lex-ascending per FR-007. Called once per
    /// entry at walk completion.
    pub(crate) fn finalize(&mut self) {
        self.paths.sort();
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn new_entry_starts_with_one_path() {
        let e = FileTierEntry::new(
            "abc123".to_string(),
            PathBuf::from("opt/foo"),
            ContentShape::ElfBinary,
            42,
        );
        assert_eq!(e.paths.len(), 1);
        assert_eq!(e.paths[0], PathBuf::from("opt/foo"));
        assert_eq!(e.sha256_hex, "abc123");
        assert_eq!(e.shape, ContentShape::ElfBinary);
        assert_eq!(e.size_bytes, 42);
    }

    #[test]
    fn push_path_appends_in_order() {
        let mut e = FileTierEntry::new(
            "abc123".to_string(),
            PathBuf::from("z/last"),
            ContentShape::ElfBinary,
            10,
        );
        e.push_path(PathBuf::from("a/first"));
        e.push_path(PathBuf::from("m/middle"));
        assert_eq!(
            e.paths,
            vec![
                PathBuf::from("z/last"),
                PathBuf::from("a/first"),
                PathBuf::from("m/middle"),
            ]
        );
    }

    #[test]
    fn finalize_sorts_paths_lex_ascending() {
        let mut e = FileTierEntry::new(
            "abc123".to_string(),
            PathBuf::from("z/last"),
            ContentShape::ElfBinary,
            10,
        );
        e.push_path(PathBuf::from("a/first"));
        e.push_path(PathBuf::from("m/middle"));
        e.finalize();
        assert_eq!(
            e.paths,
            vec![
                PathBuf::from("a/first"),
                PathBuf::from("m/middle"),
                PathBuf::from("z/last"),
            ]
        );
    }
}
