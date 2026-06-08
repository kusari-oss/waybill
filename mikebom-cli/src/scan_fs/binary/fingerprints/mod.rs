//! External symbol-fingerprint corpus subsystem (milestone 108).
//!
//! This module provides:
//! - A typed corpus loader (`load_corpus`) that returns either a
//!   cached external corpus OR the bundled in-source fallback,
//!   depending on operator opt-in and cache state.
//! - A typed `FingerprintRecord` shape that both the bundled and
//!   external paths produce, so the matcher in
//!   `super::symbol_fingerprint::scan` consumes a unified slice
//!   regardless of source.
//! - A `CorpusSource` enum tracking provenance for the
//!   `mikebom:fingerprint-corpus-sha` SBOM annotation (FR-005).
//!
//! Phase 2B/2C scope: types + loader + bundled-fallback path. Phase 4
//! adds the network-fetch path (`fetch.rs`); Phase 4 also wires
//! `--fingerprints-corpus` and stamps the annotation. Until then,
//! `load_corpus(LoadOptions::default())` returns the bundled corpus
//! and `symbol_fingerprint::scan` calls into it without behavioral
//! change.
//!
//! See `specs/108-fingerprint-corpus/`.

pub(crate) mod cache;
pub(crate) mod confidence;
pub(crate) mod fetch;
pub(crate) mod loader;
pub(crate) mod matcher;
pub(crate) mod multi_source;
pub(crate) mod record;
pub(crate) mod self_identity;
pub(crate) mod source_config;
pub(crate) mod source_sha;
pub(crate) mod v2_bridge;

use std::sync::OnceLock;

pub(crate) use record::FingerprintRecord;
pub(crate) use source_sha::CorpusSha;

use loader::LoaderError;
use source_config::Sources;

/// Provenance tag for the corpus that produced a match. Surfaces as
/// the value of the `mikebom:fingerprint-corpus-sha` SBOM annotation
/// (FR-005): the 12-hex SHA for cached/fetched paths, the literal
/// `"bundled"` for the fallback path.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum CorpusSource {
    /// In-source bundled fallback (the seeded 7-library corpus from
    /// milestone 099, frozen at milestone-108 ship time).
    Bundled,
    /// External corpus loaded from a populated cache (no network fetch
    /// was needed during this scan).
    Cached { sha: CorpusSha },
    /// External corpus loaded after a successful cache-miss fetch.
    Fetched { sha: CorpusSha },
}

#[allow(dead_code)]
impl CorpusSource {
    /// SBOM annotation value per FR-005. 12-hex truncation for
    /// `Cached`/`Fetched`; literal `"bundled"` for `Bundled`.
    pub fn annotation_value(&self) -> String {
        match self {
            CorpusSource::Bundled => "bundled".to_string(),
            CorpusSource::Cached { sha } | CorpusSource::Fetched { sha } => sha.to_short_hex(),
        }
    }
}

/// Container the matcher consumes. Holds the validated records + the
/// source tag for downstream annotation emission.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct FingerprintCorpus {
    pub records: Vec<FingerprintRecord>,
    pub source: CorpusSource,
}

/// Options accepted by `load_corpus`. `external_enabled` controls the
/// opt-in (FR-001 / SC-003: when false, bundled fallback only — no
/// cache access, no annotation stamping). `offline` short-circuits the
/// network fetch on a cache miss. `sha_override` (milestone 108 US5)
/// lets operators pin a specific corpus SHA at runtime — the cache
/// lookup, fetch URL, AND SBOM annotation all reflect the override.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct LoadOptions {
    /// True when the operator passed `--fingerprints-corpus` (or set
    /// `MIKEBOM_FINGERPRINTS_CORPUS=1`). When false, the bundled
    /// fallback is returned unconditionally — no cache access.
    pub external_enabled: bool,
    /// True when the operator passed `--offline` (the existing global
    /// flag). On a cache miss with `offline = true`, mikebom skips the
    /// network fetch entirely and falls back to bundled defaults with
    /// a single `tracing::warn!`.
    pub offline: bool,
    /// Runtime-override SHA per milestone-108 US5 (`--fingerprints-rev`
    /// or `MIKEBOM_FINGERPRINTS_REV=<SHA>`). When `Some(sha)`, mikebom
    /// uses this SHA instead of the build-time-embedded one for both
    /// cache lookup and any cache-miss fetch. The SBOM annotation
    /// reflects the override.
    pub sha_override: Option<CorpusSha>,
    /// Milestone 110 Phase 5-Slim — multi-source configuration
    /// materialized from `MIKEBOM_FINGERPRINTS_SOURCES` and
    /// `MIKEBOM_FINGERPRINTS_NO_DEFAULT`. PR-1 of the Phase 5-Slim
    /// slice parses this into the struct but does not yet fetch
    /// extras — those land in PR-2. `load_corpus` logs the parsed
    /// extras so operators can confirm their configuration is being
    /// picked up.
    pub sources: Sources,
}

