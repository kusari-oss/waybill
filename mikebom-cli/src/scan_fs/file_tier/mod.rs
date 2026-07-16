// `dead_code` lifted in US1.B; remaining items reach production via
// `scan_cmd::scan` once the integration in this PR lands. Allow
// kept until the final wiring change in this PR makes every item
// transitively reachable.
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

use std::collections::BTreeMap;
use std::path::PathBuf;

use mikebom_common::resolution::{
    ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
};
use mikebom_common::types::hash::{ContentHash, HashAlgorithm};
use mikebom_common::types::purl::Purl;
use serde_json::json;

pub(crate) use content_shape::ContentShape;

/// CDX component type emitted for file-tier components per FR-001.
/// The CDX 1.6 schema reserves `"file"` for components that ARE
/// files at a known path; mikebom file-tier entries qualify by
/// definition (the SHA-256 hash IS the identity, paths are
/// rootfs-relative).
pub(crate) const FILE_TIER_CDX_TYPE: &str = "file";

/// `mikebom:component-tier` annotation key. Mark every file-tier
/// component with `value = "file"` per FR-002. Emission code paths
/// (CDX `builder.rs`, SPDX 2.3 `packages.rs`, SPDX 3 packages
/// builder) recognize this annotation and adapt — CDX overrides the
/// `type` field and drops `purl`; the SPDX paths swap the element
/// shape per the format's file-element semantics (US1.C polish).
/// The scan-side code stays format-neutral per FR-017.
pub(crate) const COMPONENT_TIER_KEY: &str = "mikebom:component-tier";

/// Annotation value paired with [`COMPONENT_TIER_KEY`] for file-tier
/// emission. Plain string per existing mikebom:* annotation
/// conventions (compares equal in CDX property + SPDX annotation
/// envelope round-trips).
pub(crate) const COMPONENT_TIER_FILE_VALUE: &str = "file";

/// `mikebom:file-paths` annotation key. Value is a JSON-encoded
/// string array carrying every path the entry covers, sorted
/// lex-ascending per FR-007. Capped at 100 entries; when the cap
/// fires, the companion [`FILE_PATHS_TRUNCATED_KEY`] annotation is
/// also set to `"true"`.
pub(crate) const FILE_PATHS_KEY: &str = "mikebom:file-paths";

/// `mikebom:file-paths-truncated` annotation key. Emitted alongside
/// [`FILE_PATHS_KEY`] when the entry's path list was capped at the
/// 100-entry limit (defensive — protects against pathological cases
/// like a `.gitignore` file content duplicated across 1000s of nested
/// repos).
pub(crate) const FILE_PATHS_TRUNCATED_KEY: &str = "mikebom:file-paths-truncated";

