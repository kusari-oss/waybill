//! Thin OCI distribution-spec HTTP client (milestone 032; auth in 034).
//!
//! Replaces the milestone-031 `oci-client::Client` integration. Built
//! on the workspace's `reqwest 0.12 + rustls-tls (ring)` (no new
//! HTTP/TLS deps) and `oci-spec 0.9` (types-only).
//!
//! Auth: when a registry returns 401 with
//! `WWW-Authenticate: Bearer realm="...",service="...",scope="..."`
//! we fetch a token from the realm. Credentials (when available from
//! the Docker keychain — see `super::auth`) are sent as Basic auth on
//! the realm GET. Without credentials the realm GET is anonymous,
//! covering Docker Hub's "anonymous-but-token-required" handshake +
//! direct-anonymous registries (gcr.io, ghcr.io public, etc.).
//!
//! Endpoints (per OCI distribution-spec v1):
//!   - `GET /v2/<repo>/manifests/<reference>` — manifest or index
//!   - `GET /v2/<repo>/blobs/<digest>`        — config or layer blob

use anyhow::{anyhow, bail, Context, Result};
use sha2::Digest as _;

use oci_spec::image::{ImageIndex, ImageManifest};

use super::auth::Credential;
use super::cache::Cache;
use super::reference::ImageReference;

/// Manifest media types we accept (sent on the `Accept` header
/// for the manifest fetch + dispatched on the response
/// `Content-Type`).
const MANIFEST_MEDIA_TYPES: &[&str] = &[
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
];

/// Either a single-platform image manifest or a multi-platform
/// image index (manifest list). The caller dispatches on which.
///
/// Both variants box their payload — `ImageManifest` is far
/// larger than `ImageIndex` (it carries layer descriptors), and
/// the `clippy::large_enum_variant` lint flagged the size
/// disparity. Boxing makes the enum's stack size constant.
#[allow(clippy::large_enum_variant)]
pub(super) enum ManifestOrIndex {
    Manifest(ImageManifest),
    Index(ImageIndex),
}

/// Thin async HTTP client over the OCI distribution-spec.
pub(super) struct RegistryClient {
    http: reqwest::Client,
    /// Resolved credentials for the target registry (milestone 034).
    /// `None` means anonymous-pull mode. Credentials are bound at
    /// construction time and applied as Basic auth on the bearer-
    /// token realm fetch in [`Self::fetch_bearer_token`].
    credentials: Option<Credential>,
    /// Optional disk cache for blob fetches (milestone 036). `None`
    /// means no caching: every blob is fetched from the network.
    /// When set, [`Self::fetch_blob`] consults the cache first and
    /// inserts on miss.
    cache: Option<Cache>,
}

impl RegistryClient {
    /// Build a client for `reference`, resolving Docker-keychain
    /// credentials for the target registry from
    /// `~/.docker/config.json` (or `$DOCKER_CONFIG/config.json`) at
    /// construction time. Missing config / no entry for this registry
    /// → anonymous mode.
    ///
    /// `cache` is the optional disk cache for blob bodies; `None`
    /// disables caching.
    pub(super) fn new(reference: &ImageReference, cache: Option<Cache>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("mikebom/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building reqwest::Client for OCI registry")?;
        let credentials = super::auth::load_default_docker_config()
            .and_then(|cfg| super::auth::resolve_credentials(&cfg, &reference.registry));
        if credentials.is_some() {
            tracing::debug!(
                registry = %reference.registry,
                "resolved registry credentials from Docker keychain"
            );
        }
        Ok(Self {
            http,
            credentials,
            cache,
        })
    }

    /// Fetch the manifest for `reference`. Returns either a
    /// single-platform manifest or a multi-platform index.
    /// Handles bearer-token retry transparently.
    pub(super) async fn fetch_manifest(
        &self,
        reference: &ImageReference,
    ) -> Result<ManifestOrIndex> {
        let url = manifest_url(reference);
        let body = self.fetch_with_bearer_retry(&url, MANIFEST_MEDIA_TYPES).await?;
        let content_type = body.content_type;
        let bytes = body.bytes;

        // Dispatch on the response's content-type. Two flavors —
        // single manifest vs index/list.
        if is_index_media_type(&content_type) {
            let index: ImageIndex = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing manifest list at {url}"))?;
            return Ok(ManifestOrIndex::Index(index));
        }
        let manifest: ImageManifest = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing manifest at {url}"))?;
        Ok(ManifestOrIndex::Manifest(manifest))
    }

