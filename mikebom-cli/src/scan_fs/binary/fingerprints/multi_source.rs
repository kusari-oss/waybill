//! Multi-source corpus orchestrator (milestone 110 Phase 5-Slim PR 2).
//!
//! Given a `&[CorpusSource]` materialized by [`crate::scan_fs::binary::
//! fingerprints::source_config::Sources::materialize`], this module:
//!   - Fetches each source's tarball (the milestone-108 default via
//!     the build-time-embedded SHA path; arbitrary sources via the
//!     new content-SHA path)
//!   - Loads v2 records from each per-source cache directory
//!   - Merges the results into a single `Vec<CorpusRecordV2>`
//!   - Records per-source success / failure status with the
//!     `contracts/cli-flags.md` SC-005 categorization
//!
//! Behavior matrix (PR-2 of the slim slice):
//!
//! | Source                  | Offline | Fetch path                | Loader path                          |
//! |-------------------------|---------|---------------------------|--------------------------------------|
//! | milestone-108 default   | false   | `fetch::fetch_corpus`     | `load_v2_records_from_cache`         |
//! | milestone-108 default   | true    | skip                      | `load_v2_records_from_cache` (cache) |
//! | arbitrary               | false   | `fetch_arbitrary_source`  | `load_v2_records_from_source_cache`  |
//! | arbitrary               | true    | skip + warn               | n/a                                  |
//!
//! What this orchestrator does NOT yet do (deferred to follow-on slices):
//!   - bearer-token auth per source (FR-007)
//!   - sigstore signature verification (FR-008)
//!   - 24-hour TTL cache freshness (FR-012a) — non-default sources are
//!     re-fetched every scan in Phase 5-Slim, matching the user-locked
//!     "defer TTL" scope
//!
//! PR-2 lands this code as DEAD CODE behind `#[allow(dead_code)]`;
//! PR-3 wires `load_corpus` to dispatch here when extras are
//! configured.

use super::cache;
use super::fetch;
use super::loader;
use super::record::CorpusRecordV2;
use super::source_config::{CorpusSource, CorpusSourceId};
use super::source_sha::CorpusSha;

