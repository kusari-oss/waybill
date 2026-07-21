//! v2 matcher (milestone 110, Phase 4 Slice A).
//!
//! Multi-indicator confidence fusion + per-record matching. Phase 4 Slice
//! A scope: matcher infrastructure + per-indicator matchers + fusion
//! algorithm + `match_binary` entry point. Unit-tested via synthetic
//! `BinaryArtifact` inputs.
//!
//! Out of scope for Slice A (deferred to Slice B):
//! - Wiring the matcher into the production scan path (still uses the
//!   milestone-108 matcher in `symbol_fingerprint::scan`).
//! - Annotation emission (CDX-native `evidence.identity[]` + the
//!   parity-bridging SPDX annotations).
//! - Collision handling — multi-record cross-references (FR-014, US4).
//! - Self-identity suppression — the `SelfIdentity::matches_record` stub
//!   still returns `false` (Phase 6).
//!
//! Slice B will wire the matcher in production AND extend the existing
//! milestone-108 emission path to use `MatchResult` for the new v2
//! records. Until Slice B ships, the matcher operates purely on
//! synthetic test inputs — production scans continue through the
//! milestone-108 pipeline.

use waybill_common::types::purl::Purl;

use super::confidence::{Confidence, FusedConfidence};
use super::record::{CorpusRecordV2, IndicatorKind, IndicatorSpec};
use super::self_identity::SelfIdentity;
use super::source_config::CorpusSourceId;

/// The extracted-indicator inputs a binary contributes to the matcher.
///
/// Slice A introduces this as a matcher-internal synthesis struct so unit
/// tests can construct it without running the full file-walking +
/// extractor pipeline. Slice B will populate it from the existing
/// per-format extractors (milestones 023 / 024 / 026 / 028 / 099 / 305 /
/// 309) at the production scan-call site.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // Phase 2/4: declared; Phase 4 Slice B wires production.
pub(crate) struct BinaryArtifact {
    /// ELF `.dynsym` / Mach-O `LC_SYMTAB` externals / PE
    /// `IMAGE_EXPORT_DIRECTORY` — symbol names the binary exports.
    pub exported_symbols: Vec<String>,
    /// `.rodata` string literals (cf. milestone 026 extractor).
    pub rodata_strings: Vec<String>,
    /// ELF `.note.gnu.build-id`, lowercase hex (milestone 023).
    pub build_id: Option<String>,
    /// Mach-O `LC_UUID`, lowercase hex (milestone 024).
    pub macho_uuid: Option<String>,
    /// PE PDB GUID:age (milestone 028).
    pub pe_pdb: Option<String>,
}

/// One identified library claim emitted by the matcher.
///
/// Per `contracts/matcher-api.md` + data-model.md. Multiple results from
/// a single binary scan indicate a collision case (FR-014) — Slice B
/// will populate `also_detected_via` with the cross-references; Slice A
/// always emits one result per matched record with an empty
/// `also_detected_via`.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Slice A: populated by match_binary; Slice B consumes.
pub(crate) struct MatchResult {
    /// Canonical PURL of the matched record.
    pub purl: Purl,
    /// Alias PURLs from the record (cross-ecosystem identifiers).
    pub purl_aliases: Vec<Purl>,
    /// CPE candidates from the record.
    pub cpe_candidates: Vec<String>,
    /// Post-fusion confidence bucket — `High` or `Medium` per FR-017.
    pub confidence: FusedConfidence,
    /// Numeric fused-confidence value (the source-of-truth from which
    /// the bucket was derived). Used by Slice B's annotation emission to
    /// populate `waybill:fingerprint-confidence` losslessly.
    pub confidence_score: Confidence,
    /// Which indicators actually matched, for SBOM-emission annotations.
    pub indicators_matched: Vec<IndicatorKind>,
    /// SemVer range string from the record (or `"unknown"`).
    pub version_range: String,
    /// Stable record identifier for provenance chain back to the corpus.
    pub record_id: String,
    /// Which configured source contributed this record.
    pub source_id: CorpusSourceId,
    /// Other records whose indicators matched this binary (collision).
    /// Slice A leaves this empty; Slice B (US4) populates from
    /// cross-record correlation.
    pub also_detected_via: Vec<Purl>,
}

