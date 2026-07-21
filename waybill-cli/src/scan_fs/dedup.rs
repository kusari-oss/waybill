//! Cross-reader deduplication pipeline (milestone 105 FR-015).
//!
//! When two or more readers identify the same library (e.g., gRPC's
//! `abseil-cpp` matched by BOTH the `git-submodule` reader AND a
//! `conan-recipe` reader scanning a third_party/abseil-cpp/conanfile.py),
//! waybill MUST emit exactly one SBOM component. This module is the
//! arbiter: takes the union of per-reader `DetectionRecord`s, groups
//! by canonical PURL string, selects a deterministic winner per group
//! per the precedence table in `data-model.md`, and emits a
//! `DedupResult` whose `also_detected_via` field records the losing
//! readers' source-mechanism values for the `waybill:also-detected-via`
//! annotation (and the parallel CDX-native
//! `evidence.identity[].methods[]` emission per research R1).
//!
//! **Determinism guarantees (SC-010)**:
//!
//! - The sort key is canonical PURL → precedence rank →
//!   source-mechanism discriminant string. All three are deterministic,
//!   so filesystem walk order CANNOT influence the chosen winner.
//! - The `also_detected_via` list is sorted lexicographically by the
//!   source-mechanism's canonical-string form. Identical SBOM output
//!   across walk-order permutations.
//!
//! **Dead-code suppression**: this module's public API is consumed by
//! the per-US implementations (US1-US6, T028+) and by the CDX hybrid
//! emitter (T022). Until those land, the public surface is
//! `#[allow(dead_code)]` to satisfy clippy's `dead_code` lint in the
//! intermediate state.

use std::collections::BTreeMap;

use super::package_db::PackageDbEntry;

/// Closed-enum identifier for *how* a component was discovered by a
/// reader. Maps 1:1 to the C55 catalog row's documented closed-enum
/// values (see `docs/reference/sbom-format-mapping.md`).
///
/// 7 variants existed before milestone 105 (alpha.41 / PR #272). The
/// six `Cpm*`, `Zephyr*`, `Idf*`, `VcpkgClassic`, `GitSubmodule`
/// variants are introduced by milestone 105 and become referenced as
/// the corresponding US phases land.
///
/// **Variant declaration order is load-bearing**: the derived `Ord`
/// is used as the Stage 3 tie-break in [`dedup`] (when two records
/// land in the same tier with the same PURL specificity rank). Two
/// invariants the order encodes:
///
/// 1. `VcpkgManifest < VcpkgClassic` — when both readers find the
///    same `pkg:vcpkg/<name>@<ver>`, the manifest-mode declaration
///    wins (spec US5 scenario 2).
/// 2. Higher-signal mechanisms (the ones that produce richer PURLs)
///    appear earlier in the list within each tier.
///
/// Reordering variants without updating the dedup tests is a
/// semantic break.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceMechanism {
    // ----- alpha.41 closed enum (PR #272) -----
    // Manifest-mode readers come first so they win Stage 3 lex
    // tie-breaks against mixed/filesystem readers in the same tier.
    VcpkgManifest,
    ConanRecipe,
    BazelHttpArchive,
    // Milestone 105 manifest-mode additions follow alphabetically
    // by ecosystem within the manifest-mode block.
    CpmCmake,
    IdfComponent,
    IdfComponentLocal,
    VcpkgClassic, // ranks BELOW VcpkgManifest per US5 scenario 2
    ZephyrWest,
    // Milestone 107 — Yocto / OpenEmbedded readers (FR-010 precedence:
    // installed-DB > image-manifest > recipe-declaration).
    // OpkgInstalled outranks YoctoImageManifest because it observes
    // what's ACTUALLY installed on the device vs what was INTENDED to
    // ship. BitbakeRecipe is lowest — recipes declared by a layer may
    // never have been selected by any image build.
    OpkgInstalled,
    YoctoImageManifest,
    BitbakeRecipe,
    // Mixed-tier (manifest-driven but non-canonical PURL form).
    CmakeFetchcontentGit,
    CmakeFetchcontentUrl,
    CmakeExternalproject,
    // Filesystem-derived (lowest priority within filesystem tier).
    CmakeVendored,
    GitSubmodule,
}