    /// Fetch a blob (config or layer) and verify its SHA-256
    /// matches the declared `digest`. The digest is the
    /// `<algorithm>:<hex>` form straight from the descriptor.
    ///
    /// When [`Self::cache`] is `Some`, the cache is consulted before
    /// any network call; on hit the cached bytes (already SHA-256
    /// verified by [`Cache::get`]) are returned. On miss the
    /// network bytes are verified, inserted into the cache (errors
    /// logged but non-fatal), and returned.
    pub(super) async fn fetch_blob(
        &self,
        reference: &ImageReference,
        digest: &str,
    ) -> Result<Vec<u8>> {
        if let Some(cache) = self.cache.as_ref() {
            if let Some(bytes) = cache.get(digest) {
                return Ok(bytes);
            }
        }
        let url = blob_url(reference, digest);
        // Blob endpoint accepts any media type; we send `*/*`.
        let body = self.fetch_with_bearer_retry(&url, &["*/*"]).await?;
        verify_sha256(&body.bytes, digest)
            .with_context(|| format!("verifying blob {digest} from {url}"))?;
        if let Some(cache) = self.cache.as_ref() {
            if let Err(e) = cache.insert(digest, &body.bytes) {
                tracing::warn!(
                    %digest,
                    error = %e,
                    "OCI blob cache insert failed; scan continues without caching this blob"
                );
            }
        }
        Ok(body.bytes)
    }

    /// GET `url` with the supplied Accept media types. Handles
    /// 401 → bearer-token-fetch → retry. Returns the body bytes
    /// + the Content-Type header so the caller can dispatch.
    async fn fetch_with_bearer_retry(
        &self,
        url: &str,
        accept: &[&str],
    ) -> Result<ResponseBody> {
        let accept_header = accept.join(", ");
        let first = self
            .http
            .get(url)
            .header("Accept", &accept_header)
            .send()
            .await
            .with_context(|| format!("sending GET {url}"))?;
        let status = first.status();
        if status.is_success() {
            return ResponseBody::from_response(first).await;
        }
        if status.as_u16() == 401 {
            // Parse the bearer challenge, fetch a token, retry.
            let www_auth = first
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .ok_or_else(|| {
                    anyhow!("registry returned 401 without WWW-Authenticate header for GET {url}")
                })?
                .to_str()
                .context("WWW-Authenticate is not valid UTF-8")?
                .to_string();
            let challenge = parse_bearer_challenge(&www_auth)?;
            let token = self.fetch_bearer_token(&challenge).await?;
            let retry = self
                .http
                .get(url)
                .header("Accept", &accept_header)
                .bearer_auth(&token)
                .send()
                .await
                .with_context(|| format!("retrying GET {url} with bearer token"))?;
            if retry.status().is_success() {
                return ResponseBody::from_response(retry).await;
            }
            if self.credentials.is_some() {
                bail!(
                    "registry authentication failed for GET {url} \
                     (got {} after bearer-token retry). Verify credentials \
                     in ~/.docker/config.json or your credential helper.",
                    retry.status()
                );
            }
            bail!(
                "registry returned {} for GET {url} after anonymous \
                 bearer-token retry. For private registries, configure \
                 ~/.docker/config.json (`auth` or `identitytoken` field) \
                 or a credential helper.",
                retry.status()
            );
        }
        // 403 / 404 / 5xx etc.
        bail!("registry returned {status} for GET {url}.");
    }