/// FR-007 / FR-002: cap the per-entry `paths[]` length at 100
/// entries. Excess paths are dropped and the truncation flag fires.
pub(crate) const FILE_PATHS_CAP: usize = 100;

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

    /// Convert this file-tier accumulator entry into a
    /// `ResolvedComponent` ready for the SBOM emission pipeline.
    ///
    /// **Placeholder PURL strategy** (FR-009 — "no PURL"):
    /// `ResolvedComponent.purl` is non-optional, so we synthesize a
    /// content-addressed placeholder PURL
    /// (`pkg:generic/file-tier?content-sha256=<sha>`) for
    /// in-process identity. The CDX / SPDX 2.3 / SPDX 3 emission
    /// code paths recognize the [`COMPONENT_TIER_KEY`] annotation
    /// and STRIP the PURL field at write time, so the on-disk SBOM
    /// honors FR-009. The placeholder is never serialized.
    ///
    /// **Name** (FR-009): basename of the first sorted path.
    ///
    /// **Version**: empty string (file-tier components have no
    /// version concept).
    ///
    /// **Hashes**: SHA-256 in the canonical `ContentHash` form.
    ///
    /// **Annotations** seeded:
    /// - `mikebom:component-tier = "file"` (FR-002)
    /// - `mikebom:file-paths = <sorted JSON array of paths>` (FR-007)
    ///   — native JSON array, NOT a JSON-string-encoded array.
    ///   The stringified-array shape (pre-milestone-145) was a wire
    ///   bug; consumers were forced to do a second `JSON.parse(value)`
    ///   to extract the paths. Fixed in milestone 145 US1 per
    ///   `specs/145-annotation-parity-fixes/contracts/file-paths-shape.md`.
    /// - `mikebom:file-paths-truncated = "true"` (only when capped)
    pub(crate) fn into_resolved_component(self) -> ResolvedComponent {
        let basename = self
            .paths
            .first()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();

        let placeholder_purl_raw = format!(
            "pkg:generic/file-tier?content-sha256={}",
            self.sha256_hex
        );
        let purl = Purl::new(&placeholder_purl_raw)
            .expect("placeholder file-tier PURL syntax is valid");

        let hashes = ContentHash::with_algorithm(HashAlgorithm::Sha256, &self.sha256_hex)
            .ok()
            .map(|h| vec![h])
            .unwrap_or_default();

        // Cap path list at FILE_PATHS_CAP and set the truncated
        // flag when we drop overflow. FR-007 demands sorted-
        // ascending; `finalize()` already sorted, so a simple
        // truncate keeps determinism.
        let truncated = self.paths.len() > FILE_PATHS_CAP;
        let mut paths_str: Vec<String> = self
            .paths
            .iter()
            .take(FILE_PATHS_CAP)
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        // Keep sort-stable in the emitted property too.
        paths_str.sort();

        let mut extra_annotations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        extra_annotations.insert(
            COMPONENT_TIER_KEY.to_string(),
            json!(COMPONENT_TIER_FILE_VALUE),
        );
        // Milestone 145 US1 (FR-001 + research §A): emit the value as a
        // native JSON array, NOT a JSON-string-encoding of the array.
        // Pre-145: `Value::String("[\"path1\",\"path2\"]")`.
        // Post-145: `Value::Array([Value::String("path1"), ...])`.
        extra_annotations.insert(FILE_PATHS_KEY.to_string(), json!(paths_str));
        if truncated {
            extra_annotations.insert(FILE_PATHS_TRUNCATED_KEY.to_string(), json!("true"));
        }

        ResolvedComponent {
            purl,
            name: basename,
            version: String::new(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::HashMatch,
                confidence: 1.0,
                source_connection_ids: vec![],
                source_file_paths: paths_str.clone(),
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes,
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            build_inclusion: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: None,
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: vec![],
            extra_annotations,
            binary_role: None,
        }
    }
}

/// Operator-facing file-inventory mode per FR-015. Wired through
/// from the `--file-inventory` CLI flag to the orphan walker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInventoryMode {
    /// **Default (US1.B)**: no file-tier emission. Preserves
    /// byte-identity with pre-milestone-133 SBOMs. US1.C flips
    /// this default to [`FileInventoryMode::Orphan`].
    Off,
    /// **US1.C default (planned)**: emit file-tier components for
    /// content surviving the FR-005 allowlist AND failing the
    /// FR-011 hybrid dedupe.
    Orphan,
    /// **US3**: emit a file-tier component for every regular file
    /// surviving the FR-005 allowlist, regardless of dedupe
    /// coverage. Used by forensic / compliance consumers cataloguing
    /// every hash on disk.
    Full,
}

