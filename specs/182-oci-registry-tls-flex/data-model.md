# Data Model: OCI registry TLS + transport flexibility (m182)

**Feature**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Research**: [research.md](./research.md)

## 1. New Types Introduced

### 1.1 `HostMatcher` enum (parser-local, in `tls_config.rs`)

```rust
/// Parsed form of a single `--insecure-registry <val>` occurrence.
/// Constructed at CLI-parse time; consulted per-URL at scheme-decision time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostMatcher {
    /// `--insecure-registry example.com` — matches any port on that host.
    HostOnly(String),
    /// `--insecure-registry core:8080` — matches only that exact host:port.
    HostPort(String, u16),
}

impl HostMatcher {
    /// Parse a single flag value. Returns typed error on malformed input
    /// per FR-014 (parse errors fail-fast before any network call).
    pub(crate) fn parse(val: &str) -> Result<Self, HostMatcherParseError>;

    /// Check whether this matcher matches the given URL host + port.
    /// - `HostOnly(h)` matches iff `h == host` (any port).
    /// - `HostPort(h, p)` matches iff `h == host` AND `Some(p) == port`.
    pub(crate) fn matches(&self, host: &str, port: Option<u16>) -> bool;
}
```

**Parse error variants**:
- `MissingHost` — empty value or leading `:`
- `InvalidPort(String)` — non-numeric port after `:`
- `PortOutOfRange(u32)` — port doesn't fit u16
- Displayed via `thiserror` derive with actionable messages naming the offending value.

### 1.2 `InsecureRegistryMatcher` (in `tls_config.rs`)

```rust
/// Wraps a `Vec<HostMatcher>` with a single query method.
#[derive(Debug, Clone, Default)]
pub(crate) struct InsecureRegistryMatcher {
    matchers: Vec<HostMatcher>,
}

impl InsecureRegistryMatcher {
    /// True iff ANY registered matcher matches `(host, port)`.
    /// False when the vec is empty (default state — no flags set).
    pub(crate) fn matches(&self, host: &str, port: Option<u16>) -> bool;

    /// True iff at least one matcher is registered (used for debug logging).
    pub(crate) fn is_configured(&self) -> bool;
}
```

### 1.3 `RegistryTlsConfig` (in `tls_config.rs`)

```rust
/// The unified TLS/transport configuration passed from CLI to
/// RegistryClient. Immutable per-scan. Zero-cost when the operator
/// passes no m182 flags (all fields default-initialize to their
/// "unchanged behavior" values).
#[derive(Debug, Clone, Default)]
pub(crate) struct RegistryTlsConfig {
    /// From `--insecure-registry <host[:port]>` flags. Empty vec →
    /// default state (all URLs use https://).
    pub(crate) insecure_matcher: InsecureRegistryMatcher,
    /// From `--registry-ca-cert <path>` flags, one Certificate per PEM
    /// cert block found across all files. Empty vec → webpki-roots only.
    pub(crate) ca_bundle: Vec<reqwest::Certificate>,
    /// From `--insecure-tls-skip-verify` flag. Default: false.
    pub(crate) skip_verify: bool,
}

impl RegistryTlsConfig {
    /// Construct from clap-parsed CLI args. Returns error on parse or
    /// PEM-load failure (per FR-014, fails BEFORE any network call).
    pub(crate) fn from_args(
        insecure_registries: &[String],
        ca_cert_paths: &[std::path::PathBuf],
        skip_verify: bool,
    ) -> anyhow::Result<Self>;

    /// True iff the scheme for `(host, port)` should be `http://`.
    /// Consulted by `manifest_url` / `blob_url` in registry.rs.
    pub(crate) fn is_insecure_registry(&self, host: &str, port: Option<u16>) -> bool {
        self.insecure_matcher.matches(host, port)
    }
}
```

## 2. Existing Types Extended

### 2.1 `ScanArgs` in `mikebom-cli/src/cli/scan_cmd.rs`

Three new fields added next to the existing `registry_credentials_dir` at line 172:

```rust
/// `--insecure-registry <host[:port]>` — repeatable. When set, mikebom
/// pulls from the named host over `http://` instead of `https://`.
/// Host-only form matches any port; explicit `<host>:<port>` matches
/// only that port. Consumers: Harbor devenv (`--insecure-registry
/// core:8080`), air-gapped mirrors, local dev registries.
///
/// **Security**: only enable for registries you trust. Plain-HTTP
/// exposes credentials + blobs to network observers.
#[arg(long = "insecure-registry", value_name = "HOST[:PORT]", action = clap::ArgAction::Append)]
pub insecure_registry: Vec<String>,

