//! OCI registry TLS + transport configuration (milestone 182).
//!
//! Three CLI flags surface through this module:
//!   * `--insecure-registry <host[:port]>` — pull over plain HTTP.
//!   * `--registry-ca-cert <path>` — trust an additional CA (or bundle).
//!   * `--insecure-tls-skip-verify` — disable cert chain / hostname
//!     / expiry checks for all HTTPS pulls in this scan.
//!
//! Threading: `ScanArgs` → `RegistryTlsConfig::from_args` → passed as
//! `&RegistryTlsConfig` through `pull_to_tarball` → `RegistryClient::new`.
//! The struct is default-constructible; the "no flags set" state
//! behaves identically to pre-m182 (SC-004 byte-identity gate).
//!
//! Fail-fast semantics (FR-014): flag parse + PEM load errors surface
//! at scan startup before any network call. Error messages name the
//! offending value and the flag that produced it.

use anyhow::{bail, Context, Result};

/// Parsed form of a single `--insecure-registry <val>` occurrence.
///
/// Constructed at CLI-parse time; consulted per-URL at scheme-decision
/// time. Matching is on the user-facing registry name (what the operator
/// typed in `--image`), NOT on the resolved endpoint — so
/// `--insecure-registry docker.io` does NOT match `registry-1.docker.io`
/// (FR-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostMatcher {
    /// `--insecure-registry example.com` — matches any port on that host.
    HostOnly(String),
    /// `--insecure-registry core:8080` — matches only that exact host+port.
    HostPort(String, u16),
}

/// Parse errors surfaced from `HostMatcher::parse`. All variants
/// name the offending value so operators can spot the typo without
/// re-reading the invocation.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub(crate) enum HostMatcherParseError {
    #[error("--insecure-registry value is empty")]
    Empty,
    #[error("--insecure-registry `{value}` has empty host")]
    MissingHost { value: String },
    #[error(
        "--insecure-registry `{value}`: port `{port_str}` is not a valid u16 (expected 1..=65535)"
    )]
    InvalidPort { value: String, port_str: String },
}

impl HostMatcher {
    /// Parse a single `--insecure-registry <val>` value. Fail-fast per
    /// FR-014 — actionable error message with the offending value.
    pub(crate) fn parse(val: &str) -> std::result::Result<Self, HostMatcherParseError> {
        if val.is_empty() {
            return Err(HostMatcherParseError::Empty);
        }
        match val.rsplit_once(':') {
            Some((host, port_str)) => {
                if host.is_empty() {
                    return Err(HostMatcherParseError::MissingHost {
                        value: val.to_string(),
                    });
                }
                let port = port_str
                    .parse::<u16>()
                    .map_err(|_| HostMatcherParseError::InvalidPort {
                        value: val.to_string(),
                        port_str: port_str.to_string(),
                    })?;
                Ok(HostMatcher::HostPort(host.to_string(), port))
            }
            None => Ok(HostMatcher::HostOnly(val.to_string())),
        }
    }

    /// True iff this matcher matches the given URL host + port.
    ///   * `HostOnly(h)` matches iff `h == host` (any port).
    ///   * `HostPort(h, p)` matches iff `h == host` AND `Some(p) == port`.
    pub(crate) fn matches(&self, host: &str, port: Option<u16>) -> bool {
        match self {
            HostMatcher::HostOnly(h) => h == host,
            HostMatcher::HostPort(h, p) => h == host && Some(*p) == port,
        }
    }
}

/// Composite matcher wrapping the parsed `--insecure-registry` values.
///
/// Empty vec → default state (all URLs use `https://`). This is the
/// zero-cost path when the operator passes no `--insecure-registry`
/// flags — byte-identity guarantee for SC-004.
#[derive(Debug, Clone, Default)]
pub(crate) struct InsecureRegistryMatcher {
    matchers: Vec<HostMatcher>,
}