impl LoadOptions {
    /// Build the options from the process env. Drives the milestone-108
    /// env-var bridge that `scan_cmd::execute` populates from the CLI
    /// flag (same pattern as `MIKEBOM_INCLUDE_VENDORED`) — keeps the
    /// caller from threading params through `scan_path`'s 75-callsite
    /// chain. Malformed `MIKEBOM_FINGERPRINTS_REV` values are caught
    /// at clap parse time; reaching `from_env()` with a bad value
    /// implies an external caller (e.g., a test or a downstream
    /// embedder) set the env directly — we emit a warn and discard
    /// the override rather than panicking mid-scan.
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        let sha_override =
            std::env::var("MIKEBOM_FINGERPRINTS_REV").ok().and_then(|s| {
                CorpusSha::from_hex(&s)
                    .map_err(|e| {
                        tracing::warn!(
                            value = %s,
                            error = %e,
                            "MIKEBOM_FINGERPRINTS_REV is malformed; ignoring (using build-time-embedded SHA instead)",
                        );
                    })
                    .ok()
            });
        Self {
            external_enabled: env_flag("MIKEBOM_FINGERPRINTS_CORPUS"),
            offline: env_flag("MIKEBOM_OFFLINE"),
            sha_override,
            sources: Sources::from_env(),
        }
    }
}

fn env_flag(name: &str) -> bool {
    match std::env::var_os(name) {
        Some(v) => v == "1" || v == "true",
        None => false,
    }
}

/// PR-1 informational log: list the parsed source configuration so
/// operators can confirm CLI-flag / env-var propagation is correct.
/// Emits at most one line per scan invocation. Behavior contract:
/// extras logged here are NOT yet fetched — that wiring lands in PR-2.
fn log_configured_sources(sources: &Sources) {
    if sources.extras.is_empty() && !sources.no_default {
        // OSS default — nothing to surface.
        return;
    }
    let extra_urls: Vec<&str> = sources.extras.iter().map(|s| s.url.as_str()).collect();
    tracing::info!(
        no_default = sources.no_default,
        extra_source_count = sources.extras.len(),
        extra_sources = ?extra_urls,
        "fingerprint corpus: multi-source configuration parsed; \
         Phase 5-Slim PR-1 logs only — extras will be fetched starting in PR-2",
    );
}

/// Return the bundled in-source 7-library corpus. Memoized via
/// `OnceLock` so the 7 owned-string allocations happen exactly once
/// per process.
#[allow(dead_code)]
pub(crate) fn load_bundled() -> &'static FingerprintCorpus {
    static BUNDLED: OnceLock<FingerprintCorpus> = OnceLock::new();
    BUNDLED.get_or_init(|| FingerprintCorpus {
        records: super::symbol_fingerprint::bundled_records(),
        source: CorpusSource::Bundled,
    })
}