/// `--registry-ca-cert <path>` — repeatable. Additional CA certificate(s)
/// to trust for HTTPS registry pulls, on top of the webpki root set.
/// Each file may be a PEM bundle (multiple concatenated certificates);
/// ALL certificates in each file are added.
///
/// **Failure modes** (fail-fast at scan startup, before network calls):
/// - File not found → error names the path
/// - Empty or non-PEM content → error names the path + "no PEM certs found"
/// - Malformed PEM → error names the path + underlying parse error
#[arg(long = "registry-ca-cert", value_name = "PATH", action = clap::ArgAction::Append)]
pub registry_ca_cert: Vec<std::path::PathBuf>,

/// `--insecure-tls-skip-verify` — disable TLS certificate chain/host/expiry
/// verification for ALL HTTPS registry pulls in this scan. Emits a
/// WARN-level structured log at scan start (Constitution Principle X).
///
/// **Security**: extremely dangerous in production. Use ONLY for CI/dev
/// against self-signed or hostname-mismatched certs where fetching the
/// CA is impractical. For private-CA production registries, prefer
/// `--registry-ca-cert <path>` instead.
#[arg(long = "insecure-tls-skip-verify")]
pub insecure_tls_skip_verify: bool,
```

### 2.2 `pull_to_tarball` signature in `oci_pull/mod.rs`

```rust
pub async fn pull_to_tarball(
    image_ref: &str,
    image_platform: Option<&str>,
    cache_size_cap: Option<u64>,
    creds_dir: Option<&Path>,
    // NEW m182:
    tls_config: &RegistryTlsConfig,
) -> Result<tempfile::TempDir> {
    // ... (unchanged body except pass tls_config to RegistryClient::new)
    let client = RegistryClient::new(&reference, cache_handle, creds_dir, tls_config)?;
    // ...
}
```

### 2.3 `RegistryClient::new` in `oci_pull/registry.rs`

```rust
pub(super) fn new(
    reference: &ImageReference,
    cache: Option<Cache>,
    creds_dir: Option<&std::path::Path>,
    // NEW m182:
    tls_config: &RegistryTlsConfig,
) -> Result<Self> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("mikebom/", env!("CARGO_PKG_VERSION")));

    // m182 CA bundle: add each additional certificate to the trust store.
    for cert in &tls_config.ca_bundle {
        builder = builder.add_root_certificate(cert.clone());
    }

    // m182 skip-verify: dangerous — emit the FR-007 WARN log.
    if tls_config.skip_verify {
        builder = builder.danger_accept_invalid_certs(true);
        tracing::warn!(
            image = %reference.registry,
            "TLS verification DISABLED via --insecure-tls-skip-verify — cert chain, hostname, and expiry checks are skipped for this scan. Use only for CI/dev against self-signed or hostname-mismatched certs. For production private-CA registries, use --registry-ca-cert instead."
        );
    }

    let http = builder
        .build()
        .context("building reqwest::Client for OCI registry")?;

    let credentials = super::auth::resolve_credentials_layered(
        &reference.registry,
        creds_dir,
    );
    // ... credentials logging unchanged ...

    Ok(Self {
        http,
        credentials,
        cache,
        tls_config: tls_config.clone(),  // NEW — stored for scheme decisions
    })
}
```

### 2.4 `RegistryClient` struct — new field

```rust
pub(super) struct RegistryClient {
    http: reqwest::Client,
    credentials: Option<Credentials>,
    cache: Option<Cache>,
    // NEW m182 — consulted by manifest_url / blob_url:
    tls_config: RegistryTlsConfig,
}
```

### 2.5 `manifest_url` / `blob_url` — scheme selection

```rust
fn manifest_url(reference: &ImageReference, tls_config: &RegistryTlsConfig) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    let (host, port) = split_host_port(registry);
    let scheme = if tls_config.is_insecure_registry(host, port) {
        "http"
    } else {
        "https"
    };
    format!(
        "{scheme}://{registry}/v2/{}/manifests/{}",
        reference.repository,
        reference.resolved_reference()
    )
}

fn blob_url(reference: &ImageReference, digest: &str, tls_config: &RegistryTlsConfig) -> String {
    let registry = resolve_registry_for_url(&reference.registry);
    let (host, port) = split_host_port(registry);
    let scheme = if tls_config.is_insecure_registry(host, port) {
        "http"
    } else {
        "https"
    };
    format!(
        "{scheme}://{registry}/v2/{}/blobs/{}",
        reference.repository, digest
    )
}

/// Helper: split "host:port" or "host" into (host, Option<port>).
fn split_host_port(hostport: &str) -> (&str, Option<u16>) {
    match hostport.rsplit_once(':') {
        Some((host, port_str)) => match port_str.parse::<u16>() {
            Ok(port) => (host, Some(port)),
            Err(_) => (hostport, None),
        },
        None => (hostport, None),
    }
}
```

## 3. Error Type Additions

Actionable error messages per FR-014, using `thiserror` for the parse layer and `anyhow::Context` for the load layer:

```rust
// In tls_config.rs — parse errors surfaced from clap value parsing:
#[derive(thiserror::Error, Debug)]
pub(crate) enum HostMatcherParseError {
    #[error("--insecure-registry value is empty")]
    Empty,
    #[error("--insecure-registry `{value}` has empty host")]
    MissingHost { value: String },
    #[error("--insecure-registry `{value}`: port `{port_str}` is not a valid u16 (expected 1..=65535)")]
    InvalidPort { value: String, port_str: String },
}