/// Match a single record's indicators against a binary artifact.
///
/// Returns `None` when no indicators match OR when the fused confidence
/// falls below the `Medium` floor (0.70) per the 2026-06-03 Q1
/// clarification. Returns `Some` with the fused bucket + the numeric
/// score + the list of contributing indicator kinds otherwise.
#[allow(dead_code)] // Slice A: called by match_binary.
fn fuse_indicators(
    record: &CorpusRecordV2,
    binary: &BinaryArtifact,
    self_identity: Option<&SelfIdentity>,
) -> Option<(FusedConfidence, Confidence, Vec<IndicatorKind>)> {
    let suppressed = self_identity.is_some_and(|id| id.matches_record(record));
    let mut matching: Vec<(IndicatorKind, Confidence)> = Vec::new();

    for (kind, spec) in &record.indicators {
        // Per-indicator self-suppression — when self-identity matches AND
        // this indicator opted in via `suppress_when_self_identity_matches`,
        // skip the indicator entirely (Phase 6 wires the real resolver
        // ladder; Slice A still gets the type-level skip via the stub).
        if suppressed && spec_suppresses_on_self(spec) {
            continue;
        }
        if let Some(confidence) = match_indicator(spec, binary) {
            matching.push((*kind, confidence));
        }
    }

    if matching.is_empty() {
        return None;
    }

    let fused_score = fuse_confidence(&matching)?;
    let bucket = FusedConfidence::from_fused(fused_score)?;
    // Deterministic emission order: matched indicators sorted by Ord.
    let mut indicators_matched: Vec<IndicatorKind> =
        matching.into_iter().map(|(k, _)| k).collect();
    indicators_matched.sort();
    Some((bucket, fused_score, indicators_matched))
}

/// "Max + bump" confidence fusion per research R2 + design doc §7.
///
/// `confidence = max(per-indicator confidence) over all matching indicators`
/// `for each AGREEING additional indicator: confidence = min(0.99, +0.05)`
///
/// Returns `None` only when `matching` is empty (callers gate on this).
/// The bucketing-to-Medium-floor check lives in `FusedConfidence::from_fused`.
#[allow(dead_code)] // Slice A: called by fuse_indicators.
fn fuse_confidence(matching: &[(IndicatorKind, Confidence)]) -> Option<Confidence> {
    if matching.is_empty() {
        return None;
    }
    let mut fused = matching
        .iter()
        .map(|(_, c)| c.into_inner())
        .fold(f64::MIN, f64::max);
    // Apply +0.05 per AGREEING additional indicator (i.e., for each
    // matching indicator beyond the first contributor to the max).
    let additional = matching.len().saturating_sub(1);
    for _ in 0..additional {
        fused = (fused + 0.05).min(0.99);
    }
    // Fused is in [0.0, 0.99] since we capped at 0.99 and started from
    // a Confidence value in [0.0, 1.0]. try_from cannot fail here, but
    // we use try_from rather than an unchecked constructor to keep the
    // boundary type-safe per constitution principle IV.
    Confidence::try_from(fused).ok()
}

/// Dispatch a per-indicator match check based on the indicator spec's
/// variant. Returns `Some(baseline_confidence)` when the indicator
/// matches; `None` otherwise.
#[allow(dead_code)] // Slice A.
fn match_indicator(spec: &IndicatorSpec, binary: &BinaryArtifact) -> Option<Confidence> {
    match spec {
        IndicatorSpec::SymbolSet {
            required,
            min_match,
            confidence_baseline,
            ..
        } => match_symbol_set(required, *min_match, *confidence_baseline, binary),
        IndicatorSpec::RodataLiteral {
            patterns,
            confidence_baseline,
            ..
        } => match_rodata_literal(patterns, *confidence_baseline, binary),
        IndicatorSpec::ExactHash {
            sha_or_uuid_set,
            confidence_baseline,
            ..
        } => match_exact_hash(sha_or_uuid_set, *confidence_baseline, binary),
    }
}

