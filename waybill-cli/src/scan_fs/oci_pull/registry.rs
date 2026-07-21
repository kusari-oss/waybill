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
use super::tls_config::RegistryTlsConfig;

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
    /// Milestone 182 — TLS/transport configuration consulted at
    /// URL-scheme decision time (`manifest_url` / `blob_url`) and
    /// during `reqwest::Client` construction (`add_root_certificate` +
    /// `danger_accept_invalid_certs`). Default state is byte-identical
    /// to pre-m182 behavior.
    tls_config: RegistryTlsConfig,
}

impl RegistryClient {
    /// Build a client for `reference`, resolving Docker-keychain
    /// credentials for the target registry via the layered resolver
    /// (issue #235): per-registry env vars, generic env vars,
    /// optional `--registry-credentials-dir` mount, then default
    /// `~/.docker/config.json` (or `$DOCKER_CONFIG/config.json`).
    /// Missing config across all sources → anonymous mode.
    ///
    /// `cache` is the optional disk cache for blob bodies; `None`
    /// disables caching.
    /// `creds_dir` is the optional path supplied via
    /// `--registry-credentials-dir`; `None` skips the K8s
    /// secret-mount probe layer.
    pub(super) fn new(
        reference: &ImageReference,
        cache: Option<Cache>,
        creds_dir: Option<&std::path::Path>,
        // Milestone 182 — TLS/transport configuration surfaced by the
        // three m182 flags. Consumed at client-build time (CA bundle
        // additions + skip-verify) and stored for later scheme
        // selection in manifest_url / blob_url.
        tls_config: &RegistryTlsConfig,
    ) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("mikebom/", env!("CARGO_PKG_VERSION")));

        // m182 CA bundle: additive to webpki-roots — nothing removed.
        for cert in &tls_config.ca_bundle {
            builder = builder.add_root_certificate(cert.clone());
        }

        // m182 skip-verify: dangerous — emit the FR-007 WARN log
        // per Constitution Principle X (operator-visible audit trail).
        if tls_config.skip_verify {
            builder = builder.danger_accept_invalid_certs(true);
            tracing::warn!(
                registry = %reference.registry,
                "TLS verification DISABLED via --insecure-tls-skip-verify — \
                 cert chain, hostname, and expiry checks are skipped for this scan. \
                 Use only for CI/dev against self-signed or hostname-mismatched certs. \
                 For production private-CA registries, use --registry-ca-cert instead."
            );
        }

        let http = builder
            .build()
            .context("building reqwest::Client for OCI registry")?;
        let credentials = super::auth::resolve_credentials_layered(
            &reference.registry,
            creds_dir,
        );
        if credentials.is_some() {
            tracing::debug!(
                registry = %reference.registry,
                "resolved registry credentials (layered resolver)"
            );
        }
        Ok(Self {
            http,
            credentials,
            cache,
            tls_config: tls_config.clone(),
        })
    }

    /// Fetch the manifest for `reference`. Returns either a
    /// single-platform manifest or a multi-platform index.
    /// Handles bearer-token retry transparently.
    pub(super) async fn fetch_manifest(
        &self,
        reference: &ImageReference,
    ) -> Result<ManifestOrIndex> {
        let url = manifest_url(reference, &self.tls_config);
        let body = self.fetch_with_auth_retry(&url, MANIFEST_MEDIA_TYPES).await?;
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

    /// Milestone 186 (#442) — fetch the raw manifest body (bytes only,
    /// without the manifest-vs-index dispatch step) for the referenced image.
    ///
    /// Used by [`super::try_fetch_referrer_sbom`] to derive the manifest's
    /// SHA-256 digest when the operator supplied a tag reference (not a
    /// pinned `@sha256:...` reference). The `Docker-Content-Digest` response
    /// header would be authoritative per OCI Distribution Spec §manifest-get,
    /// but not every registry emits it consistently; hashing the body is the
    /// portable fallback and matches OCI Distribution Spec §Content Verification.
    pub(super) async fn fetch_manifest_body(
        &self,
        reference: &ImageReference,
    ) -> Result<Vec<u8>> {
        let url = manifest_url(reference, &self.tls_config);
        let body = self.fetch_with_auth_retry(&url, MANIFEST_MEDIA_TYPES).await?;
        Ok(body.bytes)
    }

    /// Milestone 186 (#442) — GET `/v2/<repo>/referrers/<manifest-digest>`
    /// per OCI Distribution Spec v1.1 §Referrers.
    ///
    /// Reuses [`Self::fetch_with_auth_retry`] for the same bearer/basic
    /// auth-retry semantics as manifest / blob fetches.
    ///
    /// Returns:
    ///   * `Ok(Some(index))` — the endpoint responded HTTP 200 with a valid
    ///     `ImageIndex` body (may contain zero descriptors).
    ///   * `Ok(None)` — the endpoint returned HTTP 404, signaling
    ///     "registry does not support Referrers API (pre-v1.1)". Distinct
    ///     from the auth-retry-then-fail branch which returns `Err`.
    ///   * `Err(_)` — HTTP / auth / body-parse failure. `fetch_with_auth_retry`
    ///     bails on non-2xx non-401, so we detect 404 by pre-checking the
    ///     response status inline.
    ///
    /// The `manifest_digest` is `sha256:<hex>` — the resolved single-platform
    /// manifest's digest (NOT the multi-arch index digest).
    pub(super) async fn fetch_referrers(
        &self,
        reference: &ImageReference,
        manifest_digest: &str,
    ) -> Result<Option<ImageIndex>> {
        let url = referrers_url(reference, manifest_digest, &self.tls_config);
        // Distribution Spec v1.1 mandates the OCI Image Index media type for
        // the Referrers response.
        let accept = &["application/vnd.oci.image.index.v1+json"][..];
        // We pre-check for HTTP 404 (which is a spec-blessed signal, not an
        // error) before delegating to fetch_with_auth_retry (which treats
        // 404 as a hard error). Send an unauthenticated GET first; if it
        // returns 404, short-circuit; otherwise let the shared retry helper
        // handle 401 challenges + 2xx bodies.
        let accept_header = accept.join(", ");
        let probe = match self
            .http
            .get(&url)
            .header("Accept", &accept_header)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => return Err(classify_transport_error(&url, err)),
        };
        if probe.status().as_u16() == 404 {
            tracing::info!(
                registry = %reference.registry,
                repository = %reference.repository,
                %url,
                "Referrers endpoint returned HTTP 404 — registry does not support OCI Distribution Spec v1.1 Referrers API",
            );
            return Ok(None);
        }
        // Fall through to the shared auth-retry path. This re-issues the GET
        // (a small duplicate cost on 200 responses; on 401 the probe already
        // paid for the challenge round-trip). We prefer this over duplicating
        // fetch_with_auth_retry's ~90 lines of Bearer/Basic logic.
        drop(probe);
        let body = self.fetch_with_auth_retry(&url, accept).await?;
        let index: ImageIndex = serde_json::from_slice(&body.bytes)
            .with_context(|| format!("parsing Referrers response at {url}"))?;
        Ok(Some(index))
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
        let url = blob_url(reference, digest, &self.tls_config);
        // Blob endpoint accepts any media type; we send `*/*`.
        let body = self.fetch_with_auth_retry(&url, &["*/*"]).await?;
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
    /// 401 → auth-challenge → retry for both `Bearer` (token-realm
    /// flow) and `Basic` (direct-credentials flow, used by ECR).
    /// Returns the body bytes + the Content-Type header so the
    /// caller can dispatch.
    async fn fetch_with_auth_retry(
        &self,
        url: &str,
        accept: &[&str],
    ) -> Result<ResponseBody> {
        let accept_header = accept.join(", ");
        let first = match self
            .http
            .get(url)
            .header("Accept", &accept_header)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => return Err(classify_transport_error(url, err)),
        };
        let status = first.status();
        if status.is_success() {
            return ResponseBody::from_response(first).await;
        }
        if status.as_u16() == 401 {
            let www_auth = first
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .ok_or_else(|| {
                    anyhow!("registry returned 401 without WWW-Authenticate header for GET {url}")
                })?
                .to_str()
                .context("WWW-Authenticate is not valid UTF-8")?
                .to_string();
            let challenge = parse_auth_challenge(&www_auth)?;
            let retry = match challenge {
                AuthChallenge::Bearer(bearer) => {
                    let token = self.fetch_bearer_token(&bearer).await?;
                    self.http
                        .get(url)
                        .header("Accept", &accept_header)
                        .bearer_auth(&token)
                        .send()
                        .await
                        .with_context(|| format!("retrying GET {url} with bearer token"))?
                }
                AuthChallenge::Basic { realm } => {
                    tracing::debug!(
                        url,
                        %realm,
                        "registry sent Basic auth challenge; applying cached docker credentials"
                    );
                    let creds = self.credentials.as_ref().ok_or_else(|| {
                        anyhow!(
                            "registry returned 401 with Basic auth challenge for GET {url}, \
                             but no credentials are configured for this registry. \
                             Run `docker login <registry>` (or for AWS ECR, \
                             `aws ecr get-login-password | docker login --username AWS \
                             --password-stdin <registry>`) so the credentials land in \
                             ~/.docker/config.json."
                        )
                    })?;
                    self.http
                        .get(url)
                        .header("Accept", &accept_header)
                        .basic_auth(&creds.username, Some(&creds.secret))
                        .send()
                        .await
                        .with_context(|| {
                            format!("retrying GET {url} with Basic auth")
                        })?
                }
            };
            if retry.status().is_success() {
                return ResponseBody::from_response(retry).await;
            }
            if self.credentials.is_some() {
                bail!(
                    "registry authentication failed for GET {url} \
                     (got {} after auth retry). Verify credentials \
                     in ~/.docker/config.json or your credential helper.",
                    retry.status()
                );
            }
            bail!(
                "registry returned {} for GET {url} after anonymous \
                 auth retry. For private registries, configure \
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
#[derive(Debug)]
struct BearerChallenge {
    realm: String,
    service: Option<String>,
    scope: Option<String>,
}

/// Parsed `WWW-Authenticate` challenge. Two schemes matter for OCI
/// distribution-spec implementations in the wild:
///
/// - `Bearer`: standard distribution-spec auth (Docker Hub, GHCR,
///   gcr.io, …) — fetch a token from the realm endpoint, retry the
///   request with `Authorization: Bearer <token>`.
/// - `Basic`: AWS ECR's flavor — apply `Authorization: Basic
///   <base64(user:secret)>` directly on the original request from
///   the credentials cached in `~/.docker/config.json` (populated
///   by `aws ecr get-login-password | docker login`). No realm
///   round-trip.
#[derive(Debug)]
enum AuthChallenge {
    Bearer(BearerChallenge),
    Basic { realm: String },
}

/// Parse a `WWW-Authenticate: <scheme> ...` header value. Supports
/// both `Bearer` (token-realm flow) and `Basic` (direct cred apply)
/// schemes. Anything else errors out.
fn parse_auth_challenge(value: &str) -> Result<AuthChallenge> {
    let trimmed = value.trim_start();
    if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("Bearer ") {
        let after = &trimmed[7..];
        let mut realm: Option<String> = None;
        let mut service: Option<String> = None;
        let mut scope: Option<String> = None;
        for (k, v) in iter_kv_pairs(after) {
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
        return Ok(AuthChallenge::Bearer(BearerChallenge {
            realm,
            service,
            scope,
        }));
    }
    // Match `Basic` followed by either whitespace (parameters
    // present) or end-of-string (bare scheme token). RFC 7617
    // requires `realm`; we accept its absence defensively (some
    // non-conforming registries may omit it) and store an empty
    // string. The `realm` value is purely diagnostic for this
    // scheme — the credentials apply regardless.
    let lower = trimmed.to_ascii_lowercase();
    let basic_match = lower == "basic" || lower.starts_with("basic ");
    if basic_match {
        let after = if trimmed.len() > 5 {
            &trimmed[5..]
        } else {
            ""
        };
        let mut realm: Option<String> = None;
        for (k, v) in iter_kv_pairs(after) {
            if k == "realm" {
                realm = Some(v);
            }
        }
        return Ok(AuthChallenge::Basic {
            realm: realm.unwrap_or_default(),
        });
    }
    bail!("WWW-Authenticate uses an unsupported scheme (mikebom understands Bearer and Basic): {value}")
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

fn manifest_url(reference: &ImageReference, tls_config: &RegistryTlsConfig) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    let scheme = scheme_for_registry(&reference.registry, tls_config);
    format!(
        "{scheme}://{registry}/v2/{}/manifests/{}",
        reference.repository,
        reference.resolved_reference()
    )
}

fn blob_url(reference: &ImageReference, digest: &str, tls_config: &RegistryTlsConfig) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    let scheme = scheme_for_registry(&reference.registry, tls_config);
    format!(
        "{scheme}://{registry}/v2/{}/blobs/{}",
        reference.repository, digest
    )
}

/// Milestone 186 (#442) — Referrers-endpoint URL per OCI Distribution Spec
/// v1.1 §Referrers. Composes with the m182 `is_insecure_registry` matcher
/// (plain HTTP for local dev / kind-mirror registries).
fn referrers_url(
    reference: &ImageReference,
    manifest_digest: &str,
    tls_config: &RegistryTlsConfig,
) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    let scheme = scheme_for_registry(&reference.registry, tls_config);
    format!(
        "{scheme}://{registry}/v2/{}/referrers/{}",
        reference.repository, manifest_digest
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

/// Milestone 182 — pick `http` or `https` per the m182
/// `--insecure-registry` matcher.
///
/// Matches on the *user-facing* registry name (FR-005) — the same
/// string the operator typed in `--image`. `docker.io` in the flag
/// does NOT auto-expand to `registry-1.docker.io`.
fn scheme_for_registry(user_facing_registry: &str, tls_config: &RegistryTlsConfig) -> &'static str {
    let (host, port) = split_host_port(user_facing_registry);
    if tls_config.is_insecure_registry(host, port) {
        "http"
    } else {
        "https"
    }
}

/// Split `"host:port"` into `("host", Some(port))` or `"host"` into
/// `("host", None)`. Falls back to `(input, None)` on invalid port
/// (e.g. IPv6 literals without brackets — mikebom does not currently
/// scan IPv6-literal registry hosts).
fn split_host_port(hostport: &str) -> (&str, Option<u16>) {
    match hostport.rsplit_once(':') {
        Some((host, port_str)) => match port_str.parse::<u16>() {
            Ok(port) => (host, Some(port)),
            Err(_) => (hostport, None),
        },
        None => (hostport, None),
    }
}

/// Milestone 182 — classify a `reqwest` transport error into an
/// actionable message per FR-014.
///
/// Two specific TLS shapes name the fix flag:
///
///   * Chain / hostname / expiry validation failure → Case 2
///     (points at `--registry-ca-cert` + `--insecure-tls-skip-verify`).
///     Fires when the registry uses a private CA or self-signed cert
///     and the operator didn't supply either flag.
///
///   * TLS handshake failure (usually plain-HTTP served on the TLS
///     port, or an unreachable-over-TLS listener) → Case 1 (points at
///     `--insecure-registry`). Fires against Harbor devenv-style
///     deployments.
///
/// Everything else falls through to the pre-m182 message shape so we
/// don't over-fit the diagnostic. Chain-error is checked BEFORE
/// handshake-error because chain-invalid is more specific.
fn classify_transport_error(url: &str, err: reqwest::Error) -> anyhow::Error {
    if is_tls_chain_error(&err) {
        let host_display = url_host_display(url);
        return anyhow!(
            "TLS certificate chain validation failed for GET {url}. \
             For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>. \
             For self-signed dev/CI certs, pass --insecure-tls-skip-verify \
             (unsafe for production). {host_hint}Underlying error: {err}",
            host_hint = if host_display.is_empty() {
                String::new()
            } else {
                format!("(host `{host_display}`) ")
            }
        );
    }
    if is_tls_handshake_error(&err) {
        let host_display = url_host_display(url);
        return anyhow!(
            "TLS handshake failed for GET {url}. If this registry uses plain HTTP, \
             pass --insecure-registry {host_display}. Underlying error: {err}"
        );
    }
    // Unchanged path — carries the pre-m182 shape.
    anyhow::Error::new(err).context(format!("sending GET {url}"))
}

/// Extract `host[:port]` from a URL string for display in the FR-014
/// error hint. Falls back to `""` on parse failure — the message is
/// still actionable without it (it's only supplementary).
fn url_host_display(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => String::new(),
        },
        Err(_) => String::new(),
    }
}

/// True iff `err`'s chain contains a signal that the peer's TLS
/// certificate chain / hostname / expiry check failed. Uses substring
/// matching over the whole error-chain display — reqwest 0.12's error
/// enum does NOT surface rustls internals via a stable pattern-match
/// API, so this is the documented fallback tactic.
fn is_tls_chain_error(err: &reqwest::Error) -> bool {
    let chain_text = format_error_chain(err);
    // rustls error text patterns for cert-verify failures.
    chain_text.contains("certificate verify failed")
        || chain_text.contains("UnknownIssuer")
        || chain_text.contains("unknown issuer")
        || chain_text.contains("invalid certificate")
        || chain_text.contains("InvalidCertificate")
        || chain_text.contains("BadSignature")
        || chain_text.contains("NotValidForName")
        || chain_text.contains("not valid for name")
        // rustls 0.23 hostname-mismatch variant.
        || chain_text.contains("CertNotValidForName")
}

/// True iff `err`'s chain contains a signal that the TLS handshake
/// itself failed (not that a chain check rejected a valid handshake).
/// Fires against plain-HTTP-on-TLS-port scenarios (Harbor devenv).
fn is_tls_handshake_error(err: &reqwest::Error) -> bool {
    let chain_text = format_error_chain(err);
    chain_text.contains("handshake")
        || chain_text.contains("HandshakeFailure")
        // reqwest bubbles rustls messages like "received corrupt message"
        // when a plain-HTTP server responds to a TLS ClientHello.
        || chain_text.contains("received corrupt message")
        || chain_text.contains("CorruptMessage")
        // Some Linux stacks surface plain-HTTP-on-TLS as ECONNRESET.
        || chain_text.contains("connection closed via error")
}

/// Format `err` plus its `source()` chain into a single lowercase-
/// safe string for substring matching. `Debug` is used because
/// `Display` on `reqwest::Error` often omits the underlying rustls
/// text.
fn format_error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut out = format!("{err} {err:?}");
    let mut cur: Option<&(dyn std::error::Error + 'static)> = err.source();
    while let Some(s) = cur {
        out.push(' ');
        out.push_str(&format!("{s} {s:?}"));
        cur = s.source();
    }
    out
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

#[derive(Debug)]
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

    fn parse_bearer_for_test(value: &str) -> Result<BearerChallenge> {
        match parse_auth_challenge(value)? {
            AuthChallenge::Bearer(b) => Ok(b),
            AuthChallenge::Basic { .. } => {
                bail!("expected Bearer challenge, got Basic")
            }
        }
    }

    #[test]
    fn parse_auth_challenge_extracts_realm_service_scope() {
        // Docker Hub's actual challenge format.
        let v = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#;
        let c = parse_bearer_for_test(v).unwrap();
        assert_eq!(c.realm, "https://auth.docker.io/token");
        assert_eq!(c.service.as_deref(), Some("registry.docker.io"));
        assert_eq!(c.scope.as_deref(), Some("repository:library/alpine:pull"));
    }

    #[test]
    fn parse_auth_challenge_handles_realm_only() {
        let v = r#"Bearer realm="https://example.com/token""#;
        let c = parse_bearer_for_test(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
        assert_eq!(c.service, None);
        assert_eq!(c.scope, None);
    }

    #[test]
    fn parse_auth_challenge_handles_unquoted_values() {
        // RFC 7235 allows token-style values without quotes.
        let v = "Bearer realm=https://example.com/token,service=example.com";
        let c = parse_bearer_for_test(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
        assert_eq!(c.service.as_deref(), Some("example.com"));
    }

    #[test]
    fn parse_auth_challenge_recognizes_basic_scheme() {
        // ECR's WWW-Authenticate response shape.
        let v = r#"Basic realm="https://767397973649.dkr.ecr.us-east-1.amazonaws.com/",service="ecr.amazonaws.com""#;
        let c = parse_auth_challenge(v).unwrap();
        match c {
            AuthChallenge::Basic { realm } => {
                assert_eq!(
                    realm,
                    "https://767397973649.dkr.ecr.us-east-1.amazonaws.com/"
                );
            }
            AuthChallenge::Bearer(_) => panic!("expected Basic challenge"),
        }
    }

    #[test]
    fn parse_auth_challenge_basic_without_realm_succeeds_with_empty() {
        let v = "Basic";
        let c = parse_auth_challenge(v).unwrap();
        match c {
            AuthChallenge::Basic { realm } => assert_eq!(realm, ""),
            AuthChallenge::Bearer(_) => panic!("expected Basic challenge"),
        }
    }

    #[test]
    fn parse_auth_challenge_rejects_unknown_scheme() {
        let v = r#"Digest realm="x""#;
        let err = parse_auth_challenge(v).unwrap_err().to_string();
        assert!(
            err.contains("unsupported scheme"),
            "expected error mentioning unsupported scheme, got: {err}"
        );
    }

    #[test]
    fn parse_auth_challenge_rejects_missing_realm_on_bearer() {
        let v = r#"Bearer service="x",scope="y""#;
        assert!(parse_auth_challenge(v).is_err());
    }

    #[test]
    fn parse_auth_challenge_handles_case_insensitive_scheme() {
        let v = r#"bearer realm="https://example.com/token""#;
        let c = parse_bearer_for_test(v).unwrap();
        assert_eq!(c.realm, "https://example.com/token");
        let v2 = r#"basic realm="x""#;
        let c2 = parse_auth_challenge(v2).unwrap();
        assert!(matches!(c2, AuthChallenge::Basic { .. }));
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
        let cfg = RegistryTlsConfig::default();
        let url = manifest_url(&reference, &cfg);
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
        let cfg = RegistryTlsConfig::default();
        let url = manifest_url(&reference, &cfg);
        assert_eq!(
            url,
            "https://gcr.io/v2/distroless/static-debian12/manifests/latest"
        );
    }

    #[test]
    fn blob_url_uses_digest_directly() {
        let reference = super::super::reference::parse_reference("alpine:3.19").unwrap();
        let cfg = RegistryTlsConfig::default();
        let url = blob_url(&reference, "sha256:abc123", &cfg);
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
            tls_config: RegistryTlsConfig::default(),
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
            tls_config: RegistryTlsConfig::default(),
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

    /// End-to-end Basic-auth wire-up test (milestone 044 commit 2):
    /// when a registry returns 401 with `WWW-Authenticate: Basic
    /// realm="..."` (ECR's flavor) and the `RegistryClient` has
    /// credentials, the retry carries `Authorization: Basic
    /// <b64(user:secret)>` directly on the original URL — no realm
    /// round-trip.
    ///
    /// We spin up a TCP listener that speaks two HTTP request/response
    /// pairs over a single connection: first the unauthenticated
    /// challenge, then the authenticated retry that returns the
    /// manifest body.
    #[tokio::test]
    async fn fetch_with_auth_retry_handles_basic_challenge_with_credentials() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // Connection 1: unauthenticated GET → 401 Basic challenge.
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
            let req1 = String::from_utf8_lossy(&buf[..total]).into_owned();
            let resp1 = "HTTP/1.1 401 Unauthorized\r\n\
                         WWW-Authenticate: Basic realm=\"https://registry.example/\",service=\"ecr.amazonaws.com\"\r\n\
                         Content-Length: 0\r\n\
                         Connection: close\r\n\r\n";
            stream.write_all(resp1.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            // Connection 2: authenticated retry → 200 with body.
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
            let req2 = String::from_utf8_lossy(&buf[..total]).into_owned();
            let body = r#"{"hello":"world"}"#;
            let resp2 = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp2.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            (req1, req2)
        });

        let client = RegistryClient {
            http: reqwest::Client::new(),
            credentials: Some(Credential {
                username: "AWS".to_string(),
                secret: "ecr-token-d34db33f".to_string(),
            }),
            cache: None,
            tls_config: RegistryTlsConfig::default(),
        };

        let url = format!("http://{addr}/v2/foo/bar/manifests/latest");
        let body = client
            .fetch_with_auth_retry(&url, &["application/json"])
            .await
            .unwrap();
        assert_eq!(body.bytes, br#"{"hello":"world"}"#.to_vec());

        let (req1, req2) = server.await.unwrap();
        let lower1 = req1.to_ascii_lowercase();
        assert!(
            !lower1
                .lines()
                .any(|l| l.starts_with("authorization:")),
            "first GET must be unauthenticated; got:\n{req1}"
        );
        // base64("AWS:ecr-token-d34db33f") = QVdTOmVjci10b2tlbi1kMzRkYjMzZg==
        assert!(
            req2.contains("Authorization: Basic QVdTOmVjci10b2tlbi1kMzRkYjMzZg==")
                || req2.contains("authorization: Basic QVdTOmVjci10b2tlbi1kMzRkYjMzZg=="),
            "retry must carry Basic auth header; got:\n{req2}"
        );
    }

    /// Counterpart: when the registry sends a Basic challenge but
    /// `RegistryClient` has NO credentials, the error message
    /// guides the user to `docker login` (or
    /// `aws ecr get-login-password | docker login` for ECR).
    #[tokio::test]
    async fn fetch_with_auth_retry_basic_without_credentials_errors_helpfully() {
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
            let resp = "HTTP/1.1 401 Unauthorized\r\n\
                        WWW-Authenticate: Basic realm=\"https://registry.example/\"\r\n\
                        Content-Length: 0\r\n\
                        Connection: close\r\n\r\n";
            stream.write_all(resp.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let client = RegistryClient {
            http: reqwest::Client::new(),
            credentials: None,
            cache: None,
            tls_config: RegistryTlsConfig::default(),
        };

        let url = format!("http://{addr}/v2/foo/bar/manifests/latest");
        let err = client
            .fetch_with_auth_retry(&url, &["application/json"])
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Basic auth challenge")
                && err.contains("docker login"),
            "expected error to guide the user toward `docker login`; got: {err}"
        );

        server.await.unwrap();
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
            tls_config: RegistryTlsConfig::default(),
        };

        let got = client.fetch_blob(&reference, &digest).await.unwrap();
        assert_eq!(
            got, bytes,
            "cache hit should return the previously-inserted bytes \
             without making a network call"
        );
    }

    // ── Milestone 182 — scheme-selection tests ────────────────────

    fn make_reference(registry: &str) -> ImageReference {
        ImageReference {
            registry: registry.to_string(),
            repository: "library/foo".to_string(),
            tag: Some("1.0".to_string()),
            digest: None,
        }
    }

    #[test]
    fn manifest_url_uses_https_by_default() {
        let cfg = RegistryTlsConfig::default();
        let r = make_reference("ghcr.io");
        assert_eq!(
            manifest_url(&r, &cfg),
            "https://ghcr.io/v2/library/foo/manifests/1.0"
        );
    }

    #[test]
    fn manifest_url_uses_http_when_insecure_matches() {
        let cfg = RegistryTlsConfig::from_args(
            &["127.0.0.1:5000".to_string()],
            &[],
            false,
        )
        .unwrap();
        let r = make_reference("127.0.0.1:5000");
        assert_eq!(
            manifest_url(&r, &cfg),
            "http://127.0.0.1:5000/v2/library/foo/manifests/1.0"
        );
    }

    #[test]
    fn blob_url_uses_http_when_insecure_matches() {
        let cfg = RegistryTlsConfig::from_args(
            &["dev-registry".to_string()],
            &[],
            false,
        )
        .unwrap();
        let r = make_reference("dev-registry");
        let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            blob_url(&r, digest, &cfg),
            format!("http://dev-registry/v2/library/foo/blobs/{digest}")
        );
    }

    #[test]
    fn scheme_for_registry_ignores_docker_io_endpoint_expansion() {
        // FR-005 regression pin: --insecure-registry docker.io does NOT
        // match registry-1.docker.io (the resolved endpoint). The
        // matcher fires on the user-facing name (docker.io) — but the
        // URL uses the resolved endpoint (registry-1.docker.io). This
        // proves the scheme decision is made pre-resolve, on the
        // user-facing name, per FR-005.
        let cfg = RegistryTlsConfig::from_args(
            &["docker.io".to_string()],
            &[],
            false,
        )
        .unwrap();
        // docker.io is what the operator typed → matches, so http.
        assert_eq!(scheme_for_registry("docker.io", &cfg), "http");
        // registry-1.docker.io is the resolved endpoint → does not match, so https.
        assert_eq!(scheme_for_registry("registry-1.docker.io", &cfg), "https");
    }
}
