//! Multi-source corpus configuration types (milestone 110).
//!
//! Phase 2 introduced the type definitions; Phase 5-Slim (this slice)
//! adds the layered parser that materializes a `Vec<CorpusSource>`
//! from CLI flags, env vars, and the implicit milestone-108 default.
//! The fetch / cache / merge orchestration that consumes these
//! sources lives in the PR-2 and PR-3 slices.
//!
//! Per-source bearer-token auth (FR-007), per-source sigstore
//! allowed-issuers (FR-008), and the 24-hour TTL (FR-012a) are
//! deferred to a follow-on slice; this parser accepts the `=ENV_VAR`
//! suffix syntax from `contracts/cli-flags.md` but warns and discards
//! it (no auth header is attached at fetch time yet).
//!
//! Per `contracts/cli-flags.md` + research R3 / R4 in
//! `specs/110-pluggable-corpus-v2/research.md`.

use data_encoding::BASE32_NOPAD;
use sha2::{Digest, Sha256};

/// Env-var name (alias for repeated `--fingerprints-source`). Comma-
/// separated list of URLs, optionally each suffixed with `=ENV_VAR`
/// (the suffix is reserved for FR-007 and currently warns + strips).
pub(crate) const SOURCES_ENV: &str = "WAYBILL_FINGERPRINTS_SOURCES";

/// Env-var name for `--fingerprints-source-no-default`. Boolean (`1`,
/// `true`).
pub(crate) const NO_DEFAULT_ENV: &str = "WAYBILL_FINGERPRINTS_NO_DEFAULT";

/// Milestone-108 public default corpus URL — the implicit source loaded
/// when `--fingerprints-corpus` is on and `--fingerprints-source-no-
/// default` is NOT set. This URL also drives the
/// `CorpusSourceId::MILESTONE_108_DEFAULT` sentinel.
pub(crate) const MILESTONE_108_DEFAULT_URL: &str =
    "https://github.com/kusari-sandbox/waybill-fingerprints";

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
    /// `~/.cache/waybill/fingerprints/public-milestone-108/` (the
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
        // kusari-sandbox/waybill-fingerprints repo counts.
        url.starts_with("https://kusari-sandbox.github.io/waybill-fingerprints/")
            || url.contains("kusari-sandbox/waybill-fingerprints")
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
    /// env-var NAME is stored. waybill reads `$<credential_env>` at
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

    /// Construct the implicit milestone-108 default source.
    pub(crate) fn milestone_108_default() -> Self {
        Self::unauthenticated(MILESTONE_108_DEFAULT_URL.to_string())
    }

    /// Parse a single CLI-flag / env-var value (`URL` or `URL=ENV_VAR`).
    ///
    /// The optional `=ENV_VAR` suffix is reserved for FR-007 (deferred
    /// in Phase 5-Slim). When present, this parser warns once via
    /// `tracing::warn!` and discards the binding — the URL portion
    /// proceeds without auth. Operators get a clear "this knob does
    /// not work yet" signal without their config silently dropping
    /// sources.
    ///
    /// Returns `None` only when the trimmed input is empty (so layered
    /// callers can quietly skip blank entries from
    /// `WAYBILL_FINGERPRINTS_SOURCES=,,foo,` without warning noise).
    pub(crate) fn parse_flag_value(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        // Split on the LAST `=` and check if the right side looks like
        // an env-var name (`[A-Z_][A-Z0-9_]*`). If yes, treat as the
        // FR-007 auth-binding syntax and strip + warn. If no, the `=`
        // belongs to the URL (e.g. query-string).
        let (url_part, _credential_env) = match raw.rsplit_once('=') {
            Some((url, tail)) if looks_like_env_var_name(tail) => {
                tracing::warn!(
                    source = url,
                    credential_env = tail,
                    "per-source bearer-token auth (FR-007) is not yet supported; \
                     stripping credential binding and proceeding without auth. \
                     The URL itself will still be fetched unauthenticated.",
                );
                (url, Some(tail.to_string()))
            }
            _ => (raw, None),
        };
        let url = url_part.trim().to_string();
        if url.is_empty() {
            return None;
        }
        Some(Self::unauthenticated(url))
    }
}