/// Does this indicator opt in to self-identity suppression?
#[allow(dead_code)]
fn spec_suppresses_on_self(spec: &IndicatorSpec) -> bool {
    match spec {
        IndicatorSpec::SymbolSet {
            suppress_when_self_identity_matches,
            ..
        }
        | IndicatorSpec::RodataLiteral {
            suppress_when_self_identity_matches,
            ..
        }
        | IndicatorSpec::ExactHash {
            suppress_when_self_identity_matches,
            ..
        } => *suppress_when_self_identity_matches,
    }
}

/// Count how many of `required` are present in `binary.exported_symbols`.
/// Match iff count >= `min_match`. Per-indicator matcher for
/// `IndicatorSpec::SymbolSet`.
#[allow(dead_code)]
fn match_symbol_set(
    required: &[String],
    min_match: usize,
    baseline: Confidence,
    binary: &BinaryArtifact,
) -> Option<Confidence> {
    // Use a HashSet of the binary's exported symbols for O(N+M) lookup
    // rather than O(N*M) — N=record-required-count (~10), M=binary-
    // exported-count (potentially thousands).
    let exported: std::collections::HashSet<&str> = binary
        .exported_symbols
        .iter()
        .map(|s| s.as_str())
        .collect();
    let matched = required.iter().filter(|s| exported.contains(s.as_str())).count();
    if matched >= min_match {
        Some(baseline)
    } else {
        None
    }
}

/// Substring search across `binary.rodata_strings` for any of `patterns`.
/// Match iff any pattern is found in any rodata string. Per-indicator
/// matcher for `IndicatorSpec::RodataLiteral`.
#[allow(dead_code)]
fn match_rodata_literal(
    patterns: &[String],
    baseline: Confidence,
    binary: &BinaryArtifact,
) -> Option<Confidence> {
    for s in &binary.rodata_strings {
        for p in patterns {
            if s.contains(p.as_str()) {
                return Some(baseline);
            }
        }
    }
    None
}

/// Lower-case hex equality check against the binary's Build-ID /
/// LC_UUID / PE PDB GUID. The indicator's variant pairs with the
/// appropriate `BinaryArtifact` field based on the value-space —
/// Slice A treats all three as equivalent string-equality checks
/// against the union of the three binary fields, which is sufficient
/// for the v2 records' use case (each record-author picks ONE field
/// type per indicator). Slice B may refine to per-field dispatch if
/// needed.
#[allow(dead_code)]
fn match_exact_hash(
    sha_or_uuid_set: &[String],
    baseline: Confidence,
    binary: &BinaryArtifact,
) -> Option<Confidence> {
    let candidates = [
        binary.build_id.as_deref(),
        binary.macho_uuid.as_deref(),
        binary.pe_pdb.as_deref(),
    ];
    for c in candidates.iter().flatten() {
        for target in sha_or_uuid_set {
            if c.eq_ignore_ascii_case(target.as_str()) {
                return Some(baseline);
            }
        }
    }
    None
}