/// Resolve the active corpus for this scan per FR-004.
///
/// Decision tree:
/// - `!opts.external_enabled` → bundled fallback. No cache access, no
///   network. Preserves SC-003 byte-identity for non-opt-in operators.
/// - `external_enabled` + cache hit → load from cache, tag `Cached`.
/// - `external_enabled` + cache miss + `!offline` → attempt fetch.
///   - fetch ok → load from now-populated cache, tag `Fetched`.
///   - fetch fail → `tracing::warn!` and return bundled (tag stays
///     `Bundled`; SBOM annotation will surface the `bundled` sentinel
///     so consumers can tell the opt-in operator fell back).
/// - `external_enabled` + cache miss + `offline` → `tracing::warn!`
///   and return bundled.
///
/// The build-time-embedded SHA from `tests/fingerprints.rev` drives
/// both the cache key and (in a Phase-4 follow-on) the fetch URL.
/// Phase 7 (US5) will accept a runtime `sha_override` for hermetic
/// builds; until then the build-time SHA is authoritative.
#[allow(dead_code)]
pub(crate) fn load_corpus(opts: LoadOptions) -> FingerprintCorpus {
    if !opts.external_enabled {
        return load_bundled().clone();
    }
    // Milestone 110 Phase 5-Slim (PR-1): surface the parsed multi-
    // source configuration via a single info log so operators can
    // confirm `--fingerprints-source` and `MIKEBOM_FINGERPRINTS_SOURCES`
    // are being picked up. The extras are NOT yet fetched in PR-1;
    // PR-2 wires the per-source fetch + cache layout and PR-3
    // dispatches to the multi-source orchestrator. The implicit
    // milestone-108 default continues to drive the single-source
    // load below.
    log_configured_sources(&opts.sources);
    // Milestone 108 US5: runtime override (when present) wins over
    // the build-time-embedded SHA. The override flows through to
    // BOTH the cache key AND the fetch URL, so the SBOM annotation
    // and any disk artifacts reflect the operator's pin.
    let sha = opts
        .sha_override
        .unwrap_or_else(CorpusSha::build_time_embedded);
    resolve_external_or_fallback(&sha, opts.offline)
}

fn resolve_external_or_fallback(sha: &CorpusSha, offline: bool) -> FingerprintCorpus {
    if cache::cache_hit(sha) {
        return load_cached_or_fallback(sha, CorpusSource::Cached { sha: *sha });
    }
    if offline {
        tracing::warn!(
            sha = %sha.to_full_hex(),
            "external corpus requested but cache is empty and --offline is set; falling back to bundled defaults",
        );
        return load_bundled().clone();
    }
    match fetch::fetch_corpus(sha) {
        Ok(()) => load_cached_or_fallback(sha, CorpusSource::Fetched { sha: *sha }),
        Err(e) => {
            tracing::warn!(
                sha = %sha.to_full_hex(),
                error = %e,
                "fingerprint corpus fetch failed; falling back to bundled defaults",
            );
            load_bundled().clone()
        }
    }
}

fn load_cached_or_fallback(
    sha: &CorpusSha,
    on_success_source: CorpusSource,
) -> FingerprintCorpus {
    match loader::load_corpus_from_cache(sha) {
        Ok(records) => FingerprintCorpus {
            records,
            source: on_success_source,
        },
        Err(LoaderError::CacheNotFound { .. }) => {
            // Stale `cache_hit` result (rare; only happens if another
            // process deleted the cache between the hit check and the
            // load). Fall back to bundled rather than recurse.
            tracing::warn!(
                sha = %sha.to_full_hex(),
                "cache disappeared between hit check and load; falling back to bundled",
            );
            load_bundled().clone()
        }
        Err(LoaderError::CacheCorrupt { reason, .. }) => {
            tracing::warn!(
                sha = %sha.to_full_hex(),
                reason,
                "fingerprint corpus cache is corrupt; falling back to bundled (consider `mikebom fingerprints cache-clear`)",
            );
            load_bundled().clone()
        }
    }
}