impl SourceMechanism {
    /// Returns the C55 canonical string for this source-mechanism. The
    /// returned value MUST match the `waybill:source-mechanism`
    /// annotation value emitted in the SBOM exactly (for parity-catalog
    /// round-trip).
    #[allow(dead_code)]
    pub fn canonical_str(self) -> &'static str {
        match self {
            Self::CmakeFetchcontentGit => "cmake-fetchcontent-git",
            Self::CmakeFetchcontentUrl => "cmake-fetchcontent-url",
            Self::CmakeExternalproject => "cmake-externalproject",
            Self::CmakeVendored => "cmake-vendored",
            Self::BazelHttpArchive => "bazel-http-archive",
            Self::VcpkgManifest => "vcpkg-manifest",
            Self::ConanRecipe => "conan-recipe",
            Self::CpmCmake => "cpm-cmake",
            Self::ZephyrWest => "zephyr-west",
            Self::IdfComponent => "idf-component",
            Self::IdfComponentLocal => "idf-component-local",
            Self::VcpkgClassic => "vcpkg-classic",
            Self::GitSubmodule => "git-submodule",
            Self::OpkgInstalled => "opkg-installed",
            Self::YoctoImageManifest => "yocto-image-manifest",
            Self::BitbakeRecipe => "bitbake-recipe",
        }
    }
}

/// A single reader's detection of a component. Each reader produces a
/// `Vec<DetectionRecord>` for its scan; the dispatcher concatenates
/// the vectors and feeds the union into [`dedup`] which collapses
/// duplicates by canonical PURL.
///
/// Note: only `Clone` + `Debug` are derived. `PartialEq` / `Eq` are
/// NOT derived because `PackageDbEntry` (the `reader_output` field)
/// doesn't derive them. Tests assert against individual fields
/// rather than whole-struct equality.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DetectionRecord {
    /// Canonical PURL string after normalization. Used as the
    /// grouping key. Two records with the same `canonical_purl`
    /// represent the same component identified by two different
    /// readers.
    pub canonical_purl: String,
    /// Which reader identified this component.
    pub source_mechanism: SourceMechanism,
    /// The underlying reader output (the existing `PackageDbEntry`
    /// shape). The winning record's `reader_output` flows through to
    /// SBOM emission.
    pub reader_output: PackageDbEntry,
}

/// The output of the dedup pipeline: one entry per unique canonical
/// PURL, with the winning reader's record and the losing
/// source-mechanisms listed for `waybill:also-detected-via` emission.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DedupResult {
    pub winners: Vec<DedupedComponent>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DedupedComponent {
    pub canonical_purl: String,
    pub winning_source_mechanism: SourceMechanism,
    pub winning_reader_output: PackageDbEntry,
    /// Source-mechanism values of any OTHER readers that produced the
    /// same canonical PURL. Sorted lexicographically by
    /// `canonical_str()` for determinism (SC-010). Empty when only
    /// one reader matched the PURL.
    pub also_detected_via: Vec<SourceMechanism>,
}

/// Precedence rank for a single detection record. Smaller value
/// wins per FR-015. The total ordering is:
///
/// 1. **Tier rank** (Stage 1 of `data-model.md`'s precedence model):
///    - 0 = Manifest-mode (VcpkgManifest, VcpkgClassic, ConanRecipe,
///      CpmCmake, ZephyrWest, IdfComponent, IdfComponentLocal,
///      BazelHttpArchive)
///    - 1 = Mixed (CmakeFetchcontentGit, CmakeFetchcontentUrl,
///      CmakeExternalproject)
///    - 2 = Filesystem-derived (GitSubmodule, CmakeVendored)
///
/// 2. **PURL specificity** (Stage 2): inside a tier, more specific
///    PURL prefixes outrank less specific:
///    - 0 = `pkg:conan/`, `pkg:vcpkg/`, `pkg:idf/`
///    - 1 = `pkg:github/`, `pkg:bazel/`
///    - 2 = `pkg:git+https://`, `pkg:git+ssh://`
///    - 3 = anything else (including `pkg:generic/`)
///
/// 3. **Discriminant tie-break** (Stage 3): lexicographic comparison
///    of `canonical_str()`. Used only when stages 1+2 don't resolve
///    (rare; would require two records in the same tier with the
///    same PURL prefix rank).
///
/// The return value packs the three stages into a 16-bit composite:
/// `(tier << 8) | purl_rank`. The discriminant tie-break happens
/// outside this fn via a `then_with` on the canonical-string compare.
#[allow(dead_code)]
pub fn precedence_rank(record: &DetectionRecord) -> u16 {
    let tier: u16 = match record.source_mechanism {
        // Manifest-mode tier (highest priority).
        SourceMechanism::VcpkgManifest
        | SourceMechanism::VcpkgClassic
        | SourceMechanism::ConanRecipe
        | SourceMechanism::CpmCmake
        | SourceMechanism::ZephyrWest
        | SourceMechanism::IdfComponent
        | SourceMechanism::IdfComponentLocal
        | SourceMechanism::BazelHttpArchive => 0,
        // Mixed tier — manifest-driven but produces non-canonical PURLs.
        SourceMechanism::CmakeFetchcontentGit
        | SourceMechanism::CmakeFetchcontentUrl
        | SourceMechanism::CmakeExternalproject => 1,
        // Milestone 107 — Yocto / OpenEmbedded readers.
        // OpkgInstalled is tier 0 (highest authority — what's actually
        // installed on the device, same authority as installed-DB
        // readers in other ecosystems). YoctoImageManifest is tier 0
        // too (BitBake-recorded; authoritative for the image's
        // intended contents). BitbakeRecipe is tier 2 (layer
        // declaration only — recipes may never have been built).
        SourceMechanism::OpkgInstalled | SourceMechanism::YoctoImageManifest => 0,
        SourceMechanism::BitbakeRecipe => 2,
        // Filesystem-derived tier (lowest priority).
        SourceMechanism::GitSubmodule | SourceMechanism::CmakeVendored => 2,
    };
    let purl_rank = purl_specificity_rank(&record.canonical_purl);
    (tier << 8) | purl_rank
}