    /// Bearer-token fetch from the realm. Used when the registry's
    /// 401 response includes a
    /// `Bearer realm="...",service="...",scope="..."` challenge.
    ///
    /// When [`Self::credentials`] is `Some`, sends `Basic <b64(user:secret)>`
    /// on the realm GET; the realm validates and returns a bearer
    /// token scoped per the credentials. When `None`, the request is
    /// anonymous (covers the public-Hub / public-GHCR / gcr.io flow).
    async fn fetch_bearer_token(&self, challenge: &BearerChallenge) -> Result<String> {
        let mut req = self.http.get(&challenge.realm);
        if let Some(service) = challenge.service.as_deref() {
            req = req.query(&[("service", service)]);
        }
        if let Some(scope) = challenge.scope.as_deref() {
            req = req.query(&[("scope", scope)]);
        }
        if let Some(c) = self.credentials.as_ref() {
            req = req.basic_auth(&c.username, Some(&c.secret));
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("fetching bearer token from {}", challenge.realm))?;
        if !resp.status().is_success() {
            bail!(
                "bearer token endpoint {} returned {}",
                challenge.realm,
                resp.status()
            );
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .context("parsing bearer token response as JSON")?;
        // Different registries return different field names; check
        // common ones.
        for field in ["token", "access_token"] {
            if let Some(t) = body.get(field).and_then(|v| v.as_str()) {
                return Ok(t.to_string());
            }
        }
        Err(anyhow!(
            "bearer token response missing `token` / `access_token` field"
        ))
    }
}

/// The `WWW-Authenticate: Bearer realm="...",service="...",scope="..."`
/// challenge fields. realm is required; service and scope are
/// optional (some registries emit only realm).
struct BearerChallenge {
    realm: String,
    service: Option<String>,
    scope: Option<String>,
}

/// Parse a `WWW-Authenticate: Bearer realm="...",service="...",scope="..."`
/// header value into its fields.
fn parse_bearer_challenge(value: &str) -> Result<BearerChallenge> {
    // Strip the `Bearer ` prefix (case-insensitive).
    let trimmed = value.trim_start();
    let after_scheme = if trimmed.len() >= 7
        && trimmed[..7].eq_ignore_ascii_case("Bearer ")
    {
        &trimmed[7..]
    } else {
        bail!("WWW-Authenticate is not a Bearer challenge: {value}");
    };

    // Split on commas at the top level. Values may contain commas
    // inside double-quotes; we respect that.
    let mut realm: Option<String> = None;
    let mut service: Option<String> = None;
    let mut scope: Option<String> = None;
    for (k, v) in iter_kv_pairs(after_scheme) {
        match k.as_str() {
            "realm" => realm = Some(v),
            "service" => service = Some(v),
            "scope" => scope = Some(v),
            _ => {}
        }
    }
    let realm = realm.ok_or_else(|| {
        anyhow!("WWW-Authenticate Bearer challenge missing `realm`: {value}")
    })?;
    Ok(BearerChallenge {
        realm,
        service,
        scope,
    })
}

/// Iterate `key="value"` pairs respecting double-quoted values
/// (which may contain commas, equals signs, etc.).
fn iter_kv_pairs(s: &str) -> impl Iterator<Item = (String, String)> + '_ {
    let mut chars = s.chars().peekable();
    std::iter::from_fn(move || {
        // Skip leading whitespace + commas.
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }
        // Read key up to `=`.
        let mut key = String::new();
        for c in chars.by_ref() {
            if c == '=' {
                break;
            }
            key.push(c);
        }
        if key.is_empty() {
            return None;
        }
        let key = key.trim().to_string();
        // Read value: either `"quoted,maybe,with,commas"` or bare.
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next(); // consume opening quote
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        value.push(escaped);
                    }
                } else if c == '"' {
                    break;
                } else {
                    value.push(c);
                }
            }
        } else {
            for c in chars.by_ref() {
                if c == ',' {
                    break;
                }
                value.push(c);
            }
        }
        Some((key, value.trim().to_string()))
    })
}

/// Detect whether a manifest `Content-Type` header indicates a
/// multi-arch image index (manifest list), as opposed to a
/// single-platform manifest.
fn is_index_media_type(content_type: &str) -> bool {
    // Strip any `; charset=utf-8`-style parameters.
    let mt = content_type.split(';').next().unwrap_or("").trim();
    matches!(
        mt,
        "application/vnd.oci.image.index.v1+json"
            | "application/vnd.docker.distribution.manifest.list.v2+json"
    )
}

fn manifest_url(reference: &ImageReference) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    format!(
        "https://{registry}/v2/{}/manifests/{}",
        reference.repository,
        reference.resolved_reference()
    )
}

fn blob_url(reference: &ImageReference, digest: &str) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    format!(
        "https://{registry}/v2/{}/blobs/{}",
        reference.repository, digest
    )
}

/// `docker.io` is the user-facing registry name; the actual API
/// endpoint is `registry-1.docker.io`. Other registries use their
/// hostname directly.
fn resolve_registry_for_url(registry: &str) -> &str {
    if registry == "docker.io" {
        "registry-1.docker.io"
    } else {
        registry
    }
}

fn verify_sha256(bytes: &[u8], expected_digest: &str) -> Result<()> {
    let (algo, expected_hex) = expected_digest
        .split_once(':')
        .ok_or_else(|| anyhow!("digest missing `<algorithm>:<hex>` separator: {expected_digest}"))?;
    if !algo.eq_ignore_ascii_case("sha256") {
        bail!("only sha256 digests supported, got `{algo}` in `{expected_digest}`");
    }
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    let actual_hex = format!("{:x}", hasher.finalize());
    if !actual_hex.eq_ignore_ascii_case(expected_hex) {
        bail!(
            "blob digest mismatch: expected sha256:{expected_hex}, got sha256:{actual_hex}"
        );
    }
    Ok(())
}

