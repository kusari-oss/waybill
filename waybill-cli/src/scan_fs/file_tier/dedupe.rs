#![allow(dead_code)] // lifted by scan_cmd wiring at the bottom of this PR.

//! Milestone 133 US1 — FR-011 hybrid dedupe index.
//!
//! Before the file-tier walker emits an entry, it consults this
//! index to know whether a candidate file is ALREADY claimed by a
//! package-tier or binary-tier component. Claim sources:
//!
//! - **Path coverage**: every component's `evidence.occurrences[]`
//!   `location` field. After milestone 133 US2.3 (already shipped)
//!   this field covers 2925 / 2926 components (99.96 %) on the
//!   audit baseline — every cargo / npm / nuget / maven / pypi /
//!   gem / golang component PLUS every OS-package (apk / dpkg /
//!   rpm) deep-hash occurrence.
//!
//! - **Hash coverage**: every component's `hashes[]` SHA-256 value
//!   (binary-tier components from milestone-104 readers carry per-
//!   file hashes; some package-tier readers also carry manifest-
//!   level hashes — both flow into the same set).
//!
//! **`waybill:component-paths` is NOT consulted**: the spec's
//! original FR-011 references this property name, but waybill has
//! never emitted it. US2.3 ships standards-native `evidence.occurrences[]`
//! instead; that's the source this index reads from.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use waybill_common::resolution::ResolvedComponent;
use waybill_common::types::hash::HashAlgorithm;

/// Hybrid dedupe set per FR-011 (CORRECTED): a candidate file is
/// covered when EITHER its rootfs-relative path appears in any
/// component's `evidence.occurrences[].location` OR its SHA-256
/// matches any component's `hashes[]` entry.
///
/// Built once per scan AFTER all package-DB and binary-tier
/// readers complete. Immutable thereafter.
#[derive(Debug, Default)]
pub(crate) struct DedupeIndex {
    /// Rootfs-relative paths claimed by a package-tier or
    /// binary-tier component via the CDX-native `evidence.occurrences[]`
    /// field. Per the milestone-133 US2.1 normalization convention
    /// every path here is rootfs-relative with NO leading `/`.
    claimed_paths: HashSet<PathBuf>,
    /// Lowercase-hex SHA-256 hashes claimed by ANY component's
    /// `hashes[]` field. Captures binary-tier per-file hashes
    /// (milestone 104) AND OS-package deep-hash component roots
    /// (milestones 038 / 039 / 040).
    claimed_hashes: HashSet<String>,
}

impl DedupeIndex {
    /// Build the index from the already-resolved component vector.
    /// MUST be called AFTER every reader (package-DB, binary-tier,
    /// enrichment) completes — the walker reads downstream of
    /// component resolution so the index has full coverage at
    /// inspection time.
    pub(crate) fn build(components: &[ResolvedComponent]) -> Self {
        let mut claimed_paths: HashSet<PathBuf> = HashSet::new();
        let mut claimed_hashes: HashSet<String> = HashSet::new();

        for c in components {
            for occ in &c.occurrences {
                // Strip leading `/` to match the no-leading-`/`
                // convention from FR-007 / FR-012. Occurrences
                // populated by US2.3 are already rootfs-relative
                // without leading `/`; OS-package deep-hash
                // occurrences (apk / dpkg / rpm) use the
                // dpkg-declared path WITH leading `/`. Normalize
                // here so both shapes index identically.
                let normalized = occ.location.trim_start_matches('/');
                claimed_paths.insert(PathBuf::from(normalized));
            }
            for hash in &c.hashes {
                if hash.algorithm == HashAlgorithm::Sha256 {
                    // `HexString::new` lowercases at construction;
                    // `as_str` returns the lowercased canonical form.
                    claimed_hashes.insert(hash.value.as_str().to_string());
                }
            }
        }

        Self {
            claimed_paths,
            claimed_hashes,
        }
    }

    /// FR-011 hybrid coverage check. Returns `true` when the file
    /// is COVERED (skip file-tier emission), `false` when it's
    /// orphan (emit).
    ///
    /// Path comparison uses the same rootfs-relative + no-leading-`/`
    /// normalization the index applied at build time. Hash
    /// comparison uses lowercase-hex.
    pub(crate) fn is_covered(&self, rel_path: &Path, sha256_hex: &str) -> bool {
        let normalized = rel_path.strip_prefix("/").unwrap_or(rel_path);
        if self.claimed_paths.contains(normalized) {
            return true;
        }
        if self.claimed_hashes.contains(&sha256_hex.to_ascii_lowercase()) {
            return true;
        }
        false
    }

    /// Diagnostic counter — how many distinct paths the index
    /// claims. Used by skip-counter / inventory annotations.
    pub(crate) fn claimed_path_count(&self) -> usize {
        self.claimed_paths.len()
    }