/// True when `s` matches the POSIX env-var-name shape `[A-Z_][A-Z0-9_]*`
/// — used to disambiguate the `URL=ENV_VAR` suffix from a URL that
/// happens to contain `=` (e.g. `?key=value`).
fn looks_like_env_var_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().expect("non-empty checked above");
    if !(first.is_ascii_uppercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Layered source configuration produced by `Sources::from_env`. Holds
/// the operator's explicit additional sources plus the include-default
/// flag; the materialization step (`Sources::materialize`) unions them
/// with the milestone-108 default at consumption time.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // PR-1 declares; PR-2/PR-3 consume.
pub(crate) struct Sources {
    /// Sources declared via CLI flag / env var. Empty in the default
    /// OSS-out-of-the-box configuration.
    pub extras: Vec<CorpusSource>,
    /// True when the operator passed `--fingerprints-source-no-default`
    /// (or set `WAYBILL_FINGERPRINTS_NO_DEFAULT=1`). Suppresses the
    /// implicit milestone-108 default.
    pub no_default: bool,
}

#[allow(dead_code)]
impl Sources {
    /// Build from the process env. Reads `WAYBILL_FINGERPRINTS_SOURCES`
    /// (comma-separated `URL[=ENV_VAR]` entries) and the boolean
    /// `WAYBILL_FINGERPRINTS_NO_DEFAULT`.
    pub fn from_env() -> Self {
        let extras = match std::env::var(SOURCES_ENV) {
            Ok(raw) => parse_sources_list(&raw),
            Err(_) => Vec::new(),
        };
        let no_default = bool_env(NO_DEFAULT_ENV);
        Self { extras, no_default }
    }

    /// Materialize the union of explicit sources + the implicit
    /// milestone-108 default (unless suppressed). The default is
    /// PREPENDED so source ordering visible in logs / diagnostics
    /// matches operator intuition (default first, then extras in
    /// flag-declaration order). Duplicate URLs collapse via
    /// `CorpusSourceId` equality so an operator who passes the
    /// milestone-108 URL explicitly as a `--fingerprints-source`
    /// doesn't get it fetched twice.
    pub fn materialize(&self) -> Vec<CorpusSource> {
        let mut out: Vec<CorpusSource> = Vec::with_capacity(self.extras.len() + 1);
        if !self.no_default {
            out.push(CorpusSource::milestone_108_default());
        }
        for src in &self.extras {
            if out.iter().any(|s| s.source_id == src.source_id) {
                continue;
            }
            out.push(src.clone());
        }
        out
    }
}

fn parse_sources_list(raw: &str) -> Vec<CorpusSource> {
    raw.split(',')
        .filter_map(CorpusSource::parse_flag_value)
        .collect()
}

fn bool_env(name: &str) -> bool {
    match std::env::var_os(name) {
        Some(v) => v == "1" || v == "true",
        None => false,
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn milestone_108_default_url_gets_sentinel_id() {
        let id = CorpusSourceId::from_url(
            "https://kusari-sandbox.github.io/waybill-fingerprints/release.json",
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

    // ============================================================
    // Phase 5-Slim parser tests (PR 1)
    // ============================================================

    #[test]
    fn parse_flag_value_accepts_bare_url() {
        let s = CorpusSource::parse_flag_value("https://corpus.example/a.tar.gz").unwrap();
        assert_eq!(s.url, "https://corpus.example/a.tar.gz");
        assert!(s.credential_env.is_none());
    }

    #[test]
    fn parse_flag_value_trims_whitespace() {
        let s = CorpusSource::parse_flag_value("  https://corpus.example/a.tar.gz  ").unwrap();
        assert_eq!(s.url, "https://corpus.example/a.tar.gz");
    }

    #[test]
    fn parse_flag_value_empty_returns_none() {
        assert!(CorpusSource::parse_flag_value("").is_none());
        assert!(CorpusSource::parse_flag_value("   ").is_none());
    }

    #[test]
    fn parse_flag_value_strips_env_var_suffix_with_warning() {
        // FR-007 deferred: the `=ENV_VAR` suffix is accepted but the
        // credential binding is dropped (the URL portion still parses).
        let s = CorpusSource::parse_flag_value(
            "https://corpus.example/private.tar.gz=KUSARI_CORPUS_TOKEN",
        )
        .unwrap();
        assert_eq!(s.url, "https://corpus.example/private.tar.gz");
        // PR-2/3 will start populating this; PR-1 leaves it None to
        // signal "auth not yet wired".
        assert!(s.credential_env.is_none());
    }

    #[test]
    fn parse_flag_value_treats_query_string_equals_as_url_part() {
        // A URL like `?key=value` must NOT be misparsed as the
        // auth-binding syntax — `value` is not a valid env-var name
        // (lowercase). The full URL is preserved.
        let s = CorpusSource::parse_flag_value(
            "https://corpus.example/release.json?key=value",
        )
        .unwrap();
        assert_eq!(s.url, "https://corpus.example/release.json?key=value");
    }

    #[test]
    fn looks_like_env_var_name_matches_posix_shape() {
        assert!(looks_like_env_var_name("KUSARI_CORPUS_TOKEN"));
        assert!(looks_like_env_var_name("_HIDDEN"));
        assert!(looks_like_env_var_name("VAR1"));
        assert!(!looks_like_env_var_name("kusari_corpus_token")); // lowercase
        assert!(!looks_like_env_var_name("1VAR"));               // leading digit
        assert!(!looks_like_env_var_name(""));
        assert!(!looks_like_env_var_name("with-dash"));
    }

    #[test]
    fn parse_sources_list_handles_comma_separated_input() {
        let out = parse_sources_list(
            "https://corpus.example/a.tar.gz,https://other.example/b.tar.gz",
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://corpus.example/a.tar.gz");
        assert_eq!(out[1].url, "https://other.example/b.tar.gz");
    }

    #[test]
    fn parse_sources_list_skips_blank_entries() {
        let out = parse_sources_list(",,https://corpus.example/a.tar.gz,,");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn parse_sources_list_with_mixed_auth_and_bare_entries() {
        // Auth-suffix entries warn-and-strip; bare entries pass through.
        // Both survive the parse.
        let out = parse_sources_list(
            "https://corpus.example/private.tar.gz=KUSARI_TOKEN,https://other.example/public.tar.gz",
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://corpus.example/private.tar.gz");
        assert_eq!(out[1].url, "https://other.example/public.tar.gz");
    }

    // Sources-layer tests use the shared test_env_lock (see
    // fingerprints/mod.rs::test_env_lock) so they don't race with
    // cache/loader env-mutating tests.
    use super::super::test_env_lock as env_lock;

    fn with_env<R>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> R) -> R {
        let _g = env_lock();
        let saved: Vec<(String, Option<std::ffi::OsString>)> = vars
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var_os(k)))
            .collect();
        unsafe {
            for (k, v) in vars {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
        let out = f();
        unsafe {
            for (k, prev) in &saved {
                match prev {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
        out
    }

    #[test]
    fn sources_from_env_empty_when_no_env_set() {
        let s = with_env(
            &[(SOURCES_ENV, None), (NO_DEFAULT_ENV, None)],
            Sources::from_env,
        );
        assert!(s.extras.is_empty());
        assert!(!s.no_default);
    }

    #[test]
    fn sources_from_env_parses_multi_url_list() {
        let s = with_env(
            &[(
                SOURCES_ENV,
                Some("https://a.example/x.tar.gz,https://b.example/y.tar.gz"),
            ), (NO_DEFAULT_ENV, None)],
            Sources::from_env,
        );
        assert_eq!(s.extras.len(), 2);
    }

    #[test]
    fn sources_from_env_picks_up_no_default_flag() {
        let s = with_env(
            &[(SOURCES_ENV, None), (NO_DEFAULT_ENV, Some("1"))],
            Sources::from_env,
        );
        assert!(s.no_default);
    }

    #[test]
    fn sources_materialize_prepends_default_when_not_suppressed() {
        let extras = vec![
            CorpusSource::unauthenticated("https://corpus.example/a.tar.gz".to_string()),
        ];
        let s = Sources {
            extras,
            no_default: false,
        };
        let out = s.materialize();
        assert_eq!(out.len(), 2);
        // Default sentinel comes first.
        assert_eq!(
            out[0].source_id.as_str(),
            CorpusSourceId::MILESTONE_108_DEFAULT
        );
        assert_eq!(out[1].url, "https://corpus.example/a.tar.gz");
    }

    #[test]
    fn sources_materialize_suppresses_default_when_no_default_set() {
        let extras = vec![
            CorpusSource::unauthenticated("https://corpus.example/a.tar.gz".to_string()),
        ];
        let s = Sources {
            extras,
            no_default: true,
        };
        let out = s.materialize();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].url, "https://corpus.example/a.tar.gz");
    }

    #[test]
    fn sources_materialize_dedups_explicit_default_url() {
        // Operator passes the milestone-108 URL explicitly alongside
        // the implicit default — must collapse to ONE entry, not two.
        let extras = vec![
            CorpusSource::unauthenticated(
                "https://github.com/kusari-sandbox/waybill-fingerprints/archive/abc.tar.gz"
                    .to_string(),
            ),
        ];
        let s = Sources {
            extras,
            no_default: false,
        };
        let out = s.materialize();
        assert_eq!(out.len(), 1, "milestone-108 URL must dedupe with implicit default");
        assert_eq!(
            out[0].source_id.as_str(),
            CorpusSourceId::MILESTONE_108_DEFAULT
        );
    }

    #[test]
    fn sources_materialize_default_only_when_no_extras() {
        let s = Sources::default();
        let out = s.materialize();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].source_id.as_str(),
            CorpusSourceId::MILESTONE_108_DEFAULT
        );
    }
}