// In tls_config.rs — CA bundle load layer, propagated via anyhow::Context:
pub(crate) fn load_ca_bundle_from_paths(
    paths: &[std::path::PathBuf],
) -> anyhow::Result<Vec<reqwest::Certificate>> {
    let mut out = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path)
            .with_context(|| format!(
                "reading --registry-ca-cert file `{}`",
                path.display()
            ))?;
        // Iterate PEM blocks — supports multi-cert bundles per FR-006.
        let mut certs_found = 0;
        for cert_result in rustls_pemfile::certs(&mut std::io::Cursor::new(&bytes)) {
            let cert_der = cert_result
                .with_context(|| format!(
                    "parsing PEM in --registry-ca-cert file `{}`",
                    path.display()
                ))?;
            let cert = reqwest::Certificate::from_der(&cert_der)
                .with_context(|| format!(
                    "decoding certificate from --registry-ca-cert file `{}`",
                    path.display()
                ))?;
            out.push(cert);
            certs_found += 1;
        }
        if certs_found == 0 {
            anyhow::bail!(
                "no PEM certificates found in --registry-ca-cert file `{}` \
                 (expected one or more `-----BEGIN CERTIFICATE-----` blocks)",
                path.display()
            );
        }
    }
    Ok(out)
}
```

## 4. `fetch_with_auth_retry` — Error Enhancement (FR-014)

The existing error messages at `oci_pull/registry.rs:239-253` name credentials issues; m182 extends them to also name the m182 flags in TLS-failure paths:

```rust
// When the initial GET fails with a TLS/transport error (not HTTP 401):
match err.kind() {
    // TLS chain validation failure:
    _ if is_tls_chain_error(&err) => bail!(
        "TLS certificate chain validation failed for GET {url}. \
         For private-CA registries, pass --registry-ca-cert <path-to-ca.pem>. \
         For self-signed dev/CI certs, pass --insecure-tls-skip-verify \
         (unsafe for production). Underlying error: {err}"
    ),
    // Connection refused / plain-HTTP mismatch:
    _ if is_tls_handshake_error(&err) => bail!(
        "TLS handshake failed for GET {url}. If this registry uses plain HTTP, \
         pass --insecure-registry {host_display}. Underlying error: {err}"
    ),
    // Everything else — unchanged path.
    _ => Err(err).with_context(|| format!("sending GET {url}"))?,
}
```

Detection functions `is_tls_chain_error` and `is_tls_handshake_error` inspect `reqwest::Error`'s error chain for `rustls::Error` variants. Fallback if reqwest's error introspection is insufficient: substring-match on error text (documented as a fallback tactic in Phase 5 implementation notes).

## 5. Test Contract

**Unit tests in `tls_config.rs`**:
- `host_matcher_parse_host_only`
- `host_matcher_parse_host_port`
- `host_matcher_parse_rejects_missing_host`
- `host_matcher_parse_rejects_invalid_port`
- `host_matcher_matches_host_only_any_port`
- `host_matcher_matches_host_port_exact_only`
- `insecure_matcher_empty_never_matches`
- `insecure_matcher_multi_declaration`
- `load_ca_bundle_empty_paths_ok`
- `load_ca_bundle_missing_file_actionable_error`
- `load_ca_bundle_non_pem_content_actionable_error`
- `load_ca_bundle_multi_cert_bundle_loads_all`

**Clap parse tests in `scan_cmd.rs` tests module** (matching existing pattern at line 3500):
- `insecure_registry_flag_repeatable_parses`
- `registry_ca_cert_flag_repeatable_parses`
- `insecure_tls_skip_verify_bool_defaults_false`
- `all_three_flags_combined_parse_ok`

**Integration tests** (per user story):
- `oci_pull_plain_http.rs` — US1: wiremock plain-HTTP server, verify success WITH flag / failure WITHOUT
- `oci_pull_custom_ca.rs` — US2: rcgen-generated CA + wiremock HTTPS server, verify success WITH flag / failure WITHOUT
- `oci_pull_skip_verify.rs` — US3: intentionally bad cert (CN mismatch), verify success WITH flag / failure WITHOUT / WARN log fires
- `oci_pull_flag_composition.rs` — US4: three separate wiremock instances, all three flags in one invocation
- `oci_pull_backward_compat.rs` — SC-004: public-CA scan with NO m182 flags produces byte-identical SBOM output
