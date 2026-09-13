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

/// Directories that the usrmerge transition unified. On every modern
/// Linux distribution `/bin`, `/sbin`, `/lib` and `/lib64` are symlinks
/// into `/usr`, so `/bin/sh` and `/usr/bin/sh` name the same file.
const USRMERGE_DIRS: [&str; 4] = ["bin", "sbin", "lib", "lib64"];

/// Collapse a usrmerge alias onto one canonical spelling — the form
/// without the `usr/` prefix.
///
/// Applied to BOTH the claimed path at index-build time and the observed
/// path at lookup, so the two spellings of one file collide on a single
/// key. Without this the comparison is literal, and a file dpkg declares
/// as `/usr/lib/libc.so` but the walker reaches as `lib/libc.so` reads
/// as unclaimed — emitting a duplicate file-tier component for a file a
/// package already owns. On the `postgres:16` corpus target that
/// produced 564 phantom components; before the walker's traversal
/// changed, the same defect produced 407 under `bin/` and `sbin/`
/// instead. See #854.
///
/// **Trade-off.** On a pre-usrmerge rootfs, `/lib/foo` and
/// `/usr/lib/foo` could be genuinely distinct files, and folding them
/// means a truly-orphan file is suppressed when its same-named twin is
/// package-owned. That costs one file-tier entry; it never removes a
/// package component, and the owning package stays in the SBOM either
/// way. The alternative — today's behaviour — inflates every scan of
/// every modern image with hundreds of phantom components, so the fold
/// is the better failure.
fn fold_usrmerge(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path.to_path_buf();
    };
    let Some(rest) = s.strip_prefix("usr/") else {
        return path.to_path_buf();
    };
    match rest.split('/').next() {
        Some(head) if USRMERGE_DIRS.contains(&head) => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
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
                // Fold usrmerge aliases so a path claimed as
                // `/usr/bin/apt` indexes identically to one observed as
                // `bin/apt` (see `fold_usrmerge`).
                claimed_paths.insert(fold_usrmerge(Path::new(normalized)));
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
        if self.claimed_paths.contains(&fold_usrmerge(normalized)) {
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

    // -----------------------------------------------------------
    // #854 — usrmerge alias folding.
    //
    // dpkg declares one spelling and the walker may observe the other,
    // because /bin, /sbin, /lib and /lib64 are symlinks into /usr. A
    // literal comparison reads the file as unclaimed and emits a
    // duplicate file-tier component for a file a package already owns.
    // -----------------------------------------------------------

    #[test]
    fn claim_under_usr_covers_the_bare_alias() {
        let c = make_component(vec![occ("/usr/bin/apt", "aa")], vec![]);
        let idx = DedupeIndex::build(std::slice::from_ref(&c));
        assert!(
            idx.is_covered(&PathBuf::from("bin/apt"), "zz"),
            "a file dpkg declares at /usr/bin/apt must not re-emit when \
             the walker reaches it as bin/apt",
        );
    }

    #[test]
    fn claim_under_bare_alias_covers_the_usr_path() {
        // The mirror image: some packages declare the pre-usrmerge
        // spelling. Folding must be symmetric or the fix only works in
        // whichever direction happened to be tested.
        let c = make_component(vec![occ("/lib/x86_64-linux-gnu/libc.so.6", "aa")], vec![]);
        let idx = DedupeIndex::build(std::slice::from_ref(&c));
        assert!(
            idx.is_covered(&PathBuf::from("usr/lib/x86_64-linux-gnu/libc.so.6"), "zz"),
            "folding must work in both directions",
        );
    }

    #[test]
    fn only_the_usrmerge_directories_fold() {
        // `share` is not a usrmerge directory. Folding it would make
        // `usr/share/doc/x` and `share/doc/x` collide, suppressing a
        // genuinely orphan file.
        let c = make_component(vec![occ("/usr/share/doc/x", "aa")], vec![]);
        let idx = DedupeIndex::build(std::slice::from_ref(&c));
        assert!(
            !idx.is_covered(&PathBuf::from("share/doc/x"), "zz"),
            "non-usrmerge directories must not fold",
        );
    }

    #[test]
    fn a_genuinely_orphan_file_is_still_emitted() {
        // The regression guard for over-suppression: the fix must not
        // make everything look covered.
        let c = make_component(vec![occ("/usr/bin/apt", "aa")], vec![]);
        let idx = DedupeIndex::build(std::slice::from_ref(&c));
        assert!(
            !idx.is_covered(&PathBuf::from("usr/lib/unowned.so"), "zz"),
            "an unclaimed file must still emit",
        );
    }

    #[test]
    fn fold_usrmerge_is_idempotent_and_leaves_other_paths_alone() {
        assert_eq!(fold_usrmerge(Path::new("usr/bin/apt")), PathBuf::from("bin/apt"));
        assert_eq!(fold_usrmerge(Path::new("bin/apt")), PathBuf::from("bin/apt"));
        assert_eq!(
            fold_usrmerge(&fold_usrmerge(Path::new("usr/bin/apt"))),
            PathBuf::from("bin/apt"),
        );
        assert_eq!(fold_usrmerge(Path::new("etc/passwd")), PathBuf::from("etc/passwd"));
        assert_eq!(fold_usrmerge(Path::new("usr/share/doc")), PathBuf::from("usr/share/doc"));
        // `usrlib` must not be mistaken for the `usr/` prefix.
        assert_eq!(fold_usrmerge(Path::new("usrlib/x")), PathBuf::from("usrlib/x"));
    }

}