    /// Diagnostic counter — how many distinct SHA-256 hashes the
    /// index claims.
    pub(crate) fn claimed_hash_count(&self) -> usize {
        self.claimed_hashes.len()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::resolution::{
        FileOccurrence, ResolutionEvidence, ResolutionTechnique, ResolvedComponent,
    };
    use waybill_common::types::hash::{ContentHash, HashAlgorithm};
    use waybill_common::types::purl::Purl;
    use std::path::PathBuf;

    fn make_component(occurrences: Vec<FileOccurrence>, hashes: Vec<ContentHash>) -> ResolvedComponent {
        ResolvedComponent {
            name: "x".to_string(),
            version: "1.0".to_string(),
            purl: Purl::new("pkg:generic/x@1.0").unwrap(),
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::PackageDatabase,
                confidence: 0.85,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes,
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences,
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
            extra_annotations: std::collections::BTreeMap::new(),
            binary_role: None,
        }
    }

    fn occ(location: &str, sha256: &str) -> FileOccurrence {
        FileOccurrence {
            location: location.to_string(),
            sha256: sha256.to_string(),
            md5_legacy: None,
            apk_sha1: None,
            rpm_file_digest: None,
        }
    }

    #[test]
    fn empty_index_covers_nothing() {
        let idx = DedupeIndex::build(&[]);
        assert!(!idx.is_covered(&PathBuf::from("usr/bin/jq"), "abc"));
        assert_eq!(idx.claimed_path_count(), 0);
        assert_eq!(idx.claimed_hash_count(), 0);
    }

    #[test]
    fn path_with_leading_slash_indexes_no_leading_slash() {
        let c = make_component(
            vec![occ("/usr/bin/jq", "deadbeef")],
            vec![],
        );
        let idx = DedupeIndex::build(&[c]);
        assert!(idx.is_covered(&PathBuf::from("usr/bin/jq"), ""));
    }

    #[test]
    fn rootfs_relative_occurrence_indexes_same() {
        let c = make_component(
            vec![occ("app/Cargo.lock", "deadbeef")],
            vec![],
        );
        let idx = DedupeIndex::build(&[c]);
        assert!(idx.is_covered(&PathBuf::from("app/Cargo.lock"), ""));
    }

    fn sha256_full(seed: &str) -> ContentHash {
        // Repeat the 8-char seed 8 times → 64-char lowercase hex.
        let hex = seed.repeat(8);
        ContentHash::with_algorithm(HashAlgorithm::Sha256, &hex).unwrap()
    }

    #[test]
    fn hash_coverage_works() {
        let c = make_component(vec![], vec![sha256_full("ab12cd34")]);
        let idx = DedupeIndex::build(&[c]);
        assert!(idx.is_covered(&PathBuf::from("anywhere"), &"ab12cd34".repeat(8)));
    }

    #[test]
    fn hash_coverage_ignores_non_sha256() {
        // SHA-512 is 128 hex chars; build with the same algorithm
        // and verify SHA-256 lookup misses.
        let h = ContentHash::with_algorithm(
            HashAlgorithm::Sha512,
            &"deadbeef".repeat(16),
        )
        .unwrap();
        let c = make_component(vec![], vec![h]);
        let idx = DedupeIndex::build(&[c]);
        assert!(!idx.is_covered(&PathBuf::from("anywhere"), &"deadbeef".repeat(8)));
    }

    #[test]
    fn unknown_path_and_hash_returns_false() {
        let c = make_component(vec![occ("usr/bin/jq", "abc")], vec![]);
        let idx = DedupeIndex::build(&[c]);
        assert!(!idx.is_covered(&PathBuf::from("opt/custom-tool"), "xyz"));
    }

    #[test]
    fn diagnostic_counters_report_expected_counts() {
        let c1 = make_component(
            vec![occ("usr/bin/jq", "h1"), occ("usr/bin/jq.1.gz", "h2")],
            vec![sha256_full("deadbeef")],
        );
        let c2 = make_component(vec![occ("usr/bin/curl", "h3")], vec![]);
        let idx = DedupeIndex::build(&[c1, c2]);
        assert_eq!(idx.claimed_path_count(), 3);
        assert_eq!(idx.claimed_hash_count(), 1);
    }

    #[test]
    fn hash_match_is_case_insensitive() {
        // HexString normalizes to lowercase at construction. So
        // even if the caller passes uppercase, the index stores
        // lowercase. The lookup also lowercases. Both paths
        // converge on lowercase comparison.
        let c = make_component(
            vec![],
            vec![ContentHash::with_algorithm(
                HashAlgorithm::Sha256,
                &"ABCDEF12".repeat(8),
            )
            .unwrap()],
        );
        let idx = DedupeIndex::build(&[c]);
        assert!(idx.is_covered(&PathBuf::from("anywhere"), &"abcdef12".repeat(8)));
        assert!(idx.is_covered(&PathBuf::from("anywhere"), &"ABCDEF12".repeat(8)));
    }
}
