//! Network fetch path for the external fingerprint corpus (FR-004,
//! FR-008). Downloads the GitHub archive tarball at a pinned SHA,
//! decompresses, and extracts the `corpus/` subtree into the per-host
//! cache via the atomic-write protocol from `cache-layout.md`.
//!
//! See `specs/108-fingerprint-corpus/contracts/fetch-protocol.md`.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use thiserror::Error;

use super::cache;
use super::source_sha::CorpusSha;

const PRODUCTION_BASE_URL: &str = "https://github.com";
const REPO_PATH: &str = "kusari-sandbox/mikebom-fingerprints";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 5;
const MAX_RETRY_ATTEMPTS: u32 = 3;
const RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub(crate) enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP 404: corpus SHA {sha} not found in {REPO_PATH}")]
    NotFound { sha: String },
    #[error("HTTP {status}: corpus fetch failed after {attempts} retries")]
    HttpError { status: u16, attempts: u32 },
    #[error("response body decompression failed: {0}")]
    Decompression(String),
    #[error("tar archive extraction failed: {0}")]
    Extraction(String),
    #[error("disk write failed: {0}")]
    Io(String),
}

/// Fetch the corpus at `sha` into the default cache. Production entry
/// point — uses `https://github.com` as the base URL and
/// `cache::cache_root()` as the cache destination.
#[allow(dead_code)]
pub(crate) fn fetch_corpus(sha: &CorpusSha) -> Result<(), FetchError> {
    let cache_root = cache::cache_root();
    fetch_corpus_to(sha, PRODUCTION_BASE_URL, &cache_root)
}

/// Internal helper that production + tests share. `base_url` lets tests
/// point the fetcher at a wiremock-style local server; `cache_root`
/// lets tests redirect writes into a tempdir.
///
/// The blocking HTTP call is run on a fresh OS thread so we can be
/// safely invoked from inside a tokio runtime (mikebom's `#[tokio::main]`
/// CLI entry point) — calling `reqwest::blocking::Client` from an
/// async context otherwise panics on `Runtime::drop`. Same posture as
/// `golang::graph_resolver` (`std::thread::spawn` workers around its
/// blocking proxy fetches at `graph_resolver.rs:774`).
#[allow(dead_code)]
pub(crate) fn fetch_corpus_to(
    sha: &CorpusSha,
    base_url: &str,
    cache_root: &Path,
) -> Result<(), FetchError> {
    let sha = *sha;
    let base_url = base_url.to_string();
    let cache_root = cache_root.to_path_buf();
    std::thread::scope(|s| {
        s.spawn(move || fetch_corpus_blocking(&sha, &base_url, &cache_root))
            .join()
            .map_err(|_| FetchError::Network("fetch thread panicked".to_string()))?
    })
}

fn fetch_corpus_blocking(
    sha: &CorpusSha,
    base_url: &str,
    cache_root: &Path,
) -> Result<(), FetchError> {
    let url = format!(
        "{base_url}/{REPO_PATH}/archive/{}.tar.gz",
        sha.to_full_hex()
    );
    tracing::info!(url = %url, "fetching fingerprint corpus");

    let client = build_client()?;
    let response = perform_get_with_retry(&client, &url, sha)?;
    let body = response
        .bytes()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    extract_to_cache(sha, &body, cache_root)
}

fn build_client() -> Result<reqwest::blocking::Client, FetchError> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .user_agent(concat!(
            "mikebom/",
            env!("CARGO_PKG_VERSION"),
            " (corpus-fetch)"
        ))
        .build()
        .map_err(|e| FetchError::Network(e.to_string()))
}