/// Process-wide mutex for tests that mutate the
/// `MIKEBOM_FINGERPRINTS_CACHE_DIR` env var. cargo runs tests in
/// parallel by default; without a shared lock, `cache::tests` and
/// `loader::tests` race for the same env var. Shared here (not
/// per-module) so any test in either module serializes against
/// the others.
#[cfg(test)]
pub(super) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn load_corpus_returns_bundled_when_external_disabled() {
        let corpus = load_corpus(LoadOptions {
            external_enabled: false,
            offline: false,
            sha_override: None,
            sources: Sources::default(),
        });
        assert!(matches!(corpus.source, CorpusSource::Bundled));
        assert_eq!(corpus.source.annotation_value(), "bundled");
        // Bundled corpus carries the seeded 7 libraries.
        assert_eq!(corpus.records.len(), 7);
    }

    #[test]
    fn load_corpus_falls_back_to_bundled_when_offline_and_cache_miss() {
        let _g = test_env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let corpus = load_corpus(LoadOptions {
            external_enabled: true,
            offline: true,
            sha_override: None,
            sources: Sources::default(),
        });
        assert!(matches!(corpus.source, CorpusSource::Bundled));
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn load_corpus_honors_sha_override_for_cache_lookup() {
        // Milestone 108 US5 — verify the override SHA is what drives
        // the cache-key lookup AND the annotation, not the build-time-
        // embedded SHA.
        let _g = test_env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        let override_sha =
            CorpusSha::from_hex("0123456789abcdef0123456789abcdef01234567").unwrap();
        // Pre-populate the cache at the OVERRIDE SHA (not the embedded
        // one). The embedded SHA's cache directory does not exist.
        let corpus_dir = tmp.path().join(override_sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        std::fs::write(
            corpus_dir.join("index.json"),
            r#"{"version":1,"entries":[{"library":"libfoo","path":"libfoo.json"}]}"#,
        )
        .unwrap();
        std::fs::write(
            corpus_dir.join("libfoo.json"),
            r#"{"library":"libfoo","target_purl":"pkg:generic/libfoo","symbols":["a","b","c","d","e","f","g","h"],"min_symbols":5}"#,
        )
        .unwrap();
        let corpus = load_corpus(LoadOptions {
            external_enabled: true,
            offline: true, // No network — proves the cache-lookup path used the override.
            sha_override: Some(override_sha),
            sources: Sources::default(),
        });
        // The annotation reflects the OVERRIDE — not the embedded SHA.
        assert!(matches!(corpus.source, CorpusSource::Cached { .. }));
        assert_eq!(corpus.source.annotation_value(), override_sha.to_short_hex());
        // And the records come from the override SHA's directory, not
        // the bundled fallback.
        assert_eq!(corpus.records.len(), 1);
        assert_eq!(corpus.records[0].library, "libfoo");
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }

    #[test]
    fn load_corpus_returns_cached_when_cache_hit() {
        let _g = test_env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let path_str = tmp.path().to_string_lossy().into_owned();
        unsafe {
            std::env::set_var("MIKEBOM_FINGERPRINTS_CACHE_DIR", &path_str);
        }
        // Pre-populate the cache at the build-time-embedded SHA with a
        // single-library fixture so the cache-hit path triggers.
        let sha = CorpusSha::build_time_embedded();
        let corpus_dir = tmp.path().join(sha.to_full_hex()).join("corpus");
        std::fs::create_dir_all(&corpus_dir).unwrap();
        std::fs::write(
            corpus_dir.join("index.json"),
            r#"{"version":1,"entries":[{"library":"libfoo","path":"libfoo.json"}]}"#,
        )
        .unwrap();
        std::fs::write(
            corpus_dir.join("libfoo.json"),
            r#"{"library":"libfoo","target_purl":"pkg:generic/libfoo","symbols":["a","b","c","d","e","f","g","h"],"min_symbols":5}"#,
        )
        .unwrap();
        let corpus = load_corpus(LoadOptions {
            external_enabled: true,
            offline: true, // Doesn't matter — cache hit short-circuits.
            sha_override: None,
            sources: Sources::default(),
        });
        assert!(matches!(corpus.source, CorpusSource::Cached { .. }));
        assert_eq!(corpus.records.len(), 1);
        assert_eq!(corpus.records[0].library, "libfoo");
        // Annotation value is the 12-hex of the build-time-embedded SHA.
        assert_eq!(corpus.source.annotation_value(), sha.to_short_hex());
        assert_eq!(corpus.source.annotation_value().len(), 12);
        unsafe {
            std::env::remove_var("MIKEBOM_FINGERPRINTS_CACHE_DIR");
        }
    }
}