/// Returns the PURL specificity rank (Stage 2 of the precedence
/// model). Smaller is more specific.
fn purl_specificity_rank(purl: &str) -> u16 {
    if purl.starts_with("pkg:conan/")
        || purl.starts_with("pkg:vcpkg/")
        || purl.starts_with("pkg:idf/")
    {
        0
    } else if purl.starts_with("pkg:github/") || purl.starts_with("pkg:bazel/") {
        1
    } else if purl.starts_with("pkg:git+https://") || purl.starts_with("pkg:git+ssh://") {
        2
    } else {
        3
    }
}

/// Main dedup entry point. Consumes the union of per-reader detection
/// records and produces one `DedupedComponent` per unique canonical
/// PURL.
///
/// Algorithm per `contracts/dedup-precedence.md`:
///
/// 1. Group records by `canonical_purl`.
/// 2. Within each group, select the record with the smallest
///    `precedence_rank(record)`. Ties (same tier + same PURL
///    specificity) are broken lexicographically by
///    `source_mechanism.canonical_str()`.
/// 3. Collect the losing records' source-mechanisms into the winning
///    record's `also_detected_via` list, sorted lexicographically by
///    `canonical_str()` for determinism (SC-010).
///
/// The output is sorted by `canonical_purl` (lex order) so the
/// resulting SBOM emission is byte-identical regardless of the input
/// iteration order — the SC-010 walk-order-invariance guarantee.
#[allow(dead_code)]
pub fn dedup(records: Vec<DetectionRecord>) -> DedupResult {
    // Group by canonical PURL using a BTreeMap so the iteration order
    // is lexicographic (deterministic).
    let mut groups: BTreeMap<String, Vec<DetectionRecord>> = BTreeMap::new();
    for record in records {
        groups
            .entry(record.canonical_purl.clone())
            .or_default()
            .push(record);
    }

    let mut winners: Vec<DedupedComponent> = Vec::with_capacity(groups.len());
    for (purl, mut group) in groups {
        // Sort the group by precedence_rank, then by the
        // SourceMechanism enum's declaration-order Ord (Stage 3
        // tie-break). The first element is the winner. The enum's
        // variant order encodes spec-specific tie-break invariants
        // (e.g. VcpkgManifest < VcpkgClassic per US5).
        group.sort_by(|a, b| {
            precedence_rank(a)
                .cmp(&precedence_rank(b))
                .then_with(|| a.source_mechanism.cmp(&b.source_mechanism))
        });
        // SAFETY: group is non-empty by construction (BTreeMap entry
        // exists only if we pushed at least one record into it).
        let winner = group.remove(0);
        let mut losers: Vec<SourceMechanism> =
            group.into_iter().map(|r| r.source_mechanism).collect();
        // Sort losers by enum order for the
        // `waybill:also-detected-via` annotation determinism.
        losers.sort();
        winners.push(DedupedComponent {
            canonical_purl: purl,
            winning_source_mechanism: winner.source_mechanism,
            winning_reader_output: winner.reader_output,
            also_detected_via: losers,
        });
    }

    DedupResult { winners }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use waybill_common::types::purl::Purl;

    fn rec(purl: &str, sm: SourceMechanism) -> DetectionRecord {
        DetectionRecord {
            canonical_purl: purl.to_string(),
            source_mechanism: sm,
            reader_output: PackageDbEntry {
                build_inclusion: None,
                purl: Purl::new(purl).unwrap(),
                name: "test".to_string(),
                version: "0.0".to_string(),
                arch: None,
                source_path: format!("/{}", sm.canonical_str()),
                depends: Vec::new(),
                maintainer: None,
                lifecycle_scope: None,
                requirement_ranges: Vec::new(),
                source_type: None,
                licenses: Vec::new(),
                buildinfo_status: None,
                sbom_tier: None,
                evidence_kind: None,
                binary_class: None,
                binary_stripped: None,
                linkage_kind: None,
                detected_go: None,
                confidence: None,
                binary_packed: None,
                raw_version: None,
                parent_purl: None,
                npm_role: None,
                co_owned_by: None,
                hashes: Vec::new(),
                shade_relocation: None,
                extra_annotations: Default::default(),
                binary_role: None,
            },
        }
    }

    // ----------------------------------------------------------------
    // Stage 1: tier precedence
    // ----------------------------------------------------------------

    #[test]
    fn precedence_manifest_outranks_filesystem() {
        // Same PURL discovered by ConanRecipe (manifest-mode) AND
        // GitSubmodule (filesystem). Manifest-mode wins.
        let result = dedup(vec![
            rec("pkg:generic/foo@1.0", SourceMechanism::GitSubmodule),
            rec("pkg:generic/foo@1.0", SourceMechanism::ConanRecipe),
        ]);
        assert_eq!(result.winners.len(), 1);
        assert_eq!(
            result.winners[0].winning_source_mechanism,
            SourceMechanism::ConanRecipe
        );
        assert_eq!(
            result.winners[0].also_detected_via,
            vec![SourceMechanism::GitSubmodule]
        );
    }

    #[test]
    fn precedence_manifest_outranks_mixed_outranks_filesystem() {
        // Three readers detect the same PURL: ConanRecipe (manifest),
        // CmakeFetchcontentGit (mixed), CmakeVendored (filesystem).
        let result = dedup(vec![
            rec("pkg:generic/foo@1.0", SourceMechanism::CmakeVendored),
            rec("pkg:generic/foo@1.0", SourceMechanism::CmakeFetchcontentGit),
            rec("pkg:generic/foo@1.0", SourceMechanism::ConanRecipe),
        ]);
        assert_eq!(result.winners.len(), 1);
        assert_eq!(
            result.winners[0].winning_source_mechanism,
            SourceMechanism::ConanRecipe
        );
        // Losers MUST be sorted lex by canonical_str: "cmake-fetchcontent-git" < "cmake-vendored".
        assert_eq!(
            result.winners[0].also_detected_via,
            vec![
                SourceMechanism::CmakeFetchcontentGit,
                SourceMechanism::CmakeVendored,
            ]
        );
    }

    // ----------------------------------------------------------------
    // Stage 2: PURL specificity tie-break within a tier
    // ----------------------------------------------------------------

    #[test]
    fn precedence_purl_specificity_beats_within_same_tier() {
        // Both records are manifest-tier; the one with the more specific
        // PURL (pkg:conan/) wins over the one with a less specific PURL
        // (pkg:generic/).
        //
        // Note: the same canonical PURL is required for them to be in
        // the same group at all. To test specificity within a tier
        // meaningfully, we'd need two readers producing the SAME PURL.
        // In practice this happens when ConanRecipe and CpmCmake both
        // emit `pkg:github/foo/bar@1.0` (both manifest-tier; CpmCmake
        // wins by alphabetical canonical-string since they're both
        // PURL-specificity rank 1).
        let result = dedup(vec![
            rec("pkg:github/foo/bar@1.0", SourceMechanism::ConanRecipe),
            rec("pkg:github/foo/bar@1.0", SourceMechanism::CpmCmake),
        ]);
        assert_eq!(result.winners.len(), 1);
        // Tier 0 + PURL rank 1 for both → Stage 3 lex tie-break:
        // "conan-recipe" < "cpm-cmake" lexicographically.
        assert_eq!(
            result.winners[0].winning_source_mechanism,
            SourceMechanism::ConanRecipe
        );
    }

    // ----------------------------------------------------------------
    // Stage 3: lex tie-break (safety net)
    // ----------------------------------------------------------------

    #[test]
    fn precedence_lex_tie_break_when_tier_and_purl_match() {
        // Three filesystem-tier readers with the same PURL specificity
        // (rank 3, anything-else) → resolved by lex canonical-string:
        // "cmake-vendored" < "git-submodule".
        let result = dedup(vec![
            rec("pkg:generic/foo@1.0", SourceMechanism::GitSubmodule),
            rec("pkg:generic/foo@1.0", SourceMechanism::CmakeVendored),
        ]);
        assert_eq!(result.winners.len(), 1);
        assert_eq!(
            result.winners[0].winning_source_mechanism,
            SourceMechanism::CmakeVendored
        );
        assert_eq!(
            result.winners[0].also_detected_via,
            vec![SourceMechanism::GitSubmodule]
        );
    }

    // ----------------------------------------------------------------
    // Single-reader passthrough
    // ----------------------------------------------------------------

    #[test]
    fn single_reader_emits_no_also_detected_via() {
        let result = dedup(vec![rec(
            "pkg:conan/zlib@1.3.1",
            SourceMechanism::ConanRecipe,
        )]);
        assert_eq!(result.winners.len(), 1);
        assert_eq!(
            result.winners[0].winning_source_mechanism,
            SourceMechanism::ConanRecipe
        );
        assert!(
            result.winners[0].also_detected_via.is_empty(),
            "single reader → empty also_detected_via"
        );
    }

    // ----------------------------------------------------------------
    // Determinism: input order MUST NOT affect output
    // ----------------------------------------------------------------

    #[test]
    fn dedup_is_input_order_invariant() {
        // SC-010 cornerstone: two records, three different input
        // orderings, all produce byte-identical output (winners list +
        // also_detected_via sort + canonical_purl sort).
        let a = rec("pkg:generic/foo@1.0", SourceMechanism::GitSubmodule);
        let b = rec("pkg:generic/foo@1.0", SourceMechanism::ConanRecipe);
        let c = rec("pkg:vcpkg/bar@2.0", SourceMechanism::VcpkgManifest);
        let d = rec("pkg:vcpkg/bar@2.0", SourceMechanism::VcpkgClassic);

        let r1 = dedup(vec![a.clone(), b.clone(), c.clone(), d.clone()]);
        let r2 = dedup(vec![d.clone(), c.clone(), b.clone(), a.clone()]);
        let r3 = dedup(vec![b.clone(), d.clone(), a.clone(), c.clone()]);

        // All three runs MUST agree on:
        //   - winners.len() = 2 (two unique PURLs)
        //   - winners ordered by canonical_purl lex (`pkg:generic/` < `pkg:vcpkg/`)
        //   - per group: same winner + same losers list
        for result in [&r1, &r2, &r3] {
            assert_eq!(result.winners.len(), 2);
            assert_eq!(result.winners[0].canonical_purl, "pkg:generic/foo@1.0");
            assert_eq!(
                result.winners[0].winning_source_mechanism,
                SourceMechanism::ConanRecipe
            );
            assert_eq!(
                result.winners[0].also_detected_via,
                vec![SourceMechanism::GitSubmodule]
            );
            assert_eq!(result.winners[1].canonical_purl, "pkg:vcpkg/bar@2.0");
            assert_eq!(
                result.winners[1].winning_source_mechanism,
                SourceMechanism::VcpkgManifest
            );
            assert_eq!(
                result.winners[1].also_detected_via,
                vec![SourceMechanism::VcpkgClassic]
            );
        }
    }

    // ----------------------------------------------------------------
    // SourceMechanism::canonical_str — exhaustive coverage
    // ----------------------------------------------------------------

    #[test]
    fn canonical_str_covers_all_13_variants() {
        // The parity catalog C55 row depends on this set of 13 strings;
        // any new variant added here MUST also be added to
        // docs/reference/sbom-format-mapping.md's C55 enum.
        let all = [
            SourceMechanism::CmakeFetchcontentGit,
            SourceMechanism::CmakeFetchcontentUrl,
            SourceMechanism::CmakeExternalproject,
            SourceMechanism::CmakeVendored,
            SourceMechanism::BazelHttpArchive,
            SourceMechanism::VcpkgManifest,
            SourceMechanism::ConanRecipe,
            SourceMechanism::CpmCmake,
            SourceMechanism::ZephyrWest,
            SourceMechanism::IdfComponent,
            SourceMechanism::IdfComponentLocal,
            SourceMechanism::VcpkgClassic,
            SourceMechanism::GitSubmodule,
        ];
        let mut strs: Vec<&str> = all.iter().map(|s| s.canonical_str()).collect();
        strs.sort();
        strs.dedup();
        assert_eq!(strs.len(), 13, "every variant MUST map to a unique string");
    }
}