/// Perform a GET with the retry policy described in
/// `contracts/fetch-protocol.md`: retry 3x on 5xx with exponential
/// backoff (1s, 2s, 4s); respect `Retry-After` on 429 up to 60s.
/// 404 returns `NotFound` immediately (no retry — the SHA is wrong).
/// Network errors get retried like 5xx.
fn perform_get_with_retry(
    client: &reqwest::blocking::Client,
    url: &str,
    sha: &CorpusSha,
) -> Result<reqwest::blocking::Response, FetchError> {
    let mut last_status: Option<u16> = None;
    for attempt in 0..MAX_RETRY_ATTEMPTS {
        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(response);
                }
                if status.as_u16() == 404 {
                    return Err(FetchError::NotFound {
                        sha: sha.to_full_hex(),
                    });
                }
                if status.as_u16() == 429 {
                    let retry_after = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|secs| Duration::from_secs(secs).min(RETRY_AFTER_CAP))
                        .unwrap_or_else(|| backoff_for(attempt));
                    tracing::warn!(
                        attempt = attempt + 1,
                        sleep_secs = retry_after.as_secs(),
                        "corpus fetch rate-limited; sleeping per Retry-After",
                    );
                    std::thread::sleep(retry_after);
                    last_status = Some(429);
                    continue;
                }
                if status.is_server_error() {
                    let sleep = backoff_for(attempt);
                    tracing::warn!(
                        attempt = attempt + 1,
                        status = status.as_u16(),
                        sleep_secs = sleep.as_secs(),
                        "corpus fetch server error; retrying",
                    );
                    std::thread::sleep(sleep);
                    last_status = Some(status.as_u16());
                    continue;
                }
                // Other 4xx → no retry, fail with HttpError.
                return Err(FetchError::HttpError {
                    status: status.as_u16(),
                    attempts: attempt + 1,
                });
            }
            Err(e) => {
                // DNS, connect, or transport-level failure — retry.
                let sleep = backoff_for(attempt);
                tracing::warn!(
                    attempt = attempt + 1,
                    error = %e,
                    sleep_secs = sleep.as_secs(),
                    "corpus fetch network error; retrying",
                );
                if attempt + 1 == MAX_RETRY_ATTEMPTS {
                    return Err(FetchError::Network(e.to_string()));
                }
                std::thread::sleep(sleep);
            }
        }
    }
    Err(FetchError::HttpError {
        status: last_status.unwrap_or(0),
        attempts: MAX_RETRY_ATTEMPTS,
    })
}

fn backoff_for(attempt: u32) -> Duration {
    // 1s, 2s, 4s (capped).
    Duration::from_secs(1u64 << attempt.min(6))
}

/// Decompress the tarball + extract `*/corpus/*.json` entries into
/// `<cache_root>/.tmp-<uuid>/corpus/`, then atomic-rename to
/// `<cache_root>/<full-sha>/`. On any error, the staging dir is
/// removed and the cache destination is never touched.
fn extract_to_cache(
    sha: &CorpusSha,
    body: &[u8],
    cache_root: &Path,
) -> Result<(), FetchError> {
    std::fs::create_dir_all(cache_root).map_err(|e| FetchError::Io(e.to_string()))?;

    let staging = cache_root.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    let staging_corpus = staging.join("corpus");
    std::fs::create_dir_all(&staging_corpus)
        .map_err(|e| FetchError::Io(e.to_string()))?;

    let result = extract_corpus_into(body, &staging_corpus);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let dest = cache_root.join(sha.to_full_hex());
    if dest.exists() {
        // Another writer beat us to it (concurrent cache-miss fetch
        // per cache-layout.md §Concurrency model). Drop the staging
        // dir; the cached state is already correct.
        let _ = std::fs::remove_dir_all(&staging);
        tracing::debug!(
            sha = %sha.to_full_hex(),
            "concurrent writer beat us to the cache; using existing entry",
        );
        return Ok(());
    }
    std::fs::rename(&staging, &dest).map_err(|e| {
        // Race: another writer just landed; treat as success.
        if dest.exists() {
            let _ = std::fs::remove_dir_all(&staging);
            return FetchError::Io(format!(
                "concurrent writer race (cleaned up staging): {e}"
            ));
        }
        let _ = std::fs::remove_dir_all(&staging);
        FetchError::Io(e.to_string())
    })?;
    Ok(())
}

fn extract_corpus_into(body: &[u8], staging_corpus: &Path) -> Result<(), FetchError> {
    let gz = flate2::read::GzDecoder::new(body);
    let mut archive = tar::Archive::new(gz);
    let entries = archive
        .entries()
        .map_err(|e| FetchError::Decompression(e.to_string()))?;

    let mut wrote_any = false;
    for entry in entries {
        let mut entry = entry.map_err(|e| FetchError::Extraction(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| FetchError::Extraction(e.to_string()))?
            .into_owned();
        if let Some(filename) = corpus_filename_from_tar_path(&path) {
            let dest = staging_corpus.join(&filename);
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| FetchError::Extraction(e.to_string()))?;
            std::fs::write(&dest, &bytes).map_err(|e| FetchError::Io(e.to_string()))?;
            wrote_any = true;
        }
    }
    if !wrote_any {
        return Err(FetchError::Extraction(
            "tarball contained no corpus/*.json entries".to_string(),
        ));
    }
    Ok(())
}