impl InsecureRegistryMatcher {
    /// True iff ANY registered matcher matches `(host, port)`.
    /// Short-circuits on first match. Returns false when the vec is
    /// empty (default state).
    pub(crate) fn matches(&self, host: &str, port: Option<u16>) -> bool {
        self.matchers.iter().any(|m| m.matches(host, port))
    }

    /// True iff at least one matcher is registered. Used for debug
    /// logging only.
    #[allow(dead_code)]
    pub(crate) fn is_configured(&self) -> bool {
        !self.matchers.is_empty()
    }
}

/// Unified TLS/transport configuration passed from CLI to the OCI
/// pull layer. Immutable per-scan. `Default::default()` produces the
/// pre-m182 behavior (webpki-only trust, https:// for all registries,
/// full cert validation).
#[derive(Debug, Clone, Default)]
pub(crate) struct RegistryTlsConfig {
    /// From `--insecure-registry <host[:port]>` flags. Empty vec → all
    /// URLs use `https://`.
    pub(crate) insecure_matcher: InsecureRegistryMatcher,
    /// From `--registry-ca-cert <path>` flags. One entry per PEM cert
    /// block found across all files (a single file may hold a bundle).
    /// Empty vec → webpki-roots only (unchanged).
    pub(crate) ca_bundle: Vec<reqwest::Certificate>,
    /// From `--insecure-tls-skip-verify`. Default: false.
    pub(crate) skip_verify: bool,
}

impl RegistryTlsConfig {
    /// Construct from clap-parsed CLI args. Fails at scan startup on
    /// parse or PEM-load errors (FR-014) — no network call happens
    /// until every declaration is validated.
    pub(crate) fn from_args(
        insecure_registries: &[String],
        ca_cert_paths: &[std::path::PathBuf],
        skip_verify: bool,
    ) -> Result<Self> {
        let mut matchers = Vec::with_capacity(insecure_registries.len());
        for val in insecure_registries {
            let matcher = HostMatcher::parse(val)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .with_context(|| "parsing --insecure-registry value")?;
            matchers.push(matcher);
        }
        let insecure_matcher = InsecureRegistryMatcher { matchers };
        let ca_bundle = load_ca_bundle_from_paths(ca_cert_paths)?;
        Ok(Self {
            insecure_matcher,
            ca_bundle,
            skip_verify,
        })
    }

    /// True iff `(host, port)` should be contacted over `http://`
    /// instead of `https://`. Consulted by `manifest_url` / `blob_url`
    /// in `registry.rs`.
    pub(crate) fn is_insecure_registry(&self, host: &str, port: Option<u16>) -> bool {
        self.insecure_matcher.matches(host, port)
    }
}