impl FileInventoryMode {
    /// Parse the operator-facing string form. Accepts `off`,
    /// `orphan`, `full` case-insensitively. Returns `None` for
    /// unknown values so the CLI layer can produce a clap-style
    /// error message.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "orphan" => Some(Self::Orphan),
            "full" => Some(Self::Full),
            _ => None,
        }
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

    #[test]
    fn into_resolved_component_seeds_required_annotations() {
        let mut e = FileTierEntry::new(
            "0".repeat(64),
            PathBuf::from("opt/custom/foo"),
            ContentShape::ElfBinary,
            42,
        );
        e.finalize();
        let c = e.into_resolved_component();
        assert_eq!(c.name, "foo");
        assert_eq!(c.version, "");
        assert_eq!(c.hashes.len(), 1);
        assert_eq!(c.hashes[0].algorithm, HashAlgorithm::Sha256);
        assert_eq!(
            c.extra_annotations
                .get(COMPONENT_TIER_KEY)
                .and_then(|v| v.as_str()),
            Some(COMPONENT_TIER_FILE_VALUE)
        );
        // Milestone 145 US1: file-paths emitted as a native JSON array
        // (NOT a JSON-string-encoding of the array). Pre-145 this test
        // round-tripped through `serde_json::from_str(value)`; post-145
        // the value IS the array directly.
        let fp = c
            .extra_annotations
            .get(FILE_PATHS_KEY)
            .expect("file-paths annotation present");
        let parsed: Vec<String> = fp
            .as_array()
            .expect("file-paths is array (milestone 145 US1)")
            .iter()
            .map(|v| v.as_str().expect("path is string").to_string())
            .collect();
        assert_eq!(parsed, vec!["opt/custom/foo".to_string()]);
        // Single path → no truncated flag.
        assert!(!c.extra_annotations.contains_key(FILE_PATHS_TRUNCATED_KEY));
    }

    #[test]
    fn into_resolved_component_caps_paths_and_sets_truncated_flag() {
        let mut e = FileTierEntry::new(
            "1".repeat(64),
            PathBuf::from("opt/dup-0"),
            ContentShape::ElfBinary,
            10,
        );
        for i in 1..FILE_PATHS_CAP + 5 {
            e.push_path(PathBuf::from(format!("opt/dup-{i:03}")));
        }
        e.finalize();
        let c = e.into_resolved_component();
        // Milestone 145 US1: file-paths is a native JSON array (NOT a
        // JSON-string-encoding of the array). See the sibling test
        // above for the rationale.
        let fp = c
            .extra_annotations
            .get(FILE_PATHS_KEY)
            .expect("file-paths annotation present");
        let parsed: Vec<String> = fp
            .as_array()
            .expect("file-paths is array (milestone 145 US1)")
            .iter()
            .map(|v| v.as_str().expect("path is string").to_string())
            .collect();
        assert_eq!(parsed.len(), FILE_PATHS_CAP);
        assert_eq!(
            c.extra_annotations
                .get(FILE_PATHS_TRUNCATED_KEY)
                .and_then(|v| v.as_str()),
            Some("true")
        );
    }

    #[test]
    fn into_resolved_component_placeholder_purl_is_valid_and_content_addressed() {
        let sha = "abcd".repeat(16);
        let mut e = FileTierEntry::new(
            sha.clone(),
            PathBuf::from("opt/foo"),
            ContentShape::ElfBinary,
            10,
        );
        e.finalize();
        let c = e.into_resolved_component();
        // Placeholder PURL embeds the SHA-256 so different
        // contents distinct identities even before the
        // mikebom:component-tier-driven PURL drop at emission.
        assert!(c.purl.as_str().contains(&sha));
        assert!(c.purl.as_str().starts_with("pkg:generic/file-tier"));
    }

    /// Milestone 145 US1 T004 (SC-002): the `mikebom:file-paths`
    /// annotation value MUST be a `Value::Array`, NOT a `Value::String`
    /// holding a JSON-string-encoding of the array. Guards against
    /// regression of the pre-145 double-encoding bug.
    #[test]
    fn mikebom_file_paths_is_native_array_not_stringified() {
        let mut e = FileTierEntry::new(
            "abcd".repeat(16),
            PathBuf::from("usr/sbin/losetup"),
            ContentShape::ElfBinary,
            10,
        );
        e.finalize();
        let c = e.into_resolved_component();
        let value = c
            .extra_annotations
            .get(FILE_PATHS_KEY)
            .expect("mikebom:file-paths is present");
        assert!(
            value.is_array(),
            "FR-001 violation: expected Value::Array, got {value:?}"
        );
        let arr = value.as_array().expect("file-paths is array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_str(), Some("usr/sbin/losetup"));
    }

    #[test]
    fn file_inventory_mode_parse_accepts_known_values() {
        assert_eq!(FileInventoryMode::parse("off"), Some(FileInventoryMode::Off));
        assert_eq!(
            FileInventoryMode::parse("ORPHAN"),
            Some(FileInventoryMode::Orphan)
        );
        assert_eq!(
            FileInventoryMode::parse("full"),
            Some(FileInventoryMode::Full)
        );
        assert_eq!(FileInventoryMode::parse("bogus"), None);
    }
}
