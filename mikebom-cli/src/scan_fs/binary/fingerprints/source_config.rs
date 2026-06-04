//! Multi-source corpus configuration types (milestone 110).
//!
//! Phase 2: type definitions only. The fetch + cache + merge orchestration
//! that consumes these types lives in Phase 5 (US2). The milestone-108
//! `cache.rs` + `fetch.rs` modules continue to use their existing single-
//! source `CorpusSha`-keyed layout; they will be extended in Phase 5 to
//! accept a `CorpusSourceId` prefix per source.
//!
//! Per `contracts/cli-flags.md` + research R3 / R4 in
//! `specs/110-pluggable-corpus-v2/research.md`.

use data_encoding::BASE32_NOPAD;
use sha2::{Digest, Sha256};

/// Stable per-source identifier used as a cache-directory key.
///
/// Derived from the source URL by hashing 16 bytes of `sha256(url)` and
/// encoding the first 10 bytes as BASE32 (no padding) — 16-char
/// alphanumeric, safe for filesystem path segments on every platform.
/// Special-cased to the literal `"public-milestone-108"` for the
/// milestone-108 default source so operator-recognizable cache layouts
/// stay stable across default installs.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(dead_code)] // Phase 2: declared; Phase 5 (fetch) consumes.
pub(crate) struct CorpusSourceId(String);

#[allow(dead_code)]
impl CorpusSourceId {
    /// Sentinel for the milestone-108 default corpus URL. Operators see
    /// `~/.cache/mikebom/fingerprints/public-milestone-108/` (the
    /// human-recognizable name) rather than a BASE32 hash for the
    /// default install path.
    pub(crate) const MILESTONE_108_DEFAULT: &'static str = "public-milestone-108";

    /// Construct from a source URL. Returns the BASE32 hash for arbitrary
    /// URLs OR the `MILESTONE_108_DEFAULT` sentinel for the milestone-108
    /// default URL.
    pub(crate) fn from_url(url: &str) -> Self {
        if Self::is_milestone_108_default(url) {
            return Self(Self::MILESTONE_108_DEFAULT.to_string());
        }
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let digest = hasher.finalize();
        // First 10 bytes (80 bits) → 16-char BASE32 (no padding).
        let encoded = BASE32_NOPAD.encode(&digest[..10]);
        Self(encoded.to_ascii_lowercase())
    }

    /// Construct directly from a known opaque ID string (for tests or
    /// reading the existing `_meta/sources.json` index).
    pub(crate) fn from_raw(raw: String) -> Self {
        Self(raw)
    }

    /// Path-safe representation. Used as a cache subdir name.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn is_milestone_108_default(url: &str) -> bool {
        // Match the milestone-108 GitHub Pages URL + the underlying raw
        // archive URLs. Conservative: anything from the public
        // kusari-sandbox/mikebom-fingerprints repo counts.
        url.starts_with("https://kusari-sandbox.github.io/mikebom-fingerprints/")
            || url.contains("kusari-sandbox/mikebom-fingerprints")
    }
}

impl core::fmt::Display for CorpusSourceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A configured corpus source — URL + optional auth credential + sigstore
/// allowed-issuers list. Constructed from the layered config sources (CLI
/// flag, env var, config file) per `contracts/cli-flags.md` + research R4.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Phase 2: declared; Phase 5 (fetch) consumes.
pub(crate) struct CorpusSource {
    /// Source URL (HTTPS only enforced at fetch time per FR-008).
    pub url: String,
    /// Optional name of an environment variable holding the bearer token
    /// for this source. Per FR-007 + `contracts/cli-flags.md`: the
    /// credential VALUE never appears in argv or in this struct; only the
    /// env-var NAME is stored. mikebom reads `$<credential_env>` at
    /// fetch time.
    pub credential_env: Option<String>,
    /// Sigstore identity allowlist for this source's signed archives.
    /// Empty → defaults to the milestone-108 anchor per research R6.
    pub allowed_issuers: Vec<String>,
    /// Stable per-source cache-dir key.
    pub source_id: CorpusSourceId,
}

#[allow(dead_code)]
impl CorpusSource {
    /// Convenience constructor for tests + the milestone-108 default
    /// source registration.
    pub(crate) fn unauthenticated(url: String) -> Self {
        let source_id = CorpusSourceId::from_url(&url);
        Self {
            url,
            credential_env: None,
            allowed_issuers: Vec::new(),
            source_id,
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn milestone_108_default_url_gets_sentinel_id() {
        let id = CorpusSourceId::from_url(
            "https://kusari-sandbox.github.io/mikebom-fingerprints/release.json",
        );
        assert_eq!(id.as_str(), CorpusSourceId::MILESTONE_108_DEFAULT);
    }

    #[test]
    fn arbitrary_url_gets_base32_id() {
        let id = CorpusSourceId::from_url("https://corpus.example/private.tar.gz");
        // 16 chars BASE32 lowercase, no padding.
        assert_eq!(id.as_str().len(), 16);
        assert!(id.as_str().chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn identical_urls_produce_identical_ids() {
        let id1 = CorpusSourceId::from_url("https://corpus.example/a.tar.gz");
        let id2 = CorpusSourceId::from_url("https://corpus.example/a.tar.gz");
        assert_eq!(id1, id2);
    }

    #[test]
    fn different_urls_produce_different_ids() {
        let id1 = CorpusSourceId::from_url("https://corpus.example/a.tar.gz");
        let id2 = CorpusSourceId::from_url("https://corpus.example/b.tar.gz");
        assert_ne!(id1, id2);
    }

    #[test]
    fn unauthenticated_source_has_no_credential_env() {
        let s = CorpusSource::unauthenticated("https://example.com/x.tar.gz".to_string());
        assert!(s.credential_env.is_none());
        assert!(s.allowed_issuers.is_empty());
    }
}