/// Load one or more PEM files into a flat vec of `reqwest::Certificate`.
///
/// Each file may be a bundle (multiple `-----BEGIN CERTIFICATE-----`
/// blocks); all are loaded (FR-006). File-level errors surface with
/// the offending path (FR-014).
pub(crate) fn load_ca_bundle_from_paths(
    paths: &[std::path::PathBuf],
) -> Result<Vec<reqwest::Certificate>> {
    let mut out = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path).with_context(|| {
            format!("reading --registry-ca-cert file `{}`", path.display())
        })?;
        // reqwest::Certificate::from_pem_bundle handles multi-cert
        // PEM bundles natively (added in reqwest 0.12.4). Zero new
        // deps needed — no rustls-pemfile pull.
        let certs = reqwest::Certificate::from_pem_bundle(&bytes).with_context(|| {
            format!(
                "parsing --registry-ca-cert file `{}` \
                 (expected one or more `-----BEGIN CERTIFICATE-----` blocks)",
                path.display()
            )
        })?;
        if certs.is_empty() {
            bail!(
                "no PEM certificates found in --registry-ca-cert file `{}` \
                 (expected one or more `-----BEGIN CERTIFICATE-----` blocks)",
                path.display()
            );
        }
        out.extend(certs);
    }
    Ok(out)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::io::Write;

    // ---- HostMatcher::parse -----------------------------------------

    #[test]
    fn host_matcher_parse_host_only() {
        let m = HostMatcher::parse("example.com").unwrap();
        assert_eq!(m, HostMatcher::HostOnly("example.com".to_string()));
    }

    #[test]
    fn host_matcher_parse_host_port() {
        let m = HostMatcher::parse("core:8080").unwrap();
        assert_eq!(m, HostMatcher::HostPort("core".to_string(), 8080));
    }

    #[test]
    fn host_matcher_parse_rejects_missing_host() {
        let e = HostMatcher::parse(":8080").unwrap_err();
        assert!(
            matches!(e, HostMatcherParseError::MissingHost { .. }),
            "got {e:?}"
        );
        let msg = format!("{e}");
        assert!(msg.contains("--insecure-registry"), "msg: {msg}");
        assert!(msg.contains(":8080"), "msg: {msg}");

        let e = HostMatcher::parse("").unwrap_err();
        assert_eq!(e, HostMatcherParseError::Empty);
    }

    #[test]
    fn host_matcher_parse_rejects_invalid_port() {
        let e = HostMatcher::parse("foo:xyz").unwrap_err();
        assert!(
            matches!(e, HostMatcherParseError::InvalidPort { .. }),
            "got {e:?}"
        );
        let msg = format!("{e}");
        assert!(msg.contains("xyz"), "msg: {msg}");
        assert!(msg.contains("u16"), "msg: {msg}");

        // Port above u16 range.
        let e = HostMatcher::parse("foo:99999").unwrap_err();
        assert!(
            matches!(e, HostMatcherParseError::InvalidPort { .. }),
            "got {e:?}"
        );
    }

    #[test]
    fn host_matcher_matches_host_only_any_port() {
        let m = HostMatcher::parse("registry.local").unwrap();
        assert!(m.matches("registry.local", None));
        assert!(m.matches("registry.local", Some(80)));
        assert!(m.matches("registry.local", Some(5000)));
        assert!(!m.matches("registry.remote", Some(80)));
    }

    #[test]
    fn host_matcher_matches_host_port_exact_only() {
        let m = HostMatcher::parse("core:8080").unwrap();
        assert!(m.matches("core", Some(8080)));
        assert!(!m.matches("core", Some(8081)));
        assert!(!m.matches("core", None));
        assert!(!m.matches("other", Some(8080)));
    }

    // ---- InsecureRegistryMatcher -------------------------------------

    #[test]
    fn insecure_matcher_empty_never_matches() {
        let m = InsecureRegistryMatcher::default();
        assert!(!m.is_configured());
        assert!(!m.matches("anything.local", None));
        assert!(!m.matches("anything.local", Some(80)));
    }

    #[test]
    fn insecure_matcher_multi_declaration() {
        let m = InsecureRegistryMatcher {
            matchers: vec![
                HostMatcher::parse("core:8080").unwrap(),
                HostMatcher::parse("dev-registry").unwrap(),
            ],
        };
        assert!(m.is_configured());
        assert!(m.matches("core", Some(8080)));
        assert!(m.matches("dev-registry", None));
        assert!(m.matches("dev-registry", Some(5000)));
        assert!(!m.matches("core", Some(8081))); // port-mismatch on HostPort
        assert!(!m.matches("prod-registry", None));
    }

    #[test]
    fn insecure_matcher_ignores_registry_endpoint_resolution() {
        // FR-005 regression pin: the flag matches on the user-facing
        // registry name (what the operator typed in `--image`), NOT on
        // any resolved endpoint. `docker.io` → `registry-1.docker.io`
        // resolution happens in registry.rs::resolve_registry_for_url,
        // but the m182 insecure-matcher is consulted at the pre-resolve
        // layer with the user-facing name.
        let m = InsecureRegistryMatcher {
            matchers: vec![HostMatcher::parse("docker.io").unwrap()],
        };
        assert!(m.matches("docker.io", None));
        assert!(!m.matches("registry-1.docker.io", None));
    }

    // ---- load_ca_bundle_from_paths -----------------------------------

    #[test]
    fn load_ca_bundle_empty_paths_ok() {
        let paths: Vec<std::path::PathBuf> = Vec::new();
        let certs = load_ca_bundle_from_paths(&paths).unwrap();
        assert!(certs.is_empty());
    }

    #[test]
    fn load_ca_bundle_missing_file_actionable_error() {
        let missing = std::path::PathBuf::from("/nonexistent/ca-bundle.pem");
        let e = load_ca_bundle_from_paths(std::slice::from_ref(&missing)).unwrap_err();
        let msg = format!("{e:#}");
        assert!(msg.contains("--registry-ca-cert"), "msg: {msg}");
        assert!(msg.contains("/nonexistent/ca-bundle.pem"), "msg: {msg}");
    }

    #[test]
    fn load_ca_bundle_non_pem_content_actionable_error() {
        let mut tf = tempfile::NamedTempFile::new().unwrap();
        writeln!(tf, "this is not a PEM file").unwrap();
        tf.flush().unwrap();
        let path = tf.path().to_path_buf();
        let e = load_ca_bundle_from_paths(std::slice::from_ref(&path)).unwrap_err();
        let msg = format!("{e:#}");
        // Either the reqwest parser fails to find a PEM block or our
        // empty-bundle bail fires. Both are actionable and name the path.
        assert!(msg.contains(path.to_string_lossy().as_ref()), "msg: {msg}");
        assert!(msg.contains("--registry-ca-cert"), "msg: {msg}");
    }

    /// Generate a self-signed test CA cert as a PEM string.
    /// Uses rcgen 0.13 (dev-dep) — pure-Rust, no C/openssl shell-out.
    fn make_test_ca_pem() -> String {
        let cert = rcgen::generate_simple_self_signed(vec!["test-ca.local".to_string()]).unwrap();
        cert.cert.pem()
    }

    #[test]
    fn load_ca_bundle_multi_cert_bundle_loads_all() {
        // Two concatenated self-signed CAs in one PEM bundle.
        let pem1 = make_test_ca_pem();
        let pem2 = make_test_ca_pem();
        let mut tf = tempfile::NamedTempFile::new().unwrap();
        tf.write_all(pem1.as_bytes()).unwrap();
        tf.write_all(pem2.as_bytes()).unwrap();
        tf.flush().unwrap();
        let certs = load_ca_bundle_from_paths(&[tf.path().to_path_buf()]).unwrap();
        assert_eq!(certs.len(), 2, "expected 2 certs from bundle, got {}", certs.len());
    }

    // ---- RegistryTlsConfig::from_args --------------------------------

    #[test]
    fn from_args_default_shape() {
        let cfg = RegistryTlsConfig::from_args(&[], &[], false).unwrap();
        assert!(!cfg.insecure_matcher.is_configured());
        assert!(cfg.ca_bundle.is_empty());
        assert!(!cfg.skip_verify);
        assert!(!cfg.is_insecure_registry("anything.local", None));
    }

    #[test]
    fn from_args_populates_insecure_matcher() {
        let cfg = RegistryTlsConfig::from_args(
            &["core:8080".to_string(), "dev-registry".to_string()],
            &[],
            false,
        )
        .unwrap();
        assert!(cfg.insecure_matcher.is_configured());
        assert!(cfg.is_insecure_registry("core", Some(8080)));
        assert!(cfg.is_insecure_registry("dev-registry", None));
        assert!(!cfg.is_insecure_registry("prod-registry", None));
    }

    #[test]
    fn from_args_bubbles_up_parse_error() {
        let e = RegistryTlsConfig::from_args(&[":8080".to_string()], &[], false).unwrap_err();
        let msg = format!("{e:#}");
        assert!(msg.contains("--insecure-registry"), "msg: {msg}");
        assert!(msg.contains(":8080"), "msg: {msg}");
    }
}
