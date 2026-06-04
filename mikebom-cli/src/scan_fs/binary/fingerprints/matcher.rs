//! v2 matcher entry point (milestone 110).
//!
//! Phase 2: stub. The full multi-indicator fusion logic + collision
//! handling land in Phase 4 (T026–T031, US1) + Phase 6 (T055–T056, US4).
//! For Phase 2, `match_binary()` returns an empty Vec so the type
//! compiles + the existing milestone-108 matcher path remains
//! authoritative for fingerprint matching. The v1-upgrade shim in
//! Phase 3 (T017–T019, US3) populates the matcher with v1-mapped
//! records but still falls through to the milestone-108 pipeline for
//! actual symbol matching until Phase 4 lands.

use mikebom_common::types::purl::Purl;

use super::confidence::FusedConfidence;
use super::record::{CorpusRecordV2, IndicatorKind};
use super::self_identity::SelfIdentity;
use super::source_config::CorpusSourceId;

/// One identified library claim emitted by the matcher.
///
/// Per `contracts/matcher-api.md` + data-model.md. Multiple results from
/// a single binary scan indicate a collision case (FR-014) — each carries
/// `also_detected_via` cross-referencing the other PURLs.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Phase 2: declared; Phase 4 (US1) populates.
pub(crate) struct MatchResult {
    /// Canonical PURL of the matched record.
    pub purl: Purl,
    /// Alias PURLs from the record (cross-ecosystem identifiers).
    pub purl_aliases: Vec<Purl>,
    /// CPE candidates from the record.
    pub cpe_candidates: Vec<String>,
    /// Post-fusion confidence bucket — `High` or `Medium` per FR-017.
    pub confidence: FusedConfidence,
    /// Which indicators actually matched, for SBOM-emission annotations.
    pub indicators_matched: Vec<IndicatorKind>,
    /// SemVer range string from the record (or `"unknown"`).
    pub version_range: String,
    /// Stable record identifier for provenance chain back to the corpus.
    pub record_id: String,
    /// Which configured source contributed this record.
    pub source_id: CorpusSourceId,
    /// Other records whose indicators matched this binary (collision).
    pub also_detected_via: Vec<Purl>,
}

/// Match a binary against the loaded corpus, returning zero or more
/// `MatchResult`s.
///
/// Phase 2 stub: returns an empty Vec. Phase 4 (T031) implements the full
/// fusion + collision logic. Phase 3's v1-regression path does not call
/// this function — milestone-108's existing matcher (in
/// `symbol_fingerprint::scan`) continues to handle v1 records via the
/// upgrade-shim path until Phase 4 wires this matcher into the production
/// scan path (T035).
#[allow(dead_code)] // Phase 2: stub; Phase 4 (T031) implements.
pub(crate) fn match_binary(
    _binary_artifact: &(),
    _records: &[CorpusRecordV2],
    _self_identity: Option<&SelfIdentity>,
    _source_id: &CorpusSourceId,
) -> Vec<MatchResult> {
    Vec::new()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn stub_match_binary_returns_empty() {
        // Phase 2 stub: deterministically empty. Phase 4 replaces.
        let source_id = CorpusSourceId::from_url("https://example.com/test.tar.gz");
        let results = match_binary(&(), &[], None, &source_id);
        assert!(results.is_empty());
    }
}