/// Match a binary against the loaded corpus, returning zero or more
/// `MatchResult`s.
///
/// Slice A: single-record success path with deterministic emission
/// ordering. Collision handling (multi-record cross-references via
/// `also_detected_via`) lands in Slice B / Phase 6 (US4).
///
/// Determinism: identical (binary, records, self_identity) inputs
/// produce the same ordered `Vec<MatchResult>`. Order is
/// `(confidence DESC, primary_purl ASC)` per the matcher-api contract.
#[allow(dead_code)] // Slice B wires this into the production scan path.
pub(crate) fn match_binary(
    binary: &BinaryArtifact,
    records: &[CorpusRecordV2],
    self_identity: Option<&SelfIdentity>,
    source_id: &CorpusSourceId,
) -> Vec<MatchResult> {
    let mut results: Vec<MatchResult> = Vec::new();

    for record in records {
        let Some((bucket, score, indicators_matched)) =
            fuse_indicators(record, binary, self_identity)
        else {
            continue;
        };
        results.push(MatchResult {
            purl: record.purl.clone(),
            purl_aliases: record.purl_aliases.clone(),
            cpe_candidates: record.cpe_candidates.clone(),
            confidence: bucket,
            confidence_score: score,
            indicators_matched,
            version_range: record.version_range.clone(),
            record_id: record.id.clone(),
            source_id: source_id.clone(),
            also_detected_via: Vec::new(),
        });
    }

    // Deterministic order: confidence DESC, then primary_purl ASC.
    results.sort_by(|a, b| {
        // FusedConfidence::High < Medium in enum order; we want High first.
        let a_bucket_rank = match a.confidence {
            FusedConfidence::High => 0,
            FusedConfidence::Medium => 1,
        };
        let b_bucket_rank = match b.confidence {
            FusedConfidence::High => 0,
            FusedConfidence::Medium => 1,
        };
        a_bucket_rank
            .cmp(&b_bucket_rank)
            .then_with(|| {
                // Within a bucket, higher numeric score first.
                b.confidence_score
                    .into_inner()
                    .partial_cmp(&a.confidence_score.into_inner())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.purl.as_str().cmp(b.purl.as_str()))
    });

    results
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn v2_record(json: &str) -> CorpusRecordV2 {
        serde_json::from_str(json).unwrap()
    }

    fn openssl_v2_record() -> CorpusRecordV2 {
        v2_record(
            r#"{
              "id": "openssl-3.1.4-glibc-amd64",
              "purl": "pkg:github/openssl/openssl@openssl-3.1.4",
              "purl_aliases": ["pkg:deb/debian/libssl3@3.1.4-1"],
              "version_range": ">=3.1.4,<3.2.0",
              "indicators": {
                "exported_symbols": {
                  "type": "symbol-set",
                  "required": ["SSL_CTX_new", "SSL_new", "SSL_free", "OPENSSL_init_ssl", "OPENSSL_init_crypto"],
                  "min_match": 3,
                  "confidence_baseline": 0.70
                },
                "version_string": {
                  "type": "rodata-literal",
                  "patterns": ["OpenSSL 3.1.4"],
                  "confidence_baseline": 0.95
                },
                "build_id": {
                  "type": "exact-hash",
                  "sha_or_uuid_set": ["abc123def456000000000000000000000000ab12"],
                  "confidence_baseline": 0.99
                }
              },
              "provenance": {
                "tier": "automated-ingestion",
                "extracted_from": "https://example.com/libssl3.deb",
                "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "extraction_toolchain": "test",
                "extracted_at": "2026-06-01T12:00:00Z"
              },
              "schema_version": 2
            }"#,
        )
    }

    fn empty_artifact() -> BinaryArtifact {
        BinaryArtifact::default()
    }

    fn test_source_id() -> CorpusSourceId {
        CorpusSourceId::from_url("https://example.com/test.tar.gz")
    }

    // =================================================================
    // Per-indicator matchers
    // =================================================================

    #[test]
    fn symbol_set_matches_above_min_threshold() {
        let baseline = Confidence::try_from(0.70).unwrap();
        let artifact = BinaryArtifact {
            exported_symbols: vec![
                "SSL_CTX_new".into(),
                "SSL_new".into(),
                "SSL_free".into(),
                "random_other".into(),
            ],
            ..empty_artifact()
        };
        let required = vec![
            "SSL_CTX_new".into(),
            "SSL_new".into(),
            "SSL_free".into(),
            "OPENSSL_init_ssl".into(),
            "OPENSSL_init_crypto".into(),
        ];
        // 3 of 5 matched; min_match = 3 → matches.
        assert_eq!(match_symbol_set(&required, 3, baseline, &artifact), Some(baseline));
    }

    #[test]
    fn symbol_set_below_min_threshold_returns_none() {
        let baseline = Confidence::try_from(0.70).unwrap();
        let artifact = BinaryArtifact {
            exported_symbols: vec!["SSL_CTX_new".into(), "SSL_new".into()],
            ..empty_artifact()
        };
        let required = vec![
            "SSL_CTX_new".into(),
            "SSL_new".into(),
            "SSL_free".into(),
            "OPENSSL_init_ssl".into(),
            "OPENSSL_init_crypto".into(),
        ];
        // 2 of 5 matched; min_match = 3 → no match.
        assert_eq!(match_symbol_set(&required, 3, baseline, &artifact), None);
    }

    #[test]
    fn rodata_literal_matches_on_substring() {
        let baseline = Confidence::try_from(0.95).unwrap();
        let artifact = BinaryArtifact {
            rodata_strings: vec![
                "Server: nginx".into(),
                "OpenSSL 3.1.4 19 Oct 2023".into(),
                "random other string".into(),
            ],
            ..empty_artifact()
        };
        let patterns = vec!["OpenSSL 3.1.4".into()];
        assert_eq!(match_rodata_literal(&patterns, baseline, &artifact), Some(baseline));
    }

    #[test]
    fn rodata_literal_no_match_returns_none() {
        let baseline = Confidence::try_from(0.95).unwrap();
        let artifact = BinaryArtifact {
            rodata_strings: vec!["BoringSSL".into()],
            ..empty_artifact()
        };
        let patterns = vec!["OpenSSL 3.1.4".into()];
        assert_eq!(match_rodata_literal(&patterns, baseline, &artifact), None);
    }

    #[test]
    fn exact_hash_matches_build_id_case_insensitive() {
        let baseline = Confidence::try_from(0.99).unwrap();
        let artifact = BinaryArtifact {
            build_id: Some("ABC123DEF456000000000000000000000000AB12".to_string()),
            ..empty_artifact()
        };
        let targets = vec!["abc123def456000000000000000000000000ab12".to_string()];
        assert_eq!(match_exact_hash(&targets, baseline, &artifact), Some(baseline));
    }

    #[test]
    fn exact_hash_no_match_returns_none() {
        let baseline = Confidence::try_from(0.99).unwrap();
        let artifact = BinaryArtifact {
            build_id: Some("differentvalue".into()),
            ..empty_artifact()
        };
        let targets = vec!["abc123def456".to_string()];
        assert_eq!(match_exact_hash(&targets, baseline, &artifact), None);
    }

    // =================================================================
    // Fusion algorithm
    // =================================================================

    #[test]
    fn fuse_confidence_with_single_indicator_returns_max() {
        let inputs = vec![(IndicatorKind::ExportedSymbols, Confidence::try_from(0.70).unwrap())];
        let result = fuse_confidence(&inputs).unwrap();
        assert!((result.into_inner() - 0.70).abs() < 1e-9);
    }

    #[test]
    fn fuse_confidence_with_two_indicators_applies_one_bump() {
        // Max = 0.70, then one +0.05 bump = 0.75.
        let inputs = vec![
            (IndicatorKind::ExportedSymbols, Confidence::try_from(0.70).unwrap()),
            (IndicatorKind::VersionString, Confidence::try_from(0.70).unwrap()),
        ];
        let result = fuse_confidence(&inputs).unwrap();
        assert!(
            (result.into_inner() - 0.75).abs() < 1e-9,
            "expected 0.75, got {}",
            result.into_inner()
        );
    }

    #[test]
    fn fuse_confidence_takes_max_baseline_then_bumps() {
        // Max = 0.95 (from version_string), then +0.05 for the second
        // indicator = 1.00, capped at 0.99.
        let inputs = vec![
            (IndicatorKind::ExportedSymbols, Confidence::try_from(0.70).unwrap()),
            (IndicatorKind::VersionString, Confidence::try_from(0.95).unwrap()),
        ];
        let result = fuse_confidence(&inputs).unwrap();
        assert!(
            (result.into_inner() - 0.99).abs() < 1e-9,
            "expected cap at 0.99, got {}",
            result.into_inner()
        );
    }

    #[test]
    fn fuse_confidence_with_three_indicators_caps_at_99() {
        // Max = 0.95, +0.05 twice = 1.05, capped at 0.99.
        let inputs = vec![
            (IndicatorKind::ExportedSymbols, Confidence::try_from(0.70).unwrap()),
            (IndicatorKind::VersionString, Confidence::try_from(0.95).unwrap()),
            (IndicatorKind::BuildId, Confidence::try_from(0.99).unwrap()),
        ];
        let result = fuse_confidence(&inputs).unwrap();
        assert!((result.into_inner() - 0.99).abs() < 1e-9);
    }

    #[test]
    fn fuse_confidence_empty_returns_none() {
        assert!(fuse_confidence(&[]).is_none());
    }

    // =================================================================
    // fuse_indicators (single-record driver)
    // =================================================================

    #[test]
    fn fuse_indicators_returns_none_when_no_indicators_match() {
        let record = openssl_v2_record();
        let artifact = empty_artifact();
        assert!(fuse_indicators(&record, &artifact, None).is_none());
    }

    #[test]
    fn fuse_indicators_returns_medium_for_single_symbol_match() {
        // 3 of 5 OpenSSL symbols → baseline 0.70 → Medium bucket.
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            exported_symbols: vec![
                "SSL_CTX_new".into(),
                "SSL_new".into(),
                "SSL_free".into(),
            ],
            ..empty_artifact()
        };
        let (bucket, score, indicators) =
            fuse_indicators(&record, &artifact, None).unwrap();
        assert_eq!(bucket, FusedConfidence::Medium);
        assert!((score.into_inner() - 0.70).abs() < 1e-9);
        assert_eq!(indicators, vec![IndicatorKind::ExportedSymbols]);
    }

    #[test]
    fn fuse_indicators_returns_high_when_symbols_plus_version_string_agree() {
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            exported_symbols: vec![
                "SSL_CTX_new".into(),
                "SSL_new".into(),
                "SSL_free".into(),
            ],
            rodata_strings: vec!["OpenSSL 3.1.4".into()],
            ..empty_artifact()
        };
        let (bucket, score, indicators) =
            fuse_indicators(&record, &artifact, None).unwrap();
        assert_eq!(bucket, FusedConfidence::High);
        // Max(0.70, 0.95) = 0.95, +0.05 for the second indicator capped at 0.99.
        assert!(
            (score.into_inner() - 0.99).abs() < 1e-9,
            "expected 0.99, got {}",
            score.into_inner()
        );
        // Sorted by IndicatorKind Ord.
        assert!(indicators.contains(&IndicatorKind::ExportedSymbols));
        assert!(indicators.contains(&IndicatorKind::VersionString));
    }

    #[test]
    fn fuse_indicators_returns_high_when_build_id_alone_matches() {
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            build_id: Some("abc123def456000000000000000000000000ab12".into()),
            ..empty_artifact()
        };
        let (bucket, score, indicators) =
            fuse_indicators(&record, &artifact, None).unwrap();
        // Build-ID baseline is 0.99 → High bucket on its own.
        assert_eq!(bucket, FusedConfidence::High);
        assert!((score.into_inner() - 0.99).abs() < 1e-9);
        assert_eq!(indicators, vec![IndicatorKind::BuildId]);
    }

    // =================================================================
    // match_binary (multi-record driver + emission ordering)
    // =================================================================

    #[test]
    fn match_binary_empty_corpus_returns_no_results() {
        let artifact = empty_artifact();
        let source = test_source_id();
        let results = match_binary(&artifact, &[], None, &source);
        assert!(results.is_empty());
    }

    #[test]
    fn match_binary_with_one_matching_record_returns_one_result() {
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            exported_symbols: vec![
                "SSL_CTX_new".into(),
                "SSL_new".into(),
                "SSL_free".into(),
            ],
            rodata_strings: vec!["OpenSSL 3.1.4".into()],
            ..empty_artifact()
        };
        let source = test_source_id();
        let results = match_binary(&artifact, &[record], None, &source);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.purl.as_str(), "pkg:github/openssl/openssl@openssl-3.1.4");
        assert_eq!(r.confidence, FusedConfidence::High);
        assert_eq!(r.record_id, "openssl-3.1.4-glibc-amd64");
        assert_eq!(r.also_detected_via.len(), 0, "Slice A leaves cross-refs empty");
    }

    #[test]
    fn match_binary_orders_high_confidence_before_medium() {
        // Construct two records that both match the same artifact; one
        // at High confidence (symbols + version string), one at Medium
        // (symbols only). High should come first per the matcher-api
        // determinism contract.
        let high_record = openssl_v2_record();
        let medium_only = v2_record(
            r#"{
              "id": "fakelib-1.0",
              "purl": "pkg:generic/fakelib@1.0",
              "version_range": "1.0",
              "indicators": {
                "exported_symbols": {
                  "type": "symbol-set",
                  "required": ["SSL_CTX_new", "SSL_new"],
                  "min_match": 2,
                  "confidence_baseline": 0.70
                }
              },
              "provenance": {
                "tier": "manual-curation",
                "extracted_from": "test",
                "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "extraction_toolchain": "test",
                "extracted_at": "2026-06-01T12:00:00Z"
              },
              "schema_version": 2
            }"#,
        );
        let artifact = BinaryArtifact {
            exported_symbols: vec![
                "SSL_CTX_new".into(),
                "SSL_new".into(),
                "SSL_free".into(),
            ],
            rodata_strings: vec!["OpenSSL 3.1.4".into()],
            ..empty_artifact()
        };
        let source = test_source_id();
        let results = match_binary(&artifact, &[medium_only, high_record], None, &source);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].confidence, FusedConfidence::High);
        assert_eq!(results[1].confidence, FusedConfidence::Medium);
    }

    #[test]
    fn match_binary_below_floor_record_does_not_emit() {
        // A record whose only matching indicator falls below 0.70 (here:
        // we use a baseline of 0.40 deliberately) → no emission.
        let weak_record = v2_record(
            r#"{
              "id": "weak-lib",
              "purl": "pkg:generic/weak",
              "version_range": "unknown",
              "indicators": {
                "version_string": {
                  "type": "rodata-literal",
                  "patterns": ["WeakLib"],
                  "confidence_baseline": 0.40
                }
              },
              "provenance": {
                "tier": "manual-curation",
                "extracted_from": "test",
                "extracted_from_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "extraction_toolchain": "test",
                "extracted_at": "2026-06-01T12:00:00Z"
              },
              "schema_version": 2
            }"#,
        );
        let artifact = BinaryArtifact {
            rodata_strings: vec!["WeakLib v0.1".into()],
            ..empty_artifact()
        };
        let source = test_source_id();
        let results = match_binary(&artifact, &[weak_record], None, &source);
        assert!(
            results.is_empty(),
            "below-medium floor MUST suppress emission per FR-017 + the 2026-06-03 Q1 clarification"
        );
    }

    #[test]
    fn match_binary_is_deterministic_across_runs() {
        // Same inputs → same output ordering across invocations.
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            exported_symbols: vec!["SSL_CTX_new".into(), "SSL_new".into(), "SSL_free".into()],
            rodata_strings: vec!["OpenSSL 3.1.4".into()],
            ..empty_artifact()
        };
        let source = test_source_id();
        let r1 = match_binary(&artifact, std::slice::from_ref(&record), None, &source);
        let r2 = match_binary(&artifact, std::slice::from_ref(&record), None, &source);
        assert_eq!(r1.len(), r2.len());
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a.purl.as_str(), b.purl.as_str());
            assert_eq!(a.confidence, b.confidence);
            assert_eq!(a.indicators_matched, b.indicators_matched);
        }
    }

    #[test]
    fn stub_self_identity_does_not_suppress_in_slice_a() {
        // Phase 6 wires the real self-identity resolver. Slice A: the
        // SelfIdentity stub returns false for every record, so no
        // suppression fires even if the operator passes self_identity.
        let record = openssl_v2_record();
        let artifact = BinaryArtifact {
            exported_symbols: vec!["SSL_CTX_new".into(), "SSL_new".into(), "SSL_free".into()],
            ..empty_artifact()
        };
        let source = test_source_id();
        let identity = SelfIdentity {
            bare_name: Some("openssl".to_string()),
            purl: None,
        };
        // Without self-identity: matches.
        let no_id_results =
            match_binary(&artifact, std::slice::from_ref(&record), None, &source);
        assert_eq!(no_id_results.len(), 1);
        // With self-identity (Phase 6 will suppress): Slice A stub still
        // returns false → still matches.
        let with_id_results =
            match_binary(&artifact, std::slice::from_ref(&record), Some(&identity), &source);
        assert_eq!(
            with_id_results.len(),
            1,
            "Slice A stub returns false unconditionally; Phase 6 will replace"
        );
    }
}