struct ResponseBody {
    bytes: Vec<u8>,
    content_type: String,
}

impl ResponseBody {
    async fn from_response(resp: reqwest::Response) -> Result<Self> {
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = resp
            .bytes()
            .await
            .context("reading response body")?
            .to_vec();
        Ok(Self {
            bytes,
            content_type,
        })
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn parse_bearer_challenge_extracts_realm_service_scope() {
        // Docker Hub's actual challenge format.
        let v = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#;
        let c = parse_bearer_challenge(v).unwrap();
        assert_eq!(c.realm, "https://auth.docker.io/token");
        assert_eq!(c.service.as_deref(), Some("registry.docker.io"));
        assert_eq!(c.scope.as_deref(), Some("repository:library/alpine:pull"));
    }

    #[test]
    fn parse_bearer_challenge_handles_realm_only() {
        let v = r#"Bearer realm="https://example.com/token""#;
        let c = parse_bearer_challenge(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
        assert_eq!(c.service, None);
        assert_eq!(c.scope, None);
    }

    #[test]
    fn parse_bearer_challenge_handles_unquoted_values() {
        // RFC 7235 allows token-style values without quotes.
        let v = "Bearer realm=https://example.com/token,service=example.com";
        let c = parse_bearer_challenge(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
        assert_eq!(c.service.as_deref(), Some("example.com"));
    }

    #[test]
    fn parse_bearer_challenge_rejects_basic_scheme() {
        let v = r#"Basic realm="x""#;
        assert!(parse_bearer_challenge(v).is_err());
    }

    #[test]
    fn parse_bearer_challenge_rejects_missing_realm() {
        let v = r#"Bearer service="x",scope="y""#;
        assert!(parse_bearer_challenge(v).is_err());
    }

    #[test]
    fn parse_bearer_challenge_handles_case_insensitive_scheme() {
        let v = r#"bearer realm="https://example.com/token""#;
        let c = parse_bearer_challenge(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
    }

    #[test]
    fn is_index_media_type_recognizes_oci_and_docker_lists() {
        assert!(is_index_media_type(
            "application/vnd.oci.image.index.v1+json"
        ));
        assert!(is_index_media_type(
            "application/vnd.docker.distribution.manifest.list.v2+json"
        ));
        // Single-platform manifests are NOT indexes.
        assert!(!is_index_media_type(
            "application/vnd.oci.image.manifest.v1+json"
        ));
        assert!(!is_index_media_type(
            "application/vnd.docker.distribution.manifest.v2+json"
        ));
    }

    #[test]
    fn is_index_media_type_strips_charset_parameter() {
        assert!(is_index_media_type(
            "application/vnd.oci.image.index.v1+json; charset=utf-8"
        ));
    }

    #[test]
    fn verify_sha256_passes_on_match() {
        let bytes = b"hello world";
        // sha256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        let digest = "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        verify_sha256(bytes, digest).unwrap();
    }

    #[test]
    fn verify_sha256_fails_on_mismatch() {
        let bytes = b"hello world";
        let digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
        let err = verify_sha256(bytes, digest).unwrap_err();
        assert!(err.to_string().contains("digest mismatch"));
    }

    #[test]
    fn verify_sha256_rejects_non_sha256_algorithm() {
        assert!(verify_sha256(b"x", "sha512:00").is_err());
    }

    #[test]
    fn verify_sha256_rejects_malformed_digest() {
        assert!(verify_sha256(b"x", "no-separator").is_err());
    }

    #[test]
    fn manifest_url_uses_registry_1_for_docker_io() {
        let reference = super::super::reference::parse_reference("alpine:3.19").unwrap();
        let url = manifest_url(&reference);
        assert_eq!(
            url,
            "https://registry-1.docker.io/v2/library/alpine/manifests/3.19"
        );
    }

    #[test]
    fn manifest_url_uses_other_registries_directly() {
        let reference =
            super::super::reference::parse_reference("gcr.io/distroless/static-debian12:latest")
                .unwrap();
        let url = manifest_url(&reference);
        assert_eq!(
            url,
            "https://gcr.io/v2/distroless/static-debian12/manifests/latest"
        );
    }

    #[test]
    fn blob_url_uses_digest_directly() {
        let reference = super::super::reference::parse_reference("alpine:3.19").unwrap();
        let url = blob_url(&reference, "sha256:abc123");
        assert_eq!(
            url,
            "https://registry-1.docker.io/v2/library/alpine/blobs/sha256:abc123"
        );
    }

    /// End-to-end auth wire-up test (milestone 034 commit 2): when a
    /// `RegistryClient` carries a `Credential`, the bearer-token realm
    /// fetch sends `Authorization: Basic <b64(user:secret)>`. We spin
    /// up a tokio TCP listener that speaks one HTTP request and
    /// inspect the Authorization header on the wire — no mock-server
    /// crate dependency.
    #[tokio::test]
    async fn fetch_bearer_token_sends_basic_auth_when_credential_present() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read until end-of-headers (\r\n\r\n). GETs have no body,
            // so we don't need Content-Length parsing.
            let mut buf = vec![0u8; 4096];
            let mut total = 0;
            while total < buf.len() {
                let n = stream.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
                if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&buf[..total]).into_owned();
            let body = r#"{"token":"the-bearer-token"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            request
        });

        // Construct RegistryClient with explicit credentials (bypassing
        // the Docker-keychain lookup — we don't want this test to
        // depend on the developer's actual ~/.docker/config.json).
        let client = RegistryClient {
            http: reqwest::Client::new(),
            credentials: Some(Credential {
                username: "alice".to_string(),
                secret: "hunter2".to_string(),
            }),
            cache: None,
        };
        let challenge = BearerChallenge {
            realm: format!("http://{addr}/token"),
            service: Some("test".to_string()),
            scope: Some("repository:foo/bar:pull".to_string()),
        };

        let token = client.fetch_bearer_token(&challenge).await.unwrap();
        assert_eq!(token, "the-bearer-token");

        let request = server.await.unwrap();
        // base64("alice:hunter2") = YWxpY2U6aHVudGVyMg==
        assert!(
            request.contains("Authorization: Basic YWxpY2U6aHVudGVyMg==")
                || request.contains("authorization: Basic YWxpY2U6aHVudGVyMg=="),
            "realm GET should carry Basic auth header; got request:\n{request}"
        );
    }

    /// Counterpart: anonymous mode (no credentials) sends NO
    /// Authorization header on the realm GET. Guards against future
    /// regressions where a default-credential leak could pin auth on
    /// for everyone.
    #[tokio::test]
    async fn fetch_bearer_token_sends_no_auth_when_credential_absent() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let mut total = 0;
            while total < buf.len() {
                let n = stream.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
                if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&buf[..total]).into_owned();
            let body = r#"{"token":"anon-token"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            request
        });

        let client = RegistryClient {
            http: reqwest::Client::new(),
            credentials: None,
            cache: None,
        };
        let challenge = BearerChallenge {
            realm: format!("http://{addr}/token"),
            service: None,
            scope: None,
        };

        let token = client.fetch_bearer_token(&challenge).await.unwrap();
        assert_eq!(token, "anon-token");

        let request = server.await.unwrap();
        let has_auth = request
            .lines()
            .any(|l| l.to_ascii_lowercase().starts_with("authorization:"));
        assert!(
            !has_auth,
            "anonymous realm GET must not carry Authorization header; got request:\n{request}"
        );
    }

    /// End-to-end cache wire-up test (milestone 036 commit 2): when
    /// a `RegistryClient` carries a populated `Cache`, a subsequent
    /// `fetch_blob` for the same digest reads from disk without a
    /// network call. We verify "no network call" by pointing the
    /// reference at an unreachable host — if the cache misses, the
    /// fetch errors out on connect.
    #[tokio::test]
    async fn fetch_blob_returns_cached_bytes_without_network() {
        use sha2::Digest as _;

        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("sha256")).unwrap();
        let cache = super::super::cache::Cache::open_for_test(tmp.path(), 1 << 30);

        let bytes = b"hello cached world".to_vec();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        let digest = format!("sha256:{:x}", hasher.finalize());
        cache.insert(&digest, &bytes).unwrap();

        // Reference points at a nonexistent host; if the cache misses
        // the fetch will fail on connect.
        let reference = super::super::reference::parse_reference(
            "registry.invalid.mikebom-test.example/foo/bar:tag",
        )
        .unwrap();

        let client = RegistryClient {
            http: reqwest::Client::new(),
            credentials: None,
            cache: Some(cache),
        };

        let got = client.fetch_blob(&reference, &digest).await.unwrap();
        assert_eq!(
            got, bytes,
            "cache hit should return the previously-inserted bytes \
             without making a network call"
        );
    }
}