/// Strip the top-level `<repo>-<short-sha>/` directory prefix and
/// return the filename if the path is `<wrapper>/corpus/<file>.json`.
/// Anything outside that subtree (LICENSE, README, .github/, schema/)
/// is skipped — we only extract the corpus payload.
fn corpus_filename_from_tar_path(path: &Path) -> Option<String> {
    let mut components = path.components();
    components.next()?; // top-level wrapper dir
    let second = components.next()?;
    if second.as_os_str() != "corpus" {
        return None;
    }
    let third = components.next()?;
    if components.next().is_some() {
        // Nested deeper than corpus/<file> — not a record.
        return None;
    }
    let name = third.as_os_str().to_str()?;
    if !name.ends_with(".json") {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use super::super::test_env_lock as env_lock;
    use std::io::Write;
    use tokio::runtime::Runtime;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

    const SAMPLE_SHA: &str = "fff39c6ad22ce8420b506323ce1d5cce4b628d5c";

    fn build_corpus_tarball() -> Vec<u8> {
        let mut tar_bytes: Vec<u8> = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(
                &mut tar_bytes,
                flate2::Compression::default(),
            );
            let mut builder = tar::Builder::new(enc);
            let wrapper = "mikebom-fingerprints-fff39c6/";
            for name in ["index.json", "openssl.json", "zlib.json"] {
                let payload: &[u8] = match name {
                    "index.json" => br#"{"version":1,"entries":[{"library":"openssl","path":"openssl.json"},{"library":"zlib","path":"zlib.json"}]}"#,
                    "openssl.json" => br#"{"library":"openssl","target_purl":"pkg:generic/openssl","symbols":["SSL_CTX_new","SSL_library_init","OPENSSL_init_ssl","RSA_new","BN_new","X509_new","ERR_get_error","EVP_DigestInit_ex"],"min_symbols":8}"#,
                    "zlib.json" => br#"{"library":"zlib","target_purl":"pkg:generic/zlib","symbols":["deflate","inflate","crc32","adler32","deflateInit_","inflateInit_","deflateEnd","inflateEnd"],"min_symbols":8}"#,
                    _ => unreachable!(),
                };
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

    fn build_invalid_tarball_with_no_corpus_entries() -> Vec<u8> {
        let mut tar_bytes: Vec<u8> = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(
                &mut tar_bytes,
                flate2::Compression::default(),
            );
            let mut builder = tar::Builder::new(enc);
            let payload = b"# README content";
            let mut header = tar::Header::new_gnu();
            header.set_path("mikebom-fingerprints-fff39c6/README.md").unwrap();
            header.set_size(payload.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &payload[..]).unwrap();
            let mut enc = builder.into_inner().unwrap();
            enc.flush().unwrap();
            enc.finish().unwrap();
        }
        tar_bytes
    }

    /// Run an async wiremock setup synchronously, returning the server's
    /// base URL string. The server lives on a dedicated tokio runtime
    /// for the duration of the test.
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

    /// Catch-all responder that returns a configurable status N times
    /// then succeeds with the corpus tarball. Lets us exercise the
    /// 5xx-retry path.
    struct CountingResponder {
        fail_status: u16,
        fail_count: std::sync::atomic::AtomicU32,
        success_body: Vec<u8>,
    }

    impl Respond for CountingResponder {
        fn respond(&self, _req: &wiremock::Request) -> ResponseTemplate {
            use std::sync::atomic::Ordering;
            let remaining = self.fail_count.load(Ordering::SeqCst);
            if remaining > 0 {
                self.fail_count.fetch_sub(1, Ordering::SeqCst);
                ResponseTemplate::new(self.fail_status)
            } else {
                ResponseTemplate::new(200)
                    .set_body_bytes(self.success_body.clone())
                    .insert_header("content-type", "application/x-gzip")
            }
        }
    }

    #[test]
    fn fetches_200_response_extracts_to_cache() {
        let _g = env_lock();
        let tarball = build_corpus_tarball();
        let tarball_for_mock = tarball.clone();
        let harness = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+/archive/.+\.tar\.gz"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_bytes(tarball_for_mock.clone())
                            .insert_header("content-type", "application/x-gzip"),
                    )
                    .mount(server)
                    .await;
            });
        });
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        fetch_corpus_to(&sha, &harness.base_url, tmp.path()).unwrap();

        let dest = tmp.path().join(sha.to_full_hex()).join("corpus");
        assert!(dest.join("index.json").exists(), "index.json missing");
        assert!(dest.join("openssl.json").exists(), "openssl.json missing");
        assert!(dest.join("zlib.json").exists(), "zlib.json missing");
    }

    #[test]
    fn retries_on_5xx_then_succeeds() {
        let _g = env_lock();
        let tarball = build_corpus_tarball();
        let responder = std::sync::Arc::new(CountingResponder {
            fail_status: 503,
            fail_count: std::sync::atomic::AtomicU32::new(2),
            success_body: tarball,
        });
        // Wrap Arc<Respond> in a Respond newtype since wiremock takes
        // owned Respond instances by value.
        struct ResponderAdapter(std::sync::Arc<CountingResponder>);
        impl Respond for ResponderAdapter {
            fn respond(&self, req: &wiremock::Request) -> ResponseTemplate {
                self.0.respond(req)
            }
        }
        let adapter = ResponderAdapter(std::sync::Arc::clone(&responder));
        let harness = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+/archive/.+\.tar\.gz"))
                    .respond_with(adapter)
                    .mount(server)
                    .await;
            });
        });
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        // Two 503s, then 200; well under the 3-retry budget.
        fetch_corpus_to(&sha, &harness.base_url, tmp.path()).unwrap();
        assert!(tmp.path().join(sha.to_full_hex()).join("corpus/index.json").exists());
    }

    #[test]
    fn respects_retry_after_on_429() {
        let _g = env_lock();
        let tarball = build_corpus_tarball();
        // First response: 429 with Retry-After: 1. Second: 200.
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter_for_resp = std::sync::Arc::clone(&counter);
        struct Responder {
            counter: std::sync::Arc<std::sync::atomic::AtomicU32>,
            body: Vec<u8>,
        }
        impl Respond for Responder {
            fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
                use std::sync::atomic::Ordering;
                let n = self.counter.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    ResponseTemplate::new(429).insert_header("retry-after", "1")
                } else {
                    ResponseTemplate::new(200).set_body_bytes(self.body.clone())
                }
            }
        }
        let responder = Responder {
            counter: counter_for_resp,
            body: tarball,
        };
        let harness = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+/archive/.+\.tar\.gz"))
                    .respond_with(responder)
                    .mount(server)
                    .await;
            });
        });
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let start = std::time::Instant::now();
        fetch_corpus_to(&sha, &harness.base_url, tmp.path()).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_secs(1),
            "expected ≥1s sleep per Retry-After; got {elapsed:?}"
        );
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn returns_not_found_on_404() {
        let _g = env_lock();
        let harness = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+/archive/.+\.tar\.gz"))
                    .respond_with(ResponseTemplate::new(404))
                    .mount(server)
                    .await;
            });
        });
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let err = fetch_corpus_to(&sha, &harness.base_url, tmp.path()).unwrap_err();
        assert!(matches!(err, FetchError::NotFound { .. }));
        // No partial cache directory was created.
        assert!(!tmp.path().join(sha.to_full_hex()).exists());
    }

    #[test]
    fn returns_network_error_on_dns_failure() {
        let _g = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        // Unroutable invalid host — DNS resolution should fail fast on
        // every platform that respects RFC 6761 / the IETF reserved
        // TLD .invalid. Each retry is on the same unresolvable host
        // so the test wall-clock stays bounded by reqwest's connect
        // timeout, not the 30s read timeout.
        let err =
            fetch_corpus_to(&sha, "https://nonexistent.invalid", tmp.path()).unwrap_err();
        assert!(
            matches!(err, FetchError::Network(_)),
            "expected Network err; got {err:?}"
        );
    }

    #[test]
    fn cleans_up_tmp_dir_on_extraction_failure() {
        let _g = env_lock();
        let bad_tarball = build_invalid_tarball_with_no_corpus_entries();
        let harness = WiremockHarness::new(|rt, server| {
            rt.block_on(async {
                Mock::given(method("GET"))
                    .and(path_regex(r"/.+/archive/.+\.tar\.gz"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_bytes(bad_tarball.clone())
                            .insert_header("content-type", "application/x-gzip"),
                    )
                    .mount(server)
                    .await;
            });
        });
        let tmp = tempfile::tempdir().unwrap();
        let sha = CorpusSha::from_hex(SAMPLE_SHA).unwrap();
        let err = fetch_corpus_to(&sha, &harness.base_url, tmp.path()).unwrap_err();
        assert!(matches!(err, FetchError::Extraction(_)));
        // No `.tmp-*` staging dir + no `<sha>` cache dir left behind.
        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.is_empty(),
            "expected empty cache root after failure; got {entries:?}"
        );
    }

    // ---- pure-function tests for the path stripper ----

    #[test]
    fn corpus_filename_from_tar_path_matches_corpus_subtree() {
        assert_eq!(
            corpus_filename_from_tar_path(Path::new(
                "mikebom-fingerprints-fff39c6/corpus/openssl.json"
            )),
            Some("openssl.json".to_string())
        );
    }

    #[test]
    fn corpus_filename_from_tar_path_rejects_other_subtrees() {
        assert!(corpus_filename_from_tar_path(Path::new(
            "mikebom-fingerprints-fff39c6/README.md"
        ))
        .is_none());
        assert!(corpus_filename_from_tar_path(Path::new(
            "mikebom-fingerprints-fff39c6/schema/fingerprint-record.v1.json"
        ))
        .is_none());
        assert!(corpus_filename_from_tar_path(Path::new(
            "mikebom-fingerprints-fff39c6/corpus/nested/file.json"
        ))
        .is_none());
        assert!(corpus_filename_from_tar_path(Path::new(
            "mikebom-fingerprints-fff39c6/corpus/file.txt"
        ))
        .is_none());
    }
}