/// Outcome of `fetch_and_load_multi_source`. Records carry the merged
/// v2 records from every successful source; `per_source` lets callers
/// surface diagnostics for the partial-failure case.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct MultiSourceLoadResult {
    pub records: Vec<CorpusRecordV2>,
    pub per_source: Vec<SourceLoadStatus>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct SourceLoadStatus {
    pub source_id: CorpusSourceId,
    pub url: String,
    pub outcome: SourceOutcome,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum SourceOutcome {
    /// Records loaded successfully — either from a fresh fetch or
    /// from a populated cache. `content_sha` identifies the cache
    /// directory the records came from.
    Loaded {
        content_sha: CorpusSha,
        record_count: usize,
    },
    /// Source skipped — offline mode + arbitrary source (cannot
    /// resolve content SHA without network).
    SkippedOffline,
    /// Source failed; categorized per `contracts/cli-flags.md` SC-005.
    Failed {
        category: SourceFailureCategory,
        message: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceFailureCategory {
    /// DNS / connect / timeout / 4xx other than 401-403 / 5xx after retries.
    NetworkUnreachable,
    /// Tarball decompression failed or no `corpus/` entries inside.
    ArchiveMalformed,
    /// Cache loader rejected the extracted index or records (post-fetch).
    LoaderRejected,
    // FR-007 deferred:    MissingCredential, InvalidCredential.
    // FR-008 deferred:    SignatureMismatch.
}

impl SourceFailureCategory {
    /// Human-readable token for SBOM annotations / log lines per
    /// `contracts/cli-flags.md` SC-005.
    #[allow(dead_code)]
    pub(crate) fn token(self) -> &'static str {
        match self {
            Self::NetworkUnreachable => "network-unreachable",
            Self::ArchiveMalformed => "archive-malformed",
            Self::LoaderRejected => "loader-rejected",
        }
    }
}

/// Fetch + load v2 records from each configured source. Returns
/// merged records + per-source diagnostics. Per-source failures are
/// logged via `tracing::warn!` AND captured in `per_source` so the
/// caller can decide whether to surface them in the SBOM.
///
/// The `sha_override` parameter is the milestone-108 US5 runtime SHA
/// override (`--fingerprints-rev`). It applies ONLY to the milestone-
/// 108 default source; arbitrary sources resolve their content SHA
/// from the fetched bytes.
#[allow(dead_code)]
pub(crate) fn fetch_and_load_multi_source(
    sources: &[CorpusSource],
    offline: bool,
    sha_override: Option<CorpusSha>,
) -> MultiSourceLoadResult {
    let mut records: Vec<CorpusRecordV2> = Vec::new();
    let mut per_source: Vec<SourceLoadStatus> = Vec::with_capacity(sources.len());
    for source in sources {
        let outcome = if is_milestone_108_default(source) {
            fetch_and_load_default(sha_override, offline)
        } else {
            fetch_and_load_arbitrary(source, offline)
        };
        match &outcome {
            SourceOutcome::Loaded { content_sha, record_count } => {
                tracing::info!(
                    source_url = %source.url,
                    source_id = %source.source_id,
                    content_sha = %content_sha.to_full_hex(),
                    record_count,
                    "fingerprint corpus source loaded",
                );
                // Load records and merge.
                let recs = if is_milestone_108_default(source) {
                    loader::load_v2_records_from_cache(content_sha)
                } else {
                    loader::load_v2_records_from_source_cache(
                        &source.source_id,
                        content_sha,
                    )
                };
                if let Ok(mut recs) = recs {
                    records.append(&mut recs);
                }
            }
            SourceOutcome::SkippedOffline => {
                tracing::warn!(
                    source_url = %source.url,
                    "fingerprint corpus source skipped (offline mode + no usable cache); \
                     other sources unaffected. Run without --offline to fetch this source.",
                );
            }
            SourceOutcome::Failed { category, message } => {
                tracing::warn!(
                    source_url = %source.url,
                    category = category.token(),
                    error = %message,
                    "fingerprint corpus source failed; skipping this source for this scan. \
                     Other sources unaffected.",
                );
            }
        }
        per_source.push(SourceLoadStatus {
            source_id: source.source_id.clone(),
            url: source.url.clone(),
            outcome,
        });
    }
    MultiSourceLoadResult { records, per_source }
}

fn is_milestone_108_default(source: &CorpusSource) -> bool {
    source.source_id.as_str() == CorpusSourceId::MILESTONE_108_DEFAULT
}

fn fetch_and_load_default(sha_override: Option<CorpusSha>, offline: bool) -> SourceOutcome {
    let sha = sha_override.unwrap_or_else(CorpusSha::build_time_embedded);
    if cache::cache_hit(&sha) {
        return load_default_records_or_fail(&sha);
    }
    if offline {
        return SourceOutcome::Failed {
            category: SourceFailureCategory::NetworkUnreachable,
            message: "offline mode set; cache empty for milestone-108 default".to_string(),
        };
    }
    match fetch::fetch_corpus(&sha) {
        Ok(()) => load_default_records_or_fail(&sha),
        Err(e) => SourceOutcome::Failed {
            category: categorize_fetch_error(&e),
            message: e.to_string(),
        },
    }
}

fn load_default_records_or_fail(sha: &CorpusSha) -> SourceOutcome {
    match loader::load_v2_records_from_cache(sha) {
        Ok(records) => SourceOutcome::Loaded {
            content_sha: *sha,
            record_count: records.len(),
        },
        Err(e) => SourceOutcome::Failed {
            category: SourceFailureCategory::LoaderRejected,
            message: e.to_string(),
        },
    }
}

fn fetch_and_load_arbitrary(source: &CorpusSource, offline: bool) -> SourceOutcome {
    if offline {
        return SourceOutcome::SkippedOffline;
    }
    match fetch::fetch_arbitrary_source(source) {
        Ok(outcome) => load_arbitrary_records_or_fail(source, &outcome.content_sha),
        Err(e) => SourceOutcome::Failed {
            category: categorize_fetch_error(&e),
            message: e.to_string(),
        },
    }
}

fn load_arbitrary_records_or_fail(
    source: &CorpusSource,
    content_sha: &CorpusSha,
) -> SourceOutcome {
    match loader::load_v2_records_from_source_cache(&source.source_id, content_sha) {
        Ok(records) => SourceOutcome::Loaded {
            content_sha: *content_sha,
            record_count: records.len(),
        },
        Err(e) => SourceOutcome::Failed {
            category: SourceFailureCategory::LoaderRejected,
            message: e.to_string(),
        },
    }
}

fn categorize_fetch_error(err: &fetch::FetchError) -> SourceFailureCategory {
    match err {
        fetch::FetchError::Network(_)
        | fetch::FetchError::NotFound { .. }
        | fetch::FetchError::HttpError { .. } => SourceFailureCategory::NetworkUnreachable,
        fetch::FetchError::Decompression(_)
        | fetch::FetchError::Extraction(_) => SourceFailureCategory::ArchiveMalformed,
        fetch::FetchError::Io(_) => SourceFailureCategory::LoaderRejected,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn source_failure_category_tokens_are_stable() {
        // The token strings are part of the operator-facing log
        // contract per SC-005; pin them here so they don't drift.
        assert_eq!(
            SourceFailureCategory::NetworkUnreachable.token(),
            "network-unreachable"
        );
        assert_eq!(
            SourceFailureCategory::ArchiveMalformed.token(),
            "archive-malformed"
        );
        assert_eq!(
            SourceFailureCategory::LoaderRejected.token(),
            "loader-rejected"
        );
    }

    #[test]
    fn is_milestone_108_default_matches_sentinel() {
        let default_src = CorpusSource::milestone_108_default();
        assert!(is_milestone_108_default(&default_src));
        let arbitrary =
            CorpusSource::unauthenticated("https://corpus.example/x.tar.gz".to_string());
        assert!(!is_milestone_108_default(&arbitrary));
    }

    #[test]
    fn categorize_fetch_error_maps_network_variants() {
        assert_eq!(
            categorize_fetch_error(&fetch::FetchError::Network("dns".to_string())),
            SourceFailureCategory::NetworkUnreachable
        );
        assert_eq!(
            categorize_fetch_error(&fetch::FetchError::NotFound { sha: "abc".to_string() }),
            SourceFailureCategory::NetworkUnreachable
        );
        assert_eq!(
            categorize_fetch_error(&fetch::FetchError::HttpError {
                status: 502,
                attempts: 3,
            }),
            SourceFailureCategory::NetworkUnreachable
        );
    }

    #[test]
    fn categorize_fetch_error_maps_archive_variants() {
        assert_eq!(
            categorize_fetch_error(&fetch::FetchError::Decompression("bad gz".to_string())),
            SourceFailureCategory::ArchiveMalformed
        );
        assert_eq!(
            categorize_fetch_error(&fetch::FetchError::Extraction("no entries".to_string())),
            SourceFailureCategory::ArchiveMalformed
        );
    }

    #[test]
    fn arbitrary_source_skipped_in_offline_mode() {
        let source =
            CorpusSource::unauthenticated("https://corpus.example/x.tar.gz".to_string());
        let outcome = fetch_and_load_arbitrary(&source, true);
        assert!(matches!(outcome, SourceOutcome::SkippedOffline));
    }

    // ============================================================
    // End-to-end orchestration test (Phase 5-Slim PR 2 SC-006-ish)
    // ============================================================
    //
    // Configures TWO arbitrary wiremock-backed sources, suppresses the
    // milestone-108 default (so the test stays hermetic), runs the
    // orchestrator, and asserts records from both sources appear in
    // the merged result. This is the most important PR-2 test — it
    // proves the merge logic actually merges.

    use std::io::Write;
    use tokio::runtime::Runtime;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::super::test_env_lock as env_lock;

    struct WiremockHarness {
        _rt: Runtime,
        _server: MockServer,
        base_url: String,
    }

    impl WiremockHarness {
        fn new(setup: impl FnOnce(&Runtime, &MockServer)) -> Self {
            let rt = Runtime::new().unwrap();
            let server = rt.block_on(async { MockServer::start().await });
            setup(&rt, &server);
            let base_url = server.uri();
            Self {
                _rt: rt,
                _server: server,
                base_url,
            }
        }
    }

    /// Build a v2 corpus tarball containing a single record named
    /// `<lib>` with one symbol-set indicator. Wrapper dir is generic
    /// so corpus_filename_from_tar_path's "strip the wrapper" logic
    /// applies.
    fn build_minimal_v2_tarball(lib: &str) -> Vec<u8> {
        let mut tar_bytes: Vec<u8> = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(
                &mut tar_bytes,
                flate2::Compression::default(),
            );
            let mut builder = tar::Builder::new(enc);
            let wrapper = "extras-v2/";
            let index = format!(
                r#"{{"version":1,"entries":[{{"library":"{lib}","path":"{lib}.json"}}]}}"#
            );
            let record = format!(
                r#"{{
                    "id":"{lib}-1.0",
                    "purl":"pkg:github/example/{lib}@1.0.0",
                    "version_range":"1.0",
                    "indicators":{{
                        "exported_symbols":{{
                            "type":"symbol-set",
                            "required":["{lib}_init","{lib}_run","{lib}_done"],
                            "min_match":2,
                            "confidence_baseline":0.70
                        }}
                    }},
                    "provenance":{{
                        "tier":"manual-curation",
                        "extracted_from":"https://example.com/{lib}",
                        "extracted_from_sha256":"0000000000000000000000000000000000000000000000000000000000000000",
                        "extraction_toolchain":"test-fixture",
                        "extracted_at":"2026-06-01T12:00:00Z"
                    }},
                    "schema_version":2
                }}"#
            );
            for (name, payload) in [
                ("index.json", index.as_bytes()),
                (&format!("{lib}.json"), record.as_bytes()),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_path(format!("{wrapper}corpus/{name}")).unwrap();
                header.set_size(payload.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append(&header, payload).unwrap();
            }
            let mut enc = builder.into_inner().unwrap();
            enc.flush().unwrap();
            enc.finish().unwrap();
        }
        tar_bytes
    }

    fn make_harness_serving(tarball: Vec<u8>) -> WiremockHarness {
        WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+\.tar\.gz"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_bytes(tarball.clone())
                            .insert_header("content-type", "application/x-gzip"),
                    )
                    .mount(server)
                    .await;
            });
        })
    }

    #[test]
    fn merges_records_across_two_arbitrary_sources() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }

        let harness_a = make_harness_serving(build_minimal_v2_tarball("liba"));
        let harness_b = make_harness_serving(build_minimal_v2_tarball("libb"));
        let source_a = CorpusSource::unauthenticated(format!(
            "{}/extras-a.tar.gz",
            harness_a.base_url
        ));
        let source_b = CorpusSource::unauthenticated(format!(
            "{}/extras-b.tar.gz",
            harness_b.base_url
        ));

        let result =
            fetch_and_load_multi_source(&[source_a, source_b], false, None);

        // Both sources contributed records.
        assert_eq!(result.records.len(), 2, "expected one record per source");
        let mut ids: Vec<String> = result.records.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["liba-1.0".to_string(), "libb-1.0".to_string()]);
        // Per-source status: both Loaded.
        assert_eq!(result.per_source.len(), 2);
        for s in &result.per_source {
            assert!(
                matches!(s.outcome, SourceOutcome::Loaded { record_count: 1, .. }),
                "expected Loaded with 1 record; got {:?}",
                s.outcome
            );
        }

        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn partial_failure_still_loads_succeeding_sources() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }

        // One good source, one source that 404s.
        let harness_good = make_harness_serving(build_minimal_v2_tarball("libgood"));
        let harness_bad = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+\.tar\.gz"))
                    .respond_with(ResponseTemplate::new(404))
                    .mount(server)
                    .await;
            });
        });
        let good = CorpusSource::unauthenticated(format!(
            "{}/extras-good.tar.gz",
            harness_good.base_url
        ));
        let bad = CorpusSource::unauthenticated(format!(
            "{}/extras-bad.tar.gz",
            harness_bad.base_url
        ));

        let result = fetch_and_load_multi_source(&[good, bad], false, None);

        // Good source still loaded; bad source recorded as Failed.
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].id, "libgood-1.0");
        assert!(matches!(
            result.per_source[0].outcome,
            SourceOutcome::Loaded { .. }
        ));
        match &result.per_source[1].outcome {
            SourceOutcome::Failed { category, .. } => {
                assert_eq!(*category, SourceFailureCategory::NetworkUnreachable);
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }
}
